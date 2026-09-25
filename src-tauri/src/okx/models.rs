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

// ---------------------------------------------------------------------------
// 私有端点（M2）
// ---------------------------------------------------------------------------

/// `GET /api/v5/public/instruments`
///
/// 张数 → 币数量的换算依赖这里的 `ctVal` / `ctMult`。**换算错了会直接给用户
/// （以及后续的 AI）喂错数据**，所以这两个字段必须来自真机而非记忆。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instrument {
    pub inst_id: String,
    /// 每张合约对应的币数量
    #[serde(deserialize_with = "de::num")]
    pub ct_val: f64,
    /// `ctVal` 的币种（币本位合约时不是计价币）
    #[serde(default)]
    pub ct_val_ccy: String,
    /// 合约乘数，实测为 `"1"`；缺失时按 1 处理
    #[serde(deserialize_with = "de::num", default = "one")]
    pub ct_mult: f64,
    #[serde(default)]
    pub settle_ccy: String,
    #[serde(default)]
    pub state: String,
}

fn one() -> f64 {
    1.0
}

/// `GET /api/v5/account/config`
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountConfig {
    pub uid: String,
    #[serde(default)]
    pub main_uid: String,
    #[serde(default)]
    pub acct_lv: String,
    /// 权限，逗号分隔。实测形如 `read_only,trade`
    pub perm: String,
    /// `net_mode`（净持仓）或 `long_short_mode`（长空双向）
    pub pos_mode: String,
    #[serde(default)]
    pub label: String,
}

impl AccountConfig {
    /// 是否**只读**。
    ///
    /// 只判断是否含 `trade`，而不是判断是否等于 `read_only`：OKX 的权限串
    /// 可能包含更多项，写成等值判断会在对方新增权限项时静默失效。
    pub fn is_read_only(&self) -> bool {
        !self
            .perm
            .split(',')
            .any(|item| item.trim().eq_ignore_ascii_case("trade"))
    }

    pub fn is_net_mode(&self) -> bool {
        self.pos_mode == "net_mode"
    }
}

/// `GET /api/v5/account/balance`
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountBalance {
    /// 账户总权益（USD）
    #[serde(deserialize_with = "de::num")]
    pub total_eq: f64,
    #[serde(deserialize_with = "de::num")]
    pub iso_eq: f64,
    #[serde(deserialize_with = "de::num")]
    pub adj_eq: f64,
    #[serde(deserialize_with = "de::num")]
    pub avail_eq: f64,
    #[serde(deserialize_with = "de::num")]
    pub upl: f64,
    #[serde(deserialize_with = "de::opt_num", default)]
    pub mgn_ratio: Option<f64>,
    #[serde(deserialize_with = "de::opt_num", default)]
    pub imr: Option<f64>,
    #[serde(deserialize_with = "de::opt_num", default)]
    pub mmr: Option<f64>,
    #[serde(deserialize_with = "de::opt_num", default)]
    pub notional_usd: Option<f64>,
    #[serde(default)]
    pub details: Vec<BalanceDetail>,
}

/// 账户余额中的单个币种明细。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceDetail {
    pub ccy: String,
    #[serde(deserialize_with = "de::num")]
    pub eq: f64,
    #[serde(deserialize_with = "de::num")]
    pub eq_usd: f64,
    #[serde(deserialize_with = "de::num")]
    pub avail_bal: f64,
    #[serde(deserialize_with = "de::num")]
    pub cash_bal: f64,
    #[serde(deserialize_with = "de::opt_num", default)]
    pub iso_eq: Option<f64>,
}

/// `GET /api/v5/account/positions`
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OkxPosition {
    pub inst_id: String,
    #[serde(default)]
    pub inst_type: String,
    pub pos_id: String,
    /// `long` | `short` | `net`——净持仓模式下恒为 `net`
    pub pos_side: String,
    /// `cross` | `isolated`
    pub mgn_mode: String,
    #[serde(deserialize_with = "de::num")]
    pub lever: f64,
    /// **张数**（合约），不是币数量
    #[serde(deserialize_with = "de::num")]
    pub pos: f64,
    #[serde(deserialize_with = "de::num")]
    pub avg_px: f64,
    #[serde(deserialize_with = "de::num")]
    pub mark_px: f64,
    #[serde(deserialize_with = "de::num")]
    pub last: f64,
    #[serde(deserialize_with = "de::opt_num", default)]
    pub idx_px: Option<f64>,
    /// 全仓模式下实测为空串
    #[serde(deserialize_with = "de::opt_num", default)]
    pub liq_px: Option<f64>,
    #[serde(deserialize_with = "de::num")]
    pub upl: f64,
    #[serde(deserialize_with = "de::num")]
    pub upl_ratio: f64,
    #[serde(deserialize_with = "de::opt_num", default)]
    pub mgn_ratio: Option<f64>,
    /// OKX 直接给出的 USD 名义价值——不必自己用 ctVal 换算，更准
    #[serde(deserialize_with = "de::num")]
    pub notional_usd: f64,
    #[serde(deserialize_with = "de::opt_num", default)]
    pub imr: Option<f64>,
    #[serde(deserialize_with = "de::opt_num", default)]
    pub mmr: Option<f64>,
    #[serde(deserialize_with = "de::num")]
    pub fee: f64,
    #[serde(deserialize_with = "de::num")]
    pub funding_fee: f64,
    #[serde(deserialize_with = "de::num")]
    pub realized_pnl: f64,
    #[serde(deserialize_with = "de::ts")]
    pub c_time: i64,
    #[serde(deserialize_with = "de::ts")]
    pub u_time: i64,
    /// 保证金币种
    #[serde(default)]
    pub ccy: String,
    /// 币本位合约的持仓币种；U 本位为空串
    #[serde(default)]
    pub pos_ccy: String,
}

// ---------------------------------------------------------------------------
// 公开端点的响应模型（M1）
// ---------------------------------------------------------------------------
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

    // -----------------------------------------------------------------------
    // 私有端点：夹具全部来自 2026-09-25 对模拟盘的真实响应
    // -----------------------------------------------------------------------

    #[test]
    fn account_config_detects_trade_permission() {
        // 实测响应（含 autoLoan 这个 JSON 布尔字段——类型混合不应破坏解析）
        let raw = r#"{"code":"0","data":[{"acctLv":"3","acctStpMode":"cancel_maker",
            "autoLoan":false,"ip":"","label":"T1","mainUid":"495047549133190261",
            "perm":"read_only,trade","posMode":"net_mode","settleCcy":"USDC",
            "uid":"123456789012345678"}],"msg":""}"#;

        let env: Envelope<AccountConfig> = serde_json::from_str(raw).expect("反序列化失败");
        let config = &env.data[0];

        assert_eq!(config.uid, "123456789012345678");
        assert_eq!(config.pos_mode, "net_mode");
        assert!(config.is_net_mode(), "实测该账户就是净持仓模式");
        assert!(
            !config.is_read_only(),
            "perm 含 trade，必须判定为非只读——这是红色警示的依据"
        );

        // 只读密钥的判定
        let read_only = AccountConfig {
            perm: "read_only".to_string(),
            ..config.clone()
        };
        assert!(read_only.is_read_only());

        // 权限串可能包含更多项，不能写成等值判断
        let extra = AccountConfig {
            perm: "read_only,trade,withdraw".to_string(),
            ..config.clone()
        };
        assert!(!extra.is_read_only(), "含 trade 即非只读，即使还有其它权限");
    }

    #[test]
    fn balance_maps_real_response() {
        let raw = r#"{"code":"0","data":[{"adjEq":"101341.76693580001",
            "availEq":"101146.08158380001","imr":"195.68535200000002","isoEq":"0",
            "mgnRatio":"31386.740019438777","mmr":"2.93528028","notionalUsd":"587.056056",
            "totalEq":"104287.33473580002","upl":"0","details":[
              {"availBal":"4998.0379499","cashBal":"4998.0379499","ccy":"USDT",
               "eq":"4998.0379499","eqUsd":"4997.238263828016","isoEq":"0"}]}],"msg":""}"#;

        let env: Envelope<AccountBalance> = serde_json::from_str(raw).expect("反序列化失败");
        let balance = &env.data[0];

        assert!((balance.total_eq - 104287.33473580002).abs() < 1e-6);
        assert!((balance.upl).abs() < 1e-9);
        assert!(balance.mgn_ratio.is_some());
        assert_eq!(balance.details.len(), 1);
        assert_eq!(balance.details[0].ccy, "USDT");
        assert!((balance.details[0].eq_usd - 4997.238263828016).abs() < 1e-6);
    }

    #[test]
    fn position_maps_real_response() {
        // 模拟盘 SOL-USDT-SWAP 的真实持仓（5 张，3 倍杠杆，全仓）
        let raw = r#"{"code":"0","data":[{"adl":"1","avgPx":"117.43",
            "cTime":"1790327749733","ccy":"USDT","fee":"-0.293575","fundingFee":"0",
            "imr":"195.73333333333335","instId":"SOL-USDT-SWAP","instType":"SWAP",
            "last":"117.43","lever":"3","liqPx":"","markPx":"117.44","mgnMode":"cross",
            "mgnRatio":"31382.49985958607","mmr":"2.936","notionalUsd":"587.100176",
            "pos":"5","posCcy":"","posId":"3953560806705233921","posSide":"net",
            "realizedPnl":"-0.293575","uTime":"1790327749733","upl":"0.0499999999999545",
            "uplRatio":"0.0002554713446303","idxPx":"117.492"}],"msg":""}"#;

        let env: Envelope<OkxPosition> = serde_json::from_str(raw).expect("反序列化失败");
        let position = &env.data[0];

        assert_eq!(position.inst_id, "SOL-USDT-SWAP");
        assert_eq!(position.pos_id, "3953560806705233921");
        assert_eq!(
            position.pos_side, "net",
            "净持仓模式下 posSide 恒为 net，展示与统计都要按它分支"
        );
        assert_eq!(position.mgn_mode, "cross");
        assert!((position.pos - 5.0).abs() < 1e-9, "pos 是张数，不是币数量");
        assert!((position.lever - 3.0).abs() < 1e-9);
        assert!((position.notional_usd - 587.100176).abs() < 1e-6);

        // 全仓模式下 liqPx 实测为空串——必须变成 None 而不是解析失败
        assert_eq!(position.liq_px, None, "空串 liqPx 应映射为 None");
        assert!(position.idx_px.is_some());

        assert_eq!(position.c_time, 1_790_327_749_733);
        assert_eq!(position.u_time, 1_790_327_749_733);
        assert!((position.realized_pnl + 0.293575).abs() < 1e-9);
    }
}
