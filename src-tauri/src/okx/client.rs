//! OKX REST 客户端（只读）。
//!
//! 刻意只提供公开 GET。私有端点需要签名，属于 M2；更重要的是——**本类型不实现任何
//! POST 能力**，所以「应用会不会偷偷下单」这个问题在类型层面就有答案（ADR #6）。

use std::time::Duration;

use serde::de::DeserializeOwned;

use crate::error::{AppError, AppResult};
use crate::okx::endpoints::{BASE_URL, RateGroup};
use crate::okx::models::Envelope;
use crate::okx::ratelimit::RateLimiter;

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

    /// 发起一次公开 GET 请求，返回响应信封中的 `data` 数组。
    ///
    /// 重试语义：`429` 与 `5xx` 退避重试；网络错误退避重试；
    /// 业务错误（`code != "0"`）与解析失败**立即返回**——重试它们只是浪费配额。
    pub async fn get_public<T: DeserializeOwned>(
        &self,
        path: &str,
        group: RateGroup,
        query: &[(&str, &str)],
    ) -> AppResult<Vec<T>> {
        let url = format!("{BASE_URL}{path}");
        let mut attempt: u32 = 0;

        loop {
            attempt += 1;
            self.limiter.acquire(group).await;

            match self.http.get(&url).query(query).send().await {
                Ok(response) => {
                    let status = response.status();

                    if status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
                    {
                        if attempt >= MAX_ATTEMPTS {
                            return Err(AppError::Http(format!(
                                "{path} 连续 {attempt} 次失败，最后状态码 {status}"
                            )));
                        }
                        tracing::warn!(path, %status, attempt, "OKX 返回可重试状态码，退避后重试");
                        tokio::time::sleep(backoff(attempt)).await;
                        continue;
                    }

                    if !status.is_success() {
                        return Err(AppError::Http(format!("{path} 返回 {status}")));
                    }

                    let body = response
                        .text()
                        .await
                        .map_err(|err| AppError::Http(err.to_string()))?;

                    let envelope: Envelope<T> = serde_json::from_str(&body)
                        .map_err(|err| AppError::Decode(format!("{path} 响应无法解析：{err}")))?;

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
                        return Err(AppError::Http(format!("{path} 请求失败：{err}")));
                    }
                    tracing::warn!(path, %err, attempt, "OKX 请求失败，退避后重试");
                    tokio::time::sleep(backoff(attempt)).await;
                }
            }
        }
    }
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
}
