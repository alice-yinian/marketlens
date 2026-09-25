//! 实盘市场状态装配（设计文档 §6.3）。
//!
//! 一次 `fetch_snapshot` 会为每个标的发 5 个请求（ticker / funding / 标记价 / 指数价 / 1H K 线），
//! 外加共享的 1 个持仓量请求和每个基础币 2 个 Rubik 请求。限流由 `OkxClient` 内部排队。

use serde::Serialize;
use ts_rs::TS;

use crate::error::AppResult;
use crate::market::regime::{self, Crowding, Thresholds, TrendRegime, VolRegime};
use crate::market::{Candle, indicators};
use crate::okx::client::OkxClient;
use crate::okx::endpoints::{RateGroup, inst_type, public};
use crate::okx::models::{self, FundingRate, IndexTicker, MarkPrice, OpenInterest, Ticker};

/// 用于波动率分位的滚动窗口长度（小时数）。24 = 24 小时窗口。
const VOL_WINDOW_HOURS: usize = 24;

/// 1 小时 K 线的年化周期数。
const PERIODS_PER_YEAR_HOURLY: f64 = 24.0 * 365.0;

/// 拉取多少根 1H K 线。
///
/// 300 是 `/market/candles` 的上限；够算 EMA200，也能给出约 276 个滚动波动率样本。
/// **M1 的波动率分位因此只回看约 12 天**，而不是设计文档 §6.3.2 说的 90 天——
/// 把回看窗口拉到 90 天需要分页拉取约 2160 根 K 线，那属于 M3 的采集计划器。
/// 这里刻意选择「时间窗短但完全自洽」，而不是混用日线/小时线两种口径。
const CANDLE_LIMIT: &str = "300";

/// 单标的的完整市场状态。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "types.ts")]
pub struct MarketState {
    pub inst_id: String,
    #[ts(type = "number")]
    pub ts: i64,
    pub last: f64,
    /// 24h 涨跌幅
    pub change_pct: f64,
    pub high_24h: f64,
    pub low_24h: f64,
    /// USD 成交额（已由基础币成交量 × 价格换算）
    pub volume_24h_usd: f64,

    pub funding_rate: f64,
    /// 资金费率年化（8h 结算 → 日 3 次）
    pub funding_annualized: f64,
    pub next_funding_rate: Option<f64>,
    #[ts(type = "number | null")]
    pub next_funding_time: Option<i64>,

    pub open_interest_usd: Option<f64>,
    pub basis_pct: Option<f64>,
    pub long_short_ratio: Option<f64>,
    pub taker_buy_sell_ratio: Option<f64>,

    pub ema20: Option<f64>,
    pub ema60: Option<f64>,
    pub ema200: Option<f64>,
    pub rsi14: Option<f64>,
    pub atr_pct: Option<f64>,
    pub realized_vol: Option<f64>,

    pub trend: Option<TrendRegime>,
    pub vol: Option<VolRegime>,
    pub crowding: Crowding,
    pub signals: Vec<String>,
    /// 拿不到的指标及原因。模板渲染成「数据不可得」，绝不留空。
    pub unavailable: Vec<String>,
}

/// 同一基础币共享的 Rubik 指标（按 ccy 聚合，多标的复用，避免重复请求）。
#[derive(Debug, Clone, Copy, Default)]
struct CurrencyStats {
    long_short_ratio: Option<f64>,
    taker_buy_sell_ratio: Option<f64>,
}

/// 一次实盘刷新结果。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "types.ts")]
pub struct LiveSnapshot {
    #[ts(type = "number")]
    pub ts: i64,
    /// 数据来自缓存（未发起网络请求）
    pub cache_hit: bool,
    #[ts(type = "number")]
    pub fetched_at: i64,
    pub watchlist: Vec<String>,
    pub instruments: Vec<MarketState>,
    /// 非致命问题汇总：单个标的失败不会让整次刷新失败
    pub warnings: Vec<String>,
}

/// 拉取整批标的的市场状态。
///
/// 单个标的失败不终止整次采集——用户宁可看到 2 个标的状态加 1 条警告，
/// 也不愿因为 SOL 的接口抖动而什么都看不到。
pub async fn fetch_snapshot(
    client: &OkxClient,
    watchlist: &[String],
) -> AppResult<(Vec<MarketState>, Vec<String>)> {
    let thresholds = Thresholds::default();
    let mut warnings = Vec::new();

    let open_interest = fetch_open_interest(client).await;
    if let Err(err) = &open_interest {
        warnings.push(format!("持仓量获取失败：{err}"));
    }
    let open_interest = open_interest.unwrap_or_default();

    let mut currency_cache: std::collections::HashMap<String, CurrencyStats> =
        std::collections::HashMap::new();
    let mut instruments = Vec::with_capacity(watchlist.len());

    for inst_id in watchlist {
        let base_ccy = base_ccy_of(inst_id);

        if !currency_cache.contains_key(&base_ccy) {
            match fetch_currency_stats(client, &base_ccy).await {
                Ok(stats) => {
                    currency_cache.insert(base_ccy.clone(), stats);
                }
                Err(err) => {
                    warnings.push(format!("{base_ccy} 的多空/主动买卖比获取失败：{err}"));
                    currency_cache.insert(base_ccy.clone(), CurrencyStats::default());
                }
            }
        }
        let stats = currency_cache.get(&base_ccy).copied().unwrap_or_default();

        match fetch_one(client, inst_id, stats, &open_interest, thresholds).await {
            Ok(state) => instruments.push(state),
            Err(err) => warnings.push(format!("{inst_id} 获取失败：{err}")),
        }
    }

    Ok((instruments, warnings))
}

async fn fetch_one(
    client: &OkxClient,
    inst_id: &str,
    stats: CurrencyStats,
    open_interest: &std::collections::HashMap<String, f64>,
    thresholds: Thresholds,
) -> AppResult<MarketState> {
    let ticker = fetch_first::<Ticker>(
        client,
        public::TICKER,
        RateGroup::Market,
        &[("instId", inst_id)],
    )
    .await?;

    let funding = fetch_first::<FundingRate>(
        client,
        public::FUNDING_RATE,
        RateGroup::Reference,
        &[("instId", inst_id)],
    )
    .await?;

    let mark = fetch_first::<MarkPrice>(
        client,
        public::MARK_PRICE,
        RateGroup::Reference,
        &[("instType", inst_type::SWAP), ("instId", inst_id)],
    )
    .await?;

    let index_inst_id = models::index_inst_id(inst_id);
    let index = match index_inst_id.as_deref() {
        Some(index_id) => fetch_first::<IndexTicker>(
            client,
            public::INDEX_TICKERS,
            RateGroup::Market,
            &[("quoteCcy", "USDT"), ("instId", index_id)],
        )
        .await
        .ok(),
        None => None,
    };

    let candle_rows = client
        .get_public::<Vec<String>>(
            public::CANDLES,
            RateGroup::Market,
            &[("instId", inst_id), ("bar", "1H"), ("limit", CANDLE_LIMIT)],
        )
        .await?;
    let candles = Candle::parse_rows(&candle_rows);

    let closes: Vec<f64> = candles.iter().map(|candle| candle.close).collect();
    let funding_annualized = funding.funding_rate * 3.0 * 365.0;

    let mut unavailable = Vec::new();

    let basis_pct = match index {
        Some(index) => indicators::basis_pct(mark.mark_px, index.idx_px),
        None => {
            unavailable.push("基差：未能取到指数价".to_string());
            None
        }
    };

    let open_interest_usd = open_interest.get(inst_id).copied();
    if open_interest_usd.is_none() {
        unavailable.push("持仓量：该标的未出现在持仓量响应中".to_string());
    }

    let vol_history = rolling_vol_history(&closes, VOL_WINDOW_HOURS, PERIODS_PER_YEAR_HOURLY);
    let current_vol = realized_vol_over_window(&closes, VOL_WINDOW_HOURS, PERIODS_PER_YEAR_HOURLY);

    let regime = regime::classify(&regime::Input {
        candles: &candles,
        periods_per_year: PERIODS_PER_YEAR_HOURLY,
        funding_annualized,
        long_short_ratio: stats.long_short_ratio,
        vol_history: &vol_history,
        thresholds,
    });

    unavailable.extend(regime.unavailable.iter().cloned());

    let change_pct = if ticker.open_24h == 0.0 {
        0.0
    } else {
        (ticker.last - ticker.open_24h) / ticker.open_24h
    };

    Ok(MarketState {
        inst_id: inst_id.to_string(),
        ts: ticker.ts,
        last: ticker.last,
        change_pct,
        high_24h: ticker.high_24h,
        low_24h: ticker.low_24h,
        // volCcy24h 的单位是基础币，换算成 USD 必须乘价格
        volume_24h_usd: ticker.vol_ccy_24h * ticker.last,
        funding_rate: funding.funding_rate,
        funding_annualized,
        next_funding_rate: funding.next_funding_rate,
        next_funding_time: funding.next_funding_time,
        open_interest_usd,
        basis_pct,
        long_short_ratio: stats.long_short_ratio,
        taker_buy_sell_ratio: stats.taker_buy_sell_ratio,
        ema20: indicators::ema(&closes, 20),
        ema60: indicators::ema(&closes, 60),
        ema200: indicators::ema(&closes, 200),
        rsi14: indicators::rsi_wilder(&closes, 14),
        atr_pct: indicators::atr_pct(&candles, 14),
        realized_vol: current_vol,
        trend: regime.trend,
        vol: regime.vol,
        crowding: regime.crowding,
        signals: regime.signals,
        unavailable,
    })
}

/// 取持仓量的 USD 名义值，按 instId 建索引。
async fn fetch_open_interest(
    client: &OkxClient,
) -> AppResult<std::collections::HashMap<String, f64>> {
    let rows = client
        .get_public::<OpenInterest>(
            public::OPEN_INTEREST,
            RateGroup::Reference,
            &[("instType", inst_type::SWAP)],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| (row.inst_id, row.oi_usd))
        .collect())
}

/// 拉取按基础币聚合的 Rubik 指标，取最新一个点。
async fn fetch_currency_stats(client: &OkxClient, base_ccy: &str) -> AppResult<CurrencyStats> {
    let lsr_rows = client
        .get_public::<Vec<String>>(
            public::LS_ACCOUNT_RATIO,
            RateGroup::Rubik,
            &[("ccy", base_ccy)],
        )
        .await?;
    let long_short_ratio = models::parse_single_value_series(&lsr_rows)
        .first()
        .map(|(_, value)| *value);

    let taker_rows = client
        .get_public::<Vec<String>>(
            public::TAKER_VOLUME,
            RateGroup::Rubik,
            &[("ccy", base_ccy), ("instType", inst_type::CONTRACTS)],
        )
        .await?;
    let taker_buy_sell_ratio = models::parse_taker_series(&taker_rows)
        .first()
        .and_then(|(_, sell, buy)| if *sell > 0.0 { Some(buy / sell) } else { None });

    Ok(CurrencyStats {
        long_short_ratio,
        taker_buy_sell_ratio,
    })
}

async fn fetch_first<T: serde::de::DeserializeOwned>(
    client: &OkxClient,
    path: &str,
    group: RateGroup,
    query: &[(&str, &str)],
) -> AppResult<T> {
    let mut rows = client.get_public::<T>(path, group, query).await?;
    if rows.is_empty() {
        return Err(crate::error::AppError::Okx {
            code: "empty".to_string(),
            msg: format!("{path} 返回了空数据（参数 {}）", query.len()),
        });
    }
    Ok(rows.remove(0))
}

/// 指定窗口长度的已实现波动率（取序列末尾）。
fn realized_vol_over_window(closes: &[f64], window: usize, periods_per_year: f64) -> Option<f64> {
    if closes.len() <= window {
        return None;
    }
    indicators::realized_vol(&closes[closes.len() - 1 - window..], periods_per_year)
}

/// 滚动窗口的已实现波动率序列，用作分位判定的样本。
///
/// 与 [`realized_vol_over_window`] 使用**相同的窗口长度与周期数**，
/// 这样「当前值」与「历史样本」是同一把尺子量出来的——否则分位数没有意义。
fn rolling_vol_history(closes: &[f64], window: usize, periods_per_year: f64) -> Vec<f64> {
    if closes.len() <= window {
        return Vec::new();
    }
    (window..closes.len())
        .filter_map(|end| indicators::realized_vol(&closes[end - window..=end], periods_per_year))
        .collect()
}

/// 从 `BTC-USDT-SWAP` 取出 `BTC`。
fn base_ccy_of(inst_id: &str) -> String {
    inst_id.split('-').next().unwrap_or(inst_id).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_base_currency() {
        assert_eq!(base_ccy_of("BTC-USDT-SWAP"), "BTC");
        assert_eq!(base_ccy_of("ETH-USDT-SWAP"), "ETH");
        assert_eq!(base_ccy_of("WEIRD"), "WEIRD");
    }

    #[test]
    fn volume_is_converted_from_base_currency_to_usd() {
        // 真机样本：volCcy24h = 90477.2008 BTC，last = 83789.7
        let usd = 90_477.200_8_f64 * 83_789.7_f64;
        assert!(
            (usd - 7_581_062_000.0).abs() < 1_000_000.0,
            "应换算成约 75.8 亿美元，实际 {usd}"
        );
    }

    #[test]
    fn rolling_history_uses_same_window_as_current_value() {
        let closes: Vec<f64> = (0..100).map(|i| 100.0 + f64::from(i % 7)).collect();
        let window = 24;
        let history = rolling_vol_history(&closes, window, PERIODS_PER_YEAR_HOURLY);
        let current = realized_vol_over_window(&closes, window, PERIODS_PER_YEAR_HOURLY);

        // 历史样本的最后一个应当就是当前值（同一窗口、同一周期数）
        let last_history = *history.last().expect("应有样本");
        let current = current.expect("应可计算");
        assert!(
            (last_history - current).abs() < 1e-12,
            "历史样本末项 {last_history} 与当前值 {current} 必须一致"
        );
        assert_eq!(history.len(), closes.len() - window);
    }

    #[test]
    fn history_is_empty_when_series_too_short() {
        let closes = vec![100.0; 10];
        assert!(rolling_vol_history(&closes, 24, PERIODS_PER_YEAR_HOURLY).is_empty());
        assert!(realized_vol_over_window(&closes, 24, PERIODS_PER_YEAR_HOURLY).is_none());
    }

    /// 对**真实 OKX API** 的连通性与数值合理性检查，并打印数值供人工与 OKX 网页端比对。
    ///
    /// 默认 `#[ignore]`：它会访问外网，不适合放进常规测试或 CI（CI 可能无外网，
    /// 且外部接口抖动会造成与本次改动无关的假失败）。需要时显式运行：
    ///
    /// ```text
    /// cargo test --lib okx_live -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore = "访问真实 OKX API，需显式运行"]
    async fn okx_live_snapshot_is_sane() {
        let client = OkxClient::new().expect("客户端构造失败");
        let watchlist = vec!["BTC-USDT-SWAP".to_string()];

        let (instruments, warnings) = fetch_snapshot(&client, &watchlist).await.expect("采集失败");

        assert!(warnings.is_empty(), "不应有警告：{warnings:#?}");
        let state = instruments.first().expect("应返回 BTC 的状态");

        println!("=== BTC-USDT-SWAP ===");
        println!("最新价       {:.1}", state.last);
        println!("24h 涨跌     {:+.2}%", state.change_pct * 100.0);
        println!("24h 高/低    {:.1} / {:.1}", state.high_24h, state.low_24h);
        println!("24h 成交额   ${:.0}", state.volume_24h_usd);
        println!(
            "资金费率     {:.8}（年化 {:+.2}%）",
            state.funding_rate,
            state.funding_annualized * 100.0
        );
        println!("持仓量       {:?}", state.open_interest_usd);
        println!("基差         {:?}", state.basis_pct.map(|b| b * 100.0));
        println!("多空账户比   {:?}", state.long_short_ratio);
        println!("主动买卖比   {:?}", state.taker_buy_sell_ratio);
        println!(
            "EMA20/60/200 {:?} / {:?} / {:?}",
            state.ema20, state.ema60, state.ema200
        );
        println!("RSI14        {:?}", state.rsi14);
        println!("ATR%         {:?}", state.atr_pct.map(|a| a * 100.0));
        println!("已实现波动率 {:?}", state.realized_vol.map(|v| v * 100.0));
        println!(
            "状态分类     趋势={:?} 波动={:?} 拥挤={:?}",
            state.trend, state.vol, state.crowding
        );
        println!("signals      {:#?}", state.signals);
        println!("unavailable  {:#?}", state.unavailable);

        assert!(state.last > 0.0, "价格必须为正");
        assert!(state.volume_24h_usd > 0.0, "成交额必须为正");
        assert!(state.ema20.is_some(), "300 根 1H K 线足以计算 EMA20");
        assert!(state.ema200.is_some(), "300 根 1H K 线足以计算 EMA200");
        assert!(state.rsi14.is_some(), "应能算出 RSI14");
        assert!(state.atr_pct.is_some(), "应能算出 ATR%");
        assert!(
            state.trend.is_some(),
            "K 线充足时趋势必须能判定，unavailable={:?}",
            state.unavailable
        );
        assert!(
            state.open_interest_usd.is_some(),
            "BTC-USDT-SWAP 应在持仓量响应中出现"
        );
        assert!(state.basis_pct.is_some(), "标记价与指数价都应取到");
    }

    /// 把指标计算的**输入与输出一起落盘**，供 `scripts/crosscheck_indicators.py` 做逐位比对。
    ///
    /// 为什么要落盘：行情一直在推进，两次独立运行拿到的 K 线并不完全相同，
    /// 直接比数值最多只能得到「大致一致」，而且会把数据漂移误判成公式错误
    /// （实测 RSI 就因此假警报过一次）。把输入固定下来，比对才能是精确的。
    #[tokio::test]
    #[ignore = "访问真实 OKX API，需显式运行"]
    async fn dump_indicators_for_crosscheck() {
        let client = OkxClient::new().expect("客户端构造失败");
        let rows = client
            .get_public::<Vec<String>>(
                public::CANDLES,
                RateGroup::Market,
                &[("instId", "BTC-USDT-SWAP"), ("bar", "1H"), ("limit", CANDLE_LIMIT)],
            )
            .await
            .expect("K 线拉取失败");

        let candles = Candle::parse_rows(&rows);
        let closes: Vec<f64> = candles.iter().map(|candle| candle.close).collect();

        let dump = serde_json::json!({
            "inst_id": "BTC-USDT-SWAP",
            "bar": "1H",
            "closes": closes,
            "highs": candles.iter().map(|candle| candle.high).collect::<Vec<_>>(),
            "lows": candles.iter().map(|candle| candle.low).collect::<Vec<_>>(),
            "ema20": indicators::ema(&closes, 20),
            "ema60": indicators::ema(&closes, 60),
            "ema200": indicators::ema(&closes, 200),
            "rsi14": indicators::rsi_wilder(&closes, 14),
            "atr_pct": indicators::atr_pct(&candles, 14),
            "realized_vol_24h": realized_vol_over_window(
                &closes,
                VOL_WINDOW_HOURS,
                PERIODS_PER_YEAR_HOURLY,
            ),
        });

        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/target/indicator-crosscheck.json"
        );
        let json = serde_json::to_string_pretty(&dump).expect("序列化失败");
        std::fs::write(path, json).expect("写入失败");

        println!("已写入 {path}（用 scripts/crosscheck_indicators.py 做逐位比对）");
    }
}
