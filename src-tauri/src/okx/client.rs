//! OKX REST 客户端。
//!
//! 刻意只提供 GET：公开端点直接请求，私有端点带签名。**本类型不实现任何 POST 能力**，
//! 所以「应用会不会偷偷下单」这个问题在类型层面就有答案（ADR #6）。

use std::time::Duration;

use serde::de::DeserializeOwned;

use crate::error::{AppError, AppResult};
use crate::okx::credentials::Credentials;
use crate::okx::endpoints::{BASE_URL, RateGroup};
use crate::okx::models::Envelope;
use crate::okx::ratelimit::RateLimiter;
use crate::okx::sign;

/// 单个请求的最大尝试次数（含首次）。
const MAX_ATTEMPTS: u32 = 4;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const BACKOFF_CAP: Duration = Duration::from_millis(30_000);

pub struct OkxClient {
    http: reqwest::Client,
    limiter: RateLimiter,
}

impl OkxClient {
    pub fn new() -> AppResult<Self> {
        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent(concat!("MarketLens/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|err| AppError::Http(err.to_string()))?;

        Ok(Self {
            http,
            limiter: RateLimiter::new(),
        })
    }

    /// 公开 GET。
    pub async fn get_public<T: DeserializeOwned>(
        &self,
        path: &str,
        group: RateGroup,
        query: &[(&str, &str)],
    ) -> AppResult<Vec<T>> {
        let (url, request_path) = build_url(path, query)?;
        self.request::<T>(url, &request_path, group, None).await
    }

    /// 私有 GET（带签名）。只读端点，不存在对应的 POST 版本。
    pub async fn get_private<T: DeserializeOwned>(
        &self,
        creds: &Credentials,
        path: &str,
        group: RateGroup,
        query: &[(&str, &str)],
    ) -> AppResult<Vec<T>> {
        let (url, request_path) = build_url(path, query)?;
        self.request::<T>(url, &request_path, group, Some(creds))
            .await
    }

    /// 统一的请求执行：限流、签名、重试退避、信封解析。
    ///
    /// 重试语义：`429` 与 `5xx` 退避重试；网络错误退避重试；
    /// 业务错误（`code != "0"`）与解析失败**立即返回**——重试它们只是浪费配额。
    async fn request<T: DeserializeOwned>(
        &self,
        url: reqwest::Url,
        request_path: &str,
        group: RateGroup,
        creds: Option<&Credentials>,
    ) -> AppResult<Vec<T>> {
        let mut attempt: u32 = 0;

        loop {
            attempt += 1;
            self.limiter.acquire(group).await;

            let mut builder = self.http.get(url.clone());

            if let Some(creds) = creds {
                // 关键：同一个 timestamp 字符串既参与签名也进请求头。
                // 分别生成两次会出现「签名用秒、请求头发毫秒」这类错位，
                // 而服务器只会回一个含糊的 50102。
                let timestamp = iso_timestamp_now();
                let signature = sign::sign(&creds.secret_key, &timestamp, "GET", request_path, "")?;

                builder = builder
                    .header("OK-ACCESS-KEY", creds.api_key.as_str())
                    .header("OK-ACCESS-SIGN", signature)
                    .header("OK-ACCESS-TIMESTAMP", timestamp)
                    .header("OK-ACCESS-PASSPHRASE", creds.passphrase.as_str());

                if creds.demo {
                    builder = builder.header("x-simulated-trading", "1");
                }
            }

            match builder.send().await {
                Ok(response) => {
                    let status = response.status();

                    if status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
                    {
                        if attempt >= MAX_ATTEMPTS {
                            return Err(AppError::Http(format!(
                                "{request_path} 连续 {attempt} 次失败，最后状态码 {status}"
                            )));
                        }
                        tracing::warn!(path = request_path, %status, attempt, "OKX 返回可重试状态码，退避后重试");
                        tokio::time::sleep(backoff(attempt)).await;
                        continue;
                    }

                    if !status.is_success() {
                        return Err(AppError::Http(format!("{request_path} 返回 {status}")));
                    }

                    let body = response
                        .text()
                        .await
                        .map_err(|err| AppError::Http(err.to_string()))?;

                    let envelope: Envelope<T> = serde_json::from_str(&body).map_err(|err| {
                        AppError::Decode(format!("{request_path} 响应无法解析：{err}"))
                    })?;

                    if envelope.code != "0" {
                        return Err(AppError::Okx {
                            code: envelope.code,
                            msg: envelope.msg,
                        });
                    }

                    return Ok(envelope.data);
                }
                Err(err) => {
                    if attempt >= MAX_ATTEMPTS {
                        return Err(AppError::Http(format!("{request_path} 请求失败：{err}")));
                    }
                    tracing::warn!(path = request_path, %err, attempt, "OKX 请求失败，退避后重试");
                    tokio::time::sleep(backoff(attempt)).await;
                }
            }
        }
    }
}

/// 构造 URL，并同时给出**用于签名的 requestPath**。
///
/// 只构造一次是刻意的：签名算的是字符串，必须与服务器实际收到的
/// `path?query` 逐字节一致。若一边用 `reqwest` 的 `.query()` 发送、
/// 另一边手工拼字符串签名，两者的百分号编码或参数顺序一旦不同，
/// 服务器就回一个含糊的 `50113 Invalid Sign`——这个坑很难从错误信息里看出来。
fn build_url(path: &str, query: &[(&str, &str)]) -> AppResult<(reqwest::Url, String)> {
    let mut url = reqwest::Url::parse(&format!("{BASE_URL}{path}"))
        .map_err(|err| AppError::Config(format!("URL 构造失败：{err}")))?;

    if !query.is_empty() {
        url.query_pairs_mut().extend_pairs(query);
    }

    let request_path = match url.query() {
        Some(query_string) => format!("{}?{query_string}", url.path()),
        None => url.path().to_string(),
    };

    Ok((url, request_path))
}

/// OKX 要求的 ISO-8601 UTC 毫秒时间戳，形如 `2026-09-25T16:40:00.000Z`。
fn iso_timestamp_now() -> String {
    chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

/// 指数退避 + 抖动，上限 [`BACKOFF_CAP`]。
///
/// 抖动是必需的：采集计划里多条序列会同时失败、同时退避，
/// 不加抖动它们会在同一时刻集体重试（惊群），把刚恢复的配额再次打满。
fn backoff(attempt: u32) -> Duration {
    let base_ms = 200u64.saturating_mul(1u64 << attempt.min(7));
    let capped_ms = base_ms.min(BACKOFF_CAP.as_millis() as u64);

    // 抖动因子 0.5~1.0，种子取自系统时间
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let factor = 0.5 + f64::from(nanos % 500) / 1000.0;

    Duration::from_millis((capped_ms as f64 * factor) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 退避必须既不会短到无效，也不会因为位移溢出而失控。
    #[test]
    fn backoff_stays_within_bounds() {
        for attempt in 1..=16 {
            let delay = backoff(attempt);
            assert!(
                delay >= Duration::from_millis(100),
                "第 {attempt} 次退避过短：{delay:?}"
            );
            assert!(
                delay <= BACKOFF_CAP,
                "第 {attempt} 次退避超过上限：{delay:?}"
            );
        }
    }

    #[test]
    fn signed_path_matches_sent_url() {
        let (url, request_path) =
            build_url("/api/v5/account/positions", &[("instType", "SWAP")]).expect("构造失败");

        assert_eq!(request_path, "/api/v5/account/positions?instType=SWAP");
        assert_eq!(
            url.as_str(),
            format!("{BASE_URL}{request_path}"),
            "签名用的 requestPath 必须能直接拼回真实 URL"
        );
    }

    #[test]
    fn empty_query_produces_no_trailing_question_mark() {
        let (url, request_path) = build_url("/api/v5/account/config", &[]).expect("构造失败");
        assert_eq!(request_path, "/api/v5/account/config");
        assert!(!request_path.contains('?'));
        assert_eq!(url.as_str(), format!("{BASE_URL}{request_path}"));
    }

    /// 特殊字符是签名最容易出错的地方：编码方式必须与发送时完全一致。
    #[test]
    fn special_characters_are_encoded_consistently() {
        let (url, request_path) = build_url("/p", &[("a", "x y"), ("b", "1&2")]).expect("构造失败");

        assert_eq!(
            url.as_str(),
            format!("{BASE_URL}{request_path}"),
            "编码后仍必须能拼回真实 URL"
        );
        assert!(
            !request_path.contains(' '),
            "空格必须被编码：{request_path}"
        );
    }

    #[test]
    fn iso_timestamp_has_millisecond_precision() {
        let stamp = iso_timestamp_now();
        // 形如 2026-09-25T16:40:00.000Z
        assert!(stamp.ends_with('Z'), "必须是 UTC：{stamp}");
        assert_eq!(stamp.len(), 24, "长度应为 24（含毫秒）：{stamp}");
        assert!(stamp.contains('.'), "必须带毫秒：{stamp}");
        assert_eq!(stamp.matches('.').count(), 1, "只有一处小数点：{stamp}");
    }
}
