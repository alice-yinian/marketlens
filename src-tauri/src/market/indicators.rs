//! 技术指标（设计文档 §6.3.1）。
//!
//! 全部是纯函数：输入数值切片，输出 `Option<f64>`。`None` 一律表示
//! **样本不足**而不是「值为 0」——这个区分很重要，0 和「算不出来」在复盘结论里
//! 是两件完全不同的事。

use crate::market::Candle;

/// 指数移动平均，返回最新一根的值。
///
/// 以前 `period` 根的简单平均作为种子，而不是直接用第一根收盘价：
/// 否则短序列的 EMA 会被第一个价格主导，与主流看板显示不一致。
pub fn ema(closes: &[f64], period: usize) -> Option<f64> {
    if period == 0 || closes.len() < period {
        return None;
    }

    let alpha = 2.0 / (period as f64 + 1.0);
    let mut value = closes[..period].iter().sum::<f64>() / period as f64;

    for &close in &closes[period..] {
        value = alpha * close + (1.0 - alpha) * value;
    }

    Some(value)
}

/// RSI（Wilder 平滑）。
///
/// 边界语义：平均跌幅为 0 时返回 100；涨跌都为 0（价格完全不动）时返回 50，
/// 因为此时 RSI 在数学上无定义，报 100 会误导成「极度超买」。
pub fn rsi_wilder(closes: &[f64], period: usize) -> Option<f64> {
    if period == 0 || closes.len() < period + 1 {
        return None;
    }

    let (mut gain_sum, mut loss_sum) = (0.0, 0.0);
    for window in closes[..=period].windows(2) {
        let delta = window[1] - window[0];
        if delta >= 0.0 {
            gain_sum += delta;
        } else {
            loss_sum -= delta;
        }
    }

    let divisor = period as f64;
    let mut avg_gain = gain_sum / divisor;
    let mut avg_loss = loss_sum / divisor;

    for window in closes[period..].windows(2) {
        let delta = window[1] - window[0];
        let (gain, loss) = if delta >= 0.0 {
            (delta, 0.0)
        } else {
            (0.0, -delta)
        };
        avg_gain = (avg_gain * (divisor - 1.0) + gain) / divisor;
        avg_loss = (avg_loss * (divisor - 1.0) + loss) / divisor;
    }

    if avg_loss == 0.0 {
        return Some(if avg_gain == 0.0 { 50.0 } else { 100.0 });
    }

    let rs = avg_gain / avg_loss;
    Some(100.0 - 100.0 / (1.0 + rs))
}

/// 平均真实波幅（Wilder 平滑）。返回**绝对价格**，展示时通常用 [`atr_pct`]。
pub fn atr(candles: &[Candle], period: usize) -> Option<f64> {
    if period == 0 || candles.len() < period + 1 {
        return None;
    }

    let true_ranges: Vec<f64> = candles
        .windows(2)
        .map(|window| {
            let (prev, current) = (window[0], window[1]);
            (current.high - current.low)
                .max((current.high - prev.close).abs())
                .max((current.low - prev.close).abs())
        })
        .collect();

    let divisor = period as f64;
    let mut value = true_ranges[..period].iter().sum::<f64>() / divisor;
    for tr in &true_ranges[period..] {
        value = (value * (divisor - 1.0) + tr) / divisor;
    }

    Some(value)
}

/// ATR 相对最新收盘价的比例。
pub fn atr_pct(candles: &[Candle], period: usize) -> Option<f64> {
    let last_close = candles.last()?.close;
    if last_close == 0.0 {
        return None;
    }
    Some(atr(candles, period)? / last_close)
}

/// 已实现波动率：对数收益率的样本标准差 × √(年化周期数)。
///
/// `periods_per_year` 由 K 线粒度决定：1H → `24 × 365`，1D → `365`。
pub fn realized_vol(closes: &[f64], periods_per_year: f64) -> Option<f64> {
    if closes.len() < 3 {
        return None;
    }

    let returns: Vec<f64> = closes
        .windows(2)
        .filter_map(|window| {
            let (prev, current) = (window[0], window[1]);
            if prev > 0.0 && current > 0.0 {
                Some((current / prev).ln())
            } else {
                None
            }
        })
        .collect();

    if returns.len() < 2 {
        return None;
    }

    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let variance =
        returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (returns.len() - 1) as f64;

    Some(variance.sqrt() * periods_per_year.sqrt())
}

/// 基差：`(标记价 − 指数价) / 指数价`。
///
/// 真机核对（§6.1.2.1）：ticker 响应里**没有** `idxPx`，
/// 所以标记价与指数价必须分别取自 `public/mark-price` 与 `market/index-tickers`。
pub fn basis_pct(mark_px: f64, index_px: f64) -> Option<f64> {
    if index_px == 0.0 {
        return None;
    }
    Some((mark_px - index_px) / index_px)
}

/// `value` 在 `sample` 中的分位（0.0~1.0，闭区间语义：小于等于的都计入）。
pub fn percentile_rank(sample: &[f64], value: f64) -> Option<f64> {
    if sample.is_empty() {
        return None;
    }
    let at_or_below = sample.iter().filter(|&&v| v <= value).count();
    Some(at_or_below as f64 / sample.len() as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candle(high: f64, low: f64, close: f64) -> Candle {
        Candle {
            ts: 0,
            open: close,
            high,
            low,
            close,
            vol: 0.0,
            confirm: true,
        }
    }

    #[test]
    fn ema_of_constant_series_equals_that_constant() {
        let closes = vec![100.0; 50];
        let value = ema(&closes, 20).expect("应可计算");
        assert!((value - 100.0).abs() < 1e-9, "常数序列的 EMA 应等于该常数");
    }

    #[test]
    fn ema_requires_enough_samples() {
        assert_eq!(ema(&[1.0, 2.0, 3.0], 20), None);
        assert_eq!(ema(&[], 20), None);
        assert_eq!(ema(&[1.0; 30], 0), None, "period 为 0 应返回 None");
    }

    #[test]
    fn ema_lags_a_rising_series() {
        let closes: Vec<f64> = (1..=100).map(f64::from).collect();
        let value = ema(&closes, 20).expect("应可计算");
        // 上升序列中 EMA 必然低于最新价、高于均值
        assert!(value < 100.0, "EMA 应滞后于最新价，实际 {value}");
        assert!(value > 50.0);
    }

    #[test]
    fn rsi_is_100_for_monotonic_rise_and_50_when_flat() {
        let rising: Vec<f64> = (1..=30).map(f64::from).collect();
        assert!((rsi_wilder(&rising, 14).unwrap() - 100.0).abs() < 1e-9);

        let flat = vec![100.0; 30];
        assert!(
            (rsi_wilder(&flat, 14).unwrap() - 50.0).abs() < 1e-9,
            "价格完全不动时 RSI 无定义，取中性的 50 而不是 100"
        );
    }

    #[test]
    fn rsi_is_zero_for_monotonic_fall() {
        let falling: Vec<f64> = (1..=30).rev().map(f64::from).collect();
        assert!(rsi_wilder(&falling, 14).unwrap().abs() < 1e-9);
    }

    #[test]
    fn atr_is_zero_for_flat_market() {
        let candles = vec![candle(100.0, 100.0, 100.0); 20];
        assert!(atr(&candles, 14).unwrap().abs() < 1e-9);
        assert!(atr_pct(&candles, 14).unwrap().abs() < 1e-9);
    }

    #[test]
    fn atr_accounts_for_gaps() {
        // 第二根跳空高开：真实波幅必须取 |high - prevClose|，而不是 high - low
        let candles = vec![candle(100.0, 99.0, 99.5), candle(120.0, 119.0, 119.5)];
        let tr = (120.0_f64 - 99.5).abs();
        assert!((atr(&candles, 1).unwrap() - tr).abs() < 1e-9);
    }

    #[test]
    fn realized_vol_is_zero_when_price_never_moves() {
        let closes = vec![100.0; 30];
        assert!(realized_vol(&closes, 24.0 * 365.0).unwrap().abs() < 1e-12);
    }

    #[test]
    fn realized_vol_scales_with_period_count() {
        let closes: Vec<f64> = (0..30).map(|i| 100.0 + f64::from(i % 2) * 2.0).collect();
        let hourly = realized_vol(&closes, 24.0 * 365.0).unwrap();
        let daily = realized_vol(&closes, 365.0).unwrap();
        assert!(hourly > daily, "同样的收益率序列，周期数越多年化波动率越大");
        assert!((hourly / daily - 24.0_f64.sqrt()).abs() < 1e-9);
    }

    #[test]
    fn basis_handles_zero_index_safely() {
        assert!(basis_pct(100.0, 0.0).is_none(), "指数价为 0 不能产生基差");
        let value = basis_pct(101.0, 100.0).unwrap();
        assert!((value - 0.01).abs() < 1e-12);
        assert!(basis_pct(99.0, 100.0).unwrap() < 0.0, "折价应为负基差");
    }

    #[test]
    fn percentile_rank_maps_known_positions() {
        let sample = vec![1.0, 2.0, 3.0, 4.0];
        assert!((percentile_rank(&sample, 4.0).unwrap() - 1.0).abs() < 1e-12);
        assert!((percentile_rank(&sample, 0.0).unwrap()).abs() < 1e-12);
        assert!((percentile_rank(&sample, 2.0).unwrap() - 0.5).abs() < 1e-12);
        assert!(percentile_rank(&[], 1.0).is_none());
    }
}
