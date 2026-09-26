//! 复盘采集的编排层（设计文档 §6.7.2）。
//!
//! 计划的登记与取消令牌不在这里——它们现在同时服务复盘与行情，
//! 已移到 [`crate::fetch::registry`]。本模块只负责把采集回来的数据装配成结论。

use serde::Serialize;
use ts_rs::TS;

use crate::error::AppResult;
use crate::market::reconstruct;
use crate::okx::client::OkxClient;
use crate::okx::credentials::Credentials;
use crate::position::history;
use crate::position::history::ClosedPosition;
use crate::position::merge::{self, MergeReport};
use crate::position::stats::{self, ReviewStats};
use crate::position::trace::{self, TraceCoverage};
use crate::storage::Db;

/// 复盘上下文：时段的历史仓位 + 归因 + 统计。
///
/// 这是 M4 的产出物，也是 M5 提示词模板的输入。装配流程刻意分成四步且每步都
/// **如实上报自己的贡献与缺口**——复盘结论的可信度取决于用户是否知道数据从哪来。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct ReviewContext {
    #[ts(type = "number")]
    pub from: i64,
    #[ts(type = "number")]
    pub to: i64,
    pub bar: String,
    /// 时段内的历史仓位（已合并双源、已归因）
    pub positions: Vec<ClosedPosition>,
    pub stats: ReviewStats,
    pub merge: MergeReport,
    pub trace: TraceCoverage,
    /// 非致命问题：某一步失败不终止整批复盘
    pub warnings: Vec<String>,
}

/// 装配复盘上下文。
///
/// 四步：
///   1. 同步官方历史仓位（游标分页）
///   2. 与本地留痕合并（补充极值、补全官方窗口之外的仓位）
///   3. 为每笔仓位补上**开仓时刻**的市场状态
///   4. 计算统计（含按市场状态分组）
pub async fn build_context(
    client: &OkxClient,
    db: &Db,
    credentials: Option<&Credentials>,
    from: i64,
    to: i64,
    bar: &str,
) -> AppResult<ReviewContext> {
    let mut warnings = Vec::new();

    // 1. 官方历史仓位。
    //
    // 凭据为 `None`（用户没连账户）时跳过联网，直接用已落库的数据——
    // 复盘应当能用历史数据离线跑，这正是「已定型数据永久缓存」的价值。
    let mut positions: Vec<ClosedPosition> = match credentials {
        Some(credentials) => match history::sync(client, db, credentials, from, to).await {
            Ok(report) => {
                if !report.reached_from {
                    warnings
                        .push("官方历史仓位在到达起始时间前已耗尽，该时段可能不完整".to_string());
                }
                db.closed_positions_between(from, to)
                    .await
                    .unwrap_or_default()
            }
            Err(err) => {
                warnings.push(format!("官方历史仓位同步失败：{err}"));
                // 同步失败时退回已落库的数据——旧的复盘结果仍然有价值
                db.closed_positions_between(from, to)
                    .await
                    .unwrap_or_default()
            }
        },
        None => {
            warnings.push("未选择凭据：本次复盘只使用本地已落库的数据".to_string());
            db.closed_positions_between(from, to)
                .await
                .unwrap_or_default()
        }
    };

    // 2. 双源合并
    let merge_report = match merge::merge_for_range(db, &mut positions, from, to).await {
        Ok(report) => report,
        Err(err) => {
            warnings.push(format!("本地留痕合并失败：{err}"));
            MergeReport {
                from_official: positions.len(),
                from_local: 0,
                enriched: 0,
                coverage_note: "本地留痕读取失败，未参与合并。".to_string(),
            }
        }
    };

    // 3. 归因：逐笔补上开仓时刻的市场状态
    reconstruct::attribute(db, bar, &mut positions).await;

    // 4. 统计
    let stats = stats::compute(&positions);

    // 留痕覆盖度（让用户自己判断样本可信度）
    let trace = match trace::coverage(db, crate::storage::now_ms()).await {
        Ok(coverage) => coverage,
        Err(err) => {
            warnings.push(format!("留痕覆盖度统计失败：{err}"));
            TraceCoverage {
                records: 0,
                has_gaps: false,
                max_gap_ms: 0,
                last_trace_at: None,
                note: "无法统计留痕覆盖度。".to_string(),
            }
        }
    };

    Ok(ReviewContext {
        from,
        to,
        bar: bar.to_string(),
        positions,
        stats,
        merge: merge_report,
        trace,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 对**真实 OKX 模拟盘**的完整复盘管线验证。
    ///
    /// 走完整条链：M3 采集 → 官方历史仓位同步 → 双源合并 → 开仓时刻归因 → 统计。
    /// 这是 M4 唯一能证明「各模块接得上」的验证方式——单元测试各自绿并不代表
    /// 串起来能跑。
    ///
    /// ```text
    /// MARKETLENS_TEST_API_KEY=... MARKETLENS_TEST_SECRET_KEY=... \
    /// MARKETLENS_TEST_PASSPHRASE=... \
    /// cargo test --lib okx_review_context -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore = "访问真实 OKX 私有端点，需显式提供凭据"]
    async fn okx_review_context_with_real_positions() {
        use std::sync::Arc;
        use std::sync::atomic::AtomicBool;

        let credentials = Credentials {
            api_key: std::env::var("MARKETLENS_TEST_API_KEY")
                .expect("缺少 MARKETLENS_TEST_API_KEY"),
            secret_key: std::env::var("MARKETLENS_TEST_SECRET_KEY")
                .expect("缺少 MARKETLENS_TEST_SECRET_KEY"),
            passphrase: std::env::var("MARKETLENS_TEST_PASSPHRASE")
                .expect("缺少 MARKETLENS_TEST_PASSPHRASE"),
            demo: true,
        };

        let client = OkxClient::new().expect("客户端构造失败");
        let db = Db::open_in_memory().await.expect("内存库创建失败");

        let now = crate::storage::now_ms();
        let to = now;
        let from = now - 30 * 86_400_000;
        let bar = "1H";

        // 第 0 步：用 M3 的采集计划器把归因所需的 K 线与资金费率拉回来
        let watchlist = vec!["SOL-USDT-SWAP".to_string()];
        let plan = crate::fetch::plan::expand(from, to, &watchlist, bar, now);
        println!(
            "采集计划：{} 条序列，{} 请求",
            plan.series.len(),
            plan.est_requests
        );
        let fetch = crate::fetch::executor::run(
            &client,
            &db,
            &plan,
            Arc::new(AtomicBool::new(false)),
            &|_| {},
        )
        .await
        .expect("采集失败");
        println!("采集完成：{} 请求", fetch.done_requests);

        // 第 1–4 步：同步 → 合并 → 归因 → 统计
        let context = build_context(&client, &db, Some(&credentials), from, to, bar)
            .await
            .expect("装配复盘上下文失败");

        println!("\n=== 复盘上下文 ===");
        println!("时段：{from} ~ {to}（{bar}）");
        println!("仓位：{} 笔", context.positions.len());
        println!("警告：{:?}", context.warnings);
        println!("\n=== 统计 ===");
        println!("总笔数      {}", context.stats.total);
        println!("胜率        {:.1}%", context.stats.win_rate * 100.0);
        println!("盈亏比      {:?}", context.stats.profit_factor);
        println!("期望值      {:.4}", context.stats.expectancy);
        println!("总盈亏      {:.4}", context.stats.total_realized_pnl);
        println!("平均持仓    {} ms", context.stats.avg_hold_ms);
        println!("费用侵蚀    {:?}", context.stats.fee_drag);
        println!("未归因      {} 笔", context.stats.unattributed);
        println!("\n=== 逐笔归因 ===");
        for position in &context.positions {
            println!(
                "{} {} {} 倍 | 盈亏 {:.4} | 来源 {} ({}) | 开仓时状态 {:?}",
                position.inst_id,
                position.direction,
                position.lever,
                position.realized_pnl,
                position.source,
                position.time_precision,
                position.regime.as_ref().map(|regime| (
                    regime.trend,
                    regime.vol,
                    regime.crowding,
                    regime.change_24h.map(|v| (v * 100.0).round()),
                )),
            );
        }
        println!("\n=== 按开仓时趋势分组 ===");
        for group in &context.stats.by_trend {
            println!(
                "  {:<10} {} 笔 | 胜率 {:.1}% | 合计 {:.4}",
                group.key,
                group.count,
                group.win_rate * 100.0,
                group.total_pnl
            );
        }
        println!("\n=== 双源合并 ===");
        println!("{}", context.merge.coverage_note);

        assert!(
            context.warnings.iter().all(|w| !w.contains("失败")),
            "不应有步骤失败：{:#?}",
            context.warnings
        );
        assert!(
            !context.positions.is_empty(),
            "模拟盘里有一笔已平仓位，应当被同步到"
        );

        // 关键断言：**归因必须真的跑通**——这是 M4 的核心价值
        let attributed = context
            .positions
            .iter()
            .filter(|p| p.regime.as_ref().is_some_and(|r| r.trend.is_some()))
            .count();
        assert!(
            attributed > 0,
            "没有任何仓位拿到开仓时刻的市场状态——归因链路没跑通：{:#?}",
            context
                .positions
                .iter()
                .map(|p| (&p.inst_id, &p.regime))
                .collect::<Vec<_>>()
        );

        // 方向必须来自 direction 而不是 posSide
        assert!(
            context
                .positions
                .iter()
                .any(|p| p.direction == "long" || p.direction == "short"),
            "方向字段应当是 long/short，实际：{:#?}",
            context
                .positions
                .iter()
                .map(|p| &p.direction)
                .collect::<Vec<_>>()
        );
    }
}
