//! 模板渲染（设计文档 §6.6.1）。
//!
//! **`UndefinedBehavior::Strict` 是这里最重要的一个决定**：模板引用不存在的变量时
//! 直接报错，而不是静默渲染成空。理由是——用户拿到一段缺数据的提示词却浑然不觉，
//! 比拿到一个报错危险得多：他会把它粘给 AI，然后基于一个残缺的上下文得到结论。
//!
//! 同理，`na` 过滤器强制把「拿不到的数据」渲染成「[数据不可得：原因]」，
//! 而不是留空。AI 看到空白会自行编造，看到明确的不可得声明才会谨慎。

use minijinja::{Environment, UndefinedBehavior};

use crate::error::{AppError, AppResult};

/// 渲染模板。
///
/// `context` 是一个 `serde_json::Value`（由上下文装配阶段产出），
/// 这样模板引擎不需要知道任何领域类型。
pub fn render(body: &str, context: &serde_json::Value) -> AppResult<String> {
    let environment = environment();

    let template = environment
        .template_from_str(body)
        .map_err(|err| AppError::Template(format!("模板语法错误：{err}")))?;

    ensure_variables_exist(&template, context)?;

    template
        .render(context)
        .map_err(|err| AppError::Template(format!("渲染失败：{err}")))
}

/// 只解析模板语法，并列出它引用了哪些变量——**不渲染，因此不需要上下文**。
///
/// 与 [`render`] 分开是刻意的：渲染必须要有上下文（缺变量直接报错），
/// 而模板管理页需要的恰恰是「这段正文语法有没有问题」这个**不需要数据**的判断。
/// 用渲染去实现它，就得凭空造一份假上下文——那会让「语法没问题」和
/// 「变量名恰好和假上下文对得上」两件事混在一起。
pub fn check(body: &str) -> AppResult<Vec<String>> {
    let environment = environment();

    let template = environment
        .template_from_str(body)
        .map_err(|err| AppError::Template(format!("模板语法错误：{err}")))?;

    // 顺带把引用的变量列出来给作者看：写模板时最常问的就是「这个变量叫什么」。
    let mut names: Vec<String> = template.undeclared_variables(false).into_iter().collect();
    names.sort();
    Ok(names)
}

/// 渲染前把「模板引用了哪些上下文里没有的变量」一次性列全。
///
/// minijinja 自带的严格模式报错是 `undefined value (in <string>:1)`——**不说哪个变量**。
/// 模板一长，用户根本不知道该去哪儿找，严格模式也就失去了意义。
/// `undeclared_variables` 是静态分析，能拿到名字，而且能**一次列全**，
/// 而不是改一个、报下一个。
fn ensure_variables_exist(
    template: &minijinja::Template<'_, '_>,
    context: &serde_json::Value,
) -> AppResult<()> {
    let mut missing: Vec<String> = template
        .undeclared_variables(false)
        .into_iter()
        .filter(|name| context.get(name).is_none())
        .collect();

    if missing.is_empty() {
        return Ok(());
    }

    // 排序：报错信息稳定，测试与用户复现都容易
    missing.sort();
    Err(AppError::Template(format!(
        "模板引用了上下文中不存在的变量：{}",
        missing.join("、")
    )))
}

/// 创建一个配置好的模板环境。
fn environment() -> Environment<'static> {
    let mut environment = Environment::new();

    environment.set_undefined_behavior(UndefinedBehavior::Strict);

    // 模板里大量使用 `{% for %}` / `{% if %}`，不裁剪会留下大量空行，
    // 而提示词是给人看也给 AI 读的，空行过多会稀释信息密度。
    environment.set_trim_blocks(true);
    environment.set_lstrip_blocks(true);

    super::filters::register(&mut environment);

    environment
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 语法校验：**不需要上下文**，这正是它与 `render` 的分工。
    #[test]
    fn check_reports_syntax_errors_without_a_context() {
        let err = check("{% for x in %}").expect_err("语法错误必须报错");
        let message = err.to_string();
        assert!(message.contains("语法"), "要说清是语法问题：{message}");
    }

    /// 校验通过时列出引用的变量——写模板时最常问的就是「这个变量叫什么」。
    #[test]
    fn check_lists_referenced_variables() {
        let names = check("{{ meta.generated_at }} {{ positions | length }} {% if account %}{{ account.equity }}{% endif %}")
            .expect("应能通过");
        assert_eq!(
            names,
            vec!["account", "meta", "positions"],
            "变量名排序稳定：{names:?}"
        );
    }

    /// 空正文不是语法错误（用户刚清空编辑器时不该报红）。
    #[test]
    fn check_accepts_an_empty_body() {
        assert!(check("").expect("空正文应通过").is_empty());
    }

    #[test]
    fn renders_simple_substitution() {
        let output = render("价格 {{ price }}", &json!({"price": 42})).expect("应能渲染");
        assert_eq!(output, "价格 42");
    }

    /// 这是本模块存在的核心理由：缺变量必须报错，不能静默变空。
    #[test]
    fn missing_variable_is_an_error_not_a_blank() {
        let result = render("价格 {{ price }}", &json!({}));
        assert!(
            result.is_err(),
            "引用不存在的变量必须报错，实际却渲染成功了"
        );

        let message = result.unwrap_err().to_string();
        assert!(
            message.contains("price"),
            "错误信息应当指出是哪个变量：{message}"
        );
    }

    /// minijinja 自带的严格模式报错只说「undefined value」，不说是哪个变量。
    /// 所以这里必须由我们补上变量名，而且缺多个时要一次列全。
    #[test]
    fn error_names_every_missing_variable_at_once() {
        let result = render(
            "{{ alpha }} {{ beta }} {{ present }}",
            &json!({"present": 1}),
        );
        let message = result.unwrap_err().to_string();

        assert!(message.contains("alpha"), "应指出 alpha：{message}");
        assert!(message.contains("beta"), "应指出 beta：{message}");
        assert!(
            !message.contains("present"),
            "存在的变量不该出现在报错里：{message}"
        );
    }

    /// 上下文里值为 null 的字段**不算缺失**——那是「数据不可得」，
    /// 应当交给 `na` 过滤器渲染成显式声明，而不是报模板错。
    #[test]
    fn null_valued_variable_is_not_a_missing_variable() {
        let output = render("{{ value | na(\"接口未返回\") }}", &json!({"value": null}))
            .expect("null 值不应导致模板报错");
        assert_eq!(output, "[数据不可得：接口未返回]");
    }

    /// for 循环引入的变量不该被误判为「上下文缺失」。
    #[test]
    fn loop_variables_are_not_reported_as_missing() {
        let output = render(
            "{% for p in positions %}{{ p.inst_id }}{% endfor %}",
            &json!({"positions": [{"inst_id": "BTC"}]}),
        )
        .expect("循环变量不应被当作缺失变量");
        assert_eq!(output, "BTC");
    }

    #[test]
    fn syntax_error_is_reported_with_context() {
        let result = render("{% for x in %}broken", &json!({}));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("模板语法错误"));
    }

    #[test]
    fn loops_and_conditionals_work() {
        let context = json!({
            "positions": [
                {"inst_id": "BTC-USDT-SWAP", "pnl": 1.5},
                {"inst_id": "ETH-USDT-SWAP", "pnl": -0.5},
            ]
        });
        let body = "{% for p in positions %}{{ p.inst_id }}:{{ p.pnl }};{% endfor %}";
        let output = render(body, &context).expect("应能渲染");
        assert_eq!(output, "BTC-USDT-SWAP:1.5;ETH-USDT-SWAP:-0.5;");
    }

    #[test]
    fn nested_access_and_length_work() {
        let context = json!({"stats": {"total": 3}, "items": [1, 2]});
        let output =
            render("{{ stats.total }} 笔 / {{ items | length }} 项", &context).expect("应能渲染");
        assert_eq!(output, "3 笔 / 2 项");
    }
}
