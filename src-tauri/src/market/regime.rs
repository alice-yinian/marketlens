//! 市场状态分类（设计文档 §6.3.2）。
//!
//! 诚实性设计：数据不足时**不返回中性值**。返回 `Normal` 会让提示词里出现一个
//! 看起来有依据、实际是猜测的结论——AI 会当真。所以分类结果是 `Option`，
//! 拿不到就进 `unavailable` 并说明原因，由模板渲染成「数据不可得」。

use serde::Serialize;
use ts_rs::TS;

use crate::market::{Candle, indicators};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "types.ts")]
pub enum TrendRegime {
    Uptrend,
    Downtrend,
    Range,
    Transition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "types.ts")]
pub enum VolRegime {
    Low,
    Normal,
    High,
    Extreme,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "types.ts")]
pub enum Crowding {
    LongCrowded,
    ShortCrowded,
    Balanced,
}

/// 判定阈值。全部集中在此，便于后续落进 `settings` 表由用户调整。
#[derive(Debug, Clone, Copy)]
pub struct Thresholds {
    /// EMA20 与 EMA60 的差距小于此比例时，视为「无趋势」
    pub range_band: f64,
    /// 价格距 EMA200 在此比例内时，视为震荡
    pub range_ema200_band: f64,
    /// 资金费率年化超过此值时视为拥挤
    pub funding_crowded: f64,
    /// 多空账户比超过此值时视为多头拥挤
    pub lsr_crowded: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            range_band: 0.005,
            range_ema200_band: 0.015,
            funding_crowded: 0.30,
            lsr_crowded: 2.0,
        }
    }
}

/// 分类输入。
pub struct Input<'a> {
    /// 升序 K 线
    pub candles: &'a [Candle],
    /// 由 K 线粒度决定：1H → 24×365，1D → 365
    pub periods_per_year: f64,
    /// 资金费率年化（`funding × 3 × 365`，8h 结算即日 3 次）
    pub funding_annualized: f64,
    pub long_short_ratio: Option<f64>,
    /// 历史已实现波动率样本（近 90 天日度），用于分位判定
    pub vol_history: &'a [f64],
    pub thresholds: Thresholds,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "types.ts")]
pub struct Regime {
    pub trend: Option<TrendRegime>,
    pub vol: Option<VolRegime>,
    /// 资金费率必然存在，所以拥挤度总能给出结论
    pub crowding: Crowding,
    /// 规则触发说明。提示词需要「为什么」，不只是「是什么」。
    pub signals: Vec<String>,
    /// 无法判定的项及原因。渲染成「数据不可得」，绝不留空。
    pub unavailable: Vec<String>,
}

pub fn classify(input: &Input<'_>) -> Regime {
    let closes: Vec<f64> = input.candles.iter().map(|candle| candle.close).collect();
    let limits = input.thresholds;
    let last_close = closes.last().copied().unwrap_or(0.0);

    let mut signals = Vec::new();
    let mut unavailable = Vec::new();

    let trend = classify_trend(&closes, last_close, limits, &mut signals, &mut unavailable);
    let vol = classify_vol(input, &closes, &mut signals, &mut unavailable);
    let crowding = classify_crowding(input, limits, &mut signals);

    Regime {
        trend,
        vol,
        crowding,
        signals,
        unavailable,
    }
}

fn classify_trend(
    closes: &[f64],
    last_close: f64,
    limits: Thresholds,
    signals: &mut Vec<String>,
    unavailable: &mut Vec<String>,
) -> Option<TrendRegime> {
    let (Some(fast), Some(mid), Some(slow)) = (
        indicators::ema(closes, 20),
        indicators::ema(closes, 60),
        indicators::ema(closes, 200),
    ) else {
        unavailable.push(format!(
            "趋势：判定需要至少 200 根 K 线，当前仅 {} 根",
            closes.len()
        ));
        return None;
    };

    if last_close <= 0.0 {
        unavailable.push("趋势：最新收盘价无效".to_string());
        return None;
    }

    let spread = (fast - mid).abs() / last_close;
    let distance_from_slow = (last_close - slow).abs() / last_close;

    let regime = if spread < limits.range_band && distance_from_slow < limits.range_ema200_band {
        TrendRegime::Range
    } else if fast > mid && mid > slow && last_close > fast {
        TrendRegime::Uptrend
    } else if fast < mid && mid < slow && last_close < fast {
        TrendRegime::Downtrend
    } else {
        TrendRegime::Transition
    };

    signals.push(match regime {
        TrendRegime::Uptrend => format!(
            "EMA20({fast:.1}) > EMA60({mid:.1}) > EMA200({slow:.1}) 且价格在 EMA20 上方，趋势向上"
        ),
        TrendRegime::Downtrend => format!(
            "EMA20({fast:.1}) < EMA60({mid:.1}) < EMA200({slow:.1}) 且价格在 EMA20 下方，趋势向下"
        ),
        TrendRegime::Range => format!(
            "EMA20 与 EMA60 仅相差 {:.2}%，且价格贴近 EMA200，判定为震荡",
            spread * 100.0
        ),
        TrendRegime::Transition => "均线排列不完整，趋势处于转换期".to_string(),
    });

    Some(regime)
}

fn classify_vol(
    input: &Input<'_>,
    closes: &[f64],
    signals: &mut Vec<String>,
    unavailable: &mut Vec<String>,
) -> Option<VolRegime> {
    let Some(current) = indicators::realized_vol(closes, input.periods_per_year) else {
        unavailable.push("波动：K 线不足，无法计算已实现波动率".to_string());
        return None;
    };

    let Some(rank) = indicators::percentile_rank(input.vol_history, current) else {
        unavailable.push("波动：缺少历史波动率样本，无法做分位判定".to_string());
        return None;
    };

    let regime = if rank < 0.25 {
        VolRegime::Low
    } else if rank < 0.75 {
        VolRegime::Normal
    } else if rank < 0.95 {
        VolRegime::High
    } else {
        VolRegime::Extreme
    };

    signals.push(format!(
        "已实现波动率年化 {:.1}%，处于近 {} 个样本的 {:.0}% 分位",
        current * 100.0,
        input.vol_history.len(),
        rank * 100.0
    ));

    Some(regime)
}

fn classify_crowding(input: &Input<'_>, limits: Thresholds, signals: &mut Vec<String>) -> Crowding {
    let funding = input.funding_annualized;
    // 空头阈值取多头阈值的 2/3（即年化 −20%），与设计文档里「资金费率极端榜」的下限一致
    let funding_short = -limits.funding_crowded * 2.0 / 3.0;

    let crowded_long = funding >= limits.funding_crowded
        || input
            .long_short_ratio
            .is_some_and(|ratio| ratio >= limits.lsr_crowded);
    let crowded_short = funding <= funding_short
        || input
            .long_short_ratio
            .is_some_and(|ratio| ratio <= 1.0 / limits.lsr_crowded);

    let crowding = if crowded_long {
        Crowding::LongCrowded
    } else if crowded_short {
        Crowding::ShortCrowded
    } else {
        Crowding::Balanced
    };

    match crowding {
        Crowding::LongCrowded => signals.push(format!(
            "资金费率年化 {:+.1}%，多头拥挤度偏高",
            funding * 100.0
        )),
        Crowding::ShortCrowded => signals.push(format!(
            "资金费率年化 {:+.1}%，空头拥挤度偏高",
            funding * 100.0
        )),
        Crowding::Balanced => {}
    }

    crowding
}

#[cfg(test)]
mod tests {
    use super::*;

    fn series(prices: impl IntoIterator<Item = f64>) -> Vec<Candle> {
        prices
            .into_iter()
            .map(|close| Candle {
                ts: 0,
                open: close,
                high: close,
                low: close,
                close,
                vol: 0.0,
                confirm: true,
            })
            .collect()
    }

    fn input<'a>(
        candles: &'a [Candle],
        funding: f64,
        lsr: Option<f64>,
        vol_history: &'a [f64],
    ) -> Input<'a> {
        Input {
            candles,
            periods_per_year: 24.0 * 365.0,
            funding_annualized: funding,
            long_short_ratio: lsr,
            vol_history,
            thresholds: Thresholds::default(),
        }
    }

    #[test]
    fn insufficient_candles_reports_unavailable_instead_of_guessing() {
        let candles = series((1..=50).map(f64::from));
        let result = classify(&input(&candles, 0.0, None, &[]));

        assert_eq!(result.trend, None, "K 线不足时不应给出趋势结论");
        assert!(
            result.unavailable.iter().any(|r| r.contains("200 根")),
            "必须说明为什么拿不到趋势：{:?}",
            result.unavailable
        );
    }

    #[test]
    fn sustained_rise_classifies_as_uptrend() {
        let candles = series((1..=260).map(|i| 100.0 + f64::from(i)));
        let result = classify(&input(&candles, 0.0, None, &[]));

        assert_eq!(result.trend, Some(TrendRegime::Uptrend));
        assert!(result.signals.iter().any(|s| s.contains("趋势向上")));
    }

    #[test]
    fn sustained_fall_classifies_as_downtrend() {
        let candles = series((1..=260).map(|i| 400.0 - f64::from(i)));
        let result = classify(&input(&candles, 0.0, None, &[]));

        assert_eq!(result.trend, Some(TrendRegime::Downtrend));
    }

    #[test]
    fn flat_market_classifies_as_range() {
        let candles = series(std::iter::repeat_n(100.0, 260));
        let result = classify(&input(&candles, 0.0, None, &[]));

        assert_eq!(result.trend, Some(TrendRegime::Range));
    }

    #[test]
    fn missing_vol_history_is_reported_not_defaulted() {
        let candles = series((1..=260).map(|i| 100.0 + f64::from(i % 5)));
        let result = classify(&input(&candles, 0.0, None, &[]));

        assert_eq!(result.vol, None, "没有历史样本时不能猜波动档位");
        assert!(
            result
                .unavailable
                .iter()
                .any(|r| r.contains("历史波动率样本"))
        );
    }

    #[test]
    fn highest_realized_vol_lands_in_extreme_bucket() {
        // 历史样本全为极小波动，当前波动远高于它们 → 分位 100%
        let candles = series((0..260).map(|i| if i % 2 == 0 { 100.0 } else { 120.0 }));
        let history = vec![0.001, 0.002, 0.003];
        let result = classify(&input(&candles, 0.0, None, &history));

        assert_eq!(result.vol, Some(VolRegime::Extreme));
        assert!(result.signals.iter().any(|s| s.contains("分位")));
    }

    #[test]
    fn funding_extremes_drive_crowding() {
        let candles = series(std::iter::repeat_n(100.0, 260));

        let long_side = classify(&input(&candles, 0.42, None, &[]));
        assert_eq!(long_side.crowding, Crowding::LongCrowded);

        let short_side = classify(&input(&candles, -0.42, None, &[]));
        assert_eq!(short_side.crowding, Crowding::ShortCrowded);

        let neutral = classify(&input(&candles, 0.01, Some(1.1), &[]));
        assert_eq!(neutral.crowding, Crowding::Balanced);
        assert!(
            neutral.signals.iter().all(|s| !s.contains("拥挤")),
            "中性状态不应产生拥挤信号"
        );
    }

    #[test]
    fn extreme_long_short_ratio_also_marks_crowding() {
        let candles = series(std::iter::repeat_n(100.0, 260));
        // 资金费率平淡，但多空比极端
        let result = classify(&input(&candles, 0.0, Some(2.5), &[]));
        assert_eq!(result.crowding, Crowding::LongCrowded);
    }
}
