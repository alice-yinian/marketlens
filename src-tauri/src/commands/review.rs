//! 复盘采集命令（设计文档 §6.8）。

use serde::Deserialize;
use tauri::{AppHandle, Emitter, State};

use crate::error::{AppError, AppResult};
use crate::fetch::executor::PROGRESS_EVENT;
use crate::fetch::executor::{self, ExecutionReport, Progress};
use crate::fetch::plan::{self, FetchPlan, MAX_RANGE_MS};
use crate::fetch::registry::FetchRegistry;
use crate::okx::client::OkxClient;
use crate::settings;
use crate::storage::Db;
use crate::vault::Vault;

/// `review_plan` 的入参。
#[derive(Debug, Deserialize)]
pub struct PlanRequest {
    pub from: i64,
    pub to: i64,
    /// K 线粒度，缺省 `1H`
    #[serde(default)]
    pub bar: Option<String>,
    /// 标的集，缺省用当前关注列表
    #[serde(default)]
    pub inst_ids: Option<Vec<String>>,
}

/// 生成采集计划。**不联网**——只做校验与估算。
///
/// 刻意不在这一步拉数据：用户打开复盘页时不该有请求发出去，
/// 而是先看到「要拉多少、大概多久、哪些指标拿不到」，确认后再执行。
#[tauri::command]
pub async fn review_plan(
    db: State<'_, Db>,
    registry: State<'_, FetchRegistry>,
    request: PlanRequest,
) -> AppResult<FetchPlan> {
    if request.to <= request.from {
        return Err(AppError::Config("结束时间必须晚于开始时间".to_string()));
    }
    if request.to - request.from > MAX_RANGE_MS {
        return Err(AppError::RangeTooLarge);
    }

    let inst_ids = match request.inst_ids {
        Some(list) if !list.is_empty() => list,
        _ => settings::read_watchlist(&db).await?,
    };

    let bar = request.bar.unwrap_or_else(|| "1H".to_string());
    // 用白名单而不是 `bar_millis`：后者是「数字 + m/H/D/W」的泛匹配，
    // 会放行 `5H`、`100D` 这类 OKX 不接受的组合，而错误要到联网取数时才暴露
    // （报的还是含糊的 `51000 Parameter bar error`）。
    if !plan::is_supported_bar(&bar) {
        return Err(AppError::Config(format!("不支持的 K 线粒度：{bar}")));
    }

    let plan = plan::expand(
        request.from,
        request.to,
        &inst_ids,
        &bar,
        crate::storage::now_ms(),
    );
    registry.register_review(plan.clone());
    Ok(plan)
}

/// 执行采集计划。
///
/// `rename_all = "snake_case"` 是必需的，不是装饰：Tauri v2 的宏**默认把命令参数名
/// 转成 camelCase**（`tauri-macros` 的 `ArgumentCase::Camel`），而我们的契约是
/// **前后端统一 snake_case**（ADR #8）。少了这个属性，前端按契约发 `plan_id`，
/// 后端却去找 `planId`，命令会在运行时反序列化失败——而且错误信息只说「缺少参数」，
/// 很难联想到是大小写问题。多词参数名一律要加。
#[tauri::command(rename_all = "snake_case")]
pub async fn review_fetch(
    app: AppHandle,
    client: State<'_, OkxClient>,
    db: State<'_, Db>,
    registry: State<'_, FetchRegistry>,
    plan_id: String,
) -> AppResult<ExecutionReport> {
    let plan = registry.review_plan(&plan_id)?;
    let cancel = registry.begin(&plan_id);

    tracing::info!(
        plan = %plan_id,
        series = plan.series.len(),
        est_requests = plan.est_requests,
        "开始执行采集计划"
    );

    let notify = |progress: Progress| {
        if let Err(err) = app.emit(PROGRESS_EVENT, &progress) {
            // 发不出事件不影响采集本身，但要留痕（否则界面会静默停在半路）
            tracing::warn!(%err, "推送采集进度失败");
        }
    };

    let report = executor::run(&client, &db, &plan, cancel, &notify).await;

    registry.finish(&plan_id);

    match &report {
        Ok(report) => tracing::info!(
            plan = %plan_id,
            requests = report.done_requests,
            elapsed_ms = report.elapsed_ms,
            cancelled = report.cancelled,
            "采集计划结束"
        ),
        Err(err) => tracing::error!(plan = %plan_id, %err, "采集计划失败"),
    }

    report
}

/// 请求取消正在执行的计划。
///
/// 返回 `true` 表示确实有一个正在执行的计划被取消；`false` 表示它已经结束了。
/// 参数名的 `rename_all` 见 `review_fetch` 的说明。
#[tauri::command(rename_all = "snake_case")]
pub async fn review_cancel(registry: State<'_, FetchRegistry>, plan_id: String) -> AppResult<bool> {
    Ok(registry.cancel(&plan_id))
}

/// `review_context` 的入参。
#[derive(Debug, Deserialize)]
pub struct ContextRequest {
    pub credential_id: String,
    pub from: i64,
    pub to: i64,
    /// K 线粒度，缺省 `1H`
    #[serde(default)]
    pub bar: Option<String>,
}

/// 装配复盘上下文：时段内的历史仓位（双源合并 + 开仓时刻归因）+ 统计。
///
/// 与 `review_fetch` 分开是刻意的：**采集**负责把原始数据拉回本地，
/// **装配**负责把它算成结论。两者可以独立重跑——改了统计口径只需重新装配，
/// 不必再花几秒重新联网拉数据。
#[tauri::command(rename_all = "snake_case")]
pub async fn review_context(
    client: State<'_, OkxClient>,
    db: State<'_, Db>,
    vault: State<'_, Vault>,
    request: ContextRequest,
) -> AppResult<crate::review::ReviewContext> {
    if request.to <= request.from {
        return Err(AppError::Config("结束时间必须晚于开始时间".to_string()));
    }
    if request.to - request.from > MAX_RANGE_MS {
        return Err(AppError::RangeTooLarge);
    }

    let bar = request.bar.unwrap_or_else(|| "1H".to_string());
    let credentials = crate::credentials::credentials_for(&vault, &request.credential_id)?;
    crate::review::build_context(
        &client,
        &db,
        Some(&credentials),
        request.from,
        request.to,
        &bar,
    )
    .await
}
