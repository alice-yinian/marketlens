//! 用户设置。

use crate::error::{AppError, AppResult};
use crate::storage::Db;

pub const KEY_WATCHLIST: &str = "watchlist";

/// 引导完成标志的键名。
pub const KEY_ONBOARDING_DONE: &str = "onboarding_done";

/// 预勾选的默认标的集（已确认：首次启动由用户自选，但给一组合理起点）。
pub const DEFAULT_WATCHLIST: &[&str] = &["BTC-USDT-SWAP", "ETH-USDT-SWAP", "SOL-USDT-SWAP"];

/// 标的数量上限（已确认）。
///
/// 直接乘进复盘请求量：3 个标的 ≈ 22 个请求，10 个标的就会到 70+。
/// 设上限是为了让「一次复盘要等多久」保持可预期。
pub const WATCHLIST_LIMIT: usize = 10;

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

/// 写入标的集。
pub async fn write_watchlist(db: &Db, list: &[String]) -> AppResult<()> {
    validate_watchlist(list)?;
    let raw = serde_json::to_string(list)
        .map_err(|err| AppError::Config(format!("标的集无法序列化：{err}")))?;
    db.set_setting(KEY_WATCHLIST, &raw).await
}

/// 校验标的集。
pub fn validate_watchlist(list: &[String]) -> AppResult<()> {
    if list.is_empty() {
        return Err(AppError::Config("至少需要选择 1 个标的".to_string()));
    }
    if list.len() > WATCHLIST_LIMIT {
        return Err(AppError::Config(format!(
            "标的集最多 {WATCHLIST_LIMIT} 个，当前 {} 个",
            list.len()
        )));
    }
    Ok(())
}

pub fn default_watchlist() -> Vec<String> {
    DEFAULT_WATCHLIST
        .iter()
        .map(std::string::ToString::to_string)
        .collect()
}

/// 首次启动引导是否已完成。
///
/// **这个标志不是可有可无的**：密钥库在每次启动时都是锁定的（我们不持久化主密码），
/// 所以「vault 未解锁」既可能是首次使用、也可能只是需要解锁。没有这个标志，
/// 前端无法区分两者，用户每次冷启动都会被逼着重走一遍 3 步引导。
pub async fn read_onboarding_done(db: &Db) -> AppResult<bool> {
    Ok(db
        .get_setting(KEY_ONBOARDING_DONE)
        .await?
        .is_some_and(|value| value == "true"))
}

pub async fn write_onboarding_done(db: &Db) -> AppResult<()> {
    db.set_setting(KEY_ONBOARDING_DONE, "true").await
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

    #[test]
    fn rejects_empty_and_oversized_watchlists() {
        assert!(validate_watchlist(&[]).is_err(), "空标的集应被拒绝");

        let too_many: Vec<String> = (0..=WATCHLIST_LIMIT)
            .map(|i| format!("C{i}-USDT-SWAP"))
            .collect();
        let err = validate_watchlist(&too_many).expect_err("超上限应被拒绝");
        assert!(
            err.to_string().contains("最多"),
            "错误信息要说清上限：{err}"
        );

        let at_limit: Vec<String> = (0..WATCHLIST_LIMIT)
            .map(|i| format!("C{i}-USDT-SWAP"))
            .collect();
        assert!(validate_watchlist(&at_limit).is_ok(), "刚好到上限应通过");
    }
}
