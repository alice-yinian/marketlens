//! 复盘统计（设计文档 §6.4.4）。
//!
//! 全部是纯函数：输入一批已平仓位，输出统计量。这样统计口径可以被单元测试钉住——
//! 统计错了比功能坏了更危险，因为它看起来总是「有道理」。

use std::collections::BTreeMap;

use serde::Serialize;
use ts_rs::TS;

use crate::position::history::ClosedPosition;

/// 「数据缺失」分组的键名。
///
/// 界面**不应**靠匹配这个字面量来判断——用 `StatGroup::is_unattributed`。
pub const UNATTRIBUTED_KEY: &str = "未归因";

/// 分组统计。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct StatGroup {
    /// 分组名（如「趋势向上」「多」「BTC-USDT-SWAP」）
    pub key: String,
    pub count: usize,
    pub win_rate: f64,
    pub total_pnl: f64,
    pub profit_factor: Option<f64>,
    /// 该分组是否由「数据缺失」产生。
    ///
    /// 显式字段而不是让界面去匹配 key 的字面量「未归因」——那种耦合一旦
    /// 后端改了文案就会静默失效（不报错，但「这是数据缺失」的标注会消失）。
    pub is_unattributed: bool,
}

/// 整体统计。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct ReviewStats {
    pub total: usize,
    /// 胜率：`realized_pnl > 0` 的笔数占比
    pub win_rate: f64,
    /// 盈亏比：盈利笔的平均盈亏 ÷ 亏损笔的平均亏损的绝对值。
    /// 没有亏损笔时为 `None`（除零无意义，也不该报成无穷大）
    pub profit_factor: Option<f64>,
    /// 期望值：每笔的平均已实现盈亏
    pub expectancy: f64,
    pub total_realized_pnl: f64,
    /// 平均持仓时长（毫秒）
    #[ts(type = "number")]
    pub avg_hold_ms: i64,
    /// 费用侵蚀：`Σ(手续费 + 资金费)` ÷ `Σ|已实现盈亏|`
    ///
    /// 这个数字回答的是「我的利润有多少被交易成本吃掉了」，是复盘里最容易被忽略、
    /// 又最容易改进的一项。
    pub fee_drag: Option<f64>,
    /// 按**开仓时**的趋势状态分组
    pub by_trend: Vec<StatGroup>,
    /// 按方向分组
    pub by_direction: Vec<StatGroup>,
    /// 按标的
    pub by_instrument: Vec<StatGroup>,
    /// 拿不到开仓时市场状态的笔数
    pub unattributed: usize,
}

/// 计算统计。
///
/// **口径说明**：胜负判定用 `realized_pnl`（含手续费与资金费），而不是 `pnl`（纯价差）。
/// 原因是「价差赚了但被费用吃掉」在交易者眼里就是亏损；用 `pnl` 会把这类交易算成胜利，
/// 让胜率虚高、也让费用侵蚀这一项失去意义。
pub fn compute(positions: &[ClosedPosition]) -> ReviewStats {
    let total = positions.len();

    let wins: Vec<f64> = positions
        .iter()
        .map(|p| p.realized_pnl)
        .filter(|pnl| *pnl > 0.0)
        .collect();
    let losses: Vec<f64> = positions
        .iter()
        .map(|p| p.realized_pnl)
        .filter(|pnl| *pnl <= 0.0)
        .collect();

    let win_rate = if total == 0 {
        0.0
    } else {
        wins.len() as f64 / total as f64
    };

    let profit_factor = if wins.is_empty() || losses.is_empty() {
        None
    } else {
        let avg_win = wins.iter().sum::<f64>() / wins.len() as f64;
        let avg_loss = losses.iter().sum::<f64>() / losses.len() as f64;
        Some(avg_win / avg_loss.abs())
    };

    let total_realized_pnl: f64 = positions.iter().map(|p| p.realized_pnl).sum();
    let expectancy = if total == 0 {
        0.0
    } else {
        total_realized_pnl / total as f64
    };

    let avg_hold_ms = if total == 0 {
        0
    } else {
        let sum: i64 = positions
            .iter()
            .map(|p| p.close_time.saturating_sub(p.open_time).max(0))
            .sum();
        sum / total as i64
    };

    let total_fees: f64 = positions
        .iter()
        .map(|p| p.fee.abs() + p.funding_fee.abs())
        .sum();
    let abs_pnl: f64 = positions.iter().map(|p| p.realized_pnl.abs()).sum();
    let fee_drag = if abs_pnl > 0.0 {
        Some(total_fees / abs_pnl)
    } else {
        None
    };

    ReviewStats {
        total,
        win_rate,
        profit_factor,
        expectancy,
        total_realized_pnl,
        avg_hold_ms,
        fee_drag,
        by_trend: group_by(positions, |p| {
            p.regime
                .as_ref()
                .and_then(|regime| regime.trend)
                .map(|trend| trend_label(trend).to_string())
        }),
        by_direction: group_by(positions, |p| {
            Some(direction_label(&p.direction).to_string())
        }),
        by_instrument: group_by(positions, |p| Some(p.inst_id.clone())),
        unattributed: positions
            .iter()
            .filter(|p| {
                p.regime
                    .as_ref()
                    .is_none_or(|regime| regime.trend.is_none())
            })
            .count(),
    }
}

/// 通用分组。分组键为 `None` 的样本会被归入「未归因」。
fn group_by(
    positions: &[ClosedPosition],
    key_of: impl Fn(&ClosedPosition) -> Option<String>,
) -> Vec<StatGroup> {
    let mut buckets: BTreeMap<String, Vec<&ClosedPosition>> = BTreeMap::new();

    for position in positions {
        let key = key_of(position).unwrap_or_else(|| UNATTRIBUTED_KEY.to_string());
        buckets.entry(key).or_default().push(position);
    }

    buckets
        .into_iter()
        .map(|(key, items)| {
            let count = items.len();
            let wins = items.iter().filter(|p| p.realized_pnl > 0.0).count();
            let total_pnl: f64 = items.iter().map(|p| p.realized_pnl).sum();

            let win_sum: f64 = items
                .iter()
                .filter(|p| p.realized_pnl > 0.0)
                .map(|p| p.realized_pnl)
                .sum();
            let win_count = wins as f64;
            let loss_sum: f64 = items
                .iter()
                .filter(|p| p.realized_pnl <= 0.0)
                .map(|p| p.realized_pnl)
                .sum();
            let loss_count = (count - wins) as f64;

            let profit_factor = if win_count > 0.0 && loss_count > 0.0 && loss_sum != 0.0 {
                Some((win_sum / win_count) / (loss_sum / loss_count).abs())
            } else {
                None
            };

            StatGroup {
                is_unattributed: key == UNATTRIBUTED_KEY,
                key,
                count,
                win_rate: if count == 0 {
                    0.0
                } else {
                    wins as f64 / count as f64
                },
                total_pnl,
                profit_factor,
            }
        })
        .collect()
}

fn trend_label(trend: crate::market::regime::TrendRegime) -> &'static str {
    use crate::market::regime::TrendRegime;
    match trend {
        TrendRegime::Uptrend => "趋势向上",
        TrendRegime::Downtrend => "趋势向下",
        TrendRegime::Range => "区间震荡",
        TrendRegime::Transition => "趋势转换",
    }
}

fn direction_label(direction: &str) -> &'static str {
    match direction {
        "long" => "多",
        "short" => "空",
        _ => "未知",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::regime::{Crowding, TrendRegime};
    use crate::position::history::RegimeSnapshot;

    fn position(pnl: f64, fee: f64, direction: &str, trend: Option<TrendRegime>) -> ClosedPosition {
        ClosedPosition {
            pos_id: format!("p-{pnl}-{direction}-{}", trend.map_or(0, |_| 1)),
            inst_id: "BTC-USDT-SWAP".to_string(),
            direction: direction.to_string(),
            mgn_mode: "cross".to_string(),
            lever: 3.0,
            open_avg_px: 100.0,
            close_avg_px: 100.0 + pnl,
            max_contracts: 1.0,
            pnl,
            pnl_ratio: pnl / 100.0,
            realized_pnl: pnl + fee,
            fee,
            funding_fee: 0.0,
            open_time: 1_000,
            close_time: 3_600_000 + 1_000,
            source: "okx".to_string(),
            time_precision: "exact".to_string(),
            regime: trend.map(|trend| RegimeSnapshot {
                trend: Some(trend),
                vol: None,
                crowding: Some(Crowding::Balanced),
                change_24h: None,
                funding_annualized: None,
                basis_pct: None,
            }),
            max_favorable: None,
            max_adverse: None,
        }
    }

    #[test]
    fn empty_input_produces_zeroed_stats() {
        let stats = compute(&[]);
        assert_eq!(stats.total, 0);
        assert_eq!(stats.win_rate, 0.0);
        assert_eq!(stats.profit_factor, None);
        assert_eq!(stats.avg_hold_ms, 0);
        assert_eq!(stats.fee_drag, None);
    }

    #[test]
    fn win_rate_counts_only_profitable_trades() {
        let positions = vec![
            position(10.0, 0.0, "long", None),
            position(-5.0, 0.0, "long", None),
            position(3.0, 0.0, "short", None),
            position(-1.0, 0.0, "short", None),
        ];
        let stats = compute(&positions);
        assert_eq!(stats.total, 4);
        assert!((stats.win_rate - 0.5).abs() < 1e-9);
        assert!((stats.total_realized_pnl - 7.0).abs() < 1e-9);
        assert!((stats.expectancy - 1.75).abs() < 1e-9);
    }

    /// 口径的关键测试：「价差赚了但被费用吃掉」必须算作亏损。
    #[test]
    fn fees_can_turn_a_price_gain_into_a_loss() {
        // 价差赚 1，手续费 -2 → 实际亏 1
        let position = position(1.0, -2.0, "long", None);
        assert_eq!(position.pnl, 1.0);
        assert_eq!(position.realized_pnl, -1.0);

        let stats = compute(&[position]);
        assert_eq!(stats.win_rate, 0.0, "含费用后亏损，不该算胜利");
        assert!(stats.total_realized_pnl < 0.0);
    }

    #[test]
    fn profit_factor_is_none_without_both_wins_and_losses() {
        let only_wins = vec![position(10.0, 0.0, "long", None)];
        assert_eq!(
            compute(&only_wins).profit_factor,
            None,
            "没有亏损笔时不应报无穷大"
        );

        let mixed = vec![
            position(10.0, 0.0, "long", None),
            position(-5.0, 0.0, "long", None),
        ];
        let stats = compute(&mixed);
        assert!(
            (stats.profit_factor.unwrap() - 2.0).abs() < 1e-9,
            "10/5 = 2"
        );
    }

    #[test]
    fn fee_drag_measures_cost_against_absolute_pnl() {
        let positions = vec![
            position(10.0, -2.0, "long", None),
            position(-10.0, -2.0, "long", None),
        ];
        let stats = compute(&positions);
        // 费用合计 4，|已实现盈亏| 合计 = 8 + 12 = 20
        assert!((stats.fee_drag.unwrap() - 0.2).abs() < 1e-9);
    }

    #[test]
    fn groups_by_direction_and_trend() {
        let positions = vec![
            position(10.0, 0.0, "long", Some(TrendRegime::Uptrend)),
            position(-5.0, 0.0, "long", Some(TrendRegime::Range)),
            position(8.0, 0.0, "short", Some(TrendRegime::Range)),
            position(-2.0, 0.0, "short", None),
        ];
        let stats = compute(&positions);

        let long_group = stats
            .by_direction
            .iter()
            .find(|g| g.key == "多")
            .expect("应有多单分组");
        assert_eq!(long_group.count, 2);
        assert!((long_group.win_rate - 0.5).abs() < 1e-9);
        assert!((long_group.total_pnl - 5.0).abs() < 1e-9);

        let range_group = stats
            .by_trend
            .iter()
            .find(|g| g.key == "区间震荡")
            .expect("应有震荡分组");
        assert_eq!(range_group.count, 2);

        let unattributed = stats
            .by_trend
            .iter()
            .find(|g| g.is_unattributed)
            .expect("未归因样本必须单独成组，不能悄悄并入其它分组");
        assert_eq!(unattributed.count, 1);
        assert_eq!(stats.unattributed, 1);
        assert!(
            stats.by_direction.iter().all(|g| !g.is_unattributed),
            "有方向的分组不该被标成未归因"
        );
    }

    #[test]
    fn average_hold_time_is_computed_from_open_and_close() {
        let stats = compute(&[position(1.0, 0.0, "long", None)]);
        assert_eq!(stats.avg_hold_ms, 3_600_000);
    }
}
