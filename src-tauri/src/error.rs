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

    /// OKX 业务错误：HTTP 200 但信封里的 code 非 "0"
    #[error("OKX 接口返回错误 {code}：{msg}")]
    Okx { code: String, msg: String },

    #[error("网络请求失败：{0}")]
    Http(String),

    #[error("响应解析失败：{0}")]
    Decode(String),

    #[error("密钥库已锁定，请先解锁")]
    VaultLocked,

    #[error("密钥库操作失败：{0}")]
    Vault(String),

    /// 复盘时段超出已确认的 90 天硬上限。
    ///
    /// 刻意**不静默截断**：静默截断会让用户以为看全了，那是最危险的行为。
    #[error("时段超出上限：最多 90 天")]
    RangeTooLarge,

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
            AppError::Okx { .. } => "Okx",
            AppError::Http(_) => "Http",
            AppError::Decode(_) => "Decode",
            AppError::VaultLocked => "VaultLocked",
            AppError::Vault(_) => "Vault",
            AppError::RangeTooLarge => "RangeTooLarge",
            AppError::Config(_) => "Config",
        }
    }

    pub fn retryable(&self) -> bool {
        match self {
            // 迁移失败通常是 schema 与代码不匹配，重试不会自愈
            AppError::Migration(_) => false,
            AppError::Config(_) => false,
            AppError::AppDataDir(_) => false,
            // OKX 业务错误是参数/权限问题，重试无意义；解析失败同理
            AppError::Okx { .. } => false,
            AppError::Decode(_) => false,
            // 锁定与密钥库错误都需要用户动作，重试不会自愈
            AppError::VaultLocked | AppError::Vault(_) => false,
            AppError::RangeTooLarge => false,
            AppError::Database(_) | AppError::Io(_) | AppError::Http(_) => true,
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
