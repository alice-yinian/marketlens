//! 诊断导出（设计文档 §13「诊断导出」）。
//!
//! 一键导出脱敏诊断包：日志 + 版本 + 配置 + 缓存统计，**不含密钥与仓位明细**。
//!
//! ## 脱敏不是「注意别写进去」，而是三道结构性防线
//!
//! 1. **根本不读密钥库**：本模块不接触 `Vault`，连密文都拿不到。
//!    凭据只输出「有几条、什么环境、什么权限」，**连打码后的 API Key 都不输出**——
//!    打码后的前 4 位后 4 位对定位「是不是同一个 key」足够，但对不该看到的人
//!    也是额外信息，而诊断包里本来也不需要它。
//! 2. **不导出任何业务行**：账户、仓位、留痕、历史仓位一律只统计行数。
//!    「有 3 条持仓」能帮上忙，「持仓是什么」帮不上，只会泄漏。
//! 3. **日志逐行过脱敏**：长随机串、疑似密钥的键值对、家目录路径。
//!
//! 三道防线是**叠加**的：任何一道单独失效，另外两道仍然拦得住。这比
//! 「记得不要把金额写进日志」这种依赖记忆的约定可靠得多。
//!
//! 另外，诊断包会**如实列出本次脱敏替换了什么**（[`DiagnosticBundle::redactions`]）——
//! 让用户能自己确认「它到底遮了哪些东西」，而不是只能相信我们。

use serde::Serialize;
use ts_rs::TS;

use crate::error::AppResult;
use crate::storage::Db;
use crate::system::AppInfo;
use crate::system::cache::{self, CacheStats};

/// 日志尾部最多带多少行。
///
/// 200 行足够覆盖一次失败操作的前后文，又不至于让诊断包大到不好粘贴。
const LOG_TAIL_LINES: usize = 200;

/// 脱敏诊断包。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct DiagnosticBundle {
    #[ts(type = "number")]
    pub generated_at: i64,
    pub app: AppInfo,
    pub cache: CacheStats,
    /// 非敏感的设置项
    pub settings: Vec<SettingEntry>,
    /// 凭据概览：**只有数量与属性，没有任何密钥信息**
    pub credentials: CredentialSummary,
    /// 脱敏后的日志尾部
    pub log_tail: Vec<String>,
    /// 日志文件路径（便于用户自己去找完整日志）
    pub log_file: Option<String>,
    /// 本次脱敏替换掉了哪些模式（透明性：让用户能核对）
    pub redactions: Vec<RedactionNote>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct SettingEntry {
    pub key: String,
    pub value: String,
}

/// 凭据概览。刻意**不含** api_key 的任何片段。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct CredentialSummary {
    #[ts(type = "number")]
    pub count: usize,
    /// 形如 `demo/read_only` 的属性串
    pub entries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct RedactionNote {
    /// 模式的人话说明
    pub pattern: String,
    #[ts(type = "number")]
    pub hits: usize,
}

/// 组装诊断包。
///
/// `log_file` 由调用方给出（它需要 `AppHandle`，本模块不依赖 Tauri 类型，
/// 这样才能在测试里直接跑）。
pub async fn build(
    db: &Db,
    app: AppInfo,
    log_file: Option<std::path::PathBuf>,
) -> AppResult<DiagnosticBundle> {
    let mut redactions: Vec<RedactionNote> = Vec::new();
    let mut hits = [0usize; 3];

    let (log_tail, log_path) = match &log_file {
        Some(path) => match tokio::fs::read_to_string(path).await {
            Ok(content) => {
                let lines: Vec<String> = content
                    .lines()
                    .rev()
                    .take(LOG_TAIL_LINES)
                    .map(|line| redact_line(line, &mut hits))
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                (lines, Some(path.display().to_string()))
            }
            // 读不到日志不该让诊断失败——用户要诊断的往往正是「它坏了」，
            // 这时候还因为读不到日志而报错，等于在伤口上撒盐。
            Err(err) => (
                vec![format!("[无法读取日志文件：{err}]")],
                Some(path.display().to_string()),
            ),
        },
        None => (vec!["[本次运行没有日志文件]".to_string()], None),
    };

    for (pattern, count) in REDACTION_PATTERNS
        .iter()
        .map(|pattern| pattern.label)
        .zip(hits)
    {
        if count > 0 {
            redactions.push(RedactionNote {
                pattern: pattern.to_string(),
                hits: count,
            });
        }
    }

    Ok(DiagnosticBundle {
        generated_at: crate::storage::now_ms(),
        app,
        cache: cache::stats(db).await?,
        settings: settings_snapshot(db).await?,
        credentials: credential_summary(db).await?,
        log_tail,
        log_file: log_path,
        redactions,
    })
}

/// 只导出**非敏感**设置。
///
/// 用白名单而不是黑名单：新增设置项时如果忘了分类，白名单会让它**默认不出现**，
/// 黑名单会让它**默认泄漏**。前者是「少了一条信息」，后者是一次事故。
async fn settings_snapshot(db: &Db) -> AppResult<Vec<SettingEntry>> {
    const WHITELIST: &[&str] = &["watchlist", "onboarding_done"];

    let mut entries = Vec::new();
    for key in WHITELIST {
        if let Some(value) = db.get_setting(key).await? {
            entries.push(SettingEntry {
                key: (*key).to_string(),
                value,
            });
        }
    }
    Ok(entries)
}

async fn credential_summary(db: &Db) -> AppResult<CredentialSummary> {
    let credentials = db.list_credentials().await?;
    Ok(CredentialSummary {
        count: credentials.len(),
        entries: credentials
            .iter()
            .map(|credential| {
                format!(
                    "{}/{}",
                    credential.env,
                    credential.permissions.as_deref().unwrap_or("权限未知")
                )
            })
            .collect(),
    })
}

// ------------------------------------------------------------------ 日志脱敏

/// 脱敏规则。
struct RedactionPattern {
    label: &'static str,
    /// 命中即替换成这个
    replacement: &'static str,
    matches: fn(&str) -> Vec<(usize, usize)>,
}

const REDACTION_PATTERNS: &[RedactionPattern] = &[
    RedactionPattern {
        label: "疑似密钥的长随机串（20 位以上字母数字）",
        replacement: "[已脱敏:长随机串]",
        matches: find_long_tokens,
    },
    RedactionPattern {
        label: "家目录绝对路径（会暴露用户名）",
        replacement: "[已脱敏:家目录]",
        matches: find_home_paths,
    },
    RedactionPattern {
        label: "疑似凭据的键值对",
        replacement: "[已脱敏:凭据值]",
        matches: find_secret_assignments,
    },
];

/// 对一行日志做脱敏，并累计各规则的命中次数。
pub fn redact_line(line: &str, hits: &mut [usize; 3]) -> String {
    let mut output = line.to_string();

    for (index, pattern) in REDACTION_PATTERNS.iter().enumerate() {
        let spans = (pattern.matches)(&output);
        if spans.is_empty() {
            continue;
        }
        hits[index] += spans.len();

        // 从后往前替换：否则前面的替换会让后面的下标全部偏移。
        let mut result = output.clone();
        for (start, end) in spans.into_iter().rev() {
            result.replace_range(start..end, pattern.replacement);
        }
        output = result;
    }

    output
}

/// 找 20 位以上的字母数字串。
///
/// OKX 的 API Key 是 36 位、Secret 是 32 位，都远超这个阈值；
/// 而正常的日志内容（时间戳、数量、标的名）不会有这么长的连续字母数字。
fn find_long_tokens(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut start: Option<usize> = None;

    for (index, byte) in bytes.iter().enumerate() {
        if byte.is_ascii_alphanumeric() {
            if start.is_none() {
                start = Some(index);
            }
        } else if let Some(begin) = start.take() {
            if index - begin >= 20 {
                spans.push((begin, index));
            }
        }
    }
    if let Some(begin) = start {
        if bytes.len() - begin >= 20 {
            spans.push((begin, bytes.len()));
        }
    }
    spans
}

/// 找家目录绝对路径。
///
/// 路径本身不敏感，但 `/home/alice/...` 或 `/Users/bob/...` 会暴露用户名——
/// 而用户名是社工攻击的起点。替换成 `~` 既保住可读性，又去掉了身份信息。
fn find_home_paths(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    for prefix in ["/home/", "/Users/"] {
        let mut search_from = 0;
        while let Some(found) = text[search_from..].find(prefix) {
            let start = search_from + found;
            // 从前缀后第一个字符开始，取到下一个空白或路径分隔符之后
            let after = start + prefix.len();
            let end = text[after..]
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
                .map(|offset| after + offset)
                .unwrap_or(text.len());
            spans.push((start, end));
            search_from = end;
        }
    }
    spans.sort();
    spans
}

/// 找 `api_key=xxx` / `secret: xxx` / `passphrase=xxx` 这类赋值。
///
/// 兜底用：即使密钥不是长随机串（比如用户自己起了个短密码），
/// 只要它出现在带敏感键名的赋值里，也会被遮掉。
fn find_secret_assignments(text: &str) -> Vec<(usize, usize)> {
    const SENSITIVE_KEYS: &[&str] = &[
        "api_key",
        "api-key",
        "apikey",
        "secret",
        "secret_key",
        "passphrase",
        "password",
        "token",
        "authorization",
    ];

    let lower = text.to_ascii_lowercase();
    let mut spans = Vec::new();

    for key in SENSITIVE_KEYS {
        let mut search_from = 0;
        while let Some(found) = lower[search_from..].find(key) {
            let key_start = search_from + found;
            let after_key = key_start + key.len();

            // 键名后面必须是分隔符，否则 `tokens` 会被误判成 `token`
            let rest = &text[after_key..];
            let Some(separator_offset) = rest.find(['=', ':', ' ']) else {
                break;
            };
            if separator_offset > 1 {
                // 键名后跟了一长串字符，说明不是这个键
                search_from = after_key;
                continue;
            }

            let value_start = after_key
                + rest[separator_offset..]
                    .find(|c: char| !matches!(c, '=' | ':' | ' ' | '"' | '\''))
                    .unwrap_or(separator_offset);
            let value_end = text[value_start..]
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
                .map(|offset| value_start + offset)
                .unwrap_or(text.len());

            if value_end > value_start {
                spans.push((value_start, value_end));
            }
            search_from = value_end.max(after_key);
        }
    }

    spans.sort();
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn redact(line: &str) -> String {
        let mut hits = [0usize; 3];
        redact_line(line, &mut hits)
    }

    #[test]
    fn long_random_tokens_are_masked() {
        // OKX API Key 的真实形态：36 位字母数字
        let line = "凭据已保存 key=FAKEKEYNOTAREALKEY000000000000000 ok";
        let redacted = redact(line);
        assert!(
            !redacted.contains("FAKEKEYNOTAREALKEY000000000000000"),
            "长随机串必须被遮掉：{redacted}"
        );
        assert!(redacted.contains("[已脱敏:长随机串]"));
        // 其余内容要保留，否则日志就没用了
        assert!(redacted.contains("凭据已保存"));
        assert!(redacted.contains("ok"));
    }

    /// 阈值是 20：正常内容不该被误伤。
    #[test]
    fn short_tokens_are_not_masked() {
        let line = "实盘快照已刷新 instruments=3 warnings=0";
        assert_eq!(redact(line), line);

        // 19 位不遮，20 位才遮
        assert!(!redact("abc1234567890123456").contains("已脱敏"));
        assert!(redact("abc12345678901234567").contains("已脱敏"));
    }

    #[test]
    fn home_paths_are_masked() {
        let redacted = redact("数据库就绪 dir=/home/alice/.local/share/com.marketlens.app");
        assert!(
            !redacted.contains("alice"),
            "用户名不该出现在诊断包里：{redacted}"
        );
        assert!(redacted.contains("[已脱敏:家目录]"));
        assert!(redacted.contains("数据库就绪"));
    }

    #[test]
    fn macos_home_paths_are_masked_too() {
        let redacted = redact("path=/Users/bob/Library/Application Support/x");
        assert!(!redacted.contains("bob"), "{redacted}");
    }

    /// 兜底规则：密钥不是长随机串（比如用户设了短密码）时也要遮住。
    #[test]
    fn short_secrets_in_assignments_are_masked() {
        for line in [
            "probe failed api_key=abc123",
            "login secret: hunter2",
            "auth passphrase=short",
            "token = xyz",
        ] {
            let redacted = redact(line);
            assert!(
                redacted.contains("[已脱敏:凭据值]"),
                "赋值形式的凭据应被遮掉：{line} → {redacted}"
            );
        }
    }

    /// 键名必须精确匹配，否则 `tokens` 会被误伤成 `token`。
    #[test]
    fn key_names_must_match_exactly() {
        let line = "缓存 tokenize 完成，共 3 tokens";
        assert_eq!(redact(line), line, "不该把 tokenize/tokens 当成凭据");
    }

    #[test]
    fn redaction_is_idempotent() {
        let line = "key=FAKEKEYNOTAREALKEY000000000000000 dir=/home/alice/x";
        let once = redact(line);
        let twice = redact(&once);
        assert_eq!(once, twice, "重复脱敏不该继续变化");
    }
}

#[cfg(test)]
mod security {
    //! 诊断包**不含密钥与仓位明细**（设计文档 §13）。
    //!
    //! 这些测试刻意往数据库里塞真实的敏感数据，再断言它们没出现在诊断包里。
    //! 只测「脱敏函数对不对」是不够的——真正的风险是**某天有人往包里加了个字段**，
    //! 而那个字段恰好带着业务数据。所以这里断言的是**最终产物的序列化结果**。

    use super::*;
    use crate::system::AppInfo;

    const SECRET_IN_LOG: &str = "9f8e7d6c5b4a39281706f5e4d3c2b1a0";
    const API_KEY_MASKED: &str = "abcd****ef12";
    const POSITION_UPL: f64 = 12345.6789;

    fn app_info() -> AppInfo {
        AppInfo {
            app_version: "0.1.0".to_string(),
            core_version: "0.1.0".to_string(),
            schema_version: 2,
            target_os: "linux".to_string(),
        }
    }

    async fn seeded_db() -> Db {
        let db = Db::open_in_memory().await.expect("内存库创建失败");

        // 一条带真实金额的仓位留痕
        sqlx::query(
            "INSERT INTO position_trace (ts, pos_id, inst_id, pos_side, mgn_mode, contracts, upl, created_at)
             VALUES (?1, 'pos-secret', 'BTC-USDT-SWAP', 'net', 'cross', 3.0, ?2, ?1)",
        )
        .bind(crate::storage::now_ms())
        .bind(POSITION_UPL)
        .execute(db.pool_for_test())
        .await
        .expect("插入留痕失败");

        // 一条凭据元数据
        sqlx::query(
            "INSERT INTO api_credentials (id, label, env, api_key_masked, permissions, created_at)
             VALUES ('cred-1', '我的凭据', 'demo', ?1, 'read_only', ?2)",
        )
        .bind(API_KEY_MASKED)
        .bind(crate::storage::now_ms())
        .execute(db.pool_for_test())
        .await
        .expect("插入凭据失败");

        // 一条设置
        sqlx::query("INSERT INTO settings (key, value, updated_at) VALUES ('watchlist', '[\"BTC-USDT-SWAP\"]', ?1)")
            .bind(crate::storage::now_ms())
            .execute(db.pool_for_test())
            .await
            .expect("插入设置失败");

        db
    }

    /// 把诊断包序列化成 JSON —— 这正是用户会粘贴出去的东西。
    async fn bundle_json() -> String {
        let db = seeded_db().await;

        // 造一个含密钥的日志文件
        let dir = std::env::temp_dir().join(format!("ml-diag-{}", crate::storage::now_ms()));
        std::fs::create_dir_all(&dir).expect("创建临时目录失败");
        let log_path = dir.join("marketlens.log");
        std::fs::write(
            &log_path,
            format!(
                "2026-09-25 INFO 密钥库已解锁 path=/home/alice/.local/share/x\n\
                 2026-09-25 INFO 凭据已保存 secret_key={SECRET_IN_LOG}\n\
                 2026-09-25 INFO 实盘快照已刷新 instruments=3\n"
            ),
        )
        .expect("写日志失败");

        let bundle = build(&db, app_info(), Some(log_path))
            .await
            .expect("组装失败");
        let json = serde_json::to_string_pretty(&bundle).expect("序列化失败");

        std::fs::remove_dir_all(&dir).ok();
        json
    }

    #[tokio::test]
    async fn bundle_never_contains_position_amounts() {
        let json = bundle_json().await;
        assert!(
            !json.contains("12345.6789") && !json.contains("12345.68"),
            "诊断包不该含仓位金额：{json}"
        );
        // 但应当如实报告「有几条留痕」——否则诊断包就没用了
        assert!(json.contains("position_trace"), "应统计留痕行数");
        assert!(!json.contains("pos-secret"), "不该含仓位 id");
    }

    #[tokio::test]
    async fn bundle_never_contains_any_api_key_fragment() {
        let json = bundle_json().await;
        assert!(
            !json.contains(API_KEY_MASKED),
            "连打码后的 API Key 都不该出现：{json}"
        );
        assert!(!json.contains("1dd3"), "不该出现 key 的前缀");
        // 但凭据的**属性**要保留，否则排查「是不是权限问题」就没了依据
        assert!(json.contains("demo/read_only"), "应保留凭据属性：{json}");
    }

    #[tokio::test]
    async fn bundle_log_tail_is_redacted() {
        let json = bundle_json().await;
        assert!(!json.contains(SECRET_IN_LOG), "日志里的密钥必须被遮掉");
        assert!(!json.contains("alice"), "日志里的用户名必须被遮掉");
        // 日志的非敏感部分要保留
        assert!(json.contains("实盘快照已刷新"), "日志正文应保留");
    }

    #[tokio::test]
    async fn bundle_reports_what_it_redacted() {
        let json = bundle_json().await;
        assert!(
            json.contains("已脱敏"),
            "应当如实说明脱敏了什么，而不是让用户只能相信：{json}"
        );
        assert!(json.contains("redactions"), "应有 redactions 字段");
    }

    /// 设置只走白名单：将来新增的设置项默认**不出现**，而不是默认泄漏。
    #[tokio::test]
    async fn settings_are_whitelisted_not_blacklisted() {
        let db = seeded_db().await;
        sqlx::query("INSERT INTO settings (key, value, updated_at) VALUES ('some_future_secret', 'oops', ?1)")
            .bind(crate::storage::now_ms())
            .execute(db.pool_for_test())
            .await
            .expect("插入设置失败");

        let bundle = build(&db, app_info(), None).await.expect("组装失败");
        let json = serde_json::to_string(&bundle).expect("序列化失败");

        assert!(json.contains("watchlist"), "白名单内的设置应导出");
        assert!(
            !json.contains("some_future_secret"),
            "白名单外的设置默认不该出现——新增设置项忘了分类时，必须是「少一条信息」而不是「多一次泄漏」"
        );
    }
}
