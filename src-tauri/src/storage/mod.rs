mod db;

pub use db::Db;

/// 当前 Unix 毫秒时间戳。
///
/// 全项目统一用毫秒（设计文档 §7.1），所以只需要这一个入口。
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}
