//! MarketLens 核心（设计文档 §5）。
//!
//! 全部业务逻辑都在 Rust 侧，前端只是视图层。所有可被前端调用的能力都通过
//! `commands` 暴露，且刻意不包含 SQL 与密钥库的原始操作（ADR #1、#2）。

mod commands;
mod credentials;
mod error;
mod fetch;
mod market;
mod okx;
mod position;
mod review;
mod settings;
mod storage;
mod system;
mod vault;

pub use error::{AppError, AppResult};
pub use system::AppInfo;

use tauri::Manager;

/// 启动应用。
///
/// 数据库在 `setup` 阶段同步完成建库与迁移：它必须在任何命令被调用之前就绪，
/// 否则前端可能对着一个还没建表的库发查询。
///
/// 迁移失败会终止启动而不是降级运行——数据库不可用时应用没有任何可用功能，
/// 静默降级只会让用户看到一连串莫名其妙的报错。
///
/// **密钥库刻意不在这里解锁**：解锁需要用户输入主密码，是启动后由前端触发的动作。
pub fn run() {
    init_tracing();

    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();

            let db = tauri::async_runtime::block_on(storage::Db::open(&handle))?;
            app.manage(db);
            app.manage(fetch::cache::LiveCache::new());
            app.manage(okx::client::OkxClient::new()?);
            app.manage(vault::Vault::new(&handle)?);
            app.manage(review::ReviewRegistry::new());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::app_info,
            commands::system::bootstrap_state,
            commands::system::onboarding_complete,
            commands::live::live_refresh,
            commands::live::watchlist_get,
            commands::vault::vault_status,
            commands::vault::vault_unlock,
            commands::vault::vault_lock,
            commands::credentials::credentials_list,
            commands::credentials::credentials_save,
            commands::credentials::credentials_delete,
            commands::credentials::credentials_test,
            commands::account::account_snapshot,
            commands::account::watchlist_set,
            commands::account::watchlist_candidates,
            commands::review::review_plan,
            commands::review::review_fetch,
            commands::review::review_cancel,
            commands::review::review_context,
        ])
        .run(tauri::generate_context!())
        .expect("Tauri 应用启动失败");
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,marketlens_lib=debug"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}

#[cfg(test)]
mod tests {
    use sqlx::SqlitePool;
    use ts_rs::{Config, TS};

    use crate::commands::account::WatchlistCandidate;
    use crate::commands::system::BootstrapState;
    use crate::commands::vault::VaultStatus;
    use crate::credentials::{CredentialMeta, CredentialProbe};
    use crate::fetch::executor::{ExecutionReport, Progress, SeriesReport, SeriesStatus};
    use crate::fetch::plan::{AvailabilityNote, FetchPlan, SeriesKind, SeriesPlan};
    use crate::market::live::{LiveSnapshot, MarketState};
    use crate::market::regime::{Crowding, Regime, TrendRegime, VolRegime};
    use crate::position::history::{ClosedPosition, RegimeSnapshot};
    use crate::position::merge::MergeReport;
    use crate::position::stats::{ReviewStats, StatGroup};
    use crate::position::trace::TraceCoverage;
    use crate::position::{AccountOverview, AccountSnapshot, CurrencyBalance, Position};
    use crate::review::ReviewContext;
    use crate::system::AppInfo;

    /// 去掉 TS 源码里的块注释。
    ///
    /// 必须剥注释再判定：生成的声明会带上 Rust 的文档注释，而注释里完全可能
    /// 出现 "bigint" 这个词（我们的文档就在解释这个坑）。直接对全文做子串匹配
    /// 会被自己的注释绊倒。
    fn strip_block_comments(src: &str) -> String {
        let mut out = String::with_capacity(src.len());
        let mut chars = src.chars().peekable();
        let mut in_comment = false;

        while let Some(c) = chars.next() {
            if in_comment {
                if c == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    in_comment = false;
                }
                continue;
            }
            if c == '/' && chars.peek() == Some(&'*') {
                chars.next();
                in_comment = true;
                continue;
            }
            out.push(c);
        }
        out
    }

    /// 把 Rust 侧的全部导出类型写进前端的 `src/lib/types.ts`，并就地校验契约。
    ///
    /// 刻意**不用 ts-rs 的 `#[ts(export)]`**，它有两个实测踩到的坑：
    ///
    /// 1. 它为**每个类型**生成一个独立的导出测试，而它们写同一个文件时是
    ///    「第一个截断、后续追加」。于是**带过滤地跑测试**会把文件截断成只剩匹配到的
    ///    那一个类型——实测把 16 个类型截成了 1 个，而且丢的是尚未提交的内容。
    /// 2. 文件由别的测试写入，导致「校验生成结果」的测试无法确定性地读到它
    ///    （测试并行执行，顺序不保证）。
    ///
    /// 集中导出同时解决两者：过滤运行只会「不导出」而不会破坏文件；校验也能在
    /// 同一处、同一份字符串上完成，不依赖任何执行顺序。声明顺序也由这里决定，
    /// diff 才稳定（ts-rs 的自动导出顺序取决于测试调度）。
    ///
    /// **新增导出类型时必须加进下面的列表**——忘了加会让前端 import 不到该类型，
    /// 在 `npm run typecheck` 处立刻暴露，不会静默溜过去。
    #[test]
    fn export_and_verify_frontend_types() {
        let config = Config::default();
        let mut declarations: Vec<String> = Vec::new();
        macro_rules! declare {
            ($($ty:ty),+ $(,)?) => {
                $(
                    declarations.push(
                        <$ty as TS>::export_to_string(&config)
                            .expect(concat!("导出 ", stringify!($ty), " 失败")),
                    );
                )+
            };
        }

        declare!(
            AppInfo,
            MarketState,
            LiveSnapshot,
            Regime,
            TrendRegime,
            VolRegime,
            Crowding,
            VaultStatus,
            CredentialMeta,
            CredentialProbe,
            BootstrapState,
            AccountOverview,
            AccountSnapshot,
            CurrencyBalance,
            Position,
            TraceCoverage,
            WatchlistCandidate,
            FetchPlan,
            SeriesPlan,
            SeriesKind,
            SeriesStatus,
            SeriesReport,
            AvailabilityNote,
            Progress,
            ExecutionReport,
            ClosedPosition,
            RegimeSnapshot,
            ReviewStats,
            StatGroup,
            MergeReport,
            ReviewContext,
        );

        let mut output = String::from(
            "// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). \
             Do not edit this file manually.\n\n",
        );
        for declaration in &declarations {
            output.push_str(declaration);
            output.push('\n');
        }

        // 校验一：TS 声明里不得出现 bigint。
        //
        // ts-rs 把 `i64` **与 `u64`** 都映射成 `bigint`，而 IPC 走 JSON——
        // 前端拿到的实际是 number。声明成 bigint 会让类型系统撒谎：
        // 调用方以为要处理 bigint，运行时却是 number。
        //
        // 必须剥注释再判定：生成的声明带 Rust 文档注释，而注释里完全可能出现
        // "bigint" 这个词（本文档就在解释这个坑）。
        let code = strip_block_comments(&output);
        assert!(
            !code.contains("bigint"),
            "生成的 TypeScript 声明里出现了 bigint。JSON 传输后实际是 number，\
             请给对应的 i64/u64 字段加 #[ts(type = \"number\")]（可选字段用 \"number | null\"）。\
             \n实际声明：\n{code}"
        );

        // 校验二：类型数量必须与列表一致，防止「加了类型却忘了导出」。
        let declared = code.matches("export type").count();
        assert_eq!(
            declared,
            declarations.len(),
            "导出列表有 {} 项，但生成结果里有 {declared} 个类型声明",
            declarations.len()
        );

        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib/types.ts");
        std::fs::write(&path, output).expect("写入 types.ts 失败");
    }

    /// 版本号的单一来源是 tauri.conf.json（设计文档 §14）。Cargo.toml 必须与它一致，
    /// 否则界面上「应用版本」与「核心版本」会互相打架，而用户无从判断哪个是真的。
    #[test]
    fn cargo_version_matches_tauri_config() {
        let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json"))
            .expect("tauri.conf.json 不是合法 JSON");
        let declared = config["version"]
            .as_str()
            .expect("tauri.conf.json 缺少 version 字段");

        assert_eq!(
            declared,
            env!("CARGO_PKG_VERSION"),
            "tauri.conf.json 与 Cargo.toml 的版本号不一致：请以 tauri.conf.json 为准同步 Cargo.toml"
        );
    }

    /// 迁移必须能在空库上干净跑完。
    ///
    /// 这是对 0001_init.sql 的真实检验：schema 写错了在这里就会炸，
    /// 而不是等用户启动应用时才发现。
    #[tokio::test]
    async fn migrations_apply_on_empty_database() {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("无法创建内存数据库");

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("迁移执行失败");

        // 设计文档 §8 定义的 15 张业务表（不含 sqlite 内部表与 _sqlx_migrations）
        const EXPECTED_TABLES: i64 = 15;
        let tables: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name <> '_sqlx_migrations'",
        )
        .fetch_one(&pool)
        .await
        .expect("统计表数量失败");

        assert_eq!(tables, EXPECTED_TABLES, "建表数量与设计文档 §8 不一致");
    }
}
