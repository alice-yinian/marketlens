//! 复盘采集的编排层（设计文档 §6.7.2）。
//!
//! 职责是把「计划」与「执行」之间的状态管起来：计划注册表（`review_plan` 产出、
//! `review_fetch` 消费）与取消令牌。命令层只做参数校验与转发。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::error::{AppError, AppResult};
use crate::fetch::plan::FetchPlan;

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
