//! 采集计划：把「时段 + 标的 + 粒度」展开成请求清单（设计文档 §6.2）。
//!
//! 计划阶段**不联网**——这一点是刻意的：用户打开复盘页时不应该偷偷发起请求，
//! 而是先看到「这次要拉多少、大概多久、哪些指标拿不到」，确认后再执行。

use serde::Serialize;
use ts_rs::TS;

use crate::error::{AppError, AppResult};
use crate::okx::endpoints::{RateGroup, public};

/// 单次 K 线分页请求的最大条数（实测 `limit=100` 可用）。
pub const CANDLE_PAGE: usize = 100;

/// 单次资金费率分页请求的最大条数。
pub const FUNDING_PAGE: usize = 100;

/// 单个序列的页数安全阀。
///
/// 存在的意义是防止「边界判断写错 → 无限翻页」把限流配额烧光。
/// 正常计划远达不到这个值（90 天 1 小时线也只有 22 页）。
pub const MAX_PAGES: usize = 400;

/// Rubik 统计的可回溯窗口。
///
/// **实测更正**：只有最近约 48 小时有数据，不是初版写的「2023-06-09 起」
/// （那是数据的历史起点，不是 API 能取回的范围）。见设计文档 §6.1.3。
/// 这里取 44 小时留余量——边界附近可能返回空，那时会把该序列标为不可得。
pub const RUBIK_WINDOW_MS: i64 = 44 * 60 * 60 * 1000;

/// 资金费率可回溯窗口。官方称 3 个月，实测 85 天前仍有数据，取 88 天留余量。
pub const FUNDING_WINDOW_MS: i64 = 88 * 24 * 60 * 60 * 1000;

/// 资金费率结算间隔（8 小时）。
const FUNDING_INTERVAL_MS: i64 = 8 * 60 * 60 * 1000;

/// 一个序列的种类。决定用哪个端点、如何分页。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub enum SeriesKind {
    /// 价格 K 线（复盘的历史主干，完整可得）
    Candles,
    /// 标记价 K 线（与指数价组合可重建基差）
    MarkCandles,
    /// 指数价 K 线
    IndexCandles,
    /// 资金费率历史（约 90 天）
    FundingRate,
    /// 多空账户比（仅近约 48 小时）
    LongShortRatio,
    /// 主动买卖量（仅近约 48 小时）
    TakerVolume,
}

impl SeriesKind {
    pub fn label(self) -> &'static str {
        match self {
            SeriesKind::Candles => "K 线",
            SeriesKind::MarkCandles => "标记价 K 线",
            SeriesKind::IndexCandles => "指数价 K 线",
            SeriesKind::FundingRate => "资金费率",
            SeriesKind::LongShortRatio => "多空账户比",
            SeriesKind::TakerVolume => "主动买卖量",
        }
    }

    /// 该序列属于哪个限流分组——用于估算总耗时。
    pub fn rate_group(self) -> RateGroup {
        match self {
            SeriesKind::Candles | SeriesKind::MarkCandles | SeriesKind::IndexCandles => {
                RateGroup::Market
            }
            SeriesKind::FundingRate => RateGroup::Reference,
            SeriesKind::LongShortRatio | SeriesKind::TakerVolume => RateGroup::Rubik,
        }
    }

    /// 落库时 `candles.kind` 列的取值。
    pub fn candle_kind(self) -> &'static str {
        match self {
            SeriesKind::MarkCandles => "mark",
            SeriesKind::IndexCandles => "index",
            _ => "last",
        }
    }

    /// 对应的端点路径。
    pub fn candle_path(self) -> &'static str {
        match self {
            SeriesKind::MarkCandles => public::HISTORY_MARK_PRICE_CANDLES,
            SeriesKind::IndexCandles => public::HISTORY_INDEX_CANDLES,
            _ => public::HISTORY_CANDLES,
        }
    }

    /// key 里的前缀，用于从 key 反解序列参数。
    pub fn key_prefix(self) -> &'static str {
        match self {
            SeriesKind::Candles => "candles",
            SeriesKind::MarkCandles => "mark",
            SeriesKind::IndexCandles => "index",
            SeriesKind::FundingRate => "funding",
            SeriesKind::LongShortRatio => "lsr",
            SeriesKind::TakerVolume => "taker",
        }
    }
}

/// 计划中的一条序列。
///
/// 序列内**顺序**分页（游标依赖），序列间**并发**。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct SeriesPlan {
    /// 稳定标识，用作续传键。同参数的计划会得到同一个 key。
    pub key: String,
    /// 界面展示用
    pub label: String,
    pub kind: SeriesKind,
    pub inst_id: Option<String>,
    pub ccy: Option<String>,
    /// 1 = 关键（先拉，让用户尽快看到主体），3 = 锦上添花
    pub priority: u8,
    /// 预估页数
    pub est_pages: usize,
}

/// 可得性提示。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct AvailabilityNote {
    pub metric: String,
    /// 为什么拿不到，直接展示给用户
    pub reason: String,
}

/// 采集计划。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct FetchPlan {
    pub id: String,
    #[ts(type = "number")]
    pub from: i64,
    #[ts(type = "number")]
    pub to: i64,
    pub inst_ids: Vec<String>,
    pub bar: String,
    pub series: Vec<SeriesPlan>,
    pub est_requests: usize,
    /// 预估耗时（毫秒）。
    ///
    /// 显式声明为 number：ts-rs 把 `u64` 也映射成 `bigint`（不只是 i64），
    /// 而 IPC 走 JSON，前端拿到的是 number。
    #[ts(type = "number")]
    pub est_duration_ms: u64,
    /// 拿不到的指标及原因。空数组表示全部可得。
    pub warnings: Vec<AvailabilityNote>,
}

/// 时段上限（已确认的硬上限）。
pub const MAX_RANGE_MS: i64 = 90 * 24 * 60 * 60 * 1000;

/// 超过这个请求数就提示用户换更大的 bar。
///
/// 不静默截断时段——静默截断会让用户以为看全了，是最危险的行为。
pub const BUSY_PLAN_REQUESTS: usize = 800;

/// `bar` 对应的毫秒数。未知取值返回 `None`。
pub fn bar_millis(bar: &str) -> Option<i64> {
    let (value, unit) = bar.split_at(bar.len().checked_sub(1)?);
    let value: i64 = value.parse().ok()?;
    let unit_ms = match unit {
        "m" => 60_000,
        "H" => 3_600_000,
        "D" => 86_400_000,
        "W" => 604_800_000,
        _ => return None,
    };
    Some(value * unit_ms)
}

/// 行情页支持的 K 线粒度白名单：(取值, 中文标签)。
///
/// **为什么要有白名单**：`bar_millis` 是「数字 + m/H/D/W」的泛匹配，它会放行
/// `"5H"`、`"100D"` 这类 OKX 根本不接受的组合——而错误要等到联网取数时才暴露，
/// 报的还是含糊的 `51000 Parameter bar error`。在这里先拒掉，报错才能指向真正的问题。
///
/// **为什么不支持月线（`1M` / `3M`）**：`bar_millis` 手里的单位只有 m/H/D/W，
/// 而月份长度不固定（28~31 天）——换算成固定毫秒就是撒谎。与其给一个近似值，
/// 不如明确不支持（界面里也不出现）。
pub const CANDLE_BARS: &[(&str, &str)] = &[
    ("1m", "1 分钟"),
    ("3m", "3 分钟"),
    ("5m", "5 分钟"),
    ("15m", "15 分钟"),
    ("30m", "30 分钟"),
    ("1H", "1 小时"),
    ("2H", "2 小时"),
    ("4H", "4 小时"),
    ("6H", "6 小时"),
    ("12H", "12 小时"),
    ("1D", "日线"),
    ("2D", "2 日线"),
    ("3D", "3 日线"),
    ("1W", "周线"),
];

/// 该粒度是否受支持（白名单，不是 `bar_millis` 的泛匹配）。
pub fn is_supported_bar(bar: &str) -> bool {
    CANDLE_BARS.iter().any(|(value, _)| *value == bar)
}

/// 粒度的中文标签。未知粒度原样返回——不编造一个看起来很像的标签。
pub fn bar_label(bar: &str) -> String {
    CANDLE_BARS
        .iter()
        .find(|(value, _)| *value == bar)
        .map_or_else(|| bar.to_string(), |(_, label)| (*label).to_string())
}

/// 展开采集计划。**不联网。**
pub fn expand(from: i64, to: i64, inst_ids: &[String], bar: &str, now: i64) -> FetchPlan {
    let mut warnings = Vec::new();
    let mut series = Vec::new();
    let span = (to - from).max(0);

    let bar_ms = bar_millis(bar).unwrap_or(3_600_000).max(1);

    // K 线：每个标的三种（价格 / 标记价 / 指数价）。
    // 三种都要，因为**基差 = (标记价 − 指数价) / 指数价**，是复盘判断
    // 「当时多头是否溢价」的核心——只有价格线的话这个维度就丢了。
    let candle_pages = pages_for(span, bar_ms * CANDLE_PAGE as i64);
    for inst_id in inst_ids {
        for kind in [
            SeriesKind::Candles,
            SeriesKind::MarkCandles,
            SeriesKind::IndexCandles,
        ] {
            // 指数价用的是指数 ID（BTC-USDT），不是合约 ID（BTC-USDT-SWAP）
            let target = if kind == SeriesKind::IndexCandles {
                crate::okx::models::index_inst_id(inst_id).unwrap_or_else(|| inst_id.clone())
            } else {
                inst_id.clone()
            };

            series.push(SeriesPlan {
                key: format!("{}|{target}|{bar}|{from}|{to}", kind.key_prefix()),
                label: format!("{inst_id} {}", kind.label()),
                kind,
                inst_id: Some(target),
                ccy: None,
                priority: if kind == SeriesKind::Candles { 1 } else { 2 },
                est_pages: candle_pages,
            });
        }
    }

    // 资金费率：窗口有限，超出部分直接标为不可得，而不是发一个注定返回空的请求。
    if from >= now - FUNDING_WINDOW_MS {
        let funding_pages = pages_for(span, FUNDING_INTERVAL_MS * FUNDING_PAGE as i64);
        for inst_id in inst_ids {
            series.push(SeriesPlan {
                key: format!("funding|{inst_id}|{from}|{to}"),
                label: format!("{inst_id} 资金费率"),
                kind: SeriesKind::FundingRate,
                inst_id: Some(inst_id.clone()),
                ccy: None,
                priority: 3,
                est_pages: funding_pages,
            });
        }
    } else {
        warnings.push(AvailabilityNote {
            metric: "资金费率".to_string(),
            reason: format!(
                "该时段超出 OKX 保留窗口（约 {} 天）",
                FUNDING_WINDOW_MS / 86_400_000
            ),
        });
    }

    // Rubik 统计：只有近约 48 小时。对绝大多数复盘（7 天以上）都不可得。
    let rubik_available = from >= now - RUBIK_WINDOW_MS;
    let distinct_ccys = distinct_base_ccys(inst_ids);
    if rubik_available {
        for ccy in &distinct_ccys {
            series.push(SeriesPlan {
                key: format!("lsr|{ccy}|{from}|{to}"),
                label: format!("{ccy} 多空账户比"),
                kind: SeriesKind::LongShortRatio,
                inst_id: None,
                ccy: Some(ccy.clone()),
                priority: 3,
                est_pages: 1,
            });
            series.push(SeriesPlan {
                key: format!("taker|{ccy}|{from}|{to}"),
                label: format!("{ccy} 主动买卖量"),
                kind: SeriesKind::TakerVolume,
                inst_id: None,
                ccy: Some(ccy.clone()),
                priority: 3,
                est_pages: 1,
            });
        }
    } else {
        for metric in ["多空账户比", "主动买卖量", "持仓量"] {
            warnings.push(AvailabilityNote {
                metric: metric.to_string(),
                reason: "OKX 的 Rubik 统计只提供最近约 48 小时的数据".to_string(),
            });
        }
    }

    // 按优先级排序，让关键数据先出
    series.sort_by_key(|plan| plan.priority);

    let est_requests: usize = series.iter().map(|plan| plan.est_pages).sum();
    let est_duration_ms = estimate_duration_ms(&series);

    // 请求量过大时**建议**换粒度，而不是静默截断时段。
    // 静默截断会让用户以为看全了——那是最危险的行为。
    if est_requests > BUSY_PLAN_REQUESTS {
        warnings.push(AvailabilityNote {
            metric: "请求量".to_string(),
            reason: format!(
                "本次需要约 {est_requests} 个请求，等待时间较长；建议改用更大的 K 线粒度（如 1H → 4H）"
            ),
        });
    }

    FetchPlan {
        id: plan_id(from, to, inst_ids, bar),
        from,
        to,
        inst_ids: inst_ids.to_vec(),
        bar: bar.to_string(),
        series,
        est_requests,
        est_duration_ms,
        warnings,
    }
}

/// 行情页单个周期的最大根数。
///
/// 直接乘进 token：一根 K 线约 6 个数字，200 根 ≈ 1.2k token，
/// 500 根已经把一份提示词推到「贵且 AI 容易只读开头」的量级。
pub const MAX_CANDLES_PER_BAR: usize = 500;

/// 单次行情计划的根数总上限（所有周期之和），≈ 15 页。
///
/// 设它的目的是让「点一次要等多久」保持可预期——与复盘把请求数收敛在
/// 一个数量级是同一个考虑（见 `BUSY_PLAN_REQUESTS`）。
pub const MAX_TOTAL_CANDLES: usize = 1_500;

/// 单次行情计划最多几个周期。
pub const MAX_BARS_PER_PLAN: usize = 6;

/// 每根 K 线（含逐根指标列）大约占多少 token。
///
/// 实测来源：黄金测试里「300 行 × 2 个指标列」渲染出约 10.2k token，
/// 即每行约 34 token；这里向上取 40 留余量（列更多时每行更宽）。
pub const APPROX_TOKENS_PER_ROW: usize = 40;

/// 超过这个预估 token 数就在计划阶段提醒用户。
///
/// 与 `BUSY_PLAN_REQUESTS` 同一个思路：把「要花多少」在联网之前讲清楚。
pub const BUSY_PROMPT_TOKENS: usize = 20_000;

/// 行情页的一个周期：最近多少根、覆盖哪个区间。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct KlineBarPlan {
    pub bar: String,
    /// 中文标签（如 `1 小时`），界面与提示词都用它
    pub label: String,
    /// 请求的根数
    pub candle_count: usize,
    #[ts(type = "number")]
    pub from: i64,
    #[ts(type = "number")]
    pub to: i64,
    pub est_pages: usize,
    /// 指向 `series` 里对应的那条 K 线序列（执行与续传都用它）
    pub series_key: String,
}

/// 行情页的取数计划：**单标的 × 多周期**，每个周期各有自己的时间区间。
///
/// 与 [`FetchPlan`] 分开而不是复用，是刻意的：`FetchPlan` 只有一个
/// `from` / `to` / `bar`，而这里每个周期都要「最近 N 根」——不同周期的时间区间
/// 本来就不同（200 根 1H 是 8 天，200 根 1D 是 200 天）。硬塞进 `FetchPlan`
/// 就得让那几个字段说谎，而界面会把它们显示给用户。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct KlinePlan {
    pub id: String,
    pub inst_id: String,
    pub bars: Vec<KlineBarPlan>,
    /// 执行器消费的序列清单（**只有价格 K 线**：行情页不做基差重建）
    pub series: Vec<SeriesPlan>,
    pub est_requests: usize,
    #[ts(type = "number")]
    pub est_duration_ms: u64,
    /// 提示词长度的量级预估（**不含指标列**：指标是 build 阶段的参数，
    /// 计划阶段还不知道）。界面据此在联网前给出「这一份大概多长」。
    ///
    /// 用「根数 × 每行 token」估算，而不是等生成完再算——那时请求已经花掉了。
    pub est_tokens: usize,
    pub warnings: Vec<AvailabilityNote>,
}

impl KlinePlan {
    /// 全部周期的预计根数之和。
    pub fn total_candles(&self) -> usize {
        self.bars.iter().map(|item| item.candle_count).sum()
    }
}

/// 展开行情取数计划。**不联网。**
///
/// 与 [`expand`] 的区别不只是「只要 K 线」，还有**区间方向**：复盘是用户给定
/// 起止时间，行情页给定的是「最近 N 根」——所以这里由 `bar_ms × count`
/// 反推区间，而不是接受一个 `from`。
pub fn expand_kline(
    inst_id: &str,
    bars: &[String],
    candle_count: usize,
    now: i64,
) -> AppResult<KlinePlan> {
    if inst_id.trim().is_empty() {
        return Err(AppError::Config("必须先选择标的".to_string()));
    }

    // 去重但保留用户的选择顺序：界面是多选框，重复项没有意义，
    // 但打乱顺序会让「第 1 个周期」这类文案对不上。
    let mut unique: Vec<&String> = Vec::new();
    for bar in bars {
        if !unique.contains(&bar) {
            unique.push(bar);
        }
    }
    if unique.is_empty() {
        return Err(AppError::Config("至少需要选择 1 个周期".to_string()));
    }
    if unique.len() > MAX_BARS_PER_PLAN {
        return Err(AppError::Config(format!(
            "最多选择 {MAX_BARS_PER_PLAN} 个周期，当前 {} 个",
            unique.len()
        )));
    }

    if candle_count == 0 {
        return Err(AppError::Config("K 线根数至少为 1".to_string()));
    }
    if candle_count > MAX_CANDLES_PER_BAR {
        return Err(AppError::Config(format!(
            "每个周期最多 {MAX_CANDLES_PER_BAR} 根，当前 {candle_count} 根"
        )));
    }
    let bar_count = unique.len();
    let total = candle_count * bar_count;
    if total > MAX_TOTAL_CANDLES {
        return Err(AppError::Config(format!(
            "所有周期的根数之和最多 {MAX_TOTAL_CANDLES} 根（当前 {total} 根）：\
             减少周期数或降低每周期根数。根数会直接乘进提示词长度与等待时间"
        )));
    }

    let mut bar_plans = Vec::with_capacity(unique.len());
    let mut series = Vec::with_capacity(unique.len());

    for bar in unique {
        if !is_supported_bar(bar) {
            return Err(AppError::Config(format!(
                "不支持的 K 线粒度：{bar}（可用：{}）",
                CANDLE_BARS
                    .iter()
                    .map(|(value, _)| *value)
                    .collect::<Vec<_>>()
                    .join(" / ")
            )));
        }

        // 白名单保证了这里有值；用 ok_or_else 而不是 unwrap 是为了将来
        // 有人往白名单里加了月线时，这里会明确报错而不是 panic。
        let bar_ms = bar_millis(bar)
            .ok_or_else(|| AppError::Config(format!("无法换算粒度的时长：{bar}")))?;

        let to = now;
        let from = to - bar_ms * candle_count as i64;
        let span = (to - from).max(0);
        let est_pages = pages_for(span, bar_ms * CANDLE_PAGE as i64);
        let label = bar_label(bar);
        let series_key = format!(
            "{}|{inst_id}|{bar}|{from}|{to}",
            SeriesKind::Candles.key_prefix()
        );

        series.push(SeriesPlan {
            key: series_key.clone(),
            label: format!("{inst_id} {label} K 线"),
            kind: SeriesKind::Candles,
            inst_id: Some(inst_id.to_string()),
            ccy: None,
            priority: 1,
            est_pages,
        });
        bar_plans.push(KlineBarPlan {
            bar: bar.clone(),
            label,
            candle_count,
            from,
            to,
            est_pages,
            series_key,
        });
    }

    let est_requests: usize = series.iter().map(|plan| plan.est_pages).sum();
    let est_duration_ms = estimate_duration_ms(&series);
    let est_tokens = total * APPROX_TOKENS_PER_ROW;

    let plan = KlinePlan {
        // 计划 id 只用「与用户选择有关」的字段：`from` 由 `now` 反推，
        // 把它算进哈希会让同一份选择每秒产生一个新 id，注册表就会不断堆积。
        id: kline_plan_id(inst_id, &bar_plans, candle_count),
        inst_id: inst_id.to_string(),
        bars: bar_plans,
        series,
        est_requests,
        est_duration_ms,
        est_tokens,
        warnings: Vec::new(),
    };

    // 提示词长度预警。
    //
    // `APPROX_TOKENS_PER_ROW` 不是拍脑袋：黄金测试实测「300 行 × 2 个指标列」
    // 渲染出约 10.2k token，即每行约 34 token（向上取 40 留余量）。
    // 逐根附列的表达力就是拿 token 换来的，用户有权在**等待联网取数之前**
    // 就知道自己要花多少——等到提示词生成完再发现太长，那批请求已经花掉了。
    let mut warnings = Vec::new();
    if plan.est_tokens > BUSY_PROMPT_TOKENS {
        warnings.push(AvailabilityNote {
            metric: "提示词长度".to_string(),
            reason: format!(
                "本次共 {} 根 K 线，提示词预计约 {} token（不含指标列）；\
                 建议减少周期数或降低每周期根数（每根约 {APPROX_TOKENS_PER_ROW} token）",
                plan.total_candles(),
                plan.est_tokens
            ),
        });
    }

    Ok(KlinePlan { warnings, ..plan })
}

fn kline_plan_id(inst_id: &str, bars: &[KlineBarPlan], candle_count: usize) -> String {
    let mut parts = vec![inst_id.to_string(), candle_count.to_string()];
    parts.extend(bars.iter().map(|item| item.bar.clone()));
    format!("kline-{:x}", fnv1a(parts.join("|").as_bytes()))
}

/// 估算总耗时：按限流分组分别累加「请求数 ÷ 配额 × 窗口」。
///
/// 刻意不用「平均延迟 × 请求数」——真正的瓶颈是限流而不是网络，
/// 用延迟估算会给出一个乐观到误导的数字。
fn estimate_duration_ms(series: &[SeriesPlan]) -> u64 {
    let mut per_group: std::collections::HashMap<RateGroup, usize> =
        std::collections::HashMap::new();
    for plan in series {
        *per_group.entry(plan.kind.rate_group()).or_default() += plan.est_pages;
    }

    let mut total: u64 = 0;
    for (group, requests) in per_group {
        let (quota, window_ms) = group.quota();
        let windows = (requests as f64 / f64::from(quota)).ceil() as u64;
        total += windows * window_ms;
    }
    total
}

fn pages_for(span_ms: i64, per_page_ms: i64) -> usize {
    if span_ms <= 0 || per_page_ms <= 0 {
        return 1;
    }
    let pages = (span_ms + per_page_ms - 1) / per_page_ms;
    (pages.max(1) as usize).min(MAX_PAGES)
}

fn plan_id(from: i64, to: i64, inst_ids: &[String], bar: &str) -> String {
    let mut parts = vec![from.to_string(), to.to_string(), bar.to_string()];
    parts.extend(inst_ids.iter().cloned());
    format!("plan-{:x}", fnv1a(parts.join("|").as_bytes()))
}

/// FNV-1a：只需要一个稳定且短的标识，不涉及安全用途。
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn distinct_base_ccys(inst_ids: &[String]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for inst_id in inst_ids {
        let ccy = inst_id.split('-').next().unwrap_or(inst_id).to_string();
        if !seen.contains(&ccy) {
            seen.push(ccy);
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3_600_000;
    const DAY: i64 = 86_400_000;

    fn watchlist() -> Vec<String> {
        vec!["BTC-USDT-SWAP".to_string(), "ETH-USDT-SWAP".to_string()]
    }

    /// 三种 K 线共用同一套游标分页，测试里多处需要一起判断。
    fn is_candle_kind(kind: SeriesKind) -> bool {
        matches!(
            kind,
            SeriesKind::Candles | SeriesKind::MarkCandles | SeriesKind::IndexCandles
        )
    }

    #[test]
    fn parses_bar_durations() {
        assert_eq!(bar_millis("1m"), Some(60_000));
        assert_eq!(bar_millis("15m"), Some(900_000));
        assert_eq!(bar_millis("1H"), Some(HOUR));
        assert_eq!(bar_millis("4H"), Some(4 * HOUR));
        assert_eq!(bar_millis("1D"), Some(DAY));
        assert_eq!(bar_millis("bogus"), None);
        assert_eq!(bar_millis(""), None);
    }

    #[test]
    fn thirty_day_two_instrument_plan_matches_documented_estimate() {
        let now = 1_800_000_000_000;
        let to = now;
        let from = now - 30 * DAY;

        let plan = expand(from, to, &watchlist(), "1H", now);

        // 30 天 1 小时线 = 720 根 → 每序列 8 页（100/页）
        // 三种 K 线（价格 / 标记价 / 指数价）× 2 标的 = 6 条序列
        let candle_series: Vec<_> = plan
            .series
            .iter()
            .filter(|s| is_candle_kind(s.kind))
            .collect();
        assert_eq!(
            candle_series.len(),
            6,
            "每个标的都要有价格/标记价/指数价三种"
        );
        for series in &candle_series {
            assert_eq!(series.est_pages, 8, "720 根 / 100 = 8 页");
        }
        assert_eq!(48, candle_series.iter().map(|s| s.est_pages).sum::<usize>());

        // 30 天资金费率 = 90 条 → 每标的 1 页
        let funding: Vec<_> = plan
            .series
            .iter()
            .filter(|s| s.kind == SeriesKind::FundingRate)
            .collect();
        assert_eq!(funding.len(), 2);
        assert_eq!(funding[0].est_pages, 1);

        // Rubik 只有 48 小时，30 天前必然不可得
        assert!(
            plan.series
                .iter()
                .all(|s| !matches!(s.kind, SeriesKind::LongShortRatio | SeriesKind::TakerVolume)),
            "30 天窗口下不应计划 Rubik 序列"
        );
        assert!(
            plan.warnings.iter().any(|w| w.metric.contains("多空")),
            "必须明确告知多空比不可得：{:#?}",
            plan.warnings
        );

        // 48（K 线）+ 2（资金费率）= 50 个请求
        assert_eq!(plan.est_requests, 50);
    }

    /// 指数价序列必须用**指数 ID**（`BTC-USDT`）而不是合约 ID——
    /// 用错的话请求会直接失败，或者更糟：静默拿到另一个标的的数据。
    #[test]
    fn index_series_targets_the_index_instrument() {
        let now = 1_800_000_000_000;
        let plan = expand(now - DAY, now, &watchlist(), "1H", now);

        let index_series: Vec<_> = plan
            .series
            .iter()
            .filter(|s| s.kind == SeriesKind::IndexCandles)
            .collect();
        assert_eq!(index_series.len(), 2);
        assert!(
            index_series
                .iter()
                .all(|s| s.inst_id.as_deref() == Some("BTC-USDT")
                    || s.inst_id.as_deref() == Some("ETH-USDT")),
            "指数序列的 instId 必须是指数 ID：{:#?}",
            index_series.iter().map(|s| &s.inst_id).collect::<Vec<_>>()
        );

        // 标记价用的是合约 ID
        let mark_series: Vec<_> = plan
            .series
            .iter()
            .filter(|s| s.kind == SeriesKind::MarkCandles)
            .collect();
        assert!(
            mark_series
                .iter()
                .all(|s| s.inst_id.as_deref().is_some_and(|id| id.ends_with("-SWAP"))),
            "标记价序列的 instId 必须是合约 ID"
        );
    }

    #[test]
    fn short_window_includes_rubik_series() {
        let now = 1_800_000_000_000;
        let plan = expand(now - 24 * HOUR, now, &watchlist(), "1H", now);

        // 24 小时内 Rubik 可得：2 个基础币 × 2 个指标
        let rubik: Vec<_> = plan
            .series
            .iter()
            .filter(|s| matches!(s.kind, SeriesKind::LongShortRatio | SeriesKind::TakerVolume))
            .collect();
        assert_eq!(rubik.len(), 4, "BTC 与 ETH 各 2 个 Rubik 序列");
        assert!(
            plan.warnings.is_empty(),
            "24 小时窗口内所有指标都应可得：{:#?}",
            plan.warnings
        );
    }

    #[test]
    fn base_ccys_are_deduplicated() {
        // 同一个基础币的多个合约不应重复请求 Rubik
        let insts = vec![
            "BTC-USDT-SWAP".to_string(),
            "BTC-USD-SWAP".to_string(),
            "ETH-USDT-SWAP".to_string(),
        ];
        assert_eq!(distinct_base_ccys(&insts), vec!["BTC", "ETH"]);
    }

    #[test]
    fn priorities_order_candles_first() {
        let now = 1_800_000_000_000;
        let plan = expand(now - 12 * HOUR, now, &watchlist(), "1H", now);

        let first = plan.series.first().expect("应有序列");
        assert_eq!(first.kind, SeriesKind::Candles, "K 线必须排在最前");
        assert_eq!(first.priority, 1);

        // 序列必须按优先级非递减排列
        let priorities: Vec<u8> = plan.series.iter().map(|s| s.priority).collect();
        assert!(
            priorities.windows(2).all(|w| w[0] <= w[1]),
            "序列未按优先级排序：{priorities:?}"
        );
    }

    #[test]
    fn plan_id_is_stable_for_same_inputs() {
        let now = 1_800_000_000_000;
        let a = expand(now - DAY, now, &watchlist(), "1H", now);
        let b = expand(now - DAY, now, &watchlist(), "1H", now);
        assert_eq!(a.id, b.id, "同参数的计划必须得到同一个 id（续传依赖它）");

        let c = expand(now - DAY, now, &watchlist(), "4H", now);
        assert_ne!(a.id, c.id, "换粒度应当是不同的计划");
    }

    #[test]
    fn page_count_is_capped() {
        // 极长的时段不应产生无限页数
        let now = 1_800_000_000_000;
        let plan = expand(0, now, &["BTC-USDT-SWAP".to_string()], "1m", now);
        for series in &plan.series {
            assert!(series.est_pages <= MAX_PAGES);
        }
    }

    #[test]
    fn duration_estimate_is_driven_by_rate_limits_not_latency() {
        let now = 1_800_000_000_000;
        let plan = expand(now - 30 * DAY, now, &watchlist(), "1H", now);

        // 18 个请求，最大分组是 Market 的 16 个 / (20 per 2s) = 1 个窗口 = 2 秒
        assert!(
            plan.est_duration_ms >= 2_000,
            "估算不应低于一个限流窗口：{}",
            plan.est_duration_ms
        );
        assert!(
            plan.est_duration_ms <= 10_000,
            "18 个请求不该估算到 10 秒以上：{}",
            plan.est_duration_ms
        );
    }

    // ---------------------------------------------------------------- 行情计划

    #[test]
    fn kline_plan_covers_the_last_n_candles_of_each_bar() {
        let now = 1_800_000_000_000;
        let bars = vec!["1H".to_string(), "1D".to_string()];
        let plan = expand_kline("BTC-USDT-SWAP", &bars, 200, now).expect("应能展开");

        assert_eq!(plan.inst_id, "BTC-USDT-SWAP");
        assert_eq!(plan.bars.len(), 2);

        let hourly = &plan.bars[0];
        assert_eq!(hourly.bar, "1H");
        assert_eq!(hourly.label, "1 小时");
        assert_eq!(hourly.to, now);
        assert_eq!(hourly.from, now - 200 * HOUR, "1H 的 200 根 = 200 小时");
        assert_eq!(hourly.est_pages, 2, "200 根 / 100 = 2 页");

        let daily = &plan.bars[1];
        assert_eq!(
            daily.from,
            now - 200 * DAY,
            "同一个根数在不同周期上区间不同"
        );
        assert_eq!(daily.est_pages, 2);
        assert_ne!(hourly.from, daily.from, "各周期必须有各自的区间");

        // 只要价格线：行情页不做基差重建，标记价/指数价不该出现在计划里
        assert_eq!(plan.series.len(), 2);
        assert!(plan.series.iter().all(|s| s.kind == SeriesKind::Candles));
        assert_eq!(plan.est_requests, 4);
        assert_eq!(plan.total_candles(), 400);

        // 序列 key 必须编码区间：执行器与续传都靠它
        assert_eq!(
            plan.series[0].key,
            format!("candles|BTC-USDT-SWAP|1H|{}|{}", hourly.from, hourly.to)
        );
        assert_eq!(plan.bars[0].series_key, plan.series[0].key);
    }

    /// 计划 id 不能把 `from` 算进去：它由 `now` 反推，否则同一份选择每秒
    /// 都会产生一个新 id，注册表会不断堆积。
    #[test]
    fn kline_plan_id_is_stable_across_time() {
        let bars = vec!["1H".to_string()];
        let first = expand_kline("BTC-USDT-SWAP", &bars, 200, 1_800_000_000_000).expect("应能展开");
        let later = expand_kline("BTC-USDT-SWAP", &bars, 200, 1_800_000_600_000).expect("应能展开");

        assert_eq!(first.id, later.id, "同一份选择应得到同一个计划 id");
        assert_ne!(first.bars[0].from, later.bars[0].from, "但区间随 now 前移");
    }

    /// 月线必须被拒：`bar_millis` 只有 m/H/D/W，月份长度不固定，
    /// 换算成固定毫秒就是撒谎。这里守住「不支持就说清楚」。
    #[test]
    fn kline_plan_rejects_month_bars() {
        for bar in ["1M", "3M"] {
            let err = expand_kline("BTC-USDT-SWAP", &[bar.to_string()], 200, 0)
                .expect_err("月线必须被拒绝");
            assert!(
                err.to_string().contains("不支持的 K 线粒度"),
                "错误信息要指出是不支持的粒度：{err}"
            );
        }
    }

    /// `bar_millis` 是泛匹配，会放行 `5H` / `100D` 这类 OKX 不接受的组合。
    /// 白名单的意义就是在这里拦住它们，而不是等到联网时报 `51000`。
    #[test]
    fn kline_plan_rejects_bars_okx_does_not_accept() {
        for bar in ["5H", "100D", "bogus", "", "1"] {
            assert!(
                expand_kline("BTC-USDT-SWAP", &[bar.to_string()], 200, 0).is_err(),
                "{bar} 不该被接受"
            );
        }
    }

    #[test]
    fn kline_plan_enforces_candle_limits() {
        let bars = vec!["1H".to_string()];

        assert!(
            expand_kline("BTC-USDT-SWAP", &bars, 0, 0).is_err(),
            "根数为 0 应被拒绝"
        );
        assert!(
            expand_kline("BTC-USDT-SWAP", &bars, MAX_CANDLES_PER_BAR + 1, 0).is_err(),
            "超过单周期上限应被拒绝"
        );
        assert!(
            expand_kline("BTC-USDT-SWAP", &bars, MAX_CANDLES_PER_BAR, 0).is_ok(),
            "刚好到单周期上限应通过"
        );

        // 2 个周期 × 500 根 = 1000 ≤ 1500 → 通过
        let two = vec!["1H".to_string(), "4H".to_string()];
        assert!(expand_kline("BTC-USDT-SWAP", &two, MAX_CANDLES_PER_BAR, 0).is_ok());

        // 4 个周期 × 500 根 = 2000 > 1500 → 拒绝，且要说清怎么减
        let four = vec![
            "1H".to_string(),
            "4H".to_string(),
            "1D".to_string(),
            "1W".to_string(),
        ];
        let err = expand_kline("BTC-USDT-SWAP", &four, MAX_CANDLES_PER_BAR, 0)
            .expect_err("总根数超限应被拒绝");
        let message = err.to_string();
        assert!(message.contains("减少周期数"), "要说清怎么减：{message}");
    }

    #[test]
    fn kline_plan_rejects_empty_or_too_many_bars() {
        assert!(
            expand_kline("BTC-USDT-SWAP", &[], 200, 0).is_err(),
            "一个周期都不选应被拒绝"
        );

        let too_many: Vec<String> = CANDLE_BARS
            .iter()
            .take(MAX_BARS_PER_PLAN + 1)
            .map(|(bar, _)| (*bar).to_string())
            .collect();
        assert!(expand_kline("BTC-USDT-SWAP", &too_many, 10, 0).is_err());

        assert!(
            expand_kline("", &["1H".to_string()], 200, 0).is_err(),
            "没选标的应被拒绝"
        );
    }

    #[test]
    fn kline_plan_dedupes_bars_but_keeps_order() {
        let bars = vec!["4H".to_string(), "1H".to_string(), "4H".to_string()];
        let plan = expand_kline("BTC-USDT-SWAP", &bars, 100, 0).expect("应能展开");

        assert_eq!(plan.bars.len(), 2, "重复的周期只算一次");
        assert_eq!(plan.bars[0].bar, "4H", "保留用户选择的顺序");
        assert_eq!(plan.bars[1].bar, "1H");
    }

    /// 提示词长度要在**联网之前**就告知：等到生成完才发现太长，那批请求已经花掉了。
    #[test]
    fn kline_plan_warns_when_the_prompt_would_be_large() {
        let small = expand_kline("BTC-USDT-SWAP", &["1H".to_string()], 200, 0).expect("应能展开");
        assert!(
            small.warnings.is_empty(),
            "200 根不该预警：{:?}",
            small.warnings
        );

        let big = expand_kline(
            "BTC-USDT-SWAP",
            &["1H".to_string(), "4H".to_string(), "1D".to_string()],
            500,
            0,
        )
        .expect("应能展开");

        let warning = big
            .warnings
            .iter()
            .find(|item| item.metric.contains("提示词长度"))
            .expect("1500 根必须提前预警");
        assert!(
            warning.reason.contains("减少周期数"),
            "要说清怎么减：{}",
            warning.reason
        );
        assert!(
            warning.reason.contains("token"),
            "要给出量级而不是含糊其辞：{}",
            warning.reason
        );
    }

    /// 每个受支持的粒度都必须能算出区间与页数——白名单里混进一个
    /// `bar_millis` 不认的取值，会让整个计划在运行时报错。
    #[test]
    fn every_whitelisted_bar_is_convertible() {
        for (bar, label) in CANDLE_BARS {
            assert!(bar_millis(bar).is_some(), "{bar} 无法换算时长");
            assert_eq!(&bar_label(bar), label, "{bar} 的标签不一致");
            assert!(
                expand_kline("BTC-USDT-SWAP", &[(*bar).to_string()], 100, 0).is_ok(),
                "{bar} 应能展开"
            );
        }
    }
}
