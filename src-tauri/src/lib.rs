//! MarketLens 核心（设计文档 §5）。
//!
//! 全部业务逻辑都在 Rust 侧，前端只是视图层。所有可被前端调用的能力都通过
//! `commands` 暴露，且刻意不包含 SQL 与密钥库的原始操作（ADR #1、#2）。

mod commands;
mod error;
mod storage;
mod system;

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
pub fn run() {
    init_tracing();

    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            let db = tauri::async_runtime::block_on(storage::Db::open(&handle))?;
            app.manage(db);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![commands::system::app_info])
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

    /// 所有 i64 字段必须显式声明为 TS 的 number。
    ///
    /// ts-rs 默认把 i64 映射成 `bigint`，但我们的 IPC 走 JSON——前端拿到的实际是
    /// number。声明成 bigint 会让类型系统撒谎：调用方以为要处理 bigint，
    /// 运行时却是 number，这类错误只会在生产环境咬人。
    ///
    /// 新增导出类型时，把它加进下面的列表。
    #[test]
    fn i64_fields_are_declared_as_number_not_bigint() {
        let declared = [(
            "AppInfo",
            <AppInfo as TS>::export_to_string(&Config::default()).expect("导出失败"),
        )];

        for (name, ts) in declared {
            let code = strip_block_comments(&ts);
            assert!(
                !code.contains("bigint"),
                "{name} 的 TypeScript 声明里出现了 bigint。JSON 传输后实际是 number，\
                 请给对应的 i64 字段加 #[ts(type = \"number\")]。\n实际声明：\n{code}"
            );
        }
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
