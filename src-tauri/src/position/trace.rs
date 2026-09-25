//! 本地留痕（设计文档 §6.4.2）。
//!
//! 用户每次在实盘页拉取仓位时，**顺手**把结果写一条 `position_trace`——
//! 零额外网络请求、零额外 API 配额（数据本来就已经拉回来了）。
//!
//! 它的价值在于：官方 `positions-history` 只保留有限时间窗口，而且拿不到
//! 「持仓期间的最大浮盈/最大浮亏」——只有连续留痕才能推导出来。因此**越早开始用越好**。
//!
//! 局限也要如实呈现：留痕只覆盖「用户打开过应用」的时刻。所以 [`TraceCoverage`]
//! 会把覆盖度连同间隙一起报给界面，让用户自己判断样本可信度。

use serde::Serialize;
use ts_rs::TS;

use crate::error::AppResult;
use crate::position::Position;
use crate::storage::Db;

/// 相邻留痕间隔超过该值即视为「有明显间隙」。
///
/// 实盘刷新由用户操作触发，间隔本来就不规律，所以阈值取得较宽（30 分钟）。
const GAP_THRESHOLD_MS: i64 = 30 * 60 * 1000;

/// 留痕覆盖度。随每次刷新一起返回。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "types.ts")]
pub struct TraceCoverage {
    /// 最近 30 天内的留痕条数
    #[ts(type = "number")]
    pub records: i64,
    /// 是否存在超过阈值的间隙
    pub has_gaps: bool,
    /// 最长间隙（毫秒），无间隙时为 0
    #[ts(type = "number")]
    pub max_gap_ms: i64,
    #[ts(type = "number | null")]
    pub last_trace_at: Option<i64>,
    /// 直接展示给用户的覆盖度说明
    pub note: String,
}

/// 一条留痕记录（对应 `position_trace` 表）。
pub struct TraceRow<'a> {
    pub ts: i64,
    pub pos_id: &'a str,
    pub inst_id: &'a str,
    pub pos_side: &'a str,
    pub mgn_mode: &'a str,
    pub lever: f64,
    pub contracts: f64,
    pub avg_px: f64,
    pub mark_px: f64,
    pub liq_px: Option<f64>,
    pub upl: f64,
    pub upl_ratio: f64,
    pub mgn_ratio: Option<f64>,
    pub created_at: i64,
    pub updated_at: i64,
    /// 本次留痕之前存在明显间隙
    pub gap_before: bool,
}

/// 把当前持仓写成一批留痕。
///
/// `gap_before` 的判定用的是「距上一次留痕的间隔」而不是逐仓位的间隔：
/// 用户没打开应用时，所有仓位都断了，这个粒度足够表达「这段数据不连续」。
pub async fn record(db: &Db, positions: &[Position], now: i64) -> AppResult<TraceCoverage> {
    let last_trace_at = db.last_position_trace_at().await?;
    let gap_before =
        last_trace_at.is_some_and(|previous| now.saturating_sub(previous) > GAP_THRESHOLD_MS);

    if !positions.is_empty() {
        let rows: Vec<TraceRow<'_>> = positions
            .iter()
            .map(|position| TraceRow {
                ts: now,
                pos_id: &position.pos_id,
                inst_id: &position.inst_id,
                pos_side: &position.pos_side,
                mgn_mode: &position.mgn_mode,
                lever: position.lever,
                contracts: position.contracts,
                avg_px: position.avg_px,
                mark_px: position.mark_px,
                liq_px: position.liq_px,
                upl: position.upl,
                upl_ratio: position.upl_ratio,
                mgn_ratio: position.mgn_ratio,
                created_at: position.created_at,
                updated_at: position.updated_at,
                gap_before,
            })
            .collect();

        db.insert_position_traces(&rows).await?;
    }

    coverage(db, now).await
}

/// 统计覆盖度。
pub async fn coverage(db: &Db, now: i64) -> AppResult<TraceCoverage> {
    let since = now - 30 * 24 * 60 * 60 * 1000;
    let stamps = db.position_trace_timestamps(since).await?;

    let records = stamps.len() as i64;
    let last_trace_at = stamps.last().copied();

    let max_gap_ms = stamps
        .windows(2)
        .map(|pair| pair[1].saturating_sub(pair[0]))
        .max()
        .unwrap_or(0);
    let has_gaps = max_gap_ms > GAP_THRESHOLD_MS;

    let note = if records == 0 {
        "尚无本地留痕。每次打开实盘页都会自动记录一次，坚持使用才能积累出官方接口拿不到的信息。"
            .to_string()
    } else if has_gaps {
        format!(
            "近 30 天有 {records} 次留痕，最长间隙 {} 分钟——部分持仓时长可能不精确。",
            max_gap_ms / 60_000
        )
    } else {
        format!("近 30 天有 {records} 次留痕，记录连续。")
    };

    Ok(TraceCoverage {
        records,
        has_gaps,
        max_gap_ms,
        last_trace_at,
        note,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gap_threshold_is_thirty_minutes() {
        assert_eq!(GAP_THRESHOLD_MS, 1_800_000);
    }
}
