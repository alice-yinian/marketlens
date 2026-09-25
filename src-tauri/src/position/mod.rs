//! 仓位服务（设计文档 §6.4）。

pub mod current;
pub mod trace;

use serde::Serialize;
use ts_rs::TS;

/// 标准化后的单个持仓。
///
/// 刻意保留 `contracts`（张数）与 `size_base`（币数量）两个字段：它们在不同场景下
/// 都有用，且换算过程容易出错——把两者都摆出来，界面和复盘都能一眼看出是否合理。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct Position {
    pub inst_id: String,
    pub pos_id: String,
    /// `long` | `short` | `net`——净持仓模式下恒为 `net`
    pub pos_side: String,
    /// `cross` | `isolated`
    pub mgn_mode: String,
    pub lever: f64,
    /// 张数（合约）
    pub contracts: f64,
    /// 币数量 = 张数 × ctVal × ctMult。换算不到合约信息时为 `None`。
    pub size_base: Option<f64>,
    pub avg_px: f64,
    pub mark_px: f64,
    pub liq_px: Option<f64>,
    pub upl: f64,
    pub upl_ratio: f64,
    pub mgn_ratio: Option<f64>,
    /// USD 名义价值，直接取自 OKX 的 `notionalUsd`
    pub notional_usd: f64,
    pub imr: Option<f64>,
    pub mmr: Option<f64>,
    pub fee: f64,
    pub funding_fee: f64,
    pub realized_pnl: f64,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
}

/// 账户权益概览。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct AccountOverview {
    /// 账户总权益（USD）
    pub total_eq_usd: f64,
    pub iso_eq_usd: f64,
    pub adj_eq_usd: f64,
    pub avail_eq_usd: f64,
    pub upl: f64,
    pub mgn_ratio: Option<f64>,
    pub imr: Option<f64>,
    pub mmr: Option<f64>,
    pub notional_usd: Option<f64>,
    /// `net_mode` | `long_short_mode`
    pub pos_mode: String,
    pub currencies: Vec<CurrencyBalance>,
    #[ts(type = "number")]
    pub fetched_at: i64,
}

/// 单币种余额（只保留界面需要的字段）。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct CurrencyBalance {
    pub ccy: String,
    pub eq: f64,
    pub eq_usd: f64,
    pub avail_bal: f64,
    pub cash_bal: f64,
}

/// 一次账户刷新结果。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct AccountSnapshot {
    pub overview: AccountOverview,
    pub positions: Vec<Position>,
    /// 本地留痕的覆盖情况（每次刷新都会顺带写一条，零额外网络请求）
    pub trace: trace::TraceCoverage,
    /// 非致命问题：单个端点失败不会让整次刷新失败
    pub warnings: Vec<String>,
}
