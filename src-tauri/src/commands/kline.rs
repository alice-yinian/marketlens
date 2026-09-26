//! 行情（K 线 + 逐根指标）命令。
//!
//! 与复盘同构：`plan`（不联网，只估算）→ `fetch`（执行，带进度与取消）→ `build`（装配渲染）。
//!
//! **为什么指标是 `build` 的参数而不是 `plan` 的**：指标完全不影响取数——
//! K 线拉回来就是拉回来了。「换个指标看看」不该重新联网拉一遍，
//! 这与复盘「采集负责拉数据、装配负责算结论」是同一条分割线。

use serde::Deserialize;
use tauri::{AppHandle, Emitter, State};

use crate::error::AppResult;
use crate::fetch::executor::{self, ExecutionReport, PROGRESS_EVENT, Progress};
use crate::fetch::plan::{self, KlinePlan};
use crate::fetch::registry::FetchRegistry;
use crate::market::series::{self, IndicatorColumn, IndicatorSpec, MarketSeries};
use crate::okx::client::OkxClient;
use crate::prompt::templates::TemplateKind;
use crate::prompt::{context, privacy::PrivacyLevel};
use crate::storage::Db;

/// 一个可选的 K 线粒度（界面下拉/多选的数据源）。
///
/// 由 Rust 侧的白名单生成，而不是在前端再抄一份：抄一份的话，
/// 白名单里加一个粒度不会自动出现在界面上，而两边不一致只能靠人眼发现。
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "types.ts")]
pub struct BarOption {
    pub value: String,
    pub label: String,
}

/// 列出受支持的 K 线粒度。
#[tauri::command]
pub async fn kline_bars() -> AppResult<Vec<BarOption>> {
    Ok(plan::CANDLE_BARS
        .iter()
        .map(|(value, label)| BarOption {
            value: (*value).to_string(),
            label: (*label).to_string(),
        })
        .collect())
}

/// `kline_plan` 的入参。
#[derive(Debug, Deserialize)]
pub struct KlinePlanRequest {
    pub inst_id: String,
    /// K 线粒度（如 `1H`、`4H`、`1D`），至少一个
    pub bars: Vec<String>,
    /// 每个周期取最近多少根
    pub candle_count: usize,
}

/// 生成行情取数计划。**不联网**——只做校验与估算。
///
/// 一步都不联网是刻意的：用户调参数时不该有请求发出去，
/// 而是先看到「要拉多少、大概多久、提示词会有多长」，确认后再执行。
#[tauri::command(rename_all = "snake_case")]
pub async fn kline_plan(
    registry: State<'_, FetchRegistry>,
    request: KlinePlanRequest,
) -> AppResult<KlinePlan> {
    let plan = plan::expand_kline(
        &request.inst_id,
        &request.bars,
        request.candle_count,
        crate::storage::now_ms(),
    )?;
    registry.register_kline(plan.clone());
    Ok(plan)
}

/// 执行行情取数计划。
///
/// 与 `review_fetch` 共用执行器，所以「序列内顺序分页、序列间并发、已收盘永久缓存、
/// 序列级续传、可取消」这一整套行为是完全一样的——不是各写一遍。
#[tauri::command(rename_all = "snake_case")]
pub async fn kline_fetch(
    app: AppHandle,
    client: State<'_, OkxClient>,
    db: State<'_, Db>,
    registry: State<'_, FetchRegistry>,
    plan_id: String,
) -> AppResult<ExecutionReport> {
    let plan = registry.kline_plan(&plan_id)?;
    let cancel = registry.begin(&plan_id);

    tracing::info!(
        plan = %plan_id,
        inst = %plan.inst_id,
        bars = plan.bars.len(),
        est_requests = plan.est_requests,
        "开始执行行情取数计划"
    );

    let notify = |progress: Progress| {
        if let Err(err) = app.emit(PROGRESS_EVENT, &progress) {
            // 发不出事件不影响采集本身，但要留痕（否则界面会静默停在半路）
            tracing::warn!(%err, "推送采集进度失败");
        }
    };

    let report = executor::run(&client, &db, &plan, cancel, &notify).await;

    registry.finish(&plan_id);
    report
}

/// 请求取消正在执行的行情取数。
///
/// 返回 `true` 表示确实有一个正在执行的计划被取消；`false` 表示它已经结束了。
#[tauri::command(rename_all = "snake_case")]
pub async fn kline_cancel(registry: State<'_, FetchRegistry>, plan_id: String) -> AppResult<bool> {
    Ok(registry.cancel(&plan_id))
}

/// `kline_build` 的入参。
#[derive(Debug, Deserialize)]
pub struct KlineBuildRequest {
    pub plan_id: String,
    /// 模板 id；给了 `body` 时以 `body` 为准（编辑器实时预览）
    #[serde(default)]
    pub template_id: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    /// 逐根附列的指标清单，缺省 = 不带指标（只要原始 K 线）
    #[serde(default)]
    pub indicators: Vec<IndicatorSpec>,
}

/// 装配并渲染行情提示词。
///
/// K 线从**本地库**读（`kline_fetch` 已经把数据落了库），所以重复「换个指标看看」
/// 是纯本地操作，零请求。
#[tauri::command(rename_all = "snake_case")]
pub async fn kline_build(
    db: State<'_, Db>,
    registry: State<'_, FetchRegistry>,
    request: KlineBuildRequest,
) -> AppResult<crate::commands::prompt::PromptOutput> {
    let plan = registry.kline_plan(&request.plan_id)?;
    let specs = series::validate_specs(&request.indicators)?;

    let template = crate::commands::prompt::resolve_template(
        &db,
        request.template_id.as_deref(),
        request.body,
        TemplateKind::Market,
    )
    .await?;

    let mut damage: Vec<String> = Vec::new();
    let mut series: Vec<MarketSeries> = Vec::with_capacity(plan.bars.len());

    for bar_plan in &plan.bars {
        let market_series = load_series(&db, &plan, bar_plan, &specs).await?;

        // 一个周期都拿不到不该让整份提示词报废——如实记下来，剩下的照常渲染。
        // 与执行器「单个序列失败不终止整个计划」是同一条原则。
        if market_series.candle_count() == 0 {
            damage.push(format!("{}：没有取到任何 K 线", bar_plan.label));
        } else if market_series.is_short() {
            damage.push(format!(
                "{}：请求 {} 根，实际只有 {} 根",
                bar_plan.label,
                market_series.requested_count,
                market_series.candle_count()
            ));
        }

        series.push(market_series);
    }

    let assembled = context::market(
        &plan.inst_id,
        &series,
        // 行情不是隐私（§6.7）：这里传 L0，界面也不显示隐私选择器——
        // 显示它会让人以为「切到 L2 能脱敏价格」，而价格本来就是公开数据。
        PrivacyLevel::L0,
        damage,
        crate::storage::now_ms(),
    );

    crate::commands::prompt::finish(
        &template,
        &assembled.context,
        assembled.warnings,
        PrivacyLevel::L0,
    )
}

/// 从库里读一个周期的 K 线并算好指标。
///
/// ## 为什么读的比显示的多
///
/// 指标要在**更长的历史**上算，否则 `EMA200` 配上「只要 200 根」会得到 199 个 `—`——
/// 用户选了 200 根，看到的却是一列破折号，这个功能就等于没有。
/// 所以按 `请求根数 + 最长指标周期` 去读（库里有多少用多少，读多了没有惩罚），
/// 算完指标再**只显示最后 N 根**。
///
/// 这与图表工具的行为一致：图上画 200 根 K 线，EMA200 照样是完整的一条线。
/// 库里历史不足时仍然诚实——头部该是 `—` 就是 `—`，并由模板声明原因。
async fn load_series(
    db: &Db,
    plan: &KlinePlan,
    bar_plan: &plan::KlineBarPlan,
    specs: &[IndicatorSpec],
) -> AppResult<MarketSeries> {
    let reserve = specs
        .iter()
        .map(|spec| spec.period as usize)
        .max()
        .unwrap_or(0);
    let read_limit = (bar_plan.candle_count + reserve) as i64;

    // `kind = "last"` 与复盘的落库口径一致（见 `SeriesKind::candle_kind`）。
    let candles = db
        .candles_before(
            &plan.inst_id,
            "last",
            &bar_plan.bar,
            bar_plan.to,
            read_limit,
        )
        .await?;

    let indicators = series::compute(&candles, specs, &bar_plan.bar)?;

    // 只展示最后 `candle_count` 根
    let visible = bar_plan.candle_count.min(candles.len());
    let start = candles.len() - visible;

    Ok(MarketSeries {
        inst_id: plan.inst_id.clone(),
        bar: bar_plan.bar.clone(),
        candles: candles[start..].to_vec(),
        indicators: indicators
            .into_iter()
            .map(|column| IndicatorColumn {
                values: column.values[start..].to_vec(),
                ..column
            })
            .collect(),
        requested_count: bar_plan.candle_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::Candle;

    const HOUR: i64 = 3_600_000;
    const NOW: i64 = 1_800_000_000_000;

    async fn seeded_db(count: usize) -> Db {
        let db = Db::open_in_memory().await.expect("内存库创建失败");
        let candles: Vec<Candle> = (0..count)
            .map(|index| {
                let close = 100.0 + (index as f64 * 0.1) + ((index % 11) as f64 * 0.2);
                Candle {
                    ts: NOW - (count - index) as i64 * HOUR,
                    open: close - 0.2,
                    high: close + 0.3,
                    low: close - 0.4,
                    close,
                    vol: 500.0 + index as f64,
                    confirm: true,
                }
            })
            .collect();
        db.insert_candles("BTC-USDT-SWAP", "last", "1H", &candles)
            .await
            .expect("写入 K 线失败");
        db
    }

    fn plan_of(count: usize) -> KlinePlan {
        plan::expand_kline("BTC-USDT-SWAP", &["1H".to_string()], count, NOW).expect("应能展开")
    }

    fn spec(period: u32) -> IndicatorSpec {
        IndicatorSpec {
            kind: crate::market::series::IndicatorKind::Ema,
            period,
        }
    }

    /// 展示窗口是「请求的根数」，但指标在**更长的历史**上算。
    ///
    /// 这条是「EMA200 + 200 根 = 199 个破折号」那个坑的回归测试：
    /// 若哪天有人把读取范围改成 `candle_count`，这里会立刻变红。
    #[test]
    fn indicators_use_more_history_than_displayed() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("运行时创建失败");

        runtime.block_on(async {
            let db = seeded_db(80).await;
            let plan = plan_of(30);
            let bar_plan = &plan.bars[0];

            let series = load_series(&db, &plan, bar_plan, &[spec(20)])
                .await
                .expect("应能读取");

            // 只显示请求的 30 根
            assert_eq!(series.candle_count(), 30);
            assert!(!series.is_short(), "库里数据足够，不该报「实际更少」");
            assert_eq!(series.requested_count, 30);

            // 但 EMA20 在**每一根显示出来的 K 线**上都有值：
            // 它是在 30 + 20 = 50 根上算出来的，不是只在 30 根上算。
            let column = &series.indicators[0];
            assert_eq!(column.values.len(), 30, "指标列必须与显示的行等长");
            assert_eq!(
                column.insufficient_bars(),
                0,
                "读取范围若等于显示范围，这里会变成 19 —— 那正是要防的坑"
            );
            assert!(!column.is_unavailable());
        });
    }

    /// 库里历史不足时不撒谎：该是 `—` 就是 `—`，并且能被上层声明出来。
    #[test]
    fn short_history_leaves_leading_gaps() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("运行时创建失败");

        runtime.block_on(async {
            let db = seeded_db(25).await;
            let plan = plan_of(25);
            let bar_plan = &plan.bars[0];

            let series = load_series(&db, &plan, bar_plan, &[spec(20)])
                .await
                .expect("应能读取");

            assert_eq!(series.candle_count(), 25);
            let column = &series.indicators[0];
            assert_eq!(column.values.len(), 25);
            // EMA20 的种子是前 20 根的均值，所以第 20 根才有第一个值 →
            // 前面 19 根不可得，后面 6 根有值。
            assert_eq!(column.insufficient_bars(), 19);
            assert!(!column.is_unavailable(), "后半段算得出来，整列不算不可得");
        });
    }

    /// 一根都没取到（例如从没跑过 fetch）时不该 panic，也不能假装有数据。
    #[test]
    fn empty_database_yields_an_empty_series() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("运行时创建失败");

        runtime.block_on(async {
            let db = Db::open_in_memory().await.expect("内存库创建失败");
            let plan = plan_of(30);
            let bar_plan = &plan.bars[0];

            let series = load_series(&db, &plan, bar_plan, &[spec(20)])
                .await
                .expect("应能读取");

            assert_eq!(series.candle_count(), 0);
            assert!(series.is_short(), "0 根当然比请求的少");
            assert!(series.indicators[0].values.is_empty());
            assert!(!series.indicators[0].is_unavailable());
        });
    }
}
