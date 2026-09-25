//! 复盘采集的编排层（设计文档 §6.7.2）。
//!
//! 职责是把「计划」与「执行」之间的状态管起来：计划注册表（`review_plan` 产出、
//! `review_fetch` 消费）与取消令牌。命令层只做参数校验与转发。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use ts_rs::TS;

use crate::error::{AppError, AppResult};
use crate::fetch::plan::FetchPlan;
use crate::market::reconstruct;
use crate::okx::client::OkxClient;
use crate::position::history;
use crate::position::history::ClosedPosition;
use crate::position::merge::{self, MergeReport};
use crate::position::stats::{self, ReviewStats};
use crate::position::trace::{self, TraceCoverage};
use crate::storage::Db;
use crate::vault::Vault;

/// 计划与取消令牌的注册表。
#[derive(Default)]
pub struct ReviewRegistry {
    plans: Mutex<HashMap<String, FetchPlan>>,
    cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl ReviewRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记一个计划。同 id 的重复登记会覆盖（`review_plan` 是幂等的）。
    pub fn register(&self, plan: FetchPlan) -> String {
        let id = plan.id.clone();
        self.plans().insert(id.clone(), plan);
        id
    }

    pub fn plan(&self, id: &str) -> AppResult<FetchPlan> {
        self.plans()
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::Config(format!("采集计划 {id} 不存在（可能已重启应用）")))
    }

    /// 开始执行：建立取消令牌。重复调用会返回同一个令牌。
    pub fn begin(&self, id: &str) -> Arc<AtomicBool> {
        let mut cancels = self.cancels.lock().unwrap_or_else(|err| err.into_inner());
        cancels
            .entry(id.to_string())
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .clone()
    }

    /// 请求取消。返回 `true` 表示确实有一个正在执行的计划被标记为取消。
    ///
    /// 返回布尔值而不是静默成功，是为了让界面能区分「已请求取消」与
    /// 「计划早就结束了，点了也没用」——否则用户会一直等一个不会来的结果。
    pub fn cancel(&self, id: &str) -> bool {
        {
            let cancels = self.cancels.lock().unwrap_or_else(|err| err.into_inner());
            if let Some(token) = cancels.get(id) {
                token.store(true, Ordering::Relaxed);
                tracing::info!(plan = id, "已请求取消采集");
            }
        }
        self.is_cancelled(id)
    }

    pub fn is_cancelled(&self, id: &str) -> bool {
        self.cancels
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .get(id)
            .is_some_and(|token| token.load(Ordering::Relaxed))
    }

    /// 执行结束后清理取消令牌（计划本身保留，便于「再次查看」）。
    pub fn finish(&self, id: &str) {
        self.cancels
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .remove(id);
    }

    fn plans(&self) -> std::sync::MutexGuard<'_, HashMap<String, FetchPlan>> {
        self.plans.lock().unwrap_or_else(|err| err.into_inner())
    }
}

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
    vault: &Vault,
    credential_id: &str,
    from: i64,
    to: i64,
    bar: &str,
) -> AppResult<ReviewContext> {
    let mut warnings = Vec::new();

    // 1. 官方历史仓位
    let mut positions: Vec<ClosedPosition> =
        match crate::credentials::credentials_for(vault, credential_id) {
            Ok(credentials) => match history::sync(client, db, &credentials, from, to).await {
                Ok(report) => {
                    if !report.reached_from {
                        warnings.push(
                            "官方历史仓位在到达起始时间前已耗尽，该时段可能不完整".to_string(),
                        );
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
            Err(err) => {
                warnings.push(format!("凭据不可用：{err}"));
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
    use crate::fetch::plan;

    fn sample_plan() -> FetchPlan {
        let now = 1_800_000_000_000;
        plan::expand(
            now - 86_400_000,
            now,
            &["BTC-USDT-SWAP".to_string()],
            "1H",
            now,
        )
    }

    #[test]
    fn registers_and_returns_plan() {
        let registry = ReviewRegistry::new();
        let id = registry.register(sample_plan());
        let fetched = registry.plan(&id).expect("应能取回计划");
        assert_eq!(fetched.id, id);
        assert!(!fetched.series.is_empty());
    }

    #[test]
    fn unknown_plan_is_a_clear_error() {
        let registry = ReviewRegistry::new();
        let err = registry.plan("nope").expect_err("不存在的计划应报错");
        assert!(
            err.to_string().contains("不存在"),
            "错误信息应说清原因：{err}"
        );
    }

    #[test]
    fn cancel_token_is_shared_and_latched() {
        let registry = ReviewRegistry::new();
        let id = registry.register(sample_plan());

        let token = registry.begin(&id);
        assert!(!token.load(Ordering::Relaxed));

        // 重复 begin 必须拿到同一个令牌，否则取消会失效
        let again = registry.begin(&id);
        registry.cancel(&id);
        assert!(again.load(Ordering::Relaxed), "取消必须反映到同一个令牌上");
        assert!(registry.is_cancelled(&id));
    }

    #[test]
    fn cancelling_an_idle_plan_is_not_an_error() {
        let registry = ReviewRegistry::new();
        registry.cancel("never-started");
        assert!(!registry.is_cancelled("never-started"));
    }

    #[test]
    fn finish_clears_the_token_but_keeps_the_plan() {
        let registry = ReviewRegistry::new();
        let id = registry.register(sample_plan());
        registry.begin(&id);
        registry.finish(&id);

        assert!(!registry.is_cancelled(&id), "结束后不应残留取消状态");
        assert!(registry.plan(&id).is_ok(), "计划本身应保留以便再次查看");
    }
}
