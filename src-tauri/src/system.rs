use serde::Serialize;
use ts_rs::TS;

/// 应用与核心版本信息。
///
/// M0 验收要求「双端可启动并显示版本号」，这个结构体就是那条验收路径的载荷：
/// 它同时证明 IPC 通道可用（前端拿到了它）与迁移已执行（`schema_version` 非 0）。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "types.ts")]
pub struct AppInfo {
    /// 来自 tauri.conf.json —— 版本号的唯一来源（设计文档 §14）
    pub app_version: String,
    /// 来自 Cargo.toml 的 crate 版本，与 app_version 应当一致（有测试守住）
    pub core_version: String,
    /// 读自数据库 `_sqlx_migrations`，不是硬编码常量
    ///
    /// 显式声明为 number：ts-rs 默认把 i64 映射成 bigint，但 IPC 走 JSON，
    /// 前端实际拿到的是 number。两者不一致会让类型声明撒谎（有测试守住这条规则）。
    #[ts(type = "number")]
    pub schema_version: i64,
    /// `std::env::consts::OS`：linux / windows / android / macos
    pub target_os: String,
}
