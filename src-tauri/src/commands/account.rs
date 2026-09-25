//! 账户与仓位命令。

use serde::Serialize;
use tauri::State;
use ts_rs::TS;

use crate::credentials::credentials_for;
use crate::error::AppResult;
use crate::okx::client::OkxClient;
use crate::okx::endpoints::{RateGroup, inst_type, public};
use crate::okx::models::Ticker;
use crate::position::AccountSnapshot;
use crate::position::current;
use crate::settings;
use crate::storage::Db;
use crate::vault::Vault;

/// `account_snapshot` 的入参。
///
/// 刻意包一层结构体而不是直接接一个 `credential_id` 参数：Tauri 会把命令参数名
/// 从 snake_case 转成 camelCase 给 JS（`credential_id` → `credentialId`），
/// 而我们的契约是**前后端统一 snake_case**（ADR #8）。用结构体后参数名只有 `query`，
/// 字段命名由 serde 决定，契约保持一致。
#[derive(Debug, serde::Deserialize)]
pub struct AccountQuery {
    pub credential_id: String,
}

/// 拉取账户概览与全部持仓，并顺带写一条本地留痕。
///
/// 每次调用都会发起真实请求——界面负责按 TTL 决定是否需要刷新，
/// 这里不做缓存，避免「以为刷新了其实没有」。
#[tauri::command]
pub async fn account_snapshot(
    client: State<'_, OkxClient>,
    db: State<'_, Db>,
    vault: State<'_, Vault>,
    query: AccountQuery,
) -> AppResult<AccountSnapshot> {
    let credentials = credentials_for(&vault, &query.credential_id)?;
    current::fetch(&client, &db, &credentials).await
}

/// 写入标的集。
#[tauri::command]
pub async fn watchlist_set(db: State<'_, Db>, watchlist: Vec<String>) -> AppResult<Vec<String>> {
    settings::write_watchlist(&db, &watchlist).await?;
    Ok(watchlist)
}

/// 引导第 3 步的候选标的：按 24h 成交额排序的 Top N 永续合约。
///
/// 为什么按成交额排：成交额低的合约点差大、数据质量差，而且它们的资金费率与
/// 多空比更容易被单笔大单扭曲——用它们做复盘会得出误导性结论。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "types.ts")]
pub struct WatchlistCandidate {
    pub inst_id: String,
    pub last: f64,
    /// USD 成交额（已由基础币成交量 × 价格换算）
    pub volume_24h_usd: f64,
    /// 是否应当默认勾选
    pub preselected: bool,
}

/// 候选标的数量上限。
const CANDIDATE_LIMIT: usize = 20;

#[tauri::command]
pub async fn watchlist_candidates(
    client: State<'_, OkxClient>,
    db: State<'_, Db>,
) -> AppResult<Vec<WatchlistCandidate>> {
    let tickers = client
        .get_public::<Ticker>(
            public::TICKERS,
            RateGroup::Market,
            &[("instType", inst_type::SWAP)],
        )
        .await?;

    // 用**当前已保存的标的集**决定预勾选，而不是硬编码的默认集：
    // 首次运行时当前值就是默认集（BTC/ETH/SOL），而用户再次进入引导时
    // 看到的是自己上次的选择——否则「点下一步」会静默把标的集重置回默认值。
    let current = settings::read_watchlist(&db).await?;

    let mut candidates: Vec<WatchlistCandidate> = tickers
        .iter()
        // 只保留 U 本位永续：币本位合约的 ctValCcy 不是计价币，
        // 仓位换算与名义价值都要走另一条分支，不适合作为默认选项。
        .filter(|ticker| ticker.inst_id.ends_with("-USDT-SWAP"))
        .map(|ticker| WatchlistCandidate {
            inst_id: ticker.inst_id.clone(),
            last: ticker.last,
            // volCcy24h 的单位是基础币，换算成 USD 必须乘价格
            volume_24h_usd: ticker.vol_ccy_24h * ticker.last,
            preselected: current.contains(&ticker.inst_id),
        })
        .collect();

    candidates.sort_by(|a, b| {
        b.volume_24h_usd
            .partial_cmp(&a.volume_24h_usd)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates.truncate(CANDIDATE_LIMIT);

    Ok(candidates)
}
