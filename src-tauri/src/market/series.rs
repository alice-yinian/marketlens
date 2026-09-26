//! 逐根指标序列（行情页专用）。
//!
//! ## 与 `market::indicators` 的分工
//!
//! `indicators` 里是**纯函数、只算最新一根**——实盘快照要回答的是「现在 RSI 多少」。
//! 行情页要回答的是「RSI 从 30 回到 55、EMA20 上穿 EMA60」，需要**整段序列**。
//!
//! ## 实现方式：复用已验证的纯函数 + 滑动窗口
//!
//! 对第 `i` 根调用 `f(&data[..=i], period)`，即 O(n²)。取 n ≤ 500 时约 25 万次
//! 基本运算（微秒~毫秒级），换来的是**不必把交叉验证过的公式再实现一遍**
//! （见设计文档 §13.2：指标公式是与 Python 独立实现逐位比对过的）。
//! 写增量版本会更快，但那等于让「公式到底对不对」这个问题重新出现一次。
//!
//! ## 一条不能破的约束
//!
//! 每列的长度**必须**与 K 线根数相同。`live.rs::rolling_vol_history` 用的是
//! `filter_map`，样本不足的头部会被吞掉，返回的 Vec 比输入短——直接照抄会让
//! 指标列与 K 线列**静默错位**，而错位的数据看起来完全正常。
//! 所以这里一律产出 `Vec<Option<f64>>`，并用测试钉住长度。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, AppResult};
use crate::fetch::plan::bar_millis;
use crate::market::{Candle, indicators};

/// 单个指标的周期上限。
///
/// 不是随便取的：周期大于 K 线根数时整列都是「样本不足」，让用户填 5000
/// 只会得到一列破折号；而 500 已经覆盖 EMA200 这类长周期。
pub const MAX_INDICATOR_PERIOD: u32 = 500;

/// 一次最多几个指标。
///
/// 每个指标都会给每一根 K 线加一列，直接乘进 token。
pub const MAX_INDICATORS: usize = 8;

/// 可选的指标种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "types.ts")]
#[serde(rename_all = "snake_case")]
pub enum IndicatorKind {
    /// 指数移动平均
    Ema,
    /// 相对强弱（Wilder 平滑）
    Rsi,
    /// 平均真实波幅占收盘价的比例
    ///
    /// 显式重命名成 `atr_pct`：默认 snake_case 会给出 `atr`，而 [`IndicatorKind::key`]
    /// 与上下文元数据里用的是 `atr_pct`——同一个概念出现两个名字，
    /// 前端的文案查表就会拿到 `undefined`（有测试钉住这一点）。
    #[serde(rename = "atr_pct")]
    Atr,
    /// 已实现波动率（按该粒度年化）
    RealizedVol,
}

impl IndicatorKind {
    /// 列名里用的短标签。
    pub fn label(self) -> &'static str {
        match self {
            IndicatorKind::Ema => "EMA",
            IndicatorKind::Rsi => "RSI",
            IndicatorKind::Atr => "ATR%",
            // 不用 `VOL`：那会与成交量混淆；`RVol` 是 realized volatility 的常见缩写
            IndicatorKind::RealizedVol => "RVol",
        }
    }

    /// 一句话说明，模板用它自动生成图例——**不让用户手写**，
    /// 否则加了指标忘了改图例，AI 就会按错误的列名理解数据。
    pub fn description(self) -> &'static str {
        match self {
            IndicatorKind::Ema => "指数移动平均（收盘价）",
            IndicatorKind::Rsi => "相对强弱指标（Wilder 平滑）",
            IndicatorKind::Atr => "平均真实波幅占收盘价的比例",
            IndicatorKind::RealizedVol => "已实现波动率（按该粒度年化）",
        }
    }

    /// 落进计划/配置里的稳定取值。
    pub fn key(self) -> &'static str {
        match self {
            IndicatorKind::Ema => "ema",
            IndicatorKind::Rsi => "rsi",
            IndicatorKind::Atr => "atr_pct",
            IndicatorKind::RealizedVol => "realized_vol",
        }
    }
}

/// 一个指标的规格：种类 + 周期。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "types.ts")]
pub struct IndicatorSpec {
    pub kind: IndicatorKind,
    pub period: u32,
}

impl IndicatorSpec {
    /// 展示名（如 `EMA20`），同时用作表格列名。
    pub fn name(&self) -> String {
        format!("{}{}", self.kind.label(), self.period)
    }
}

/// 校验并规范化指标清单：去重、限制数量与周期范围。
///
/// 这是**安全边界**而不是文案打磨：`period = 0` 会让底层的纯函数走进
/// 「除零」分支，`period` 极大则整列不可得。入口拒掉，错误才指向真正的问题。
pub fn validate_specs(specs: &[IndicatorSpec]) -> AppResult<Vec<IndicatorSpec>> {
    let mut unique: Vec<IndicatorSpec> = Vec::new();
    for spec in specs {
        if spec.period == 0 || spec.period > MAX_INDICATOR_PERIOD {
            return Err(AppError::Config(format!(
                "{} 的周期必须在 1~{MAX_INDICATOR_PERIOD} 之间，当前 {}",
                spec.kind.label(),
                spec.period
            )));
        }
        // 去重放在数量校验**之前**：前端重复提交同一个指标不该被当成「选太多」。
        if !unique.contains(spec) {
            unique.push(*spec);
        }
    }

    if unique.len() > MAX_INDICATORS {
        return Err(AppError::Config(format!(
            "最多 {MAX_INDICATORS} 个指标，当前 {} 个",
            unique.len()
        )));
    }

    Ok(unique)
}

/// 该粒度一年有多少根 K 线——已实现波动率靠它年化。
///
/// 用 `bar_millis` 反推而不是手写一张表：手写的表与 `bar_millis` 迟早会不一致，
/// 而不一致的后果是「同一个波动率在不同页面显示不同值」。
pub fn periods_per_year(bar: &str) -> Option<f64> {
    let millis = bar_millis(bar)?;
    if millis <= 0 {
        return None;
    }
    Some(365.0 * 24.0 * 3600.0 * 1000.0 / millis as f64)
}

/// 一个周期的完整数据：K 线 + 逐根指标。
///
/// 它是「取数」与「装配」之间的交接物：命令层把库里的 K 线读出来、算出指标，
/// 再交给 `prompt::context::market` 摊平成模板友好的 JSON。
#[derive(Debug, Clone)]
pub struct MarketSeries {
    pub inst_id: String,
    pub bar: String,
    pub candles: Vec<Candle>,
    pub indicators: Vec<IndicatorColumn>,
    /// 计划里**请求**的根数。实际拿到的可能更少（新上市合约、接口返回不足），
    /// 两者不一致时必须如实告诉用户，否则他会以为 AI 看到了 200 根。
    pub requested_count: usize,
}

impl MarketSeries {
    /// 收到的 K 线根数。
    pub fn candle_count(&self) -> usize {
        self.candles.len()
    }

    /// 是否比请求的根数少。
    pub fn is_short(&self) -> bool {
        self.candles.len() < self.requested_count
    }
}

/// 一列指标，**与 K 线等长**。
#[derive(Debug, Clone)]
pub struct IndicatorColumn {
    pub spec: IndicatorSpec,
    pub name: String,
    pub values: Vec<Option<f64>>,
}

impl IndicatorColumn {
    /// 开头有多少根因为样本不足而算不出。
    ///
    /// 用它（而不是用 `period` 反推）来写「前 N 根为 —」的声明：
    /// 各类指标需要的样本数差一根两根，反推迟早会写错，而观测值不会。
    pub fn insufficient_bars(&self) -> usize {
        self.values
            .iter()
            .take_while(|value| value.is_none())
            .count()
    }

    /// 整列都算不出来（用户要的根数比指标需要的样本还少）。
    pub fn is_unavailable(&self) -> bool {
        !self.values.is_empty() && self.values.iter().all(Option::is_none)
    }

    /// 非头部位置的缺失数量。正常情况下应为 0；不为 0 时必须如实上报，
    /// 否则「前 N 根为 —」这句话就在撒谎。
    pub fn interior_missing(&self) -> usize {
        let leading = self.insufficient_bars();
        self.values
            .iter()
            .skip(leading)
            .filter(|value| value.is_none())
            .count()
    }
}

/// 逐根计算全部指标列。
///
/// `bar` 用于已实现波动率的年化因子；不是白名单里的粒度时**报错而不是猜一个**，
/// 因为猜出来的年化系数会让数字看起来合理但完全错误。
pub fn compute(
    candles: &[Candle],
    specs: &[IndicatorSpec],
    bar: &str,
) -> AppResult<Vec<IndicatorColumn>> {
    let per_year = periods_per_year(bar)
        .ok_or_else(|| AppError::Config(format!("无法为粒度 {bar} 计算年化因子")))?;

    let closes: Vec<f64> = candles.iter().map(|candle| candle.close).collect();

    let columns = specs
        .iter()
        .map(|spec| {
            let period = spec.period as usize;
            // 逐根都从 `[..end]` 重算：见文件头对 O(n²) 的说明。
            let values: Vec<Option<f64>> = (1..=candles.len())
                .map(|end| match spec.kind {
                    IndicatorKind::Ema => indicators::ema(&closes[..end], period),
                    IndicatorKind::Rsi => indicators::rsi_wilder(&closes[..end], period),
                    IndicatorKind::Atr => indicators::atr_pct(&candles[..end], period),
                    IndicatorKind::RealizedVol => {
                        realized_vol_over_window(&closes[..end], period, per_year)
                    }
                })
                .collect();

            debug_assert_eq!(
                values.len(),
                candles.len(),
                "指标列必须与 K 线等长，否则表格会静默错位"
            );

            IndicatorColumn {
                spec: *spec,
                name: spec.name(),
                values,
            }
        })
        .collect();

    Ok(columns)
}

/// 用最后 `window` 个收益率（需要 `window + 1` 根收盘价）算已实现波动率。
///
/// 与 `live.rs` 里那个同名函数长得像，但**不是同一份**：那个是给实盘快照用的，
/// 只取尾部一次。这里要对每一根都调一次，所以必须自己带足长度检查。
fn realized_vol_over_window(closes: &[f64], window: usize, periods_per_year: f64) -> Option<f64> {
    if window == 0 || closes.len() < window + 1 {
        return None;
    }
    indicators::realized_vol(&closes[closes.len() - 1 - window..], periods_per_year)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3_600_000;

    /// 造一段确定性的 K 线：收盘价按正弦波动，避免「一条直线」让 RSI 之类退化。
    fn candles(count: usize) -> Vec<Candle> {
        (0..count)
            .map(|index| {
                let phase = index as f64 / 7.0;
                let close = 100.0 + (phase.sin() * 5.0) + (index as f64 * 0.05);
                let open = close - 0.3;
                Candle {
                    ts: 1_700_000_000_000 + index as i64 * HOUR,
                    open,
                    high: close.max(open) + 0.4,
                    low: close.min(open) - 0.4,
                    close,
                    vol: 1_000.0 + index as f64,
                    confirm: true,
                }
            })
            .collect()
    }

    fn spec(kind: IndicatorKind, period: u32) -> IndicatorSpec {
        IndicatorSpec { kind, period }
    }

    /// 这是本模块最重要的一条：列长度与 K 线根数**不等**时，
    /// 表格里的指标会整体错位一根，而数据看起来完全正常。
    #[test]
    fn every_column_is_as_long_as_the_candles() {
        let data = candles(120);
        let specs = vec![
            spec(IndicatorKind::Ema, 20),
            spec(IndicatorKind::Rsi, 14),
            spec(IndicatorKind::Atr, 14),
            spec(IndicatorKind::RealizedVol, 24),
        ];

        let columns = compute(&data, &specs, "1H").expect("应能计算");
        assert_eq!(columns.len(), specs.len());
        for column in &columns {
            assert_eq!(
                column.values.len(),
                data.len(),
                "{} 的列长度与 K 线不一致",
                column.name
            );
        }
    }

    /// 头部不可得、之后全部可算——而且「不可得」用的是观测值，不是猜的。
    #[test]
    fn leading_bars_are_unavailable_and_the_rest_are_filled() {
        let data = candles(60);
        let columns = compute(&data, &[spec(IndicatorKind::Ema, 20)], "1H").expect("应能计算");
        let column = &columns[0];

        assert_eq!(column.insufficient_bars(), 19, "EMA20 的第 20 根才有值");
        assert!(!column.is_unavailable());
        assert_eq!(column.interior_missing(), 0, "中间不该有洞");
        assert!(column.values[18].is_none());
        assert!(column.values[19].is_some());
    }

    /// 滑动窗口的最后一个值必须与「只算最新一根」的纯函数完全一致。
    /// 不一致就说明窗口切错了，而那种错误会静默地给出偏掉的指标。
    #[test]
    fn last_value_matches_the_single_shot_function() {
        let data = candles(80);
        let closes: Vec<f64> = data.iter().map(|candle| candle.close).collect();
        let specs = vec![
            spec(IndicatorKind::Ema, 20),
            spec(IndicatorKind::Rsi, 14),
            spec(IndicatorKind::Atr, 14),
            spec(IndicatorKind::RealizedVol, 24),
        ];

        let columns = compute(&data, &specs, "1H").expect("应能计算");

        assert_eq!(
            columns[0].values.last().copied().flatten(),
            indicators::ema(&closes, 20)
        );
        assert_eq!(
            columns[1].values.last().copied().flatten(),
            indicators::rsi_wilder(&closes, 14)
        );
        assert_eq!(
            columns[2].values.last().copied().flatten(),
            indicators::atr_pct(&data, 14)
        );
        assert_eq!(
            columns[3].values.last().copied().flatten(),
            realized_vol_over_window(&closes, 24, periods_per_year("1H").expect("应能换算"))
        );
    }

    /// 自定义周期必须真的生效——这正是选择「开放自定义周期」的意义所在。
    #[test]
    fn custom_periods_change_the_values() {
        let data = candles(120);
        let fast = compute(&data, &[spec(IndicatorKind::Ema, 20)], "1H").expect("应能计算");
        let slow = compute(&data, &[spec(IndicatorKind::Ema, 34)], "1H").expect("应能计算");

        assert_eq!(fast[0].name, "EMA20");
        assert_eq!(slow[0].name, "EMA34");
        assert_eq!(slow[0].insufficient_bars(), 33, "周期越大，头部越长");
        assert_ne!(
            fast[0].values.last().copied().flatten(),
            slow[0].values.last().copied().flatten(),
            "不同周期的 EMA 不该给出同一个值"
        );
    }

    /// 根数比指标需要的样本还少时，整列不可得——绝不能悄悄给个 0。
    #[test]
    fn whole_column_is_unavailable_when_history_is_too_short() {
        let data = candles(5);
        let columns = compute(&data, &[spec(IndicatorKind::Ema, 20)], "1H").expect("应能计算");

        assert!(columns[0].is_unavailable());
        assert_eq!(columns[0].insufficient_bars(), 5);
        assert!(columns[0].values.iter().all(Option::is_none));
    }

    #[test]
    fn spec_validation_bounds_period_and_count() {
        assert!(
            validate_specs(&[spec(IndicatorKind::Ema, 0)]).is_err(),
            "周期 0 应被拒绝"
        );
        assert!(
            validate_specs(&[spec(IndicatorKind::Ema, MAX_INDICATOR_PERIOD + 1)]).is_err(),
            "超上限周期应被拒绝"
        );
        assert!(
            validate_specs(&[spec(IndicatorKind::Ema, MAX_INDICATOR_PERIOD)]).is_ok(),
            "刚好到上限应通过"
        );

        let too_many: Vec<IndicatorSpec> = (1..=MAX_INDICATORS as u32 + 1)
            .map(|period| spec(IndicatorKind::Ema, period))
            .collect();
        assert!(validate_specs(&too_many).is_err());

        let duplicates = vec![
            spec(IndicatorKind::Ema, 20),
            spec(IndicatorKind::Ema, 20),
            spec(IndicatorKind::Rsi, 14),
        ];
        let unique = validate_specs(&duplicates).expect("应通过");
        assert_eq!(unique.len(), 2, "重复项只保留一个，且不该因此报「选太多」");
    }

    /// IPC 入参名与上下文元数据名必须是同一个字符串。
    ///
    /// 这里曾经不一致：`Atr` 的默认 serde 名是 `atr`，而 `key()` 返回 `atr_pct`。
    /// 前端按 `atr_pct` 查文案就会拿到 `undefined`——而这类不一致**不会报错**，
    /// 只会让界面少一段文字。
    #[test]
    fn wire_name_matches_the_metadata_key() {
        for (kind, expected) in [
            (IndicatorKind::Ema, "ema"),
            (IndicatorKind::Rsi, "rsi"),
            (IndicatorKind::Atr, "atr_pct"),
            (IndicatorKind::RealizedVol, "realized_vol"),
        ] {
            assert_eq!(kind.key(), expected, "key() 与约定的取值不一致");

            let json = serde_json::to_string(&spec(kind, 14)).expect("应能序列化");
            assert!(
                json.contains(&format!("\"kind\":\"{expected}\"")),
                "{expected} 的序列化名与 key() 不一致：{json}"
            );
        }
    }

    #[test]
    fn annualization_factor_follows_the_bar() {
        assert_eq!(periods_per_year("1H"), Some(8_760.0));
        assert_eq!(periods_per_year("1D"), Some(365.0));
        assert!(periods_per_year("bogus").is_none());
        assert!(periods_per_year("1W").is_some_and(|value| (52.0..53.0).contains(&value)));
    }

    /// 未知粒度必须报错，不能退回一个默认年化系数——
    /// 那会让数字看起来合理但完全错误。
    #[test]
    fn compute_rejects_unknown_bars() {
        let data = candles(30);
        assert!(compute(&data, &[spec(IndicatorKind::Ema, 5)], "bogus").is_err());
    }

    #[test]
    fn empty_candles_yield_empty_columns() {
        let columns = compute(&[], &[spec(IndicatorKind::Ema, 20)], "1H").expect("应能计算");
        assert!(columns[0].values.is_empty());
        assert!(!columns[0].is_unavailable(), "空输入不算「整列不可得」");
    }
}
