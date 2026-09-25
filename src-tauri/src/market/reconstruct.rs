//! 关键时点归因：重建某个历史时刻的市场状态（设计文档 §6.3.4）。
//!
//! 这是「市场状态」与「历史仓位」真正咬合的地方——有了它才能回答
//! 「我在震荡市做趋势单的胜率是多少」，而不是只给出「我这笔赚了多少」。
//!
//! 数据来源全部是**已落库的** K 线与资金费率（由 M3 的采集计划器拉回），
//! 所以这一步不联网、可重复跑、可离线。

use crate::error::AppResult;
use crate::fetch::plan::SeriesKind;
use crate::market::indicators;
use crate::market::regime::{self, Thresholds};
use crate::position::history::RegimeSnapshot;
use crate::storage::Db;

/// 归因窗口：开仓前回看多少小时。
///
/// 24 小时是刻意的——它对应「开仓那一刻的交易者看到了什么」这个尺度。
/// 回看 90 天会把长期趋势混进来，而复盘要问的是「我当时为什么在这个位置进场」。
pub const LOOKBACK_HOURS: i64 = 24;

/// 判定趋势需要的 K 线根数（EMA200 + 一点余量）。
const MIN_CANDLES: i64 = 220;

/// 1 小时 K 线的年化周期数。
const PERIODS_PER_YEAR_HOURLY: f64 = 24.0 * 365.0;

/// 重建 `at` 时刻的市场状态。
///
/// 拿不到的指标一律为 `None`，**不猜**：
///   * 资金费率：约 90 天内可得，更早为 `None`
///   * 多空比 / 持仓量：只有最近约 48 小时（§6.1.3），旧仓位必然为 `None`
pub async fn regime_at(db: &Db, inst_id: &str, bar: &str, at: i64) -> AppResult<RegimeSnapshot> {
    let candles = db
        .candles_before(
            inst_id,
            SeriesKind::Candles.candle_kind(),
            bar,
            at,
            MIN_CANDLES,
        )
        .await?;

    let closes: Vec<f64> = candles.iter().map(|candle| candle.close).collect();

    // 基差：标记价与指数价都要有，缺一不可
    let mark = db
        .candles_before(inst_id, SeriesKind::MarkCandles.candle_kind(), bar, at, 1)
        .await?;
    let index_id =
        crate::okx::models::index_inst_id(inst_id).unwrap_or_else(|| inst_id.to_string());
    let index = db
        .candles_before(
            &index_id,
            SeriesKind::IndexCandles.candle_kind(),
            bar,
            at,
            1,
        )
        .await?;
    let basis_pct = match (mark.first(), index.first()) {
        (Some(mark), Some(index)) => indicators::basis_pct(mark.close, index.close),
        _ => None,
    };

    // 资金费率：取 at 之前最近的一个结算点
    let funding_rate = db.metric_value_before(inst_id, "funding", at).await?;
    let funding_annualized = funding_rate.map(|rate| rate * 3.0 * 365.0);

    // 开仓前 24 小时的涨跌幅
    let change_24h = change_over(&closes, LOOKBACK_HOURS as usize);

    // 多空比：只有近约 48 小时有数据，旧仓位必然为 None
    let ccy = inst_id.split('-').next().unwrap_or(inst_id);
    let long_short_ratio = db.metric_value_before(ccy, "lsr", at).await?;

    if candles.len() < MIN_CANDLES as usize {
        // 样本不足时不做趋势判定，但要明确告知调用方「归因不完整」
        tracing::debug!(
            inst_id,
            at,
            have = candles.len(),
            need = MIN_CANDLES,
            "归因窗口内的 K 线不足，趋势与波动将不可得"
        );
    }

    let vol_history = rolling_vol_history(&closes);
    let regime = regime::classify(&regime::Input {
        candles: &candles,
        periods_per_year: PERIODS_PER_YEAR_HOURLY,
        funding_annualized: funding_annualized.unwrap_or(0.0),
        long_short_ratio,
        vol_history: &vol_history,
        thresholds: Thresholds::default(),
    });

    Ok(RegimeSnapshot {
        trend: regime.trend,
        vol: regime.vol,
        // 资金费率拿不到时拥挤度无从判定——给 None 而不是「均衡」，
        // 后者会让统计里出现一个看似有依据、实为猜测的分组
        crowding: funding_annualized.map(|_| regime.crowding),
        change_24h,
        funding_annualized,
        basis_pct,
    })
}

/// 区间涨跌幅：末值相对前 `lookback` 根之前的值。
fn change_over(closes: &[f64], lookback: usize) -> Option<f64> {
    if lookback == 0 || closes.len() <= lookback {
        return None;
    }
    let start = closes[closes.len() - 1 - lookback];
    let end = closes.last().copied()?;
    if start == 0.0 {
        return None;
    }
    Some((end - start) / start)
}

/// 滚动 24 小时窗口的已实现波动率，用作分位判定的样本。
///
/// 与 `market/live.rs` 里同一套做法：**当前值与历史样本用相同的窗口与周期数**，
/// 否则分位数没有意义。
fn rolling_vol_history(closes: &[f64]) -> Vec<f64> {
    let window = LOOKBACK_HOURS as usize;
    if closes.len() <= window {
        return Vec::new();
    }
    (window..closes.len())
        .filter_map(|end| {
            indicators::realized_vol(&closes[end - window..=end], PERIODS_PER_YEAR_HOURLY)
        })
        .collect()
}

/// 给一批已平仓位补上开仓时刻的市场状态。
///
/// 逐条独立处理：某一条的归因失败不应该让整批复盘不可用，
/// 它的 `regime` 保持 `None`，统计时归入「未归因」。
pub async fn attribute(
    db: &Db,
    bar: &str,
    positions: &mut [crate::position::history::ClosedPosition],
) {
    for position in positions.iter_mut() {
        match regime_at(db, &position.inst_id, bar, position.open_time).await {
            Ok(snapshot) => position.regime = Some(snapshot),
            Err(err) => {
                tracing::warn!(
                    pos_id = %position.pos_id,
                    inst_id = %position.inst_id,
                    %err,
                    "归因失败，该仓位将计入未归因"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::Candle;

    fn candles(count: usize, base: f64) -> Vec<Candle> {
        (0..count)
            .map(|index| Candle {
                ts: index as i64 * 3_600_000,
                open: base,
                high: base,
                low: base,
                close: base + index as f64,
                vol: 1.0,
                confirm: true,
            })
            .collect()
    }

    #[test]
    fn change_over_uses_the_lookback_window() {
        let closes: Vec<f64> = (0..=24).map(f64::from).collect();
        // 末值 24，24 根之前是 0 → 但 0 会导致除零，所以从 1 开始
        let closes: Vec<f64> = closes.iter().map(|v| v + 1.0).collect();
        let change = change_over(&closes, 24).expect("应可计算");
        assert!((change - (25.0 - 1.0) / 1.0).abs() < 1e-9);
    }

    #[test]
    fn change_over_returns_none_when_history_is_too_short() {
        let closes = vec![100.0; 10];
        assert_eq!(change_over(&closes, 24), None);
        assert_eq!(change_over(&[], 24), None);
        assert_eq!(change_over(&[1.0, 2.0], 0), None, "窗口为 0 无意义");
    }

    #[test]
    fn rolling_vol_history_matches_current_window_length() {
        let closes: Vec<f64> = (0..100).map(|i| 100.0 + f64::from(i % 5)).collect();
        let history = rolling_vol_history(&closes);
        assert_eq!(
            history.len(),
            closes.len() - LOOKBACK_HOURS as usize,
            "样本数应为「长度 − 窗口」"
        );
        assert!(history.iter().all(|value| value.is_finite()));
    }

    #[test]
    fn rolling_vol_history_is_empty_for_short_series() {
        assert!(
            rolling_vol_history(
                &candles(10, 100.0)
                    .iter()
                    .map(|c| c.close)
                    .collect::<Vec<_>>()
            )
            .is_empty()
        );
    }
}
