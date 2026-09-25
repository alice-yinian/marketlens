//! 应用级命令：版本信息与启动状态。

use serde::Serialize;
use tauri::{AppHandle, State};
use ts_rs::TS;

use crate::error::AppResult;
use crate::settings;
use crate::storage::Db;
use crate::system::AppInfo;
use crate::vault::Vault;

/// 读取应用与核心版本信息。
///
/// 这个命令同时承担 M0 的验收职责：前端能拿到它，就说明 IPC 通道、
/// 数据库迁移、错误模型三条链路都通了。
#[tauri::command]
pub async fn app_info(app: AppHandle, db: State<'_, Db>) -> AppResult<AppInfo> {
    Ok(AppInfo {
        app_version: app.package_info().version.to_string(),
        core_version: env!("CARGO_PKG_VERSION").to_string(),
        schema_version: db.schema_version().await?,
        target_os: std::env::consts::OS.to_string(),
    })
}

/// 启动时前端需要知道的一切。
///
/// 合成一个命令而不是让前端串行调用三个，是因为它们的组合决定了「进引导页还是主界面」
/// 这一个决策；分开调用会出现「解锁状态已知、引导标志还没到」的中间态，
/// 界面就会闪一下错误的页面。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct BootstrapState {
    /// 密钥库文件是否已存在
    pub vault_exists: bool,
    pub vault_unlocked: bool,
    /// 首次启动引导是否已完成
    pub onboarding_done: bool,
    /// 当前标的集（已保存的值，未设置过则是预勾选的默认集）
    pub watchlist: Vec<String>,
}

#[tauri::command]
pub async fn bootstrap_state(
    db: State<'_, Db>,
    vault: State<'_, Vault>,
) -> AppResult<BootstrapState> {
    Ok(BootstrapState {
        vault_exists: vault.exists(),
        vault_unlocked: vault.is_unlocked(),
        onboarding_done: settings::read_onboarding_done(&db).await?,
        watchlist: settings::read_watchlist(&db).await?,
    })
}

/// 标记首次启动引导已完成。
#[tauri::command]
pub async fn onboarding_complete(db: State<'_, Db>) -> AppResult<()> {
    settings::write_onboarding_done(&db).await
}
