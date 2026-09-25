//! 采集与缓存（设计文档 §6.2、§9）。
//!
//! M1 只需要实盘快照的 TTL 缓存；M3 补上复盘所需的采集计划器与执行器。

pub mod cache;
pub mod executor;
pub mod paging;
pub mod plan;
