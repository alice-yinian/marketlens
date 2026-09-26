//! 采集执行器（设计文档 §6.2.2）。
//!
//! 结构：序列内顺序分页（游标依赖），序列间并发（`Semaphore` 限流）。
//!
//! 三条刻意的设计选择：
//!
//! 1. **速率控制交给限流器，而不是靠串行等待**。并发只用来重叠网络往返；
//!    串行版在 M1 实测过，3 个标的要 18.6 秒。
//! 2. **单个序列失败不终止整个计划**。用户宁可拿到 17/18 条序列加一条警告，
//!    也不愿因为某个 Rubik 端点抖动而什么都没有。
//! 3. **续传是序列级的**：已完成的序列整条跳过；被取消的那条下次重拉。
//!    页级续传需要把游标持久化到每一页，收益是省几页请求——当前不值得那个复杂度，
//!    所以这里如实说明粒度，而不是假装支持页级。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use futures::future::join_all;
use serde::Serialize;
use tokio::sync::Semaphore;
use ts_rs::TS;

use crate::error::AppResult;
use crate::fetch::paging;
use crate::fetch::plan::{FetchPlan, SeriesKind, SeriesPlan};
use crate::market::Candle;
use crate::okx::client::OkxClient;
use crate::okx::models;
use crate::storage::Db;

/// 默认并发上限。
pub const DEFAULT_CONCURRENCY: usize = 4;

/// 采集进度事件名。
///
/// 放在执行器旁边而不是某个命令文件里：它由执行器产出的 [`Progress`] 驱动，
/// 而复盘与行情**共用同一条进度通道**（前端只监听这一个事件名）。
/// 常量若挂在某一个功能下，另一个功能引用它就会形成毫无道理的依赖。
pub const PROGRESS_EVENT: &str = "fetch://progress";

/// 执行器需要的最小计划视图。
///
/// 为什么不直接收 `&FetchPlan`：行情页的计划是「单标的 × 多周期」，
/// 每个周期各有自己的时间区间，塞进 `FetchPlan` 的单个 `from`/`to`/`bar`
/// 就得让那几个字段说谎——而界面会把它们显示给用户。
///
/// 两种计划共用同一条执行路径，也就是共用「序列内顺序分页、序列间并发、
/// 已收盘数据永久缓存、进度上报、序列级续传、可取消」这一整套行为。
pub trait ExecutablePlan: Send + Sync {
    fn plan_id(&self) -> &str;
    fn series(&self) -> &[SeriesPlan];
    /// 预估请求总数（进度条的分母）
    fn est_requests(&self) -> usize;
}

impl ExecutablePlan for FetchPlan {
    fn plan_id(&self) -> &str {
        &self.id
    }

    fn series(&self) -> &[SeriesPlan] {
        &self.series
    }

    fn est_requests(&self) -> usize {
        self.est_requests
    }
}

impl ExecutablePlan for crate::fetch::plan::KlinePlan {
    fn plan_id(&self) -> &str {
        &self.id
    }

    fn series(&self) -> &[SeriesPlan] {
        &self.series
    }

    fn est_requests(&self) -> usize {
        self.est_requests
    }
}

/// 进度快照。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct Progress {
    pub plan_id: String,
    pub done_series: usize,
    pub total_series: usize,
    pub done_requests: usize,
    pub total_requests: usize,
    /// 正在拉的序列，界面据此显示「正在拉 X」
    pub current: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub enum SeriesStatus {
    Ok,
    /// 数据在该时段确实不存在（例如超出 Rubik 的 48 小时窗口）
    Unavailable,
    Failed,
    /// 续传：上次已完成
    Skipped,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct SeriesReport {
    pub key: String,
    pub label: String,
    pub status: SeriesStatus,
    pub rows: usize,
    pub pages: usize,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct ExecutionReport {
    pub plan_id: String,
    pub series: Vec<SeriesReport>,
    pub done_requests: usize,
    #[ts(type = "number")]
    pub elapsed_ms: i64,
    pub cancelled: bool,
}

struct ProgressState {
    done_series: usize,
    done_requests: usize,
    current: String,
}

/// 进度上报通道。把「改状态」与「通知界面」绑在一起，
/// 避免出现「状态更新了但界面没收到」这种半吊子。
struct ProgressSink<'a> {
    plan: &'a dyn ExecutablePlan,
    state: &'a Mutex<ProgressState>,
    /// 必须带 `Send + Sync`：这个回调会被并发执行的序列共用，
    /// 而 Tauri 命令的 future 本身要求 `Send`。
    notify: &'a (dyn Fn(Progress) + Send + Sync),
}

impl ProgressSink<'_> {
    fn page_done(&self, label: &str) {
        let snapshot = {
            let mut state = self.state.lock().unwrap_or_else(|err| err.into_inner());
            state.done_requests += 1;
            state.current = label.to_string();
            Progress {
                plan_id: self.plan.plan_id().to_string(),
                done_series: state.done_series,
                total_series: self.plan.series().len(),
                done_requests: state.done_requests,
                total_requests: self.plan.est_requests(),
                current: state.current.clone(),
            }
        };
        (self.notify)(snapshot);
    }

    fn series_done(&self) {
        let snapshot = {
            let mut state = self.state.lock().unwrap_or_else(|err| err.into_inner());
            state.done_series += 1;
            Progress {
                plan_id: self.plan.plan_id().to_string(),
                done_series: state.done_series,
                total_series: self.plan.series().len(),
                done_requests: state.done_requests,
                total_requests: self.plan.est_requests(),
                current: state.current.clone(),
            }
        };
        (self.notify)(snapshot);
    }
}

/// 执行一个采集计划。
pub async fn run(
    client: &OkxClient,
    db: &Db,
    plan: &dyn ExecutablePlan,
    cancel: Arc<AtomicBool>,
    notify: &(dyn Fn(Progress) + Send + Sync),
) -> AppResult<ExecutionReport> {
    let started = std::time::Instant::now();
    let state = Mutex::new(ProgressState {
        done_series: 0,
        done_requests: 0,
        current: String::new(),
    });
    let sink = ProgressSink {
        plan,
        state: &state,
        notify,
    };

    // 计划已按优先级排序，而 tokio 的 Semaphore 是公平的（FIFO），
    // 因此先排队的序列会先拿到许可 —— 优先级顺序得以保持。
    let semaphore = Arc::new(Semaphore::new(DEFAULT_CONCURRENCY));

    let futures = plan.series().iter().map(|series| {
        let permit = semaphore.clone();
        let cancel = cancel.clone();
        let sink = &sink;
        async move {
            let _permit = permit.acquire_owned().await;
            execute_series(client, db, series, &cancel, sink).await
        }
    });

    let mut series: Vec<SeriesReport> = join_all(futures).await;

    // 报告顺序按计划顺序（并发完成顺序是乱的，直接返回会让界面每次都不一样）
    let order: std::collections::HashMap<&str, usize> = plan
        .series()
        .iter()
        .enumerate()
        .map(|(index, item)| (item.key.as_str(), index))
        .collect();
    series.sort_by_key(|report| {
        order
            .get(report.key.as_str())
            .copied()
            .unwrap_or(usize::MAX)
    });

    let done_requests = {
        let state = state.lock().unwrap_or_else(|err| err.into_inner());
        state.done_requests
    };

    Ok(ExecutionReport {
        plan_id: plan.plan_id().to_string(),
        series,
        done_requests,
        elapsed_ms: started.elapsed().as_millis() as i64,
        cancelled: cancel.load(Ordering::Relaxed),
    })
}

async fn execute_series(
    client: &OkxClient,
    db: &Db,
    series: &SeriesPlan,
    cancel: &AtomicBool,
    sink: &ProgressSink<'_>,
) -> SeriesReport {
    let mut report = SeriesReport {
        key: series.key.clone(),
        label: series.label.clone(),
        status: SeriesStatus::Failed,
        rows: 0,
        pages: 0,
        reason: None,
    };

    // 续传：上次已完成的序列整条跳过
    match db.fetch_state_status(&series.key).await {
        Ok(Some(status)) if status == "ok" => {
            report.status = SeriesStatus::Skipped;
            report.reason = Some("上次已采集完成，跳过".to_string());
            sink.series_done();
            return report;
        }
        Ok(_) => {}
        Err(err) => {
            // 读不到续传状态不该阻止采集，但要留痕
            tracing::warn!(key = %series.key, %err, "读取续传状态失败，按未完成处理");
        }
    }

    let outcome = match series.kind {
        SeriesKind::Candles | SeriesKind::MarkCandles | SeriesKind::IndexCandles => {
            fetch_and_store_candles(client, db, series, cancel, sink).await
        }
        SeriesKind::FundingRate => fetch_and_store_funding(client, db, series, cancel, sink).await,
        SeriesKind::LongShortRatio | SeriesKind::TakerVolume => {
            fetch_and_store_rubik(client, db, series, sink).await
        }
    };

    match outcome {
        Ok(outcome) => {
            report.rows = outcome.rows;
            report.pages = outcome.pages;

            if outcome.cancelled {
                report.status = SeriesStatus::Failed;
                report.reason = Some("已取消".to_string());
                let _ = db
                    .mark_fetch_state(&series.key, "partial", Some("已取消"))
                    .await;
            } else if outcome.rows == 0 {
                // 拉到了但一条都没有：对这个时段而言就是不可得，不是失败
                report.status = SeriesStatus::Unavailable;
                report.reason = Some("该时段没有数据".to_string());
                let _ = db
                    .mark_fetch_state(&series.key, "unavailable", Some("该时段没有数据"))
                    .await;
            } else {
                report.status = SeriesStatus::Ok;
                // 数据在到达起始时间前就耗尽了：不是错误，但用户需要知道
                // 「这段可能不完整」——沉默地给一段残缺数据是最危险的做法。
                if !outcome.reached_from {
                    report.reason =
                        Some("数据在到达起始时间前已耗尽，该时段可能不完整".to_string());
                }
                let _ = db.mark_fetch_state(&series.key, "ok", None).await;
            }
        }
        Err(err) => {
            report.status = SeriesStatus::Failed;
            report.reason = Some(err.to_string());
            let _ = db
                .mark_fetch_state(&series.key, "error", Some(&err.to_string()))
                .await;
        }
    }

    sink.series_done();
    report
}

/// 一条序列的采集结果（执行器内部用）。
struct SeriesOutcome {
    rows: usize,
    pages: usize,
    cancelled: bool,
    /// 是否触及了 `from` 边界。`false` 表示数据在到达起始时间前就耗尽了。
    reached_from: bool,
}

/// 从序列 key 反解 `[from, to]`。
///
/// key 的形状因种类而异（`candles|inst|bar|from|to` 与 `lsr|ccy|from|to` 段数不同），
/// 所以**从尾部取**——这样将来在前缀里加字段也不会破坏解析。
fn range_from_key(key: &str) -> (i64, i64) {
    let mut parts = key.rsplit('|');
    let to = parts
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let from = parts
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    (from, to)
}

async fn fetch_and_store_candles(
    client: &OkxClient,
    db: &Db,
    series: &SeriesPlan,
    cancel: &AtomicBool,
    sink: &ProgressSink<'_>,
) -> AppResult<SeriesOutcome> {
    let Some(inst_id) = series.inst_id.as_deref() else {
        return Err(crate::error::AppError::Config(
            "K 线序列缺少标的".to_string(),
        ));
    };
    let bar = series.key.split('|').nth(2).unwrap_or("1H").to_string();
    let (from, to) = range_from_key(&series.key);

    let mut on_page = |pages: usize| {
        sink.page_done(&series.label);
        !cancel.load(Ordering::Relaxed) && pages < crate::fetch::plan::MAX_PAGES
    };

    let result = paging::fetch_candles(
        client,
        series.kind.candle_path(),
        inst_id,
        &bar,
        from,
        to,
        &mut on_page,
    )
    .await?;

    let candles: Vec<Candle> = Candle::parse_rows(&result.rows);
    if !candles.is_empty() {
        db.insert_candles(inst_id, series.kind.candle_kind(), &bar, &candles)
            .await?;
    }

    Ok(SeriesOutcome {
        rows: candles.len(),
        pages: result.pages,
        cancelled: result.cancelled,
        reached_from: result.reached_from,
    })
}

async fn fetch_and_store_funding(
    client: &OkxClient,
    db: &Db,
    series: &SeriesPlan,
    cancel: &AtomicBool,
    sink: &ProgressSink<'_>,
) -> AppResult<SeriesOutcome> {
    let Some(inst_id) = series.inst_id.as_deref() else {
        return Err(crate::error::AppError::Config(
            "资金费率序列缺少标的".to_string(),
        ));
    };
    let (from, to) = range_from_key(&series.key);

    let mut on_page = |pages: usize| {
        sink.page_done(&series.label);
        !cancel.load(Ordering::Relaxed) && pages < crate::fetch::plan::MAX_PAGES
    };

    let result = paging::fetch_funding_history(client, inst_id, from, to, &mut on_page).await?;

    let points: Vec<(i64, f64)> = result
        .rows
        .iter()
        .map(|record| (record.funding_time, record.funding_rate))
        .collect();
    if !points.is_empty() {
        db.insert_metric_series(inst_id, "funding", &points).await?;
    }

    Ok(SeriesOutcome {
        rows: result.rows.len(),
        pages: result.pages,
        cancelled: result.cancelled,
        reached_from: result.reached_from,
    })
}

async fn fetch_and_store_rubik(
    client: &OkxClient,
    db: &Db,
    series: &SeriesPlan,
    sink: &ProgressSink<'_>,
) -> AppResult<SeriesOutcome> {
    let Some(ccy) = series.ccy.as_deref() else {
        return Err(crate::error::AppError::Config(
            "Rubik 序列缺少币种".to_string(),
        ));
    };
    let (from, to) = range_from_key(&series.key);

    let rows = paging::fetch_rubik_series(client, series.kind, ccy, from, to).await?;
    sink.page_done(&series.label);

    let metric = match series.kind {
        SeriesKind::LongShortRatio => "lsr",
        SeriesKind::TakerVolume => "taker_ratio",
        _ => "unknown",
    };

    let points: Vec<(i64, f64)> = match series.kind {
        SeriesKind::LongShortRatio => models::parse_single_value_series(&rows),
        SeriesKind::TakerVolume => models::parse_taker_series(&rows)
            .into_iter()
            .filter_map(|(ts, sell, buy)| {
                if sell > 0.0 {
                    Some((ts, buy / sell))
                } else {
                    None
                }
            })
            .collect(),
        _ => Vec::new(),
    };

    if !points.is_empty() {
        db.insert_metric_series(ccy, metric, &points).await?;
    }

    Ok(SeriesOutcome {
        rows: points.len(),
        pages: 1,
        cancelled: false,
        // Rubik 是单次区间请求，没有翻页概念，视为已覆盖请求区间
        reached_from: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::plan;

    const DAY_MS: i64 = 86_400_000;

    fn watchlist() -> Vec<String> {
        vec!["BTC-USDT-SWAP".to_string(), "ETH-USDT-SWAP".to_string()]
    }

    /// 对**真实 OKX API** 的端到端采集验证（M3 的验收场景）。
    ///
    /// ```text
    /// cargo test --lib okx_thirty_day -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore = "访问真实 OKX API，需显式运行"]
    async fn okx_thirty_day_two_instrument_fetch() {
        let client = OkxClient::new().expect("客户端构造失败");
        let db = Db::open_in_memory().await.expect("内存库创建失败");

        let now = crate::storage::now_ms();
        let to = now;
        let from = now - 30 * DAY_MS;
        let plan = plan::expand(from, to, &watchlist(), "1H", now);

        println!("=== 计划 ===");
        println!(
            "序列 {} 条，预估 {} 请求，预估耗时 {} ms",
            plan.series.len(),
            plan.est_requests,
            plan.est_duration_ms
        );
        for warning in &plan.warnings {
            println!("  可得性提示：{} — {}", warning.metric, warning.reason);
        }

        let cancel = Arc::new(AtomicBool::new(false));
        let started = std::time::Instant::now();
        let report = run(&client, &db, &plan, cancel, &|_progress| {})
            .await
            .expect("执行失败");
        let elapsed = started.elapsed();

        println!("\n=== 实际 ===");
        println!(
            "{} 请求，耗时 {:.1} 秒（预估 {} ms）",
            report.done_requests,
            elapsed.as_secs_f64(),
            plan.est_duration_ms
        );
        for series in &report.series {
            println!(
                "  [{:?}] {:<28} {} 行 / {} 页{}",
                series.status,
                series.label,
                series.rows,
                series.pages,
                series
                    .reason
                    .as_deref()
                    .map(|reason| format!("（{reason}）"))
                    .unwrap_or_default()
            );
        }

        // 不应有失败：不可得是允许的（Rubik 在 30 天窗口下必然不可得），
        // 但 Failed 说明有东西真的坏了。
        let failed: Vec<_> = report
            .series
            .iter()
            .filter(|s| s.status == SeriesStatus::Failed)
            .collect();
        assert!(failed.is_empty(), "不应有失败的序列：{failed:#?}");
        assert!(!report.cancelled);
        assert_eq!(report.series.len(), plan.series.len());

        // 6 条 K 线序列（价格 / 标记价 / 指数价 × 2 标的）必须都成功
        let ok_candles = report
            .series
            .iter()
            .filter(|s| s.label.contains("K 线") && s.status == SeriesStatus::Ok)
            .count();
        assert_eq!(ok_candles, 6, "6 条 K 线序列都应成功：{:#?}", report.series);

        // 落库验证：三种 K 线都写进去了，且条数足够覆盖 30 天
        for inst_id in ["BTC-USDT-SWAP", "ETH-USDT-SWAP"] {
            for kind in ["last", "mark"] {
                let count = db.count_candles(inst_id, kind).await.expect("统计失败");
                assert!(
                    count >= 700,
                    "{inst_id} 的 {kind} K 线只有 {count} 根，30 天应有约 720 根"
                );
            }
        }
        for index_id in ["BTC-USDT", "ETH-USDT"] {
            let count = db.count_candles(index_id, "index").await.expect("统计失败");
            assert!(count >= 700, "{index_id} 的指数 K 线只有 {count} 根");
        }

        // 续传状态应当被记录（每条序列一行）
        let states = db.count_fetch_states().await.expect("统计失败");
        assert_eq!(
            states as usize,
            plan.series.len(),
            "每条序列都应留下续传状态"
        );

        // 验收：30 天 2 标的应在 10 秒内完成
        assert!(
            elapsed.as_secs_f64() < 10.0,
            "30 天 2 标的耗时 {:.1} 秒，超过 10 秒的验收线",
            elapsed.as_secs_f64()
        );
    }

    /// 续传：同一计划再跑一次，已完成的序列应当整条跳过。
    #[tokio::test]
    #[ignore = "访问真实 OKX API，需显式运行"]
    async fn second_run_skips_completed_series() {
        let client = OkxClient::new().expect("客户端构造失败");
        let db = Db::open_in_memory().await.expect("内存库创建失败");

        let now = crate::storage::now_ms();
        // 用很短的窗口，让这个测试跑得快
        let plan = plan::expand(now - 6 * 3_600_000, now, &watchlist(), "1H", now);

        let first = run(
            &client,
            &db,
            &plan,
            Arc::new(AtomicBool::new(false)),
            &|_| {},
        )
        .await
        .expect("首次执行失败");
        let first_requests = first.done_requests;
        assert!(first_requests > 0);

        let second = run(
            &client,
            &db,
            &plan,
            Arc::new(AtomicBool::new(false)),
            &|_| {},
        )
        .await
        .expect("二次执行失败");

        println!(
            "首次 {} 请求，二次 {} 请求",
            first_requests, second.done_requests
        );
        assert_eq!(
            second.done_requests, 0,
            "二次执行不应发起任何请求——已完成的序列应当整条跳过"
        );
        assert!(
            second
                .series
                .iter()
                .all(|s| s.status == SeriesStatus::Skipped),
            "所有序列都应是 Skipped：{:#?}",
            second.series
        );
    }
}
