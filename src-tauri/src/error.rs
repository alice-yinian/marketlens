//! 统一错误模型（设计文档 §6.8）。
//!
//! 前端按 `code` 分支处理（VaultLocked → 弹解锁框、RateLimited → 倒计时、
//! DataUnavailable → 标注不可得而非报错……），因此错误必须序列化为结构化载荷，
//! 而不是一个字符串。

use serde::Serialize;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("数据库操作失败：{0}")]
    Database(#[from] sqlx::Error),

    #[error("数据库迁移失败：{0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("文件系统操作失败：{0}")]
    Io(#[from] std::io::Error),

    #[error("无法解析应用数据目录：{0}")]
    AppDataDir(String),

    #[error("{0}")]
    Config(String),
}

/// 传给前端的结构化错误载荷
#[derive(Debug, Serialize)]
pub struct ErrorPayload {
    pub code: &'static str,
    pub message: String,
    /// 前端据此决定是否展示「重试」按钮
    pub retryable: bool,
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Database(_) => "Database",
            AppError::Migration(_) => "Migration",
            AppError::Io(_) => "Io",
            AppError::AppDataDir(_) => "AppDataDir",
            AppError::Config(_) => "Config",
        }
    }

    pub fn retryable(&self) -> bool {
        match self {
            // 迁移失败通常是 schema 与代码不匹配，重试不会自愈
            AppError::Migration(_) => false,
            AppError::Config(_) => false,
            AppError::AppDataDir(_) => false,
            AppError::Database(_) | AppError::Io(_) => true,
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ErrorPayload {
            code: self.code(),
            message: self.to_string(),
            retryable: self.retryable(),
        }
        .serialize(serializer)
    }
}
