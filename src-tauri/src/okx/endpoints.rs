//! 端点定义与限流元数据。
//!
//! 端点路径集中在此，避免散落在调用点；限流参数也放在这里，
//! 因为「核对官方文档后只改这张表」是最低成本的维护方式。

/// base URL。
///
/// 真机核对（2026-09-25）：`www.okx.com` 可达，`aws.okx.com` 不可达，
/// 因此固定用前者，不做多域名故障转移（那是没验证过的复杂度）。
pub const BASE_URL: &str = "https://www.okx.com";

/// 限流分组。
///
/// OKX 的限流规则既有按 IP 也有按 User ID，必须分开计数——用一个全局桶会让
/// 私有端点的配额被公开行情吃掉，反之亦然。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RateGroup {
    /// 行情类（ticker / K 线 / 指数价）——按 IP
    Market,
    /// 基础数据类（标记价 / 资金费率 / 持仓量）——按 IP
    Reference,
    /// Rubik 统计类——按 IP
    Rubik,
    /// 私有账户端点——**按 User ID**，与上面三组互不侵占
    Account,
}

impl RateGroup {
    /// 返回 (配额, 窗口毫秒)。
    ///
    /// ⚠️ 初值一律取**保守值**：搜索结果里 `history-candles` 的限流数值互相矛盾
    /// （20/2s vs 40/2s），本文档不采信任何未从官方文档逐页确认的数字。
    /// 保守的代价只是慢一点，猜大的代价是被 429 挡住。
    pub fn quota(self) -> (u32, u64) {
        match self {
            RateGroup::Market => (20, 2_000),
            RateGroup::Reference => (20, 2_000),
            // 实测该分组最紧的端点是 rubik 系列
            RateGroup::Rubik => (5, 2_000),
            // 公开可查的私有端点上限是 10 req/2s，这里取 8 留余量
            RateGroup::Account => (8, 2_000),
        }
    }
}

/// 私有端点（需签名）。
///
/// 这里**只列 GET**。本应用不存在任何私有 POST 的调用点——「会不会偷偷下单」
/// 这个问题在类型层面就有答案（ADR #6）。
pub mod private {
    pub const ACCOUNT_CONFIG: &str = "/api/v5/account/config";
    pub const ACCOUNT_BALANCE: &str = "/api/v5/account/balance";
    pub const ACCOUNT_POSITIONS: &str = "/api/v5/account/positions";
}

/// 公开端点（无需鉴权）
///
/// 只列出**当前有调用方**的端点：M3 的采集计划器会补上
/// `HISTORY_CANDLES` / `FUNDING_RATE_HISTORY`，M2 会补上 `INSTRUMENTS`。
/// 提前登记没有调用方的路径只会变成死代码。
pub mod public {
    pub const TICKER: &str = "/api/v5/market/ticker";
    /// 全市场行情：引导第 3 步的候选标的排序依据
    pub const TICKERS: &str = "/api/v5/market/tickers";
    pub const CANDLES: &str = "/api/v5/market/candles";
    /// 历史 K 线：复盘的历史主干，可完整回溯（游标分页）
    pub const HISTORY_CANDLES: &str = "/api/v5/market/history-candles";
    /// 标记价 K 线（与指数价组合重建基差）
    pub const HISTORY_MARK_PRICE_CANDLES: &str = "/api/v5/market/history-mark-price-candles";
    /// 指数价 K 线（instId 用指数 ID，如 `BTC-USDT`）
    pub const HISTORY_INDEX_CANDLES: &str = "/api/v5/market/history-index-candles";
    pub const INDEX_TICKERS: &str = "/api/v5/market/index-tickers";
    pub const MARK_PRICE: &str = "/api/v5/public/mark-price";
    pub const FUNDING_RATE: &str = "/api/v5/public/funding-rate";
    /// 资金费率历史：约 90 天，游标分页
    pub const FUNDING_RATE_HISTORY: &str = "/api/v5/public/funding-rate-history";
    pub const OPEN_INTEREST: &str = "/api/v5/public/open-interest";
    /// 合约元数据：张数 → 币数量的换算依据
    pub const INSTRUMENTS: &str = "/api/v5/public/instruments";
    pub const LS_ACCOUNT_RATIO: &str = "/api/v5/rubik/stat/contracts/long-short-account-ratio";
    pub const TAKER_VOLUME: &str = "/api/v5/rubik/stat/taker-volume";
}

/// Rubik 端点的 `instType` 合法值。
///
/// 真机核对：传 `SWAP` 会返回 `51000 Parameter instType error, support [SPOT,CONTRACTS]`。
/// 初版设计文档写的是 `SWAP`，已修正。
pub mod inst_type {
    pub const CONTRACTS: &str = "CONTRACTS";
    pub const SWAP: &str = "SWAP";
}
