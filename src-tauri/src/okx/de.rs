//! OKX 响应字段的解析辅助。
//!
//! 真机核对（2026-09-25）：OKX 把**所有数值都当字符串传**，并用**空串表示缺失值**
//! （例如 `funding-rate` 的 `nextFundingRate` 实测就是 `""`）。直接把 `""` 反序列化
//! 到 `f64` 会失败，因此所有数值字段都必须经由这里的函数解析。

use serde::{Deserialize, Deserializer};

/// 必填数值字段（字符串形式）。
pub fn num<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    let raw = String::deserialize(deserializer)?;
    raw.parse::<f64>().map_err(serde::de::Error::custom)
}

/// 可选数值字段：空串与无法解析都映射为 `None`。
///
/// 刻意容忍解析失败而不是报错：OKX 偶尔会在本该是数字的位置返回占位内容，
/// 让整个快照因为一个字段而失败，对用户毫无价值。
pub fn opt_num<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<f64>, D::Error> {
    let raw = String::deserialize(deserializer)?;
    if raw.is_empty() {
        return Ok(None);
    }
    Ok(raw.parse::<f64>().ok())
}

/// 必填时间戳字段（Unix 毫秒，字符串形式）。
pub fn ts<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
    let raw = String::deserialize(deserializer)?;
    raw.parse::<i64>().map_err(serde::de::Error::custom)
}

/// 可选时间戳字段。
pub fn opt_ts<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<i64>, D::Error> {
    let raw = String::deserialize(deserializer)?;
    if raw.is_empty() {
        return Ok(None);
    }
    Ok(raw.parse::<i64>().ok())
}
