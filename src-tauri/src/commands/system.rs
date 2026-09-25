//! 应用级命令：版本信息与启动状态。

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use ts_rs::TS;

use crate::error::{AppError, AppResult};
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
    collect_app_info(&app, &db).await
}

/// 组装版本信息。命令与诊断导出共用同一份实现——
/// 两处各写一份的话，诊断包里的版本号迟早会和界面上显示的不一致。
async fn collect_app_info(app: &AppHandle, db: &Db) -> AppResult<AppInfo> {
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

// ------------------------------------------------------------------ M6：诊断与缓存

/// 缓存占用统计。
#[tauri::command]
pub async fn cache_stats(db: State<'_, Db>) -> AppResult<crate::system::cache::CacheStats> {
    crate::system::cache::stats(&db).await
}

/// 按保留策略清理缓存并回收空间。
#[tauri::command]
pub async fn cache_cleanup(db: State<'_, Db>) -> AppResult<crate::system::cache::CleanupReport> {
    let report = crate::system::cache::cleanup(&db).await?;
    tracing::info!(
        tables = report.deleted.len(),
        before = report.bytes_before,
        after = report.bytes_after,
        "缓存已清理"
    );
    Ok(report)
}

/// 导出结果：诊断包本身 + 落盘路径。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct DiagnosticExport {
    pub bundle: crate::system::diagnostics::DiagnosticBundle,
    /// 写到哪里了（供界面提示用户）
    pub path: String,
    /// 落盘后的文件大小，便于用户判断「这个包大不大、好不好发」
    #[ts(type = "number")]
    pub bytes: i64,
}

/// 一键导出脱敏诊断包。
///
/// **同时返回内容与落盘路径**：内容让界面能直接展示/复制（用户往往想马上贴给别人），
/// 路径让用户能自己去看完整文件。只给其中一个都会让另一种用法变得别扭。
#[tauri::command]
pub async fn diagnostics_export(app: AppHandle, db: State<'_, Db>) -> AppResult<DiagnosticExport> {
    let app_info = collect_app_info(&app, &db).await?;
    let bundle = crate::system::diagnostics::build(&db, app_info, latest_log_file(&app)).await?;

    let json = serde_json::to_string_pretty(&bundle)
        .map_err(|err| AppError::Config(format!("诊断包序列化失败：{err}")))?;

    let dir = app
        .path()
        .app_data_dir()
        .map_err(|err| AppError::AppDataDir(err.to_string()))?
        .join("diagnostics");
    std::fs::create_dir_all(&dir)?;

    let path = dir.join(format!("diagnostic-{}.json", bundle.generated_at));
    std::fs::write(&path, &json)?;

    tracing::info!(path = %path.display(), bytes = json.len(), "诊断包已导出");

    Ok(DiagnosticExport {
        bundle,
        path: path.display().to_string(),
        bytes: json.len() as i64,
    })
}

/// 找到最新的日志文件。
///
/// 日志按天轮转（`marketlens.2026-09-25.log`），所以要按**修改时间**取最新的那个，
/// 而不是拼今天的日期——跨零点时按日期拼会指向一个还不存在的文件。
fn latest_log_file(app: &AppHandle) -> Option<std::path::PathBuf> {
    let dir = app.path().app_log_dir().ok()?;
    let entries = std::fs::read_dir(&dir).ok()?;

    entries
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("marketlens")
                && entry.file_name().to_string_lossy().ends_with(".log")
        })
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, entry.path()))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}
