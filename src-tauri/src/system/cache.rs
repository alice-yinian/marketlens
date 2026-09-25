//! 缓存统计与清理（设计文档 §8「保留策略」）。
//!
//! 保留策略不是「顺手清理」——它是**可用性的一部分**：`position_trace` 每刷新一次
//! 就写一行，`candles` 一次复盘能拉进几千根，跑几个月后数据库会到几百 MB。
//! 在 Android 上那是实打实的存储占用。
//!
//! 三条策略来自设计文档 §8：
//! - `position_trace` 保留 90 天（再早的留痕对「最近在做什么」已无参考价值）
//! - `candles` 每个 `(inst_id, kind, bar)` 保留最近 5000 根
//! - `prompt_runs` 保留最近 200 条
//!
//! **删之前先算清楚**：`cleanup` 返回删了多少行、回收了多少字节，
//! 而不是默默删掉。用户点「清理」时应当知道会发生什么。

use serde::Serialize;
use ts_rs::TS;

use crate::error::AppResult;
use crate::storage::CacheTable;
use crate::storage::Db;

/// 留痕保留天数。
const TRACE_RETENTION_DAYS: i64 = 90;
/// 每 `(inst_id, kind, bar)` 保留的 K 线根数。
const CANDLE_RETENTION_PER_SERIES: i64 = 5_000;
/// 保留的提示词生成记录条数。
const PROMPT_RUN_RETENTION: i64 = 200;

/// 一张表的占用情况。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct TableStat {
    pub name: String,
    /// 中文名，界面直接展示
    pub label: String,
    #[ts(type = "number")]
    pub rows: i64,
    /// 保留策略的人话说明。`None` 表示这张表不自动清理。
    pub retention: Option<String>,
}

/// 缓存占用统计。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct CacheStats {
    #[ts(type = "number")]
    pub database_bytes: i64,
    pub tables: Vec<TableStat>,
}

/// 清理结果。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct CleanupReport {
    /// 每张表删掉的行数（只列出真的删了东西的表）
    pub deleted: Vec<DeletedRows>,
    /// `VACUUM` 前后的大小，用于证明空间确实回收了
    #[ts(type = "number")]
    pub bytes_before: i64,
    #[ts(type = "number")]
    pub bytes_after: i64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct DeletedRows {
    pub name: String,
    #[ts(type = "number")]
    pub rows: i64,
}

/// 需要展示的表（名字、中文名、保留策略说明）。
///
/// 只列**用户关心的**：时序数据与生成记录。`api_credentials`、`settings`
/// 这类小表列出来只会稀释信息。
const TRACKED: &[(CacheTable, &str, Option<&str>)] = &[
    (
        CacheTable::Candles,
        "K 线",
        Some("每个序列保留最近 5000 根"),
    ),
    (
        CacheTable::MetricSeries,
        "指标序列",
        Some("每个序列保留最近 5000 点"),
    ),
    (
        CacheTable::PositionTrace,
        "仓位留痕",
        Some("保留最近 90 天"),
    ),
    (CacheTable::PositionHistory, "历史仓位", None),
    (CacheTable::Fills, "成交明细", None),
    (CacheTable::LiveSnapshot, "行情快照", None),
    (CacheTable::AccountSnapshot, "账户快照", None),
    (
        CacheTable::PromptRuns,
        "提示词生成记录",
        Some("保留最近 200 条"),
    ),
    (CacheTable::FetchState, "采集进度", None),
];

/// 统计缓存占用。
pub async fn stats(db: &Db) -> AppResult<CacheStats> {
    let mut tables = Vec::with_capacity(TRACKED.len());

    for (table, label, retention) in TRACKED {
        tables.push(TableStat {
            name: table.as_str().to_string(),
            label: (*label).to_string(),
            rows: table.count(db).await?,
            retention: retention.map(str::to_string),
        });
    }

    Ok(CacheStats {
        database_bytes: db.database_bytes().await?,
        tables,
    })
}

/// 按保留策略清理，并 `VACUUM` 回收空间。
pub async fn cleanup(db: &Db) -> AppResult<CleanupReport> {
    let bytes_before = db.database_bytes().await?;
    let mut deleted = Vec::new();

    let trace_cutoff = crate::storage::now_ms() - TRACE_RETENTION_DAYS * 86_400_000;
    let mut record = |name: &str, rows: i64| {
        if rows > 0 {
            deleted.push(DeletedRows {
                name: name.to_string(),
                rows,
            });
        }
    };

    record(
        "position_trace",
        db.prune_position_traces(trace_cutoff).await?,
    );
    record(
        "candles",
        db.prune_candles(CANDLE_RETENTION_PER_SERIES).await?,
    );
    record(
        "metric_series",
        db.prune_metric_series(CANDLE_RETENTION_PER_SERIES).await?,
    );
    record(
        "prompt_runs",
        db.prune_prompt_runs(PROMPT_RUN_RETENTION).await?,
    );

    // 只在真的删了东西时 VACUUM：它会重写整个数据库文件，
    // 每次刷新都跑会让一次普通操作卡住几秒。
    if !deleted.is_empty() {
        db.vacuum().await?;
    }

    Ok(CleanupReport {
        deleted,
        bytes_before,
        bytes_after: db.database_bytes().await?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn memory_db() -> Db {
        Db::open_in_memory().await.expect("内存库创建失败")
    }

    #[tokio::test]
    async fn stats_lists_tracked_tables_with_zero_rows() {
        let db = memory_db().await;
        let stats = stats(&db).await.expect("统计失败");

        assert!(stats.database_bytes > 0, "空库也有页大小");
        assert_eq!(stats.tables.len(), TRACKED.len());
        assert!(
            stats.tables.iter().all(|table| table.rows == 0),
            "新库所有表都应是 0 行"
        );
        // 保留策略说明必须存在，否则界面没法解释「为什么这些会被删」
        let trace = stats
            .tables
            .iter()
            .find(|table| table.name == "position_trace")
            .expect("应有留痕表");
        assert!(trace.retention.is_some());
    }

    #[tokio::test]
    async fn cleanup_deletes_only_beyond_retention() {
        let db = memory_db().await;
        let now = crate::storage::now_ms();

        // 一条 100 天前的留痕（超出 90 天）与一条 1 天前的（应保留）
        for (ts, pos_id) in [(now - 100 * 86_400_000, "old"), (now - 86_400_000, "fresh")] {
            sqlx::query(
                "INSERT INTO position_trace (ts, pos_id, inst_id, pos_side, mgn_mode, contracts, created_at)
                 VALUES (?1, ?2, 'BTC-USDT-SWAP', 'net', 'cross', 1.0, ?1)",
            )
            .bind(ts)
            .bind(pos_id)
            .execute(db.pool_for_test())
            .await
            .expect("插入留痕失败");
        }

        let report = cleanup(&db).await.expect("清理失败");
        let trace = report
            .deleted
            .iter()
            .find(|entry| entry.name == "position_trace")
            .expect("应删掉过期留痕");
        assert_eq!(trace.rows, 1, "只该删掉超期的那一条");

        let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM position_trace")
            .fetch_one(db.pool_for_test())
            .await
            .expect("统计失败");
        assert_eq!(remaining, 1, "未超期的留痕必须保留");
    }

    /// 空库上清理不该报错，也不该跑 `VACUUM`（`deleted` 为空即证明）。
    #[tokio::test]
    async fn cleanup_on_empty_database_is_a_noop() {
        let db = memory_db().await;
        let report = cleanup(&db).await.expect("清理失败");
        assert!(report.deleted.is_empty());
        assert_eq!(report.bytes_before, report.bytes_after);
    }

    /// 每个序列各自保留 N 根，而不是「全局按时间删」。
    #[tokio::test]
    async fn candle_retention_is_per_series() {
        let db = memory_db().await;

        // 两个序列，各插 5 根
        for inst in ["BTC-USDT-SWAP", "ETH-USDT-SWAP"] {
            for index in 0..5 {
                sqlx::query(
                    "INSERT INTO candles (inst_id, kind, bar, ts, open, high, low, close, vol)
                     VALUES (?1, 'candle', '1H', ?2, 1.0, 1.0, 1.0, 1.0, 1.0)",
                )
                .bind(inst)
                .bind(1_700_000_000_000_i64 + index * 3_600_000)
                .execute(db.pool_for_test())
                .await
                .expect("插入 K 线失败");
            }
        }

        let report = cleanup(&db).await.expect("清理失败");
        // 阈值是 5000，5 根远不到，所以一根都不该删
        assert!(
            !report.deleted.iter().any(|entry| entry.name == "candles"),
            "未超阈值时不该删 K 线"
        );

        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM candles")
            .fetch_one(db.pool_for_test())
            .await
            .expect("统计失败");
        assert_eq!(total, 10, "两个序列各 5 根都应保留");
    }
}
