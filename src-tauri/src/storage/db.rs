use std::time::Duration;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use tauri::{AppHandle, Manager};

use crate::credentials::CredentialMeta;
use crate::error::{AppError, AppResult};
use crate::market::Candle;
use crate::position::history::{ClosedPosition, ClosedPositionRow};
use crate::position::trace::TraceRow;
use crate::storage::TraceSnapshot;

/// `candles` 表的读取行。
///
/// `confirm` 在库里是 0/1，读回来要还原成 bool——直接用 `Candle` 反序列化做不到。
#[derive(Debug, Clone, sqlx::FromRow)]
struct CandleRow {
    ts: i64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    vol: f64,
    confirm: i64,
}

impl From<CandleRow> for Candle {
    fn from(row: CandleRow) -> Self {
        Self {
            ts: row.ts,
            open: row.open,
            high: row.high,
            low: row.low,
            close: row.close,
            vol: row.vol,
            confirm: row.confirm != 0,
        }
    }
}

/// 超过这个「年龄」的时序点视为已定型。
///
/// 取 10 分钟：比 Rubik 的 5 分钟粒度略长，足以覆盖交易所对最近几个点的修订。
const FINALIZED_AFTER_MS: i64 = 10 * 60 * 1000;

/// 用户提示词模板的数据库形态（对应 0001 已建的 `prompt_templates`）。
///
/// `description` 在表里可空，这里收成 `String`：模板正文与说明的区别，
/// 用户不该关心「NULL 和空串有什么不一样」。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PromptTemplateRow {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    /// 表里的列名是 `scope`（`live` | `review`）
    pub scope: String,
    pub body: String,
    pub updated_at: i64,
}

/// 缓存管理涉及的时序表。
///
/// 用枚举而不是字符串：调用方只能引用这些常量，**不存在把任意字符串拼进 SQL
/// 的可能**——`SELECT COUNT(*) FROM {name}` 这种写法一旦允许传入任意名字，
/// 就等价于开了一个注入点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheTable {
    Candles,
    MetricSeries,
    PositionTrace,
    PositionHistory,
    Fills,
    LiveSnapshot,
    AccountSnapshot,
    PromptRuns,
    FetchState,
}

impl CacheTable {
    pub fn as_str(self) -> &'static str {
        match self {
            CacheTable::Candles => "candles",
            CacheTable::MetricSeries => "metric_series",
            CacheTable::PositionTrace => "position_trace",
            CacheTable::PositionHistory => "position_history",
            CacheTable::Fills => "fills",
            CacheTable::LiveSnapshot => "live_snapshot",
            CacheTable::AccountSnapshot => "account_snapshot",
            CacheTable::PromptRuns => "prompt_runs",
            CacheTable::FetchState => "fetch_state",
        }
    }

    /// 表不存在时返回 0 行而不是报错。
    ///
    /// 迁移历史里可能有表被合并过；让「缓存管理」因为一张可选表不存在就整页打不开，
    /// 不值得。
    pub async fn count(self, db: &Db) -> AppResult<i64> {
        // 每个分支都是**字面量**，不是拼接出来的字符串：注入面为零，
        // 而且 sqlx 也因此能接受（它拒绝动态 SQL 字符串，这个拒绝是对的）。
        let sql = match self {
            CacheTable::Candles => "SELECT COUNT(*) FROM candles",
            CacheTable::MetricSeries => "SELECT COUNT(*) FROM metric_series",
            CacheTable::PositionTrace => "SELECT COUNT(*) FROM position_trace",
            CacheTable::PositionHistory => "SELECT COUNT(*) FROM position_history",
            CacheTable::Fills => "SELECT COUNT(*) FROM fills",
            CacheTable::LiveSnapshot => "SELECT COUNT(*) FROM live_snapshot",
            CacheTable::AccountSnapshot => "SELECT COUNT(*) FROM account_snapshot",
            CacheTable::PromptRuns => "SELECT COUNT(*) FROM prompt_runs",
            CacheTable::FetchState => "SELECT COUNT(*) FROM fetch_state",
        };
        Ok(sqlx::query_scalar(sql)
            .fetch_one(&db.pool)
            .await
            .unwrap_or(0))
    }
}

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

    /// 删除一条设置。
    ///
    /// 用于「清除代理」这类语义：把 `proxy` 写成空串会留下一条**看起来有配置、
    /// 实际是空**的记录，读的一侧就得永远记得处理空串；删掉则只有「有 / 没有」两态。
    pub async fn delete_setting(&self, key: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM settings WHERE key = ?")
            .bind(key)
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

    pub async fn list_prompt_templates(&self) -> AppResult<Vec<PromptTemplateRow>> {
        let rows = sqlx::query_as::<_, PromptTemplateRow>(
            "SELECT id, name, description, scope, body, updated_at \
             FROM prompt_templates ORDER BY updated_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn get_prompt_template(&self, id: &str) -> AppResult<Option<PromptTemplateRow>> {
        let row = sqlx::query_as::<_, PromptTemplateRow>(
            "SELECT id, name, description, scope, body, updated_at \
             FROM prompt_templates WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// 插入或覆盖一个用户模板。
    ///
    /// `created_at` 只在首次插入时写入——覆盖不该让「创建时间」跟着往后跑。
    /// `builtin` 恒为 0：内置模板随代码发布，不入库（见 `prompt::templates`）。
    pub async fn upsert_prompt_template(&self, row: &PromptTemplateRow) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO prompt_templates \
                (id, name, description, scope, body, profile, builtin, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, 'standard', 0, ?6, ?6) \
             ON CONFLICT(id) DO UPDATE SET \
                name = excluded.name, description = excluded.description, \
                scope = excluded.scope, body = excluded.body, updated_at = excluded.updated_at",
        )
        .bind(&row.id)
        .bind(&row.name)
        .bind(&row.description)
        .bind(&row.scope)
        .bind(&row.body)
        .bind(row.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// 删除用户模板。返回 `false` 表示本来就不存在。
    pub async fn delete_prompt_template(&self, id: &str) -> AppResult<bool> {
        let result = sqlx::query("DELETE FROM prompt_templates WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// 数据库文件大小（`page_count × page_size`）。
    /// **仅测试用**：让测试能直接造数据，而不必为每个测试场景都加一个业务方法。
    ///
    /// 只在 `cfg(test)` 下存在，所以它不会被任何发布构建引用到——
    /// 「命令层不能执行任意 SQL」这条边界（ADR #2）依然成立。
    #[cfg(test)]
    pub(crate) fn pool_for_test(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn database_bytes(&self) -> AppResult<i64> {
        let page_count: i64 = sqlx::query_scalar("PRAGMA page_count")
            .fetch_one(&self.pool)
            .await?;
        let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
            .fetch_one(&self.pool)
            .await?;
        Ok(page_count * page_size)
    }

    /// 删除早于 `before_ms` 的仓位留痕，返回删除行数。
    pub async fn prune_position_traces(&self, before_ms: i64) -> AppResult<i64> {
        let result = sqlx::query("DELETE FROM position_trace WHERE ts < ?1")
            .bind(before_ms)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() as i64)
    }

    /// 每个 `(inst_id, kind, bar)` 只保留最近 `keep` 根 K 线。
    ///
    /// 按序列分组，而不是「全局按时间删」——否则交易活跃的标的会把不活跃标的的
    /// 数据一起挤掉，而那些正是复盘时最需要的（长期持仓往往在冷门标的上）。
    pub async fn prune_candles(&self, keep: i64) -> AppResult<i64> {
        let result = sqlx::query(
            "DELETE FROM candles WHERE rowid IN (
                SELECT rowid FROM (
                    SELECT rowid, ROW_NUMBER() OVER (
                        PARTITION BY inst_id, kind, bar ORDER BY ts DESC
                    ) AS rn FROM candles
                ) WHERE rn > ?1
            )",
        )
        .bind(keep)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() as i64)
    }

    /// 每个 `(inst_id, metric)` 只保留最近 `keep` 个点。
    pub async fn prune_metric_series(&self, keep: i64) -> AppResult<i64> {
        let result = sqlx::query(
            "DELETE FROM metric_series WHERE rowid IN (
                SELECT rowid FROM (
                    SELECT rowid, ROW_NUMBER() OVER (
                        PARTITION BY inst_id, metric ORDER BY ts DESC
                    ) AS rn FROM metric_series
                ) WHERE rn > ?1
            )",
        )
        .bind(keep)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() as i64)
    }

    /// 只保留最近 `keep` 条提示词生成记录。
    pub async fn prune_prompt_runs(&self, keep: i64) -> AppResult<i64> {
        let result = sqlx::query(
            "DELETE FROM prompt_runs WHERE id NOT IN (
                SELECT id FROM prompt_runs ORDER BY id DESC LIMIT ?1
            )",
        )
        .bind(keep)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() as i64)
    }

    /// 重写数据库文件以回收空间。
    ///
    /// `VACUUM` 会重写整个库，所以**只在真的删了东西时调用**：
    /// 每次刷新都跑会让一次普通操作卡住几秒。
    pub async fn vacuum(&self) -> AppResult<()> {
        sqlx::query("VACUUM").execute(&self.pool).await?;
        Ok(())
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

    // ---- 历史仓位 ---------------------------------------------------------

    /// 写入（或更新）已平仓位。
    ///
    /// `regime_snapshot` 存 JSON：归因数据是后来算出来的，用单独一列承载它，
    /// 这样「同步」与「归因」两个阶段可以分开跑、分开重跑。
    pub async fn upsert_closed_positions(&self, rows: &[ClosedPosition]) -> AppResult<()> {
        let mut tx = self.pool.begin().await?;
        let synced_at = crate::storage::now_ms();

        for row in rows {
            let regime = row
                .regime
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|err| AppError::Config(format!("归因快照序列化失败：{err}")))?;

            sqlx::query(
                "INSERT OR REPLACE INTO position_history
                    (pos_id, inst_id, direction, mgn_mode, lever, open_avg_px, close_avg_px,
                     max_contracts, pnl, pnl_ratio, fee, funding_fee, realized_pnl,
                     open_time, close_time, source, time_precision, regime_snapshot,
                     max_favorable, max_adverse, synced_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&row.pos_id)
            .bind(&row.inst_id)
            .bind(&row.direction)
            .bind(&row.mgn_mode)
            .bind(row.lever)
            .bind(row.open_avg_px)
            .bind(row.close_avg_px)
            .bind(row.max_contracts)
            .bind(row.pnl)
            .bind(row.pnl_ratio)
            .bind(row.fee)
            .bind(row.funding_fee)
            .bind(row.realized_pnl)
            .bind(row.open_time)
            .bind(row.close_time)
            .bind(&row.source)
            .bind(&row.time_precision)
            .bind(regime)
            .bind(row.max_favorable)
            .bind(row.max_adverse)
            .bind(synced_at)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// 读取某时段内已平仓位（按平仓时间升序）。
    pub async fn closed_positions_between(
        &self,
        from: i64,
        to: i64,
    ) -> AppResult<Vec<ClosedPosition>> {
        let rows = sqlx::query_as::<_, ClosedPositionRow>(
            "SELECT pos_id, inst_id, direction, mgn_mode, lever, open_avg_px, close_avg_px,
                    max_contracts, pnl, pnl_ratio, fee, funding_fee, realized_pnl,
                    open_time, close_time, source, time_precision, regime_snapshot,
                    max_favorable, max_adverse
             FROM position_history
             WHERE close_time BETWEEN ? AND ?
             ORDER BY close_time ASC",
        )
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// 读取某时段内的全部留痕快照（按时间升序）。
    pub async fn position_traces_between(
        &self,
        from: i64,
        to: i64,
    ) -> AppResult<Vec<TraceSnapshot>> {
        let rows = sqlx::query_as::<_, TraceSnapshot>(
            "SELECT ts, pos_id, inst_id, mgn_mode, lever, contracts, avg_px, upl, created_at
             FROM position_trace
             WHERE ts BETWEEN ? AND ?
             ORDER BY ts ASC",
        )
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    // ---- 时段重建（归因用）-------------------------------------------------

    /// 取某时点之前最近的 `limit` 根 K 线，**按时间升序**返回。
    ///
    /// 升序是刻意的：指标计算全部假设升序，让调用方每次自己排序迟早会漏。
    pub async fn candles_before(
        &self,
        inst_id: &str,
        kind: &str,
        bar: &str,
        at: i64,
        limit: i64,
    ) -> AppResult<Vec<Candle>> {
        let mut rows = sqlx::query_as::<_, CandleRow>(
            "SELECT ts, open, high, low, close, vol, confirm FROM candles
             WHERE inst_id = ? AND kind = ? AND bar = ? AND ts <= ?
             ORDER BY ts DESC LIMIT ?",
        )
        .bind(inst_id)
        .bind(kind)
        .bind(bar)
        .bind(at)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.reverse();
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// 取某时点之前最近的一个时序指标值。
    pub async fn metric_value_before(
        &self,
        inst_id: &str,
        metric: &str,
        at: i64,
    ) -> AppResult<Option<f64>> {
        let value: Option<f64> = sqlx::query_scalar(
            "SELECT value FROM metric_series
             WHERE inst_id = ? AND metric = ? AND ts <= ?
             ORDER BY ts DESC LIMIT 1",
        )
        .bind(inst_id)
        .bind(metric)
        .bind(at)
        .fetch_optional(&self.pool)
        .await?;
        Ok(value)
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
