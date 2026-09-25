//! OKX 请求签名（设计文档 §6.1.1）。
//!
//! ```text
//! prehash   = timestamp + METHOD + requestPath + body
//! signature = base64(HMAC-SHA256(secretKey, prehash))
//! ```
//!
//! 两个极易踩错的点：
//!   * `request_path` 必须**包含 query string 但不含域名**（`/api/v5/x?y=1` 而不是完整 URL）
//!   * `timestamp` 必须与请求头 `OK-ACCESS-TIMESTAMP` **逐字节一致**——签名算的是字符串，
//!     不是时间。用秒生成签名、用毫秒发头，服务器一定报 50102。

use base64::Engine as _;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use crate::error::{AppError, AppResult};

type HmacSha256 = Hmac<Sha256>;

/// 生成签名。
pub fn sign(
    secret_key: &str,
    timestamp: &str,
    method: &str,
    request_path: &str,
    body: &str,
) -> AppResult<String> {
    let prehash = format!("{timestamp}{}{request_path}{body}", method.to_uppercase());

    let mut mac = HmacSha256::new_from_slice(secret_key.as_bytes())
        .map_err(|err| AppError::Config(format!("HMAC 密钥无效：{err}")))?;
    mac.update(prehash.as_bytes());

    Ok(base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 黄金向量。
    ///
    /// 期望值由 **Python 的 `hmac` 模块独立算出**（成熟参考实现），
    /// 而不是由「我自己对签名的理解」推导——否则公式理解错了，测试会跟着一起错。
    #[test]
    fn matches_independently_computed_vectors() {
        // 取自 OKX 文档风格的示例（密钥来自公开示例，非真实凭据）
        assert_eq!(
            sign(
                "22582BD0CFF14C41EDBF1AB98506286D",
                "2020-12-08T09:08:57.715Z",
                "GET",
                "/api/v5/account/balance?ccy=BTC",
                ""
            )
            .unwrap(),
            "HiZhvSfMtWJA3uUIVXV3a/bSXNPCWvYFXoGCVS8V4zY="
        );

        assert_eq!(
            sign(
                "22582BD0CFF14C41EDBF1AB98506286D",
                "2026-09-25T16:40:00.000Z",
                "GET",
                "/api/v5/account/positions?instType=SWAP",
                ""
            )
            .unwrap(),
            "2JNVHcw1vZ+/e0GAkzoji4EWbWOD6z1rMnBWoZFMHjg="
        );

        assert_eq!(
            sign(
                "another-secret-key",
                "2026-09-25T16:40:00.000Z",
                "GET",
                "/api/v5/account/config",
                ""
            )
            .unwrap(),
            "GuMV9XDU4ij9j2tu8BLU0Yse1k3dUoteyeStNhTnaxg="
        );
    }

    #[test]
    fn method_is_upper_cased_before_signing() {
        let lower = sign("s", "t", "get", "/p", "").unwrap();
        let upper = sign("s", "t", "GET", "/p", "").unwrap();
        assert_eq!(
            lower, upper,
            "小写 method 必须先转大写——否则签名与服务器算法不一致"
        );
    }

    #[test]
    fn body_participates_in_signature() {
        let without = sign("s", "t", "POST", "/p", "").unwrap();
        let with = sign("s", "t", "POST", "/p", "{\"a\":1}").unwrap();
        assert_ne!(
            without, with,
            "body 必须参与签名，否则带 body 的请求会被服务器拒绝"
        );
    }

    #[test]
    fn query_string_participates_in_signature() {
        let with_query = sign("s", "t", "GET", "/p?instId=BTC", "").unwrap();
        let without_query = sign("s", "t", "GET", "/p", "").unwrap();
        assert_ne!(
            with_query, without_query,
            "query string 属于 requestPath 的一部分，必须参与签名"
        );
    }
}
