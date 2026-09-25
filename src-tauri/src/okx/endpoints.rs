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
/// 私有端点的配额被公开行情吃掉。
///
/// M1 只有公开端点，因此下面三个分组**全部是 IP 作用域**；M2 接入私有账户端点时
/// 会补一个 User ID 作用域的 `Account` 分组，届时 `quota()` 的初值同样要保守。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RateGroup {
    /// 行情类（ticker / K 线 / 指数价）
    Market,
    /// 基础数据类（标记价 / 资金费率 / 持仓量）
    Reference,
    /// Rubik 统计类
    Rubik,
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
        }
    }
}

/// 公开端点（无需鉴权）
///
/// 只列出**当前有调用方**的端点：M3 的采集计划器会补上
/// `HISTORY_CANDLES` / `FUNDING_RATE_HISTORY`，M2 会补上 `INSTRUMENTS`。
/// 提前登记没有调用方的路径只会变成死代码。
pub mod public {
    pub const TICKER: &str = "/api/v5/market/ticker";
    pub const CANDLES: &str = "/api/v5/market/candles";
    pub const INDEX_TICKERS: &str = "/api/v5/market/index-tickers";
    pub const MARK_PRICE: &str = "/api/v5/public/mark-price";
    pub const FUNDING_RATE: &str = "/api/v5/public/funding-rate";
    pub const OPEN_INTEREST: &str = "/api/v5/public/open-interest";
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
