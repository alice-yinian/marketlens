//! 采集计划的注册表：`plan` 产出、`fetch` 消费，外加取消令牌。
//!
//! 命令层只做参数校验与转发，状态都收敛在这里。
//!
//! ## 为什么两种计划各存一张表
//!
//! 复盘计划（`FetchPlan`）与行情计划（`KlinePlan`）字段与语义都不同：
//! 前者是「一个时段 × 一个粒度 × N 个标的」，后者是「一个标的 × N 个粒度，
//! 每个粒度各有区间」。共用一个泛型容器只会让每个读取点都要向下转型，
//! 而转型失败只能在运行时发现。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::error::{AppError, AppResult};
use crate::fetch::plan::{FetchPlan, KlinePlan};

#[derive(Default)]
pub struct FetchRegistry {
    review_plans: Mutex<HashMap<String, FetchPlan>>,
    kline_plans: Mutex<HashMap<String, KlinePlan>>,
    cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl FetchRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记一个复盘计划。同 id 的重复登记会覆盖（`review_plan` 是幂等的）。
    pub fn register_review(&self, plan: FetchPlan) -> String {
        let id = plan.id.clone();
        self.review_plans().insert(id.clone(), plan);
        id
    }

    /// 登记一个行情计划。同 id 覆盖的语义与复盘一致。
    pub fn register_kline(&self, plan: KlinePlan) -> String {
        let id = plan.id.clone();
        self.kline_plans().insert(id.clone(), plan);
        id
    }

    pub fn review_plan(&self, id: &str) -> AppResult<FetchPlan> {
        self.review_plans()
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::Config(format!("采集计划 {id} 不存在（可能已重启应用）")))
    }

    pub fn kline_plan(&self, id: &str) -> AppResult<KlinePlan> {
        self.kline_plans()
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::Config(format!("行情计划 {id} 不存在（可能已重启应用）")))
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

    fn review_plans(&self) -> std::sync::MutexGuard<'_, HashMap<String, FetchPlan>> {
        self.review_plans
            .lock()
            .unwrap_or_else(|err| err.into_inner())
    }

    fn kline_plans(&self) -> std::sync::MutexGuard<'_, HashMap<String, KlinePlan>> {
        self.kline_plans
            .lock()
            .unwrap_or_else(|err| err.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 取消令牌按 id 隔离：两个计划同时在跑时，取消其中一个不该影响另一个。
    #[test]
    fn cancel_tokens_are_isolated() {
        let registry = FetchRegistry::new();

        let first = registry.begin("plan-a");
        let second = registry.begin("plan-b");
        assert!(!registry.is_cancelled("plan-a"));
        assert!(!registry.is_cancelled("plan-b"));

        assert!(registry.cancel("plan-a"));
        assert!(registry.is_cancelled("plan-a"));
        assert!(!registry.is_cancelled("plan-b"), "取消必须逐个计划生效");

        // 同一个 id 重复调用返回同一个令牌，否则取消会落在一个没人看的旗标上
        let again = registry.begin("plan-a");
        assert!(Arc::ptr_eq(&first, &again));
        assert!(Arc::ptr_eq(&second, &registry.begin("plan-b")));

        registry.finish("plan-a");
        assert!(!registry.is_cancelled("plan-a"), "结束后令牌应被清理");
        assert!(
            !registry.cancel("plan-a"),
            "计划结束后再取消应返回 false，而不是假装成功"
        );
    }

    #[test]
    fn unknown_plan_is_a_clear_error() {
        let registry = FetchRegistry::new();
        let err = registry.review_plan("nope").expect_err("应报错");
        assert!(err.to_string().contains("不存在"), "错误要说清原因：{err}");

        let err = registry.kline_plan("nope").expect_err("应报错");
        assert!(
            err.to_string().contains("行情计划"),
            "错误要说清是哪种计划：{err}"
        );
    }

    /// 两种计划各自往返：登记后必须能按 id 原样取回。
    /// 取错表（把行情当复盘取）会让下游用一个字段形状不对的计划去跑。
    #[test]
    fn both_plan_kinds_round_trip() {
        let registry = FetchRegistry::new();

        let now = 1_800_000_000_000;
        let review = crate::fetch::plan::expand(
            now - 86_400_000,
            now,
            &["BTC-USDT-SWAP".to_string()],
            "1H",
            now,
        );
        let review_id = registry.register_review(review);
        let fetched = registry.review_plan(&review_id).expect("应能取回复盘计划");
        assert_eq!(fetched.id, review_id);
        assert!(!fetched.series.is_empty());

        let kline = crate::fetch::plan::expand_kline(
            "BTC-USDT-SWAP",
            &["1H".to_string(), "1D".to_string()],
            200,
            now,
        )
        .expect("应能展开");
        let kline_id = registry.register_kline(kline);
        let fetched = registry.kline_plan(&kline_id).expect("应能取回行情计划");
        assert_eq!(fetched.id, kline_id);
        assert_eq!(fetched.bars.len(), 2);
        assert!(
            fetched
                .series
                .iter()
                .all(|s| s.kind == crate::fetch::plan::SeriesKind::Candles)
        );
    }

    /// 执行结束后只清取消令牌，**计划本身要留着**：界面还要用它显示参数与结果。
    #[test]
    fn finish_keeps_the_plan() {
        let registry = FetchRegistry::new();
        let now = 1_800_000_000_000;
        let plan = crate::fetch::plan::expand_kline("BTC-USDT-SWAP", &["1H".to_string()], 100, now)
            .expect("应能展开");
        let id = registry.register_kline(plan);

        registry.begin(&id);
        registry.finish(&id);

        assert!(!registry.is_cancelled(&id), "结束后不应残留取消状态");
        assert!(
            registry.kline_plan(&id).is_ok(),
            "计划本身应保留以便再次查看"
        );
    }
}
