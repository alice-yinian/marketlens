//! 隐私分级（设计文档 §6.6.5）。
//!
//! 提示词要粘到外部 AI，用户必须能控制「AI 到底看到了什么」。
//!
//! ## 为什么不是「一张敏感字段清单」
//!
//! 最直觉的做法是维护一张「哪些字段是金额」的清单，装配后按清单改写。
//! 但那种清单**漏一个字段就是一次泄漏**，而且漏了没有任何症状——
//! 用户不会发现，测试也未必覆盖到新字段。
//!
//! 这里改成**自描述数值**：上下文里所有金额/数量都必须以 [`Sensitive`] 的形状出现
//! （`{"kind":"usd","value":-1.96}`），于是分级只需要递归地把 `usd` 改写成
//! `pct` 或 `unavailable`——**凡是钱，天然就在射程内**，不存在「忘了标注」。
//!
//! 配套的 [`crate::prompt::filters`] 里的 `money` / `size` 过滤器按 `kind` 渲染，
//! 所以同一份模板在 L0/L1/L2 下都能正常工作，只是数字换了形态——
//! 模板作者不需要为隐私等级写分支。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// 隐私等级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[ts(export_to = "types.ts")]
pub enum PrivacyLevel {
    /// 全量：原始金额、张数、权益全部保留。
    #[serde(rename = "L0")]
    L0,
    /// 百分比化（默认）：金额转为占账户权益百分比；权益只给量级区间。
    ///
    /// 默认值是设计文档确认的：用户不主动选就按 L1。
    #[serde(rename = "L1")]
    #[default]
    L1,
    /// 结构脱敏：只保留方向、杠杆、相对关系与市场状态，不含任何金额与数量。
    #[serde(rename = "L2")]
    L2,
}

impl PrivacyLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            PrivacyLevel::L0 => "L0",
            PrivacyLevel::L1 => "L1",
            PrivacyLevel::L2 => "L2",
        }
    }

    /// 给用户看的说明。上下文里会带一份，让 AI 知道自己看到的是脱敏数据——
    /// 否则 AI 会把「占比 0.5%」误当成「$0.5」。
    pub fn note(self) -> &'static str {
        match self {
            PrivacyLevel::L0 => "全量数据",
            PrivacyLevel::L1 => "金额已按账户权益百分比化，账户权益只给出量级区间",
            PrivacyLevel::L2 => "已结构脱敏：不含任何金额与数量，仅保留方向、杠杆与市场状态",
        }
    }
}

/// 受隐私分级影响的数值。
///
/// 序列化形状（`kind` 是标签，不是数据）：
/// - `{"kind":"usd","value":-1.96}`
/// - `{"kind":"pct","value":-0.005}`（占权益比例，不是百分数）
/// - `{"kind":"count","value":3.0}`
/// - `{"kind":"unavailable","reason":"..."}`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Sensitive {
    /// 美元金额
    Usd { value: f64 },
    /// 占账户权益的比例（L1 的形态）
    Pct { value: f64 },
    /// 数量（张 / 币）
    Count { value: f64 },
    /// 已脱敏或不可得
    Unavailable { reason: String },
}

impl Sensitive {
    pub fn usd(value: f64) -> Self {
        Sensitive::Usd { value }
    }

    pub fn count(value: f64) -> Self {
        Sensitive::Count { value }
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Sensitive::Unavailable {
            reason: reason.into(),
        }
    }

    /// 从 `Value` 还原（供递归改写使用）。不是敏感形状就返回 `None`。
    fn from_value(value: &Value) -> Option<Sensitive> {
        serde_json::from_value(value.clone()).ok()
    }
}

/// 对已装配的上下文应用隐私分级。
///
/// `equity` 是账户权益（美元），L1 需要它来算比例。
/// 权益本身不可得时传 `None`：此时金额无法百分比化，
/// **一律降级为 `unavailable`**——宁可让 AI 知道「这块拿不到」，
/// 也不能因为算不出比例就把原始金额漏出去。
pub fn apply(context: &mut Value, level: PrivacyLevel, equity: Option<f64>) {
    match level {
        PrivacyLevel::L0 => {}
        PrivacyLevel::L1 => rewrite(context, &|sensitive| match sensitive {
            Sensitive::Usd { value } => match equity {
                Some(equity) if equity > 0.0 => Sensitive::Pct {
                    value: value / equity,
                },
                _ => Sensitive::unavailable("隐私等级 L1：账户权益不可得，无法百分比化"),
            },
            // 数量可以结合价格推算仓位规模，L1 一并脱敏
            Sensitive::Count { .. } => Sensitive::unavailable("隐私等级 L1：数量可推算账户规模"),
            other => other.clone(),
        }),
        PrivacyLevel::L2 => rewrite(context, &|sensitive| match sensitive {
            Sensitive::Usd { .. } | Sensitive::Count { .. } => {
                Sensitive::unavailable("隐私等级 L2：已结构脱敏")
            }
            other => other.clone(),
        }),
    }
}

/// 递归改写上下文里所有 `Sensitive` 形状的值。
fn rewrite(value: &mut Value, transform: &dyn Fn(&Sensitive) -> Sensitive) {
    match value {
        Value::Object(map) => {
            // 先判断本节点是不是一个 Sensitive，是就整体替换（不再下钻，
            // 否则 `value` 字段会被当普通数字处理）。
            if let Some(sensitive) = Sensitive::from_value(&Value::Object(map.clone())) {
                *value = serde_json::to_value(transform(&sensitive)).unwrap_or(Value::Null);
                return;
            }
            for (_, child) in map.iter_mut() {
                rewrite(child, transform);
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                rewrite(item, transform);
            }
        }
        _ => {}
    }
}

/// 账户权益的量级区间（L1 用）。
///
/// 给区间而不是精确值：AI 做「这笔仓位相对我大不大」的判断只需要量级，
/// 而精确权益是最能定位到具体个人的数字之一。
pub fn magnitude_bucket(equity: f64) -> String {
    const BUCKETS: [(f64, &str); 6] = [
        (1_000.0, "$0–$1k"),
        (10_000.0, "$1k–$10k"),
        (50_000.0, "$10k–$50k"),
        (100_000.0, "$50k–$100k"),
        (500_000.0, "$100k–$500k"),
        (1_000_000.0, "$500k–$1M"),
    ];

    for (upper, label) in BUCKETS {
        if equity < upper {
            return label.to_string();
        }
    }
    "$1M+".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn l0_is_untouched() {
        let mut context = json!({"pnl": {"kind": "usd", "value": -1.96}});
        apply(&mut context, PrivacyLevel::L0, Some(1000.0));
        assert_eq!(context["pnl"]["kind"], "usd");
        assert_eq!(context["pnl"]["value"], -1.96);
    }

    #[test]
    fn l1_converts_money_to_share_of_equity() {
        let mut context = json!({"pnl": {"kind": "usd", "value": -20.0}});
        apply(&mut context, PrivacyLevel::L1, Some(1000.0));
        assert_eq!(context["pnl"]["kind"], "pct");
        assert!((context["pnl"]["value"].as_f64().unwrap() - (-0.02)).abs() < 1e-12);
    }

    /// 这是本模块最关键的一条安全性质：算不出比例时**宁可说不可得**，
    /// 也不能退回原始金额。
    #[test]
    fn l1_without_equity_refuses_to_leak_the_raw_amount() {
        let mut context = json!({"pnl": {"kind": "usd", "value": -1234.56}});
        apply(&mut context, PrivacyLevel::L1, None);

        assert_eq!(context["pnl"]["kind"], "unavailable");
        let serialized = context.to_string();
        assert!(
            !serialized.contains("1234"),
            "权益不可得时原始金额绝不能留在上下文里：{serialized}"
        );
    }

    #[test]
    fn l1_also_redacts_equity_of_zero_to_avoid_division() {
        let mut context = json!({"pnl": {"kind": "usd", "value": 5.0}});
        apply(&mut context, PrivacyLevel::L1, Some(0.0));
        assert_eq!(context["pnl"]["kind"], "unavailable");
    }

    #[test]
    fn l2_strips_every_amount_and_quantity() {
        let mut context = json!({
            "positions": [
                {"inst_id": "BTC-USDT-SWAP", "side": "long", "lever": 3.0,
                 "pnl": {"kind": "usd", "value": -1.96},
                 "size": {"kind": "count", "value": 3.0}},
            ],
        });
        apply(&mut context, PrivacyLevel::L2, Some(1000.0));

        let serialized = context.to_string();
        assert!(
            !serialized.contains("-1.96"),
            "L2 不该留下金额：{serialized}"
        );
        assert!(
            !serialized.contains("\"value\":3.0"),
            "L2 不该留下数量：{serialized}"
        );
        // 方向、杠杆、标的必须保留，否则提示词就废了
        assert_eq!(context["positions"][0]["side"], "long");
        assert_eq!(context["positions"][0]["lever"], 3.0);
        assert_eq!(context["positions"][0]["inst_id"], "BTC-USDT-SWAP");
    }

    #[test]
    fn rewrite_descends_into_arrays_and_nested_objects() {
        let mut context = json!({
            "a": {"b": [{"c": {"kind": "usd", "value": 10.0}}]},
        });
        apply(&mut context, PrivacyLevel::L2, Some(100.0));
        assert_eq!(context["a"]["b"][0]["c"]["kind"], "unavailable");
    }

    /// 已经不可得的字段不该被再改写一次（原因会被覆盖成泛泛的「已结构脱敏」，
    /// 丢掉真正有用的原因，比如「Rubik 指标对 2023 年之前不可得」）。
    #[test]
    fn already_unavailable_keeps_its_specific_reason() {
        let mut context = json!({
            "indicator": {"kind": "unavailable", "reason": "Rubik 指标仅提供近 30 天"},
        });
        apply(&mut context, PrivacyLevel::L2, Some(1000.0));
        assert_eq!(context["indicator"]["reason"], "Rubik 指标仅提供近 30 天");
    }

    #[test]
    fn magnitude_bucket_covers_boundaries() {
        assert_eq!(magnitude_bucket(0.0), "$0–$1k");
        assert_eq!(magnitude_bucket(999.99), "$0–$1k");
        assert_eq!(magnitude_bucket(1_000.0), "$1k–$10k");
        assert_eq!(magnitude_bucket(9_999.0), "$1k–$10k");
        assert_eq!(magnitude_bucket(25_000.0), "$10k–$50k");
        assert_eq!(magnitude_bucket(750_000.0), "$500k–$1M");
        assert_eq!(magnitude_bucket(5_000_000.0), "$1M+");
    }
}
