use tauri::{AppHandle, State};

use crate::error::AppResult;
use crate::storage::Db;
use crate::system::AppInfo;

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
