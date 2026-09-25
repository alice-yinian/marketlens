use std::time::Duration;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use tauri::{AppHandle, Manager};

use crate::credentials::CredentialMeta;
use crate::error::{AppError, AppResult};
use crate::market::Candle;
use crate::position::trace::TraceRow;

/// 超过这个「年龄」的时序点视为已定型。
///
/// 取 10 分钟：比 Rubik 的 5 分钟粒度略长，足以覆盖交易所对最近几个点的修订。
const FINALIZED_AFTER_MS: i64 = 10 * 60 * 1000;

/// 数据库句柄。
///
/// 内部持有连接池，命令层只能调用语义化方法——前端永远拿不到「执行任意 SQL」
/// 的能力（ADR #2）。这是刻意的收敛：数据里有账户与仓位，一旦 WebView 被注入，
/// 暴露 SQL 等于交出整个数据库。
pub struct Db {
    pool: SqlitePool,
}

impl Db {
    /// 仅测试用：在内存库上打开并执行迁移。
    ///
    /// `max_connections(1)` 是必需的：SQLite 的内存库是**每连接一份**的，
    /// 多连接会让「建好的表」在另一个连接里凭空消失。
    #[cfg(test)]
    pub async fn open_in_memory() -> AppResult<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

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

    pub async fn set_setting(&self, key: &str, value: &str) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        )
        .bind(key)
        .bind(value)
        .bind(crate::storage::now_ms())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ---- 凭据元数据 -------------------------------------------------------
    // 明文密钥永远不经过这里：本表只存掩码与探测结果，明文只进 Stronghold。

    pub async fn upsert_credential(&self, meta: &CredentialMeta) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO api_credentials
                (id, label, env, api_key_masked, permissions, uid_masked,
                 last_ok_at, last_error, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                label = excluded.label,
                env = excluded.env,
                api_key_masked = excluded.api_key_masked,
                permissions = excluded.permissions,
                uid_masked = excluded.uid_masked,
                last_ok_at = excluded.last_ok_at,
                last_error = excluded.last_error",
        )
        .bind(&meta.id)
        .bind(&meta.label)
        .bind(&meta.env)
        .bind(&meta.api_key_masked)
        .bind(&meta.permissions)
        .bind(&meta.uid_masked)
        .bind(meta.last_ok_at)
        .bind(&meta.last_error)
        .bind(meta.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_credentials(&self) -> AppResult<Vec<CredentialMeta>> {
        let rows = sqlx::query_as::<_, CredentialMeta>(
            "SELECT id, label, env, api_key_masked, permissions, uid_masked,
                    last_ok_at, last_error, created_at
             FROM api_credentials
             ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn delete_credential_meta(&self, id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM api_credentials WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// 记录一次探测结果。`error` 为 `None` 表示探测成功，此时刷新 `last_ok_at`。
    pub async fn record_credential_probe(
        &self,
        id: &str,
        permissions: Option<&str>,
        uid_masked: Option<&str>,
        error: Option<&str>,
    ) -> AppResult<()> {
        let now = crate::storage::now_ms();
        sqlx::query(
            "UPDATE api_credentials
             SET permissions = COALESCE(?, permissions),
                 uid_masked = COALESCE(?, uid_masked),
                 last_ok_at = CASE WHEN ? IS NULL THEN ? ELSE last_ok_at END,
                 last_error = ?
             WHERE id = ?",
        )
        .bind(permissions)
        .bind(uid_masked)
        .bind(error)
        .bind(now)
        .bind(error)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ---- 采集计划：续传状态与序列落库 --------------------------------------

    /// 某条序列上次的状态。`None` 表示从未采集过。
    pub async fn fetch_state_status(&self, key: &str) -> AppResult<Option<String>> {
        let status: Option<String> =
            sqlx::query_scalar("SELECT status FROM fetch_state WHERE series_key = ?")
                .bind(key)
                .fetch_optional(&self.pool)
                .await?;
        Ok(status)
    }

    /// 记录某条序列的状态。
    pub async fn mark_fetch_state(
        &self,
        key: &str,
        status: &str,
        error: Option<&str>,
    ) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO fetch_state (series_key, status, error, updated_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(series_key) DO UPDATE SET
                status = excluded.status,
                error = excluded.error,
                updated_at = excluded.updated_at",
        )
        .bind(key)
        .bind(status)
        .bind(error)
        .bind(crate::storage::now_ms())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// 写入 K 线。
    ///
    /// `confirm = 1`（已收盘）的 K 线**永不再变**，是 ADR #12「已定型数据永久缓存」的
    /// 直接体现；未收盘的那根会被后续刷新覆盖。
    pub async fn insert_candles(
        &self,
        inst_id: &str,
        kind: &str,
        bar: &str,
        candles: &[Candle],
    ) -> AppResult<()> {
        let mut tx = self.pool.begin().await?;

        for candle in candles {
            sqlx::query(
                "INSERT OR REPLACE INTO candles
                    (inst_id, kind, bar, ts, open, high, low, close, vol, confirm)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(inst_id)
            .bind(kind)
            .bind(bar)
            .bind(candle.ts)
            .bind(candle.open)
            .bind(candle.high)
            .bind(candle.low)
            .bind(candle.close)
            .bind(candle.vol)
            .bind(i64::from(candle.confirm))
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// 写入时序指标。
    ///
    /// 只有**距今足够久**的点才标记为已定型。近期的点仍可能被交易所修订，
    /// 标成永久会让「刷新」永远拿不到修正值。
    pub async fn insert_metric_series(
        &self,
        inst_id: &str,
        metric: &str,
        points: &[(i64, f64)],
    ) -> AppResult<()> {
        let finalized_cutoff = crate::storage::now_ms() - FINALIZED_AFTER_MS;
        let mut tx = self.pool.begin().await?;

        for (ts, value) in points {
            sqlx::query(
                "INSERT OR REPLACE INTO metric_series (inst_id, metric, ts, value, finalized)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(inst_id)
            .bind(metric)
            .bind(ts)
            .bind(value)
            .bind(i64::from(*ts < finalized_cutoff))
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// 仅测试用：统计某标的某类 K 线的条数。
    ///
    /// 刻意加在 `Db` 上而不是让测试直接拿连接池——「前端拿不到 SQL」这条边界
    /// 连测试也不该破例，否则将来很容易被顺手放宽。
    #[cfg(test)]
    pub async fn count_candles(&self, inst_id: &str, kind: &str) -> AppResult<i64> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM candles WHERE inst_id = ? AND kind = ?")
                .bind(inst_id)
                .bind(kind)
                .fetch_one(&self.pool)
                .await?;
        Ok(count)
    }

    /// 仅测试用：统计某序列的状态记录数。
    #[cfg(test)]
    pub async fn count_fetch_states(&self) -> AppResult<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fetch_state")
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }

    // ---- 本地留痕 ---------------------------------------------------------

    /// 最近一次留痕的时间戳。
    pub async fn last_position_trace_at(&self) -> AppResult<Option<i64>> {
        let value: Option<i64> = sqlx::query_scalar("SELECT MAX(ts) FROM position_trace")
            .fetch_one(&self.pool)
            .await?;
        Ok(value)
    }

    /// 写入一批留痕。
    ///
    /// 用 `INSERT OR REPLACE`：主键是 `(ts, pos_id)`，同一毫秒内的重复刷新
    /// 不应该让整批写入失败。
    pub async fn insert_position_traces(&self, rows: &[TraceRow<'_>]) -> AppResult<()> {
        let mut tx = self.pool.begin().await?;

        for row in rows {
            sqlx::query(
                "INSERT OR REPLACE INTO position_trace
                    (ts, pos_id, inst_id, pos_side, mgn_mode, lever, contracts,
                     avg_px, mark_px, liq_px, upl, upl_ratio, mgn_ratio,
                     created_at, updated_at, gap_before)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(row.ts)
            .bind(row.pos_id)
            .bind(row.inst_id)
            .bind(row.pos_side)
            .bind(row.mgn_mode)
            .bind(row.lever)
            .bind(row.contracts)
            .bind(row.avg_px)
            .bind(row.mark_px)
            .bind(row.liq_px)
            .bind(row.upl)
            .bind(row.upl_ratio)
            .bind(row.mgn_ratio)
            .bind(row.created_at)
            .bind(row.updated_at)
            .bind(i64::from(row.gap_before))
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// 某时间点之后的全部留痕时间戳（升序、去重），用于统计覆盖度与间隙。
    pub async fn position_trace_timestamps(&self, since: i64) -> AppResult<Vec<i64>> {
        let stamps: Vec<i64> = sqlx::query_scalar(
            "SELECT DISTINCT ts FROM position_trace WHERE ts >= ? ORDER BY ts ASC",
        )
        .bind(since)
        .fetch_all(&self.pool)
        .await?;
        Ok(stamps)
    }
}
