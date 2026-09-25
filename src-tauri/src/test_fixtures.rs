//! 测试夹具（仅 `cfg(test)`）。
//!
//! 刻意让部分数据**不可得**（Rubik 指标缺失、无开仓时刻状态、无本地留痕极值）：
//! 提示词模块最需要验证的不是「数据齐全时好看」，而是「数据残缺时是否诚实」。

use crate::market::live::{LiveSnapshot, MarketState};
use crate::market::regime::{Crowding, TrendRegime, VolRegime};
use crate::position::history::{ClosedPosition, RegimeSnapshot};
use crate::position::merge::MergeReport;
use crate::position::stats::{ReviewStats, StatGroup};
use crate::position::trace::TraceCoverage;
use crate::position::{AccountOverview, CurrencyBalance, Position};
use crate::review::ReviewContext;

/// 夹具里的金额刻意选成不会与行情数字撞车的值，
/// 这样黄金测试可以直接做子串匹配来判断「有没有泄漏」。
pub const EQUITY: f64 = 12_345.67;
pub const POSITION_UPL: f64 = -1_234.56;
pub const REVIEW_PNL: f64 = 9_876.54;

const NOW: i64 = 1_700_000_000_000;

pub fn live_snapshot() -> LiveSnapshot {
    LiveSnapshot {
        ts: NOW,
        cache_hit: false,
        fetched_at: NOW,
        watchlist: vec!["BTC-USDT-SWAP".to_string(), "ETH-USDT-SWAP".to_string()],
        instruments: vec![
            MarketState {
                inst_id: "BTC-USDT-SWAP".to_string(),
                ts: NOW,
                last: 100_000.0,
                change_pct: 0.0123,
                high_24h: 101_000.0,
                low_24h: 99_000.0,
                volume_24h_usd: 1_500_000_000.0,
                funding_rate: 0.0001,
                funding_annualized: 0.1095,
                next_funding_rate: Some(0.00012),
                next_funding_time: Some(NOW + 3_600_000),
                open_interest_usd: Some(800_000_000.0),
                basis_pct: Some(0.0012),
                // Rubik 指标缺失：模拟「该标的没有 Rubik 数据」
                long_short_ratio: None,
                taker_buy_sell_ratio: None,
                ema20: Some(99_500.0),
                ema60: Some(98_000.0),
                ema200: Some(95_000.0),
                rsi14: Some(58.3),
                atr_pct: Some(0.021),
                realized_vol: Some(0.45),
                trend: Some(TrendRegime::Uptrend),
                vol: Some(VolRegime::Normal),
                crowding: Crowding::Balanced,
                signals: vec!["EMA20 > EMA60 > EMA200，趋势向上".to_string()],
                unavailable: vec!["多空比：Rubik 指标不可得".to_string()],
            },
            MarketState {
                inst_id: "ETH-USDT-SWAP".to_string(),
                ts: NOW,
                last: 3_500.0,
                change_pct: -0.0087,
                high_24h: 3_600.0,
                low_24h: 3_400.0,
                volume_24h_usd: 700_000_000.0,
                funding_rate: 0.00008,
                funding_annualized: 0.0876,
                next_funding_rate: None,
                next_funding_time: None,
                open_interest_usd: None,
                basis_pct: None,
                long_short_ratio: Some(1.8),
                taker_buy_sell_ratio: Some(0.95),
                // K 线不足：趋势与波动都判不出来
                ema20: None,
                ema60: None,
                ema200: None,
                rsi14: None,
                atr_pct: None,
                realized_vol: None,
                trend: None,
                vol: None,
                crowding: Crowding::Balanced,
                signals: vec![],
                unavailable: vec!["趋势：K 线不足".to_string(), "波动：K 线不足".to_string()],
            },
        ],
        warnings: vec![],
    }
}

pub fn account_overview() -> AccountOverview {
    AccountOverview {
        total_eq_usd: EQUITY,
        iso_eq_usd: 0.0,
        adj_eq_usd: EQUITY,
        avail_eq_usd: 10_000.0,
        upl: POSITION_UPL,
        mgn_ratio: Some(2.5),
        imr: Some(500.0),
        mmr: Some(50.0),
        notional_usd: Some(8_000.0),
        pos_mode: "net_mode".to_string(),
        currencies: vec![CurrencyBalance {
            ccy: "USDT".to_string(),
            eq: EQUITY,
            eq_usd: EQUITY,
            avail_bal: 10_000.0,
            cash_bal: EQUITY,
        }],
        fetched_at: NOW,
    }
}

pub fn positions() -> Vec<Position> {
    vec![
        Position {
            inst_id: "BTC-USDT-SWAP".to_string(),
            pos_id: "pos-1".to_string(),
            pos_side: "net".to_string(),
            mgn_mode: "cross".to_string(),
            lever: 3.0,
            contracts: 1.0,
            size_base: Some(0.01),
            avg_px: 99_000.0,
            mark_px: 100_000.0,
            liq_px: Some(70_000.0),
            upl: POSITION_UPL,
            upl_ratio: -0.1,
            mgn_ratio: Some(3.2),
            notional_usd: 8_000.0,
            imr: Some(266.67),
            mmr: Some(40.0),
            fee: -1.2,
            funding_fee: -0.3,
            realized_pnl: 0.0,
            created_at: NOW - 7_200_000,
            updated_at: NOW,
        },
        Position {
            inst_id: "ETH-USDT-SWAP".to_string(),
            pos_id: "pos-2".to_string(),
            pos_side: "net".to_string(),
            mgn_mode: "cross".to_string(),
            lever: 2.0,
            contracts: 2.0,
            size_base: None,
            avg_px: 3_600.0,
            mark_px: 3_500.0,
            // 全仓模式下没有强平价
            liq_px: None,
            upl: -20.0,
            upl_ratio: -0.028,
            mgn_ratio: None,
            notional_usd: 7_000.0,
            imr: None,
            mmr: None,
            fee: -0.5,
            funding_fee: 0.0,
            realized_pnl: 0.0,
            created_at: NOW - 86_400_000,
            updated_at: NOW,
        },
    ]
}

pub fn review_context() -> ReviewContext {
    ReviewContext {
        from: NOW - 30 * 86_400_000,
        to: NOW,
        bar: "1H".to_string(),
        positions: vec![
            // 一笔归因完整
            ClosedPosition {
                pos_id: "closed-1".to_string(),
                inst_id: "BTC-USDT-SWAP".to_string(),
                direction: "long".to_string(),
                mgn_mode: "cross".to_string(),
                lever: 3.0,
                open_avg_px: 98_000.0,
                close_avg_px: 100_000.0,
                max_contracts: 1.0,
                pnl: REVIEW_PNL,
                pnl_ratio: 0.0204,
                realized_pnl: REVIEW_PNL,
                fee: -12.5,
                funding_fee: -1.5,
                open_time: NOW - 10 * 86_400_000,
                close_time: NOW - 10 * 86_400_000 + 7_200_000,
                source: "okx".to_string(),
                time_precision: "exact".to_string(),
                regime: Some(RegimeSnapshot {
                    trend: Some(TrendRegime::Uptrend),
                    vol: Some(VolRegime::Normal),
                    crowding: Some(Crowding::Balanced),
                    change_24h: Some(0.0463),
                    funding_annualized: Some(0.1095),
                    basis_pct: Some(0.0012),
                }),
                max_favorable: Some(150.0),
                max_adverse: Some(-40.0),
            },
            // 一笔缺开仓时刻状态、缺极值：模拟超出 K 线范围 + 无本地留痕
            ClosedPosition {
                pos_id: "closed-2".to_string(),
                inst_id: "ETH-USDT-SWAP".to_string(),
                direction: "short".to_string(),
                mgn_mode: "cross".to_string(),
                lever: 2.0,
                open_avg_px: 3_700.0,
                close_avg_px: 3_650.0,
                max_contracts: 2.0,
                pnl: -500.0,
                pnl_ratio: -0.0135,
                realized_pnl: -510.0,
                fee: -10.0,
                funding_fee: 0.0,
                open_time: NOW - 60 * 86_400_000,
                close_time: NOW - 60 * 86_400_000 + 3_600_000,
                source: "okx".to_string(),
                time_precision: "exact".to_string(),
                regime: None,
                max_favorable: None,
                max_adverse: None,
            },
        ],
        stats: ReviewStats {
            total: 2,
            win_rate: 0.5,
            profit_factor: Some(19.37),
            expectancy: (REVIEW_PNL - 510.0) / 2.0,
            total_realized_pnl: REVIEW_PNL - 510.0,
            avg_hold_ms: 5_400_000,
            fee_drag: Some(0.0037),
            by_trend: vec![
                StatGroup {
                    key: "趋势向上".to_string(),
                    count: 1,
                    win_rate: 1.0,
                    total_pnl: REVIEW_PNL,
                    profit_factor: None,
                    is_unattributed: false,
                },
                StatGroup {
                    key: "未归因".to_string(),
                    count: 1,
                    win_rate: 0.0,
                    total_pnl: -510.0,
                    profit_factor: None,
                    is_unattributed: true,
                },
            ],
            by_direction: vec![
                StatGroup {
                    key: "多".to_string(),
                    count: 1,
                    win_rate: 1.0,
                    total_pnl: REVIEW_PNL,
                    profit_factor: None,
                    is_unattributed: false,
                },
                StatGroup {
                    key: "空".to_string(),
                    count: 1,
                    win_rate: 0.0,
                    total_pnl: -510.0,
                    profit_factor: None,
                    is_unattributed: false,
                },
            ],
            by_instrument: vec![
                StatGroup {
                    key: "BTC-USDT-SWAP".to_string(),
                    count: 1,
                    win_rate: 1.0,
                    total_pnl: REVIEW_PNL,
                    profit_factor: None,
                    is_unattributed: false,
                },
                StatGroup {
                    key: "ETH-USDT-SWAP".to_string(),
                    count: 1,
                    win_rate: 0.0,
                    total_pnl: -510.0,
                    profit_factor: None,
                    is_unattributed: false,
                },
            ],
            unattributed: 1,
        },
        merge: MergeReport {
            from_official: 2,
            from_local: 0,
            enriched: 1,
            coverage_note: "本地留痕与官方记录一致，并为 1 笔补充了持仓期间的极值。".to_string(),
        },
        trace: TraceCoverage {
            records: 12,
            has_gaps: false,
            max_gap_ms: 0,
            last_trace_at: Some(NOW - 3_600_000),
            note: "近 30 天有 12 次留痕，记录连续。".to_string(),
        },
        warnings: vec!["官方历史仓位在到达起始时间前已耗尽，该时段可能不完整".to_string()],
    }
}
