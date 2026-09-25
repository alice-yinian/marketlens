use std::time::Duration;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};

/// 数据库句柄。
///
/// 内部持有连接池，命令层只能调用语义化方法——前端永远拿不到「执行任意 SQL」
/// 的能力（ADR #2）。这是刻意的收敛：数据里有账户与仓位，一旦 WebView 被注入，
/// 暴露 SQL 等于交出整个数据库。
pub struct Db {
    pool: SqlitePool,
}

impl Db {
    /// 打开（必要时创建）数据库并执行迁移。迁移是幂等的，每次启动都跑。
    pub async fn open(app: &AppHandle) -> AppResult<Self> {
        let dir = app
            .path()
            .app_data_dir()
            .map_err(|e| AppError::AppDataDir(e.to_string()))?;
        std::fs::create_dir_all(&dir)?;

        let options = SqliteConnectOptions::new()
            .filename(dir.join("marketlens.db"))
            .create_if_missing(true)
            // WAL：读不阻塞写（设计文档 §8）
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;
        tracing::info!(dir = %dir.display(), "数据库就绪");

        Ok(Self { pool })
    }

    /// 当前 schema 版本，读自 sqlx 的迁移记录表。
    ///
    /// 刻意不用硬编码常量：界面上显示的这个数字本身就证明了迁移确实执行过，
    /// 而不是「代码里写了个 1」。
    pub async fn schema_version(&self) -> AppResult<i64> {
        let version: i64 =
            sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations")
                .fetch_one(&self.pool)
                .await?;
        Ok(version)
    }

    pub async fn get_setting(&self, key: &str) -> AppResult<Option<String>> {
        let value: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(value)
    }
}
