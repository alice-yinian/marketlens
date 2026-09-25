//! OKX 公开端点的响应模型。
//!
//! 字段映射用 `rename_all = "camelCase"`（OKX 用 `instId` / `markPx` / `volCcy24h`）。
//! 每个数值字段都经 `de::*` 解析，因为 OKX 传的是字符串、且用空串表示缺失。
//!
//! 这些映射不是靠文档抄来的——文件末尾的测试直接使用**真机抓取的响应样本**，
//! 任何字段名或类型映射错误都会让测试失败。

// 这些结构体是**外部 API 的线格式契约**，刻意完整建模而不是只留当前用到的字段：
// 只建模一半，等到 M2/M3 需要 `settFundingRate`、`oi`、`vol24h` 时又要回去补字段名，
// 而字段名一旦抄错就是静默的错数据。未被读取的字段由反序列化测试覆盖，不是死代码。
#![allow(dead_code)]

use serde::Deserialize;

use crate::okx::de;

/// 统一响应信封。OKX 用字符串 `code` 表示成败，只有 `"0"` 是成功。
///
/// 显式声明泛型 bound：`data` 上的 `#[serde(default)]` 会让 serde 的 derive
/// 给 `T` 加上 `Default` 约束，而业务类型不该为了能反序列化而去实现 `Default`。
#[derive(Debug, Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
pub struct Envelope<T> {
    pub code: String,
    #[serde(default)]
    pub msg: String,
    #[serde(default)]
    pub data: Vec<T>,
}

/// `GET /api/v5/market/ticker`
///
/// 注意：实测该响应**不含 `idxPx`**，基差需另取 `mark-price` 与 `index-tickers`。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ticker {
    pub inst_id: String,
    #[serde(deserialize_with = "de::num")]
    pub last: f64,
    #[serde(deserialize_with = "de::num")]
    pub open_24h: f64,
    #[serde(deserialize_with = "de::num")]
    pub high_24h: f64,
    #[serde(deserialize_with = "de::num")]
    pub low_24h: f64,
    /// 24h 成交量，**单位是张数**（合约）
    ///
    /// 真机核对：`vol24h / ctVal` 恰好等于 `volCcy24h`（9047720 ÷ 100 × 0.01 = 90477.2），
    /// 说明它是以合约张计数的，不是币也不是 USDT。
    #[serde(deserialize_with = "de::num")]
    pub vol_24h: f64,
    /// 24h 成交量，**单位是基础币**（如 BTC）
    ///
    /// 注意不是计价币：USD 成交额必须再乘价格，见 `MarketState::volume_24h_usd`。
    #[serde(deserialize_with = "de::num")]
    pub vol_ccy_24h: f64,
    #[serde(deserialize_with = "de::ts")]
    pub ts: i64,
}

/// `GET /api/v5/public/funding-rate`
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FundingRate {
    pub inst_id: String,
    #[serde(deserialize_with = "de::num")]
    pub funding_rate: f64,
    /// 实测经常是空串（本期尚未确定），所以必须是 Option
    #[serde(deserialize_with = "de::opt_num", default)]
    pub next_funding_rate: Option<f64>,
    #[serde(deserialize_with = "de::opt_ts", default)]
    pub next_funding_time: Option<i64>,
    #[serde(deserialize_with = "de::opt_num", default)]
    pub sett_funding_rate: Option<f64>,
    #[serde(deserialize_with = "de::ts")]
    pub ts: i64,
}

/// `GET /api/v5/public/open-interest`
///
/// 该端点直接提供 `oiUsd`，名义价值无需自行换算。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenInterest {
    pub inst_id: String,
    #[serde(deserialize_with = "de::num")]
    pub oi: f64,
    #[serde(deserialize_with = "de::num")]
    pub oi_ccy: f64,
    #[serde(deserialize_with = "de::num")]
    pub oi_usd: f64,
    #[serde(deserialize_with = "de::ts")]
    pub ts: i64,
}

/// `GET /api/v5/public/mark-price`
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkPrice {
    pub inst_id: String,
    #[serde(deserialize_with = "de::num")]
    pub mark_px: f64,
    #[serde(deserialize_with = "de::ts")]
    pub ts: i64,
}

/// `GET /api/v5/market/index-tickers`
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexTicker {
    pub inst_id: String,
    #[serde(deserialize_with = "de::num")]
    pub idx_px: f64,
    #[serde(deserialize_with = "de::ts")]
    pub ts: i64,
}

/// 从 `BTC-USDT-SWAP` 推出对应的指数 instId `BTC-USDT`。
pub fn index_inst_id(swap_inst_id: &str) -> Option<String> {
    swap_inst_id
        .strip_suffix("-SWAP")
        .map(std::string::ToString::to_string)
}

/// Rubik 端点返回「字符串数组的数组」，需要一个点对点解析器。
///
/// `long-short-account-ratio` 的形状是 `[[ts, ratio], ...]`，最新的点在最前。
pub fn parse_single_value_series(rows: &[Vec<String>]) -> Vec<(i64, f64)> {
    rows.iter()
        .filter_map(|row| {
            let ts = row.first()?.parse::<i64>().ok()?;
            let value = row.get(1)?.parse::<f64>().ok()?;
            Some((ts, value))
        })
        .collect()
}

/// `taker-volume` 的形状是 `[[ts, sellVol, buyVol], ...]`。
///
/// ⚠️ 元素顺序取自官方文档描述，未能在真机上独立验证方向语义——
/// 因此只用于展示比值，不用于任何方向性判断。
pub fn parse_taker_series(rows: &[Vec<String>]) -> Vec<(i64, f64, f64)> {
    rows.iter()
        .filter_map(|row| {
            let ts = row.first()?.parse::<i64>().ok()?;
            let sell = row.get(1)?.parse::<f64>().ok()?;
            let buy = row.get(2)?.parse::<f64>().ok()?;
            Some((ts, sell, buy))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 以下 JSON 全部是 2026-09-25 对 www.okx.com 的真实响应（截取），
    /// 不是手写的理想化样本——字段名写错、类型判断错都会在这里失败。

    #[test]
    fn ticker_maps_real_response() {
        let raw = r#"{"code":"0","data":[{"instType":"SWAP","instId":"BTC-USDT-SWAP",
            "last":"83789.7","lastSz":"2","askPx":"83789.7","bidPx":"83789.6",
            "open24h":"84390","high24h":"84931.3","low24h":"82812.5",
            "volCcy24h":"90477.2008","vol24h":"9047720.08","ts":"1790322078168",
            "sodUtc0":"84369.4","sodUtc8":"84383.2"}],"msg":""}"#;

        let env: Envelope<Ticker> = serde_json::from_str(raw).expect("反序列化失败");
        assert_eq!(env.code, "0");
        let t = &env.data[0];
        assert_eq!(t.inst_id, "BTC-USDT-SWAP");
        assert!((t.last - 83_789.7).abs() < 1e-9);
        assert!((t.open_24h - 84_390.0).abs() < 1e-9);
        assert!((t.high_24h - 84_931.3).abs() < 1e-9);
        assert!((t.low_24h - 82_812.5).abs() < 1e-9);
        assert!((t.vol_ccy_24h - 90_477.200_8).abs() < 1e-9);
        assert_eq!(t.ts, 1_790_322_078_168);
    }

    #[test]
    fn funding_rate_handles_empty_next_rate() {
        let raw = r#"{"code":"0","data":[{"formulaType":"withRate",
            "fundingRate":"0.0000731616566882","fundingTime":"1790323200000",
            "instId":"BTC-USDT-SWAP","instType":"SWAP","nextFundingRate":"",
            "nextFundingTime":"1790352000000","settFundingRate":"-0.0000037995722708",
            "settState":"settled","ts":"1790322065942"}],"msg":""}"#;

        let env: Envelope<FundingRate> = serde_json::from_str(raw).expect("反序列化失败");
        let f = &env.data[0];
        assert!((f.funding_rate - 0.0000731616566882).abs() < 1e-18);
        // 空串必须变成 None，而不是解析失败或 0
        assert_eq!(f.next_funding_rate, None);
        assert_eq!(f.next_funding_time, Some(1_790_352_000_000));
        assert!(f.sett_funding_rate.is_some());
    }

    #[test]
    fn open_interest_exposes_named_usd_value() {
        let raw = r#"{"code":"0","data":[{"instId":"BTC-USD-SWAP","instType":"SWAP",
            "oi":"5367960.69999999956","oiCcy":"6404.7868018427016276",
            "oiUsd":"536796069.999999956","ts":"1790322080511"}],"msg":""}"#;

        let env: Envelope<OpenInterest> = serde_json::from_str(raw).expect("反序列化失败");
        let oi = &env.data[0];
        assert_eq!(oi.inst_id, "BTC-USD-SWAP");
        assert!((oi.oi_usd - 536_796_070.0).abs() < 1.0);
    }

    #[test]
    fn mark_price_and_index_ticker_supply_basis_inputs() {
        let mark = r#"{"code":"0","data":[{"instId":"BTC-USDT-SWAP","instType":"SWAP",
            "markPx":"83792.5","ts":"1790322100602"}],"msg":""}"#;
        let index = r#"{"code":"0","data":[{"instId":"BTC-USDT","idxPx":"83832.3",
            "high24h":"84936.7","low24h":"82879.9","open24h":"84436.1","ts":"1790322100538"}],"msg":""}"#;

        let m: Envelope<MarkPrice> = serde_json::from_str(mark).unwrap();
        let i: Envelope<IndexTicker> = serde_json::from_str(index).unwrap();

        assert!((m.data[0].mark_px - 83_792.5).abs() < 1e-9);
        assert!((i.data[0].idx_px - 83_832.3).abs() < 1e-9);
        assert_eq!(index_inst_id("BTC-USDT-SWAP").as_deref(), Some("BTC-USDT"));
        assert_eq!(index_inst_id("BTC-USDT"), None);
    }

    #[test]
    fn rubik_series_parse_from_nested_string_arrays() {
        let rows: Vec<Vec<String>> = vec![
            vec!["1790322000000".into(), "1.24".into()],
            vec!["1790321700000".into(), "1.25".into()],
        ];
        let parsed = parse_single_value_series(&rows);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].0, 1_790_322_000_000);
        assert!((parsed[0].1 - 1.24).abs() < 1e-9);

        let taker: Vec<Vec<String>> = vec![vec![
            "1790322000000".into(),
            "22215751.7021".into(),
            "31869912.8624".into(),
        ]];
        let parsed = parse_taker_series(&taker);
        assert_eq!(parsed.len(), 1);
        assert!((parsed[0].1 - 22_215_751.702_1).abs() < 1e-3);
        assert!((parsed[0].2 - 31_869_912.862_4).abs() < 1e-3);
    }

    #[test]
    fn malformed_rows_are_skipped_not_fatal() {
        let rows: Vec<Vec<String>> = vec![
            vec!["1790322000000".into(), "1.24".into()],
            vec!["not-a-number".into(), "1.25".into()],
            vec!["1790321700000".into()],
            vec![],
        ];
        // 一个坏行不应该让整段序列消失
        assert_eq!(parse_single_value_series(&rows).len(), 1);
    }
}
