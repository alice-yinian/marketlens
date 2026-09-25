//! 游标分页（设计文档 §6.2.2）。
//!
//! 实测的分页语义（见 §6.1.2.1）：
//!   * `history-candles`：最新在前；`after=<ts>` 返回**更旧**的数据
//!   * `funding-rate-history`：同上，游标字段是 `fundingTime`
//!   * Rubik 系列：`begin`/`end` 区间，**只有近约 48 小时有数据**，且 `end` 分页不工作
//!
//! 因此这里对 K 线与资金费率做真正的游标翻页，对 Rubik 只做单次区间请求。

use std::collections::HashSet;

use crate::error::AppResult;
use crate::fetch::plan::{CANDLE_PAGE, FUNDING_PAGE, MAX_PAGES};
use crate::okx::client::OkxClient;
use crate::okx::endpoints::{RateGroup, inst_type, public};
use crate::okx::models::FundingRateHistory;

/// 一次分页拉取的结果。
#[derive(Debug, Clone)]
pub struct PageResult<T> {
    pub rows: Vec<T>,
    pub pages: usize,
    /// 是否因为触及 `from` 边界而正常结束。
    ///
    /// `false` 表示数据在到达边界前就耗尽了——这**不是错误**（更早的数据可能
    /// 确实不存在），但调用方应当知道「这段可能不完整」。
    pub reached_from: bool,
    /// 是否被取消
    pub cancelled: bool,
}

/// 从一页原始 K 线行里吸收落在 `[from, to]` 内、且尚未出现过的行。
///
/// 返回这一页里**最早**的时间戳（用于推进游标），整页都无法解析时返回 `None`。
///
/// 抽成纯函数是为了能直接测试边界逻辑——「时段外的数据被丢掉」「重复页被去重」
/// 「游标正确推进」这三件事写错了都不会报错，只会静默给出错误的时段数据。
pub fn absorb_candle_page(
    page: &[Vec<String>],
    from: i64,
    to: i64,
    seen: &mut HashSet<i64>,
    out: &mut Vec<Vec<String>>,
) -> Option<i64> {
    let mut oldest: Option<i64> = None;

    for row in page {
        let Some(ts) = row.first().and_then(|value| value.parse::<i64>().ok()) else {
            continue;
        };
        oldest = Some(oldest.map_or(ts, |current: i64| current.min(ts)));

        // 时段外的数据直接丢弃：OKX 的分页会跨过边界，不丢的话会拉进无关数据
        if ts < from || ts > to {
            continue;
        }
        if seen.insert(ts) {
            out.push(row.clone());
        }
    }

    oldest
}

/// 拉取 K 线，从 `to` 往旧翻页直到覆盖 `from`。
///
/// `path` 由调用方给出：价格线、标记价线、指数价线三者格式与分页语义完全一致，
/// 只是端点不同（已实测确认）。
///
/// `on_page` 每完成一页调用一次，参数是已完成的页数；返回 `false` 表示取消。
pub async fn fetch_candles(
    client: &OkxClient,
    path: &str,
    inst_id: &str,
    bar: &str,
    from: i64,
    to: i64,
    on_page: &mut (dyn FnMut(usize) -> bool + Send),
) -> AppResult<PageResult<Vec<String>>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut seen: HashSet<i64> = HashSet::new();
    let mut cursor = to;
    let mut pages = 0usize;
    let mut reached_from = false;

    loop {
        if pages >= MAX_PAGES {
            tracing::warn!(inst_id, bar, "K 线分页触及页数上限，提前停止");
            break;
        }

        let limit = CANDLE_PAGE.to_string();
        let cursor_text = cursor.to_string();
        let page = client
            .get_public::<Vec<String>>(
                path,
                RateGroup::Market,
                &[
                    ("instId", inst_id),
                    ("bar", bar),
                    ("limit", limit.as_str()),
                    ("after", cursor_text.as_str()),
                ],
            )
            .await?;

        if page.is_empty() {
            break;
        }

        let oldest = absorb_candle_page(&page, from, to, &mut seen, &mut rows);

        pages += 1;
        if !on_page(pages) {
            return Ok(PageResult {
                rows,
                pages,
                reached_from: false,
                cancelled: true,
            });
        }

        match oldest {
            // 触及边界：这段已完整
            Some(ts) if ts <= from => {
                reached_from = true;
                break;
            }
            // 游标没推进（整页都无法解析时间戳）→ 再翻也是同一页，必须停
            Some(ts) if ts >= cursor => {
                tracing::warn!(
                    inst_id,
                    bar,
                    cursor,
                    oldest = ts,
                    "分页游标未推进，停止以免死循环"
                );
                break;
            }
            Some(ts) => cursor = ts,
            None => break,
        }
    }

    // OKX 返回最新在前；统一按时间升序，指标计算都假设升序
    rows.sort_by(|a, b| a.first().cmp(&b.first()));

    Ok(PageResult {
        rows,
        pages,
        reached_from,
        cancelled: false,
    })
}

/// 拉取资金费率历史。
pub async fn fetch_funding_history(
    client: &OkxClient,
    inst_id: &str,
    from: i64,
    to: i64,
    on_page: &mut (dyn FnMut(usize) -> bool + Send),
) -> AppResult<PageResult<FundingRateHistory>> {
    let mut rows: Vec<FundingRateHistory> = Vec::new();
    let mut seen: HashSet<i64> = HashSet::new();
    let mut cursor = to;
    let mut pages = 0usize;
    let mut reached_from = false;

    loop {
        if pages >= MAX_PAGES {
            break;
        }

        let limit = FUNDING_PAGE.to_string();
        let cursor_text = cursor.to_string();
        let page = client
            .get_public::<FundingRateHistory>(
                public::FUNDING_RATE_HISTORY,
                RateGroup::Reference,
                &[
                    ("instId", inst_id),
                    ("limit", limit.as_str()),
                    ("after", cursor_text.as_str()),
                ],
            )
            .await?;

        if page.is_empty() {
            break;
        }

        let mut oldest: Option<i64> = None;
        for record in page {
            oldest = Some(oldest.map_or(record.funding_time, |current| {
                current.min(record.funding_time)
            }));

            if record.funding_time < from || record.funding_time > to {
                continue;
            }
            if seen.insert(record.funding_time) {
                rows.push(record);
            }
        }

        pages += 1;
        if !on_page(pages) {
            return Ok(PageResult {
                rows,
                pages,
                reached_from: false,
                cancelled: true,
            });
        }

        match oldest {
            Some(ts) if ts <= from => {
                reached_from = true;
                break;
            }
            Some(ts) if ts >= cursor => {
                tracing::warn!(inst_id, cursor, oldest = ts, "资金费率游标未推进，停止");
                break;
            }
            Some(ts) => cursor = ts,
            None => break,
        }
    }

    rows.sort_by_key(|record| record.funding_time);

    Ok(PageResult {
        rows,
        pages,
        reached_from,
        cancelled: false,
    })
}

/// 拉取 Rubik 统计（单次区间请求）。
///
/// 实测该系列只有近约 48 小时有数据、且 `end` 分页不工作，所以不做翻页——
/// 拉不到就是不可得，如实上报比反复重试更有用。
pub async fn fetch_rubik_series(
    client: &OkxClient,
    kind: crate::fetch::plan::SeriesKind,
    ccy: &str,
    from: i64,
    to: i64,
) -> AppResult<Vec<Vec<String>>> {
    let begin = from.to_string();
    let end = to.to_string();

    let (path, mut query) = match kind {
        crate::fetch::plan::SeriesKind::LongShortRatio => (
            public::LS_ACCOUNT_RATIO,
            vec![
                ("ccy", ccy),
                ("begin", begin.as_str()),
                ("end", end.as_str()),
            ],
        ),
        crate::fetch::plan::SeriesKind::TakerVolume => (
            public::TAKER_VOLUME,
            vec![
                ("ccy", ccy),
                ("instType", inst_type::CONTRACTS),
                ("begin", begin.as_str()),
                ("end", end.as_str()),
            ],
        ),
        other => {
            return Err(crate::error::AppError::Config(format!(
                "{} 不是 Rubik 序列",
                other.label()
            )));
        }
    };

    query.retain(|(_, value)| !value.is_empty());
    client
        .get_public::<Vec<String>>(path, RateGroup::Rubik, &query)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(ts: i64) -> Vec<String> {
        vec![
            ts.to_string(),
            "1".into(),
            "2".into(),
            "0.5".into(),
            "1.5".into(),
            "10".into(),
        ]
    }

    #[test]
    fn absorbs_only_rows_inside_the_range() {
        let from = 1_000;
        let to = 2_000;
        let page = vec![row(2_500), row(2_000), row(1_500), row(1_000), row(500)];

        let mut seen = HashSet::new();
        let mut out = Vec::new();
        let oldest = absorb_candle_page(&page, from, to, &mut seen, &mut out);

        assert_eq!(oldest, Some(500), "最早的时间戳取自整页，而不是过滤后的");
        let kept: Vec<i64> = out.iter().map(|r| r[0].parse().unwrap()).collect();
        assert_eq!(
            kept,
            vec![2_000, 1_500, 1_000],
            "时段外的 2_500 与 500 必须被丢弃，边界值保留"
        );
    }

    #[test]
    fn deduplicates_overlapping_pages() {
        let mut seen = HashSet::new();
        let mut out = Vec::new();

        absorb_candle_page(&[row(2_000), row(1_900)], 0, 3_000, &mut seen, &mut out);
        // 第二页与第一页有重叠（OKX 的分页在边界上可能重复返回）
        absorb_candle_page(&[row(1_900), row(1_800)], 0, 3_000, &mut seen, &mut out);

        let kept: Vec<i64> = out.iter().map(|r| r[0].parse().unwrap()).collect();
        assert_eq!(kept, vec![2_000, 1_900, 1_800], "重复的时间戳只能保留一次");
    }

    #[test]
    fn malformed_rows_do_not_break_the_page() {
        let page = vec![
            row(1_500),
            vec!["not-a-number".into(), "1".into()],
            vec![],
            row(1_400),
        ];

        let mut seen = HashSet::new();
        let mut out = Vec::new();
        absorb_candle_page(&page, 0, 2_000, &mut seen, &mut out);

        assert_eq!(out.len(), 2, "坏行应被跳过而不是让整页失败");
    }

    #[test]
    fn empty_page_yields_no_oldest() {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        assert_eq!(absorb_candle_page(&[], 0, 1, &mut seen, &mut out), None);
    }
}
