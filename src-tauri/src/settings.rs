//! 用户设置。

use crate::error::{AppError, AppResult};
use crate::storage::Db;

pub const KEY_WATCHLIST: &str = "watchlist";

/// 引导完成标志的键名。
pub const KEY_ONBOARDING_DONE: &str = "onboarding_done";

/// 网络代理的键名。未设置 = 直连（并**沿用系统/环境变量代理**，见 `reqwest` 的 `system-proxy`）。
///
/// 存在 `settings` 表而不是密钥库，是个刻意的取舍：代理是**网络路径**而不是凭据，
/// 冷启动、密钥库还没解锁时就得生效（否则启动后的公共行情请求照样连不上）。
/// 代价是 URL 里的 `user:pass` 会明文落盘，因此：
/// 1. 诊断导出的设置白名单**刻意不含**这个键；
/// 2. 界面明示「本地代理通常无需凭据」；
/// 3. 日志里一律走 [`redact_proxy`]。
pub const KEY_PROXY: &str = "proxy";

/// 允许的代理协议。
///
/// 只列**实现验证过**的：`socks` 特性已启用，四种 socks 变体由 `reqwest` 按
/// 域名/地址解析语义区分（`socks5h` 把解析交给代理，正好覆盖「DNS 被污染」的场景）。
const PROXY_SCHEMES: &[&str] = &["http", "https", "socks4", "socks4a", "socks5", "socks5h"];

/// 预勾选的默认标的集（已确认：首次启动由用户自选，但给一组合理起点）。
pub const DEFAULT_WATCHLIST: &[&str] = &["BTC-USDT-SWAP", "ETH-USDT-SWAP", "SOL-USDT-SWAP"];

/// 标的数量上限（已确认）。
///
/// 直接乘进复盘请求量：3 个标的 ≈ 22 个请求，10 个标的就会到 70+。
/// 设上限是为了让「一次复盘要等多久」保持可预期。
pub const WATCHLIST_LIMIT: usize = 10;

/// 读取标的集。未设置过或配置损坏时回落到默认值。
pub async fn read_watchlist(db: &Db) -> AppResult<Vec<String>> {
    let Some(raw) = db.get_setting(KEY_WATCHLIST).await? else {
        return Ok(default_watchlist());
    };

    match serde_json::from_str::<Vec<String>>(&raw) {
        Ok(list) if !list.is_empty() => Ok(list),
        Ok(_) => Ok(default_watchlist()),
        Err(err) => {
            // 不静默吞掉：配置损坏是可诊断事件，日志里必须留痕
            tracing::warn!(%err, "watchlist 配置无法解析，回落默认值");
            Ok(default_watchlist())
        }
    }
}

/// 写入标的集。
pub async fn write_watchlist(db: &Db, list: &[String]) -> AppResult<()> {
    validate_watchlist(list)?;
    let raw = serde_json::to_string(list)
        .map_err(|err| AppError::Config(format!("标的集无法序列化：{err}")))?;
    db.set_setting(KEY_WATCHLIST, &raw).await
}

/// 校验标的集。
pub fn validate_watchlist(list: &[String]) -> AppResult<()> {
    if list.is_empty() {
        return Err(AppError::Config("至少需要选择 1 个标的".to_string()));
    }
    if list.len() > WATCHLIST_LIMIT {
        return Err(AppError::Config(format!(
            "标的集最多 {WATCHLIST_LIMIT} 个，当前 {} 个",
            list.len()
        )));
    }
    Ok(())
}

pub fn default_watchlist() -> Vec<String> {
    DEFAULT_WATCHLIST
        .iter()
        .map(std::string::ToString::to_string)
        .collect()
}

/// 首次启动引导是否已完成。
///
/// **这个标志不是可有可无的**：密钥库在每次启动时都是锁定的（我们不持久化主密码），
/// 所以「vault 未解锁」既可能是首次使用、也可能只是需要解锁。没有这个标志，
/// 前端无法区分两者，用户每次冷启动都会被逼着重走一遍 3 步引导。
pub async fn read_onboarding_done(db: &Db) -> AppResult<bool> {
    Ok(db
        .get_setting(KEY_ONBOARDING_DONE)
        .await?
        .is_some_and(|value| value == "true"))
}

pub async fn write_onboarding_done(db: &Db) -> AppResult<()> {
    db.set_setting(KEY_ONBOARDING_DONE, "true").await
}

/// 读取代理配置。未设置、空串或内容损坏时都视为「没有代理」。
///
/// 损坏时**回落到直连并留日志**，而不是让启动失败：一个填错的代理 URL
/// 不应该让应用起不来，用户还得有办法进界面去改它。
pub async fn read_proxy(db: &Db) -> AppResult<Option<String>> {
    let Some(raw) = db.get_setting(KEY_PROXY).await? else {
        return Ok(None);
    };

    match validate_proxy(&raw) {
        Ok(url) => Ok(Some(url)),
        Err(err) => {
            tracing::warn!(%err, "代理配置无法解析，本次按直连处理");
            Ok(None)
        }
    }
}

/// 写入代理配置。`None` 或空串 = 清除（回到直连 / 系统代理）。
pub async fn write_proxy(db: &Db, raw: Option<&str>) -> AppResult<Option<String>> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        db.delete_setting(KEY_PROXY).await?;
        return Ok(None);
    };

    let url = validate_proxy(raw)?;
    db.set_setting(KEY_PROXY, &url).await?;
    Ok(Some(url))
}

/// 校验并规范化代理 URL。
///
/// 只接受「scheme://host[:port]」这一种形态：路径、查询串、片段一律拒绝。
/// 这不是洁癖——用户最容易犯的错是把**订阅链接**（一个带长路径与查询串的 https URL）
/// 当成代理地址粘进来，而 `reqwest` 会欣然接受它、然后在运行时才失败。
/// 在这里拒掉，错误信息才能指出真正的问题。
pub fn validate_proxy(raw: &str) -> AppResult<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::Config("代理地址不能为空".to_string()));
    }

    let url = reqwest::Url::parse(trimmed)
        .map_err(|err| AppError::Config(format!("代理地址无法解析：{err}")))?;

    if !PROXY_SCHEMES.contains(&url.scheme()) {
        return Err(AppError::Config(format!(
            "不支持的代理协议 `{}`，可用：{}",
            url.scheme(),
            PROXY_SCHEMES.join(" / ")
        )));
    }

    match url.host_str() {
        Some(host) if !host.is_empty() => {}
        _ => return Err(AppError::Config("代理地址缺少主机名".to_string())),
    }

    if url.path() != "/" && !url.path().is_empty() {
        return Err(AppError::Config(
            "代理地址不应带路径（形如 `http://127.0.0.1:7890`）".to_string(),
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(AppError::Config("代理地址不应带查询串或片段".to_string()));
    }

    // `Url` 会把 `http://127.0.0.1:7890` 规范化成带尾斜杠的形式，
    // 存回带斜杠的值既不像用户输入、也不便比对，这里去掉。
    Ok(trimmed.trim_end_matches('/').to_string())
}

/// 生成可安全写进日志的代理地址：只保留用户名，密码换成 `***`。
///
/// 代理 URL 是最容易被顺手 `tracing::info!(%url, ...)` 打出来的东西，
/// 而它可能带着密码——所以脱敏是函数而不是约定。
pub fn redact_proxy(raw: &str) -> String {
    let Ok(mut url) = reqwest::Url::parse(raw) else {
        return "<代理地址无法解析>".to_string();
    };
    if url.password().is_some() {
        let _ = url.set_password(Some("***"));
    }
    url.as_str().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_watchlist_matches_confirmed_preselection() {
        let list = default_watchlist();
        assert_eq!(list.len(), 3);
        assert!(list.contains(&"BTC-USDT-SWAP".to_string()));
        assert!(list.contains(&"ETH-USDT-SWAP".to_string()));
        assert!(list.contains(&"SOL-USDT-SWAP".to_string()));
    }

    #[test]
    fn rejects_empty_and_oversized_watchlists() {
        assert!(validate_watchlist(&[]).is_err(), "空标的集应被拒绝");

        let too_many: Vec<String> = (0..=WATCHLIST_LIMIT)
            .map(|i| format!("C{i}-USDT-SWAP"))
            .collect();
        let err = validate_watchlist(&too_many).expect_err("超上限应被拒绝");
        assert!(
            err.to_string().contains("最多"),
            "错误信息要说清上限：{err}"
        );

        let at_limit: Vec<String> = (0..WATCHLIST_LIMIT)
            .map(|i| format!("C{i}-USDT-SWAP"))
            .collect();
        assert!(validate_watchlist(&at_limit).is_ok(), "刚好到上限应通过");
    }

    #[test]
    fn proxy_accepts_the_schemes_users_actually_run() {
        for input in [
            "http://127.0.0.1:7890",
            "https://proxy.example.com:8443",
            "socks5://127.0.0.1:1080",
            "socks5h://127.0.0.1:1080",
            "socks4://127.0.0.1:1080",
            "http://user:pass@127.0.0.1:7890",
        ] {
            assert!(
                validate_proxy(input).is_ok(),
                "应接受用户实际在用的代理形态：{input}"
            );
        }
    }

    /// `Url` 会把 `http://127.0.0.1:7890` 规范化成带尾斜杠的形式。
    /// 存回带斜杠的值与用户输入不一致，比对时容易误判，所以要去掉。
    #[test]
    fn proxy_normalizes_away_trailing_slash() {
        assert_eq!(
            validate_proxy("http://127.0.0.1:7890/").expect("应通过"),
            "http://127.0.0.1:7890"
        );
        assert_eq!(
            validate_proxy("  socks5://127.0.0.1:1080  ").expect("应通过"),
            "socks5://127.0.0.1:1080",
            "首尾空白应当被容忍并去除"
        );
    }

    /// 最容易犯的错是把**订阅链接**当成代理地址粘进来：它是个带长路径与查询串的
    /// https URL，`reqwest` 会欣然接受、然后在运行时才失败。必须在输入处就拒掉。
    #[test]
    fn proxy_rejects_things_that_are_not_proxy_addresses() {
        for (input, why) in [
            ("", "空串"),
            ("   ", "纯空白"),
            ("127.0.0.1:7890", "缺少协议"),
            ("ftp://127.0.0.1:7890", "不支持的协议"),
            ("http://", "缺少主机名"),
            (
                "https://example.com/subscribe?token=abc",
                "这是订阅链接而不是代理",
            ),
            ("http://127.0.0.1:7890/path", "带路径"),
            ("http://127.0.0.1:7890/#frag", "带片段"),
        ] {
            let err = validate_proxy(input).expect_err(&format!("必须拒绝（{why}）：{input}"));
            assert!(
                matches!(err, AppError::Config(_)),
                "配置类错误才能让前端按 code 分支处理：{err:?}"
            );
        }
    }

    /// 日志里绝不能出现代理密码——脱敏是函数，不是「记得别打」的约定。
    #[test]
    fn redact_proxy_masks_only_the_password() {
        let masked = redact_proxy("http://alice:s3cret@127.0.0.1:7890");
        assert!(!masked.contains("s3cret"), "密码必须被遮掉：{masked}");
        assert!(masked.contains("alice"), "用户名不是秘密，保留便于定位");
        assert!(masked.contains("127.0.0.1:7890"), "主机端口要保留");

        let plain = redact_proxy("socks5://127.0.0.1:1080");
        assert!(
            plain.contains("127.0.0.1:1080"),
            "无凭据时原样保留：{plain}"
        );
    }
}
