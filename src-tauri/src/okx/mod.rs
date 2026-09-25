//! OKX 接入层。
//!
//! 设计约束：
//!   * 只实现**只读**端点——客户端代码里不存在任何私有 POST 能力（ADR #6、§3.2）
//!   * 所有数值字段按字符串解析（OKX 实测如此），空串一律映射为 None
//!   * 限流按端点分组（既有按 IP 也有按 User ID 的规则），见 `ratelimit`

pub mod client;
pub mod credentials;
pub mod de;
pub mod endpoints;
pub mod models;
pub mod ratelimit;
pub mod sign;
