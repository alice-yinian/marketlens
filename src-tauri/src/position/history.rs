//! 官方历史仓位同步（设计文档 §6.4.3）。
//!
//! 分页语义已真机核实：`positions-history` 的 `after` 接受**时间戳**（传 posId 会报
//! `51000`），返回 `uTime` 更旧的记录——与 K 线一致。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::AppResult;
use crate::okx::client::OkxClient;
use crate::okx::credentials::Credentials;
use crate::okx::endpoints::{RateGroup, private};
use crate::okx::models::OkxClosedPosition;
use crate::storage::Db;

/// 单页条数上限。
const PAGE_LIMIT: usize = 100;

/// 页数安全阀：防止边界判断写错导致无限翻页。
const MAX_PAGES: usize = 100;

/// 开仓时刻的市场状态快照。
///
/// 这是「市场状态」与「历史仓位」真正咬合的地方：有了它才能回答
/// 「我在震荡市做趋势单的胜率是多少」。
///
/// 各字段都是 `Option`：历史窗口内资金费率可得（约 90 天），但多空比与持仓量
/// **只有最近约 48 小时**（§6.1.3），所以对旧仓位它们必然是 `None`。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export_to = "types.ts")]
pub struct RegimeSnapshot {
    pub trend: Option<crate::market::regime::TrendRegime>,
    pub vol: Option<crate::market::regime::VolRegime>,
    pub crowding: Option<crate::market::regime::Crowding>,
    /// 开仓前 24 小时的涨跌幅
    pub change_24h: Option<f64>,
    /// 开仓时刻的资金费率年化
    pub funding_annualized: Option<f64>,
    /// 开仓时刻的基差
    pub basis_pct: Option<f64>,
}

/// 标准化后的历史仓位（对应 `position_history` 表，同时是 IPC 契约）。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct ClosedPosition {
    pub pos_id: String,
    pub inst_id: String,
    /// `long` | `short`
    ///
    /// 取自 OKX 的 `direction` 字段，**不是** `posSide`——净持仓模式下后者恒为 `net`，
    /// 用它会把多空单混为一谈。
    pub direction: String,
    /// `cross` | `isolated`
    pub mgn_mode: String,
    pub lever: f64,
    pub open_avg_px: f64,
    pub close_avg_px: f64,
    /// 持仓期间的最大张数
    pub max_contracts: f64,
    /// 价差盈亏（不含费用）
    pub pnl: f64,
    pub pnl_ratio: f64,
    /// 已实现盈亏（含费用）
    pub realized_pnl: f64,
    pub fee: f64,
    pub funding_fee: f64,
    #[ts(type = "number")]
    pub open_time: i64,
    #[ts(type = "number")]
    pub close_time: i64,
    /// `okx` | `local` | `merged`
    pub source: String,
    /// `exact` | `approximate`
    ///
    /// 本地留痕推导出的时间精度受快照间隔限制，必须如实标注——
    /// 数据可信度是复盘结论的地基。
    pub time_precision: String,
    /// 开仓时刻的市场状态。归因用。
    pub regime: Option<RegimeSnapshot>,
    /// 持仓期间的**最大浮盈**。
    ///
    /// **本地留痕独有**——官方 `positions-history` 拿不到这个数，
    /// 而它回答的是「我有没有过早离场」。
    pub max_favorable: Option<f64>,
    /// 持仓期间的**最大浮亏**。同样只有本地留痕能给。
    pub max_adverse: Option<f64>,
}

/// 一次同步的结果。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct SyncReport {
    pub fetched: usize,
    pub pages: usize,
    /// 是否触及 `from` 边界。`false` 表示数据在到达起点前就没了（不是错误，但要告知）。
    pub reached_from: bool,
}

/// 拉取 `[from, to]` 区间内的已平仓位并落库。
pub async fn sync(
    client: &OkxClient,
    db: &Db,
    credentials: &Credentials,
    from: i64,
    to: i64,
) -> AppResult<SyncReport> {
    let mut rows: Vec<ClosedPosition> = Vec::new();
    let mut cursor = to;
    let mut pages = 0usize;
    let mut reached_from = false;
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    loop {
        if pages >= MAX_PAGES {
            tracing::warn!("历史仓位分页触及页数上限，提前停止");
            break;
        }

        let limit = PAGE_LIMIT.to_string();
        let cursor_text = cursor.to_string();
        let page = client
            .get_private::<OkxClosedPosition>(
                credentials,
                private::ACCOUNT_POSITIONS_HISTORY,
                RateGroup::Account,
                &[
                    ("instType", crate::okx::endpoints::inst_type::SWAP),
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
            oldest = Some(oldest.map_or(record.u_time, |current: i64| current.min(record.u_time)));

            // 时段外的丢弃：分页会跨过边界
            if record.u_time < from || record.u_time > to {
                continue;
            }
            if seen.insert(record.pos_id.clone()) {
                rows.push(normalize(&record));
            }
        }

        pages += 1;

        match oldest {
            Some(ts) if ts <= from => {
                reached_from = true;
                break;
            }
            // 游标没推进 → 再翻也是同一页
            Some(ts) if ts >= cursor => {
                tracing::warn!(cursor, oldest = ts, "历史仓位游标未推进，停止");
                break;
            }
            Some(ts) => cursor = ts,
            None => break,
        }
    }

    if !rows.is_empty() {
        db.upsert_closed_positions(&rows).await?;
    }

    Ok(SyncReport {
        fetched: rows.len(),
        pages,
        reached_from,
    })
}

/// 数据库行 → 领域类型的中间结构。
///
/// 存在的唯一理由：`regime_snapshot` 在库里是 JSON 文本，需要反序列化才能变成
/// `RegimeSnapshot`，所以不能让 `ClosedPosition` 直接 `derive(FromRow)`。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ClosedPositionRow {
    pub pos_id: String,
    pub inst_id: String,
    pub direction: String,
    pub mgn_mode: String,
    pub lever: f64,
    pub open_avg_px: f64,
    pub close_avg_px: f64,
    pub max_contracts: f64,
    pub pnl: f64,
    pub pnl_ratio: f64,
    pub fee: f64,
    pub funding_fee: f64,
    pub realized_pnl: f64,
    pub open_time: i64,
    pub close_time: i64,
    pub source: String,
    pub time_precision: String,
    pub regime_snapshot: Option<String>,
    pub max_favorable: Option<f64>,
    pub max_adverse: Option<f64>,
}

impl From<ClosedPositionRow> for ClosedPosition {
    fn from(row: ClosedPositionRow) -> Self {
        let regime = row.regime_snapshot.as_deref().and_then(|raw| {
            match serde_json::from_str::<RegimeSnapshot>(raw) {
                Ok(snapshot) => Some(snapshot),
                Err(err) => {
                    // 不静默：归因快照解析失败意味着这条仓位的归因丢了，
                    // 统计里它会落到「不可得」那一档
                    tracing::warn!(pos_id = %row.pos_id, %err, "归因快照解析失败，按未归因处理");
                    None
                }
            }
        });

        Self {
            pos_id: row.pos_id,
            inst_id: row.inst_id,
            direction: row.direction,
            mgn_mode: row.mgn_mode,
            lever: row.lever,
            open_avg_px: row.open_avg_px,
            close_avg_px: row.close_avg_px,
            max_contracts: row.max_contracts,
            pnl: row.pnl,
            pnl_ratio: row.pnl_ratio,
            realized_pnl: row.realized_pnl,
            fee: row.fee,
            funding_fee: row.funding_fee,
            open_time: row.open_time,
            close_time: row.close_time,
            source: row.source,
            time_precision: row.time_precision,
            regime,
            max_favorable: row.max_favorable,
            max_adverse: row.max_adverse,
        }
    }
}

fn normalize(raw: &OkxClosedPosition) -> ClosedPosition {
    ClosedPosition {
        pos_id: raw.pos_id.clone(),
        inst_id: raw.inst_id.clone(),
        direction: raw.direction.clone(),
        mgn_mode: raw.mgn_mode.clone(),
        lever: raw.lever,
        open_avg_px: raw.open_avg_px,
        close_avg_px: raw.close_avg_px,
        max_contracts: raw.open_max_pos,
        pnl: raw.pnl,
        pnl_ratio: raw.pnl_ratio,
        realized_pnl: raw.realized_pnl,
        fee: raw.fee,
        funding_fee: raw.funding_fee,
        open_time: raw.c_time,
        close_time: raw.u_time,
        source: "okx".to_string(),
        time_precision: "exact".to_string(),
        regime: None,
        // 官方接口没有这两个数，等双源合并阶段由本地留痕补上
        max_favorable: None,
        max_adverse: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw() -> OkxClosedPosition {
        OkxClosedPosition {
            pos_id: "3953560806705233921".to_string(),
            inst_id: "SOL-USDT-SWAP".to_string(),
            inst_type: "SWAP".to_string(),
            uly: "SOL-USDT".to_string(),
            pos_side: "net".to_string(),
            direction: "long".to_string(),
            mgn_mode: "cross".to_string(),
            lever: 3.0,
            open_avg_px: 117.46334666666667,
            close_avg_px: 117.45,
            open_max_pos: 15.0,
            close_total_pos: 15.0,
            pnl: -0.2002,
            pnl_ratio: -0.003340701854116,
            realized_pnl: -1.9620501,
            fee: -1.7618501,
            funding_fee: 0.0,
            liq_penalty: 0.0,
            c_time: 1_790_327_693_740,
            u_time: 1_790_327_703_378,
            ccy: "USDT".to_string(),
            close_type: "2".to_string(),
        }
    }

    #[test]
    fn normalize_uses_direction_not_pos_side() {
        let normalized = normalize(&raw());
        assert_eq!(
            normalized.direction, "long",
            "方向必须取自 direction —— posSide 在净持仓模式下恒为 net"
        );
        assert_eq!(normalized.source, "okx");
        assert_eq!(normalized.time_precision, "exact");
        assert!(normalized.regime.is_none(), "同步阶段还没有归因数据");
    }

    #[test]
    fn normalize_maps_open_and_close_times() {
        let normalized = normalize(&raw());
        assert_eq!(
            normalized.open_time, 1_790_327_693_740,
            "开仓时间取自 cTime"
        );
        assert_eq!(
            normalized.close_time, 1_790_327_703_378,
            "平仓时间取自 uTime"
        );
        assert!(normalized.close_time > normalized.open_time);
    }

    #[test]
    fn normalize_keeps_both_pnl_values_distinct() {
        let normalized = normalize(&raw());
        // 价差盈亏与含费用的已实现盈亏是两个不同的数，混用会让统计偏乐观
        assert!((normalized.pnl - normalized.realized_pnl).abs() > 1e-6);
        assert!(normalized.realized_pnl < normalized.pnl);
    }
}
