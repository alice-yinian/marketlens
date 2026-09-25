mod db;

pub use db::Db;

/// `position_trace` 表的一行（读取用）。
///
/// 与写入用的 `position::trace::TraceRow` 分开：写入时字段是借用，
/// 读取时是拥有，而且读出来的 `gap_before` 只是留痕元数据、对推导生命周期没用。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TraceSnapshot {
    pub ts: i64,
    pub pos_id: String,
    pub inst_id: String,
    pub mgn_mode: String,
    pub lever: f64,
    pub contracts: f64,
    pub avg_px: f64,
    /// 浮盈，用于取持仓期间的极值
    pub upl: f64,
    /// 仓位在 OKX 侧的创建时间。
    ///
    /// 比留痕时间精确得多：它是交易所给的，而留痕只能给「我什么时候看到过它」。
    /// 本地独有的仓位因此能把开仓时间还原到接近精确。
    pub created_at: i64,
}

/// 当前 Unix 毫秒时间戳。
///
/// 全项目统一用毫秒（设计文档 §7.1），所以只需要这一个入口。
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}
