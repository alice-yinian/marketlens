//! 全局用户设置的命令。
//!
//! 这里收拢「一个值、全应用生效」的设置：**界面主题**与**隐私等级**。
//! 它们的共同点是：改完之后各页面按新值重新表现，而不是某一页自己的开关——
//! 所以命令里没有页面参数，只有值本身。
//!
//! 网络代理不在这里：它除了落库还要**热重建 HTTP 客户端**（见
//! `commands/network.rs`），带状态的那一步属于网络那条链路。

use tauri::State;

use crate::error::AppResult;
use crate::prompt::privacy::PrivacyLevel;
use crate::settings::{self, Theme};
use crate::storage::Db;

/// 读取全局隐私等级（三条提示词管线共用）。
#[tauri::command]
pub async fn privacy_get(db: State<'_, Db>) -> AppResult<PrivacyLevel> {
    settings::read_privacy(&db).await
}

/// 设置全局隐私等级。
///
/// 写到 `settings` 而不是各页面自己的状态里：三条提示词管线都读同一个值，
/// 用户不必、也不该在三个地方分别维护「AI 能看到什么」。
#[tauri::command]
pub async fn privacy_set(db: State<'_, Db>, level: PrivacyLevel) -> AppResult<PrivacyLevel> {
    let saved = settings::write_privacy(&db, level).await?;
    tracing::info!(level = saved.as_str(), "全局隐私等级已更新");
    Ok(saved)
}

/// 读取界面主题。
#[tauri::command]
pub async fn theme_get(db: State<'_, Db>) -> AppResult<Theme> {
    settings::read_theme(&db).await
}

/// 设置界面主题（`system` / `light` / `dark`）。
///
/// 前端拿到新值后写到 `<html data-theme>`：整套配色是 CSS 变量，
/// 所以换主题不需要重建任何组件。
#[tauri::command]
pub async fn theme_set(db: State<'_, Db>, theme: Theme) -> AppResult<Theme> {
    let saved = settings::write_theme(&db, theme).await?;
    tracing::info!(theme = ?saved, "界面主题已更新");
    Ok(saved)
}
