//! 用户设置。
//!
//! M1 只需要**读**标的集：写入口（设置页与首次启动引导的「选择标的」步骤）
//! 属于 M2，所以这里刻意不提供 `write_watchlist`——没有调用方的写路径是负担。

use crate::error::AppResult;
use crate::storage::Db;

pub const KEY_WATCHLIST: &str = "watchlist";

/// 预勾选的默认标的集（已确认：首次启动由用户自选，但给一组合理起点）。
pub const DEFAULT_WATCHLIST: &[&str] = &["BTC-USDT-SWAP", "ETH-USDT-SWAP", "SOL-USDT-SWAP"];

/// 读取标的集。未设置过或配置损坏时回落到默认值。
pub async fn read_watchlist(db: &Db) -> AppResult<Vec<String>> {
    let Some(raw) = db.get_setting(KEY_WATCHLIST).await? else {
        return Ok(default_watchlist());
    };

    match serde_json::from_str::<Vec<String>>(&raw) {
        Ok(list) if !list.is_empty() => Ok(list),
        Ok(_) => Ok(default_watchlist()),
        Err(err) => {
            // 不静默吞掉：配置损坏是可诊断事件，日志里必须留痕
            tracing::warn!(%err, "watchlist 配置无法解析，回落默认值");
            Ok(default_watchlist())
        }
    }
}

pub fn default_watchlist() -> Vec<String> {
    DEFAULT_WATCHLIST
        .iter()
        .map(std::string::ToString::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_watchlist_matches_confirmed_preselection() {
        let list = default_watchlist();
        assert_eq!(list.len(), 3);
        assert!(list.contains(&"BTC-USDT-SWAP".to_string()));
        assert!(list.contains(&"ETH-USDT-SWAP".to_string()));
        assert!(list.contains(&"SOL-USDT-SWAP".to_string()));
    }
}
