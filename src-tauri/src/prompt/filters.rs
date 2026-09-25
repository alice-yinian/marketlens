//! 模板过滤器（设计文档 §6.6.2）。
//!
//! 这些过滤器不是语法糖，而是**把「数据不可得」和「数值格式化」这两件事
//! 从模板作者手里收回来**：用户不必（也不能）自己决定缺失数据长什么样。
//!
//! `na` 尤其重要：AI 看到空白会自行编造，看到「[数据不可得：原因]」才会谨慎。

use minijinja::{Error, ErrorKind, Value};

use crate::prompt::privacy::Sensitive;

/// 「数据不可得」的渲染前缀。
///
/// 单独抽出来是因为它同时被三处使用：`na` 过滤器、隐私脱敏、以及**安全测试**
/// （L2 的输出里不该出现金额，但必须出现这个前缀）。
pub const UNAVAILABLE_PREFIX: &str = "[数据不可得：";

pub fn unavailable(reason: &str) -> String {
    format!("{UNAVAILABLE_PREFIX}{reason}]")
}

/// 注册全部过滤器。
pub fn register(environment: &mut minijinja::Environment<'static>) {
    environment.add_filter("na", filter_na);
    environment.add_filter("usd", filter_usd);
    environment.add_filter("usd_plain", filter_usd_plain);
    environment.add_filter("pct", filter_pct);
    environment.add_filter("num", filter_num);
    environment.add_filter("price", filter_price);
    environment.add_filter("ts", filter_ts);
    environment.add_filter("ts_date", filter_ts_date);
    environment.add_filter("ago", filter_ago);
    environment.add_filter("dur", filter_dur);
    environment.add_filter("side", filter_side);
    environment.add_filter("margin", filter_margin);
    environment.add_filter("money", filter_money);
    environment.add_filter("size", filter_size);
    environment.add_filter("table", filter_table);
    environment.add_filter("json", filter_json);
}

/// `{{ value | na("原因") }}` —— 缺失值渲染成显式的不可得声明。
///
/// 非缺失值原样透传，所以模板可以写成 `{{ x | na("...") }}` 而不用担心 x 存在时被改坏。
fn filter_na(value: Value, reason: Option<String>) -> Value {
    if value.is_none() || value.is_undefined() {
        let reason = reason.unwrap_or_else(|| "原因未说明".to_string());
        Value::from(unavailable(&reason))
    } else {
        value
    }
}

/// `1_500_000` → `1.50M`。用于展示级金额。
fn filter_usd(value: Value) -> Result<Value, Error> {
    if let Some(passthrough) = passthrough_unavailable(&value) {
        return Ok(passthrough);
    }
    let amount = to_f64(&value)?;
    Ok(Value::from(format_usd(amount)))
}

/// 带符号的原始金额（`+$1.96` / `-$1.96`）。
fn filter_usd_plain(value: Value) -> Result<Value, Error> {
    if let Some(passthrough) = passthrough_unavailable(&value) {
        return Ok(passthrough);
    }
    let amount = to_f64(&value)?;
    Ok(Value::from(format_signed_usd(amount)))
}

/// `0.0583` → `+5.83%`。`value` 是小数比例，不是百分数。
fn filter_pct(value: Value, precision: Option<usize>) -> Result<Value, Error> {
    if let Some(passthrough) = passthrough_unavailable(&value) {
        return Ok(passthrough);
    }
    let ratio = to_f64(&value)?;
    Ok(Value::from(format_pct(ratio, precision.unwrap_or(2))))
}

/// 固定小数位。
fn filter_num(value: Value, precision: Option<usize>) -> Result<Value, Error> {
    if let Some(passthrough) = passthrough_unavailable(&value) {
        return Ok(passthrough);
    }
    let number = to_f64(&value)?;
    let precision = precision.unwrap_or(2);
    Ok(Value::from(format!("{number:.precision$}")))
}

/// 价格：按量级自适应小数位。
///
/// 直接渲染 `f64` 会得到 `2718.7999999999997` 这种全精度输出——既难看，
/// 又会让 AI 误以为精度真的到小数点后 16 位。但固定小数位也不行：
/// BTC 要 2 位，DOGE 要 4 位，某些小币要 8 位。
///
/// 分档按主流交易所的报价精度来定，而不是按数学上的有效数字。
fn filter_price(value: Value) -> Result<Value, Error> {
    if let Some(passthrough) = passthrough_unavailable(&value) {
        return Ok(passthrough);
    }
    let price = to_f64(&value)?;
    Ok(Value::from(format_price(price)))
}

/// 毫秒时间戳 → 本地时间 `YYYY-MM-DD HH:MM`。
fn filter_ts(value: Value) -> Result<Value, Error> {
    if let Some(passthrough) = passthrough_unavailable(&value) {
        return Ok(passthrough);
    }
    let millis = to_i64(&value)?;
    Ok(Value::from(format_timestamp(millis, "%Y-%m-%d %H:%M")))
}

/// 毫秒时间戳 → 本地日期 `YYYY-MM-DD`。
fn filter_ts_date(value: Value) -> Result<Value, Error> {
    if let Some(passthrough) = passthrough_unavailable(&value) {
        return Ok(passthrough);
    }
    let millis = to_i64(&value)?;
    Ok(Value::from(format_timestamp(millis, "%Y-%m-%d")))
}

/// 毫秒时间戳 → 相对时间（`3 小时前`）。
fn filter_ago(value: Value) -> Result<Value, Error> {
    if let Some(passthrough) = passthrough_unavailable(&value) {
        return Ok(passthrough);
    }
    let millis = to_i64(&value)?;
    Ok(Value::from(format_ago(millis)))
}

/// 毫秒时长 → `1 天 2 小时`。
fn filter_dur(value: Value) -> Result<Value, Error> {
    if let Some(passthrough) = passthrough_unavailable(&value) {
        return Ok(passthrough);
    }
    let millis = to_i64(&value)?;
    Ok(Value::from(format_duration(millis)))
}

/// `long` / `short` / `net` → 中文。
fn filter_side(value: Value) -> Result<Value, Error> {
    if let Some(passthrough) = passthrough_unavailable(&value) {
        return Ok(passthrough);
    }
    let raw = value
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| Error::new(ErrorKind::InvalidOperation, "side 过滤器需要字符串"))?;
    Ok(Value::from(crate::position::stats::direction_label(&raw)))
}

/// 保证金模式：`cross` / `isolated` → 中文。
///
/// 文案与前端 `strings.ts` 的 `mgnMode` 一致。提示词里混着 `cross` 这种英文枚举
/// 不算错，但一份中文提示词里出现两种语言，读起来会让人怀疑是不是两套东西。
fn filter_margin(value: Value) -> Result<Value, Error> {
    if let Some(passthrough) = passthrough_unavailable(&value) {
        return Ok(passthrough);
    }
    let raw = value
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| Error::new(ErrorKind::InvalidOperation, "margin 过滤器需要字符串"))?;
    Ok(Value::from(match raw.as_str() {
        "cross" => "全仓",
        "isolated" => "逐仓",
        other => return Ok(Value::from(other.to_string())),
    }))
}

/// 渲染受隐私分级影响的**金额**。
///
/// 这是「同一份模板适配所有隐私等级」的关键：值可能是原始美元（L0）、
/// 占权益比例（L1）或已脱敏（L2），模板作者不需要为此写任何分支。
fn filter_money(value: Value) -> Result<Value, Error> {
    if let Some(passthrough) = passthrough_unavailable(&value) {
        return Ok(passthrough);
    }
    match parse_sensitive(&value)? {
        Sensitive::Usd { value } => Ok(Value::from(format_signed_usd(value))),
        Sensitive::Pct { value } => Ok(Value::from(format_pct(value, 2))),
        Sensitive::Count { value } => Ok(Value::from(format!("{value}"))),
        Sensitive::Unavailable { reason } => Ok(Value::from(unavailable(&reason))),
    }
}

/// 渲染受隐私分级影响的**数量**（张数 / 币数）。
fn filter_size(value: Value) -> Result<Value, Error> {
    if let Some(passthrough) = passthrough_unavailable(&value) {
        return Ok(passthrough);
    }
    match parse_sensitive(&value)? {
        Sensitive::Count { value } => Ok(Value::from(format!("{value}"))),
        Sensitive::Usd { value } => Ok(Value::from(format_usd(value))),
        Sensitive::Pct { value } => Ok(Value::from(format_pct(value, 2))),
        Sensitive::Unavailable { reason } => Ok(Value::from(unavailable(&reason))),
    }
}

/// 已经是「数据不可得」占位符的值直接透传。
///
/// 否则 `{{ x | na("原因") | pct }}` 会在 `na` 之后被 `pct` 拒收——
/// 而模板作者几乎必然会这么写（先兜底再格式化）。透传让这两个过滤器可以自由组合，
/// 也让同一个模板在「有数据」和「没数据」两种情况下都能渲染成功。
fn passthrough_unavailable(value: &Value) -> Option<Value> {
    value
        .as_str()
        .filter(|text| text.starts_with(UNAVAILABLE_PREFIX))
        .map(Value::from)
}

fn parse_sensitive(value: &Value) -> Result<Sensitive, Error> {
    serde_json::from_value(to_json(value)?).map_err(|_| {
        Error::new(
            ErrorKind::InvalidOperation,
            "money/size 过滤器需要受隐私分级的数值（{kind, value}）；\
             直接传原始数字会绕过隐私分级，因此这里拒绝渲染",
        )
    })
}

/// 对象数组 → Markdown 表格。
///
/// 用法：`{{ positions | table(["inst_id", "side", "pnl"]) }}`。
/// 不给列时按第一个对象的键排序，保证输出稳定（否则 HashMap 顺序会让提示词抖动）。
fn filter_table(value: Value, columns: Option<Vec<String>>) -> Result<Value, Error> {
    let json = to_json(&value)?;
    let rows = json
        .as_array()
        .ok_or_else(|| Error::new(ErrorKind::InvalidOperation, "table 过滤器需要数组"))?;

    if rows.is_empty() {
        return Ok(Value::from("（无记录）"));
    }

    let columns = match columns {
        Some(columns) if !columns.is_empty() => columns,
        _ => {
            let first = rows[0]
                .as_object()
                .ok_or_else(|| Error::new(ErrorKind::InvalidOperation, "table 的元素必须是对象"))?;
            // 排序保证输出稳定：否则键顺序会让同一份数据的提示词每次都不同，
            // 既难 diff 也白白浪费 AI 的上下文缓存。
            let mut keys: Vec<String> = first.keys().cloned().collect();
            keys.sort();
            keys
        }
    };

    let mut output = String::new();
    output.push_str("| ");
    output.push_str(&columns.join(" | "));
    output.push_str(" |\n|");
    for _ in &columns {
        output.push_str(" --- |");
    }
    output.push('\n');

    for row in rows {
        output.push_str("| ");
        let cells: Vec<String> = columns
            .iter()
            .map(|column| render_cell(row.get(column)))
            .collect();
        output.push_str(&cells.join(" | "));
        output.push_str(" |\n");
    }

    Ok(Value::from(output))
}

/// 美化 JSON，便于把结构化数据塞进提示词。
fn filter_json(value: Value, indent: Option<usize>) -> Result<Value, Error> {
    let indent = indent.unwrap_or(2);
    let serialized = serde_json::to_value(&value)
        .map_err(|err| Error::new(ErrorKind::InvalidOperation, err.to_string()))?;

    let mut buffer = Vec::new();
    let indentation = vec![b' '; indent];
    let formatter = serde_json::ser::PrettyFormatter::with_indent(&indentation);
    let mut serializer = serde_json::Serializer::with_formatter(&mut buffer, formatter);
    serde::Serialize::serialize(&serialized, &mut serializer)
        .map_err(|err| Error::new(ErrorKind::InvalidOperation, err.to_string()))?;

    Ok(Value::from(String::from_utf8_lossy(&buffer).to_string()))
}

// ---------------------------------------------------------------- 格式化实现
// 这些函数同时被 UI 侧的文本渲染复用，所以逻辑放在函数里而不是内联进过滤器。

fn render_cell(value: Option<&serde_json::Value>) -> String {
    match value {
        None | Some(serde_json::Value::Null) => "—".to_string(),
        Some(serde_json::Value::String(text)) => text.clone(),
        Some(serde_json::Value::Number(number)) => number.to_string(),
        Some(serde_json::Value::Bool(flag)) => flag.to_string(),
        Some(other) => other.to_string(),
    }
}

pub fn format_usd(amount: f64) -> String {
    let magnitude = amount.abs();
    let sign = if amount < 0.0 { "-" } else { "" };

    if magnitude >= 1_000_000_000.0 {
        format!("{sign}${:.2}B", magnitude / 1_000_000_000.0)
    } else if magnitude >= 1_000_000.0 {
        format!("{sign}${:.2}M", magnitude / 1_000_000.0)
    } else if magnitude >= 1_000.0 {
        format!("{sign}${:.2}K", magnitude / 1_000.0)
    } else {
        format!("{sign}${magnitude:.2}")
    }
}

pub fn format_signed_usd(amount: f64) -> String {
    format!(
        "{}{}",
        if amount < 0.0 { "-" } else { "+" },
        format_usd(amount.abs())
    )
}

/// 价格的展示精度：按量级分档。
///
/// 分档依据是各交易所实际的报价精度（tick size），不是有效数字：
/// BTC 报 0.1 档，DOGE 报 0.00001 档。固定 2 位会把小币全渲染成 0.00。
pub fn format_price(price: f64) -> String {
    let magnitude = price.abs();
    let decimals = if magnitude >= 1.0 {
        2
    } else if magnitude >= 0.01 {
        4
    } else if magnitude >= 0.0001 {
        6
    } else {
        8
    };
    format!("{price:.decimals$}")
}

pub fn format_pct(ratio: f64, precision: usize) -> String {
    format!("{:+.precision$}%", ratio * 100.0)
}

fn format_timestamp(millis: i64, format: &str) -> String {
    match chrono::DateTime::from_timestamp_millis(millis) {
        Some(utc) => utc.with_timezone(&chrono::Local).format(format).to_string(),
        None => format!("[时间戳越界：{millis}]"),
    }
}

fn format_ago(millis: i64) -> String {
    let now = chrono::Utc::now().timestamp_millis();
    let delta = now - millis;

    if delta < 0 {
        return format_duration(-delta) + "后";
    }
    if delta < 60_000 {
        return "刚刚".to_string();
    }
    format_duration(delta) + "前"
}

pub fn format_duration(millis: i64) -> String {
    let seconds = millis / 1000;
    if seconds < 60 {
        return format!("{seconds} 秒");
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes} 分钟");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours} 小时");
    }
    let days = hours / 24;
    let remaining_hours = hours % 24;
    if remaining_hours == 0 {
        format!("{days} 天")
    } else {
        format!("{days} 天 {remaining_hours} 小时")
    }
}

// ------------------------------------------------------------------ 数值转换

fn to_json(value: &Value) -> Result<serde_json::Value, Error> {
    serde_json::to_value(value)
        .map_err(|err| Error::new(ErrorKind::InvalidOperation, err.to_string()))
}

fn to_f64(value: &Value) -> Result<f64, Error> {
    let json = to_json(value)?;
    json.as_f64().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidOperation,
            format!("需要一个数字，实际拿到 {}", value.kind()),
        )
    })
}

fn to_i64(value: &Value) -> Result<i64, Error> {
    let json = to_json(value)?;
    json.as_i64().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidOperation,
            format!("需要一个整数，实际拿到 {}", value.kind()),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usd_scales_by_magnitude() {
        assert_eq!(format_usd(1234.5), "$1.23K");
        assert_eq!(format_usd(2_500_000.0), "$2.50M");
        assert_eq!(format_usd(-3_000_000_000.0), "-$3.00B");
        assert_eq!(format_usd(12.345), "$12.35");
        assert_eq!(format_usd(0.0), "$0.00");
    }

    #[test]
    fn signed_usd_keeps_sign_for_gains_and_losses() {
        assert_eq!(format_signed_usd(1.5), "+$1.50");
        assert_eq!(format_signed_usd(-1.5), "-$1.50");
        assert_eq!(format_signed_usd(0.0), "+$0.00");
    }

    #[test]
    fn duration_covers_each_unit_boundary() {
        assert_eq!(format_duration(45_000), "45 秒");
        assert_eq!(format_duration(90_000), "1 分钟");
        assert_eq!(format_duration(3_600_000), "1 小时");
        assert_eq!(format_duration(86_400_000), "1 天");
        // 不足一天的余数要保留小时，否则「1 天 2 小时」会退化成「1 天」
        assert_eq!(format_duration(93_600_000), "1 天 2 小时");
    }

    #[test]
    fn timestamp_out_of_range_is_explicit_not_panicking() {
        assert!(format_timestamp(i64::MAX, "%Y").starts_with("[时间戳越界"));
    }
}

#[cfg(test)]
mod template_tests {
    //! 这些测试刻意走**完整渲染路径**（而不只是调函数），
    //! 因为过滤器最容易出错的不是逻辑，而是注册名字写错——那样模板会静默报「过滤器不存在」，
    //! 或者更糟：名字碰巧撞上内置过滤器，用错了实现却看不出来。

    use crate::prompt::render::render;
    use serde_json::json;

    fn rendered(body: &str, context: &serde_json::Value) -> String {
        render(body, context).unwrap_or_else(|err| panic!("渲染失败：{err}"))
    }

    #[test]
    fn na_marks_missing_data_explicitly() {
        // 三种「不可得」都要走同一个出口：null / 字段缺失由静态检查拦下 /
        // 显式 null 由 na 渲染成声明。
        assert_eq!(
            rendered("{{ v | na(\"接口未返回\") }}", &json!({"v": null})),
            "[数据不可得：接口未返回]"
        );
        // 非缺失值必须原样透传，否则模板作者不敢加 na
        assert_eq!(rendered("{{ v | na(\"x\") }}", &json!({"v": 42})), "42");
        assert_eq!(rendered("{{ v | na(\"x\") }}", &json!({"v": "abc"})), "abc");
        // 不给原因也要有兜底文案，不能渲染成空的
        assert_eq!(
            rendered("{{ v | na }}", &json!({"v": null})),
            "[数据不可得：原因未说明]"
        );
    }

    #[test]
    fn numeric_filters_render_expected_shapes() {
        let context = json!({
            "big": 2_500_000.0,
            "small": 12.345,
            "ratio": 0.0583,
            "negative": -0.1234,
        });
        assert_eq!(rendered("{{ big | usd }}", &context), "$2.50M");
        assert_eq!(rendered("{{ small | usd }}", &context), "$12.35");
        assert_eq!(rendered("{{ small | usd_plain }}", &context), "+$12.35");
        assert_eq!(rendered("{{ ratio | pct }}", &context), "+5.83%");
        assert_eq!(rendered("{{ negative | pct }}", &context), "-12.34%");
        assert_eq!(rendered("{{ ratio | pct(1) }}", &context), "+5.8%");
        assert_eq!(rendered("{{ small | num(4) }}", &context), "12.3450");
        assert_eq!(rendered("{{ small | num }}", &context), "12.35");
    }

    #[test]
    fn time_filters_are_localised_and_stable() {
        // 用固定时间戳，断言的是格式而不是具体时区偏移
        let context = json!({"at": 1_700_000_000_000_i64});
        let stamp = rendered("{{ at | ts }}", &context);
        assert!(stamp.starts_with("2023-11-1"), "本地时间格式不对：{stamp}");
        assert_eq!(rendered("{{ at | ts_date }}", &context).len(), 10);
        assert!(rendered("{{ at | ago }}", &context).ends_with("前"));
    }

    #[test]
    fn duration_filter_is_available_to_templates() {
        assert_eq!(
            rendered("{{ d | dur }}", &json!({"d": 93_600_000})),
            "1 天 2 小时"
        );
    }

    #[test]
    fn side_filter_reuses_the_same_mapping_as_statistics() {
        // 与 stats::direction_label 共用实现：两处若各写一份，早晚会漂移
        assert_eq!(rendered("{{ s | side }}", &json!({"s": "long"})), "多");
        assert_eq!(rendered("{{ s | side }}", &json!({"s": "short"})), "空");
        assert_eq!(rendered("{{ s | side }}", &json!({"s": "net"})), "未知");
    }

    #[test]
    fn table_renders_markdown_with_stable_column_order() {
        let context = json!({
            "rows": [
                {"inst_id": "BTC-USDT-SWAP", "side": "多", "pnl": 1.5},
                {"inst_id": "ETH-USDT-SWAP", "side": "空", "pnl": null},
            ]
        });
        let output = rendered("{{ rows | table }}", &context);

        // 列按字母序：inst_id, pnl, side —— 保证同一份数据每次渲染完全一致
        assert!(
            output.starts_with("| inst_id | pnl | side |"),
            "实际：{output}"
        );
        assert!(output.contains("BTC-USDT-SWAP"), "实际：{output}");
        // 单元格里的 null 用破折号，而不是留空（留空会让 AI 以为是 0）
        assert!(output.contains("| — |"), "null 单元格应有占位符：{output}");
    }

    #[test]
    fn table_accepts_explicit_columns() {
        let context = json!({"rows": [{"a": 1, "b": 2, "c": 3}]});
        let output = rendered("{{ rows | table([\"c\", \"a\"]) }}", &context);
        assert!(output.starts_with("| c | a |"), "实际：{output}");
        assert!(!output.contains("| b |"), "未指定的列不该出现：{output}");
    }

    #[test]
    fn table_on_empty_list_is_explicit() {
        let output = rendered("{{ rows | table }}", &json!({"rows": []}));
        assert_eq!(output, "（无记录）");
    }

    #[test]
    fn json_filter_pretty_prints() {
        let output = rendered("{{ obj | json }}", &json!({"obj": {"a": 1}}));
        assert!(output.contains("\"a\": 1"), "实际：{output}");
    }

    #[test]
    fn wrong_type_reports_the_filter_and_the_kind() {
        let result = render("{{ v | usd }}", &json!({"v": "not a number"}));
        let message = result.unwrap_err().to_string();
        assert!(message.contains("数字"), "应说明期望的类型：{message}");
    }
}

#[cfg(test)]
mod composition_tests {
    //! 过滤器组合。模板作者最自然的写法是「先 na 兜底、再格式化」，
    //! 如果这两个不能组合，用户就得在每个字段上写 {% if %}，模板会长到没法维护。

    use crate::prompt::render::render;
    use serde_json::json;

    #[test]
    fn na_composes_with_numeric_filters() {
        // 有数据：na 透传原值，pct 正常格式化
        let output = render("{{ v | na(\"没有\") | pct }}", &json!({"v": 0.0523}))
            .expect("有数据时应正常格式化");
        assert_eq!(output, "+5.23%");

        // 没数据：na 产出占位符，pct 必须透传而不是报错
        let output = render("{{ v | na(\"接口未返回\") | pct }}", &json!({"v": null}))
            .expect("缺数据时不应报错");
        assert_eq!(output, "[数据不可得：接口未返回]");
    }

    #[test]
    fn na_composes_with_time_and_usd_filters() {
        for (body, expected) in [
            ("{{ v | na(\"x\") | ts }}", "[数据不可得：x]"),
            ("{{ v | na(\"x\") | ago }}", "[数据不可得：x]"),
            ("{{ v | na(\"x\") | dur }}", "[数据不可得：x]"),
            ("{{ v | na(\"x\") | usd }}", "[数据不可得：x]"),
            ("{{ v | na(\"x\") | num(3) }}", "[数据不可得：x]"),
            ("{{ v | na(\"x\") | side }}", "[数据不可得：x]"),
        ] {
            let output = render(body, &json!({"v": null}))
                .unwrap_or_else(|err| panic!("{body} 渲染失败：{err}"));
            assert_eq!(output, expected, "{body}");
        }
    }

    /// 幂等：占位符经过多层过滤器仍应保持原样，不该被二次包装。
    #[test]
    fn unavailable_placeholder_survives_repeated_filtering() {
        let output = render(
            "{{ v | na(\"第一次\") | na(\"第二次\") | pct | usd }}",
            &json!({"v": null}),
        )
        .expect("应能渲染");
        assert_eq!(output, "[数据不可得：第一次]");
    }
}

#[cfg(test)]
mod price_tests {
    use super::*;

    /// 分档依据是交易所报价精度，不是有效数字。
    #[test]
    fn price_precision_follows_magnitude() {
        // 大额：2 位（BTC/ETH 的报价精度）
        assert_eq!(format_price(84_547.0), "84547.00");
        assert_eq!(format_price(2_718.8), "2718.80");
        assert_eq!(format_price(120.44), "120.44");
        // 1 以上仍是 2 位
        assert_eq!(format_price(1.61), "1.61");
        assert_eq!(format_price(5.05), "5.05");
        // 小于 1：4 位
        assert_eq!(format_price(0.5432), "0.5432");
        assert_eq!(format_price(0.0984), "0.0984");
        // 更小：6 / 8 位
        assert_eq!(format_price(0.001234), "0.001234");
        assert_eq!(format_price(0.00000123), "0.00000123");
    }

    /// 这条是加 `price` 过滤器的直接原因：直接渲染 f64 会带出 16 位小数。
    #[test]
    fn raw_float_precision_is_never_leaked() {
        let ugly = 65.54630065390165_f64;
        assert_eq!(format_price(ugly), "65.55");
        assert!(
            !format_price(ugly).contains("65390165"),
            "不该出现 f64 的原始精度"
        );
    }
}
