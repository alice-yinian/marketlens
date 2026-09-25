//! 解密后的 API 凭据。
//!
//! 安全要点：这个类型刻意**不实现 `Clone`**，并手写 `Debug` 把所有密文字段遮蔽掉。
//! 凭据一旦能被 `{:?}` 打印或被随手复制，就迟早会出现在日志、错误信息或崩溃转储里。
//! 它只在使用期间存在于内存，落盘时只有加密后的 vault 条目。

/// 一组解密后的凭据。
pub struct Credentials {
    pub api_key: String,
    pub secret_key: String,
    pub passphrase: String,
    /// `true` = 模拟盘（请求需带 `x-simulated-trading: 1`）
    pub demo: bool,
}

impl Credentials {
    /// 用于展示的掩码，形如 `abcd****ef12`。
    ///
    /// 这是**唯一**允许离开进程的形态——数据库里也只存它。
    pub fn masked_api_key(&self) -> String {
        mask_secret(&self.api_key)
    }
}

/// 遮蔽任意密钥：保留首尾各 4 位，中间固定用 4 个星号。
///
/// 过短的输入整体遮蔽，避免「掩码比原文还长」这种反向泄漏。
pub fn mask_secret(secret: &str) -> String {
    let chars: Vec<char> = secret.chars().collect();
    if chars.len() <= 12 {
        return "*".repeat(chars.len().max(4));
    }
    let head: String = chars[..4].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}****{tail}")
}

/// vault 内的凭据存储形态。
///
/// 与 [`Credentials`] 分开是有意的：后者刻意不可 `Clone`、不可 `Debug`（只在使用期间
/// 存在于内存），而本类型需要 `serde` 以便落盘。**刻意不派生 `Debug`**——
/// 一旦能被 `{:?}` 打印，就迟早会出现在日志或崩溃转储里。
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredCredential {
    pub api_key: String,
    pub secret_key: String,
    pub passphrase: String,
    /// `true` = 模拟盘
    pub demo: bool,
}

impl From<StoredCredential> for Credentials {
    fn from(stored: StoredCredential) -> Self {
        Self {
            api_key: stored.api_key,
            secret_key: stored.secret_key,
            passphrase: stored.passphrase,
            demo: stored.demo,
        }
    }
}

impl From<&Credentials> for StoredCredential {
    fn from(creds: &Credentials) -> Self {
        Self {
            api_key: creds.api_key.clone(),
            secret_key: creds.secret_key.clone(),
            passphrase: creds.passphrase.clone(),
            demo: creds.demo,
        }
    }
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("api_key", &self.masked_api_key())
            .field("secret_key", &"<redacted>")
            .field("passphrase", &"<redacted>")
            .field("demo", &self.demo)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creds() -> Credentials {
        Credentials {
            api_key: "00000000-1111-2222-3333-444444444444".to_string(),
            secret_key: "DEADBEEFDEADBEEFDEADBEEFDEADBEEF".to_string(),
            passphrase: "super-secret-passphrase".to_string(),
            demo: true,
        }
    }

    #[test]
    fn mask_keeps_only_head_and_tail() {
        assert_eq!(
            mask_secret("00000000-1111-2222-3333-444444444444"),
            "0000****4444"
        );
    }

    #[test]
    fn short_secrets_are_fully_masked() {
        // 掩码不该比原文透露更多信息
        assert_eq!(mask_secret("abc"), "****");
        assert_eq!(mask_secret("abcdefghijkl"), "************");
    }

    /// 这是本模块存在的理由：凭据绝不能经由 Debug 泄漏。
    #[test]
    fn debug_output_never_contains_secrets() {
        let creds = creds();
        let rendered = format!("{creds:?}");

        assert!(
            !rendered.contains("DEADBEEFDEADBEEFDEADBEEFDEADBEEF"),
            "密钥泄漏：{rendered}"
        );
        assert!(
            !rendered.contains("super-secret-passphrase"),
            "口令泄漏：{rendered}"
        );
        assert!(
            !rendered.contains("1111-2222-3333"),
            "api key 中段不该出现在日志里：{rendered}"
        );
        assert!(
            rendered.contains("0000****4444"),
            "掩码应当出现：{rendered}"
        );
        assert!(rendered.contains("<redacted>"));
    }
}
