use tauri::State;

use crate::error::AppResult;
use crate::fetch::cache::LiveCache;
use crate::market::live::{self, LiveSnapshot};
use crate::okx::client::OkxClient;
use crate::settings;
use crate::storage::Db;

/// 刷新实盘市场状态。
///
/// `force = true` 绕过 TTL，对应界面上的「强制刷新」。
/// 未强制时命中 30 秒缓存直接返回——否则用户「进页面 → 切走 → 回来」
/// 的手指动作就能把限流配额打爆（§6.2.3）。
#[tauri::command]
pub async fn live_refresh(
    client: State<'_, OkxClient>,
    db: State<'_, Db>,
    cache: State<'_, LiveCache>,
    force: Option<bool>,
) -> AppResult<LiveSnapshot> {
    if !force.unwrap_or(false) {
        if let Some(snapshot) = cache.get_fresh() {
            tracing::debug!("实盘快照命中缓存");
            return Ok(snapshot);
        }
    }

    let watchlist = settings::read_watchlist(&db).await?;
    let (instruments, warnings) = live::fetch_snapshot(&client, &watchlist).await?;

    let now = crate::storage::now_ms();
    let snapshot = LiveSnapshot {
        ts: now,
        cache_hit: false,
        fetched_at: now,
        watchlist,
        instruments,
        warnings,
    };

    tracing::info!(
        instruments = snapshot.instruments.len(),
        warnings = snapshot.warnings.len(),
        "实盘快照已刷新"
    );
    cache.store(&snapshot);

    Ok(snapshot)
}

/// 读取标的集。纯本地操作，不发任何网络请求。
#[tauri::command]
pub async fn watchlist_get(db: State<'_, Db>) -> AppResult<Vec<String>> {
    settings::read_watchlist(&db).await
}
