//! OKX REST 客户端。
//!
//! 刻意只提供 GET：公开端点直接请求，私有端点带签名。**本类型不实现任何 POST 能力**，
//! 所以「应用会不会偷偷下单」这个问题在类型层面就有答案（ADR #6）。

use std::sync::RwLock;
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
    /// 用 `RwLock` 而不是直接持有 `Client`：代理是**可在引导里当场修改**的设置，
    /// 改完必须立刻生效，不能要求用户重启应用。`reqwest::Client` 的克隆是廉价的
    /// （内部是 `Arc`），所以每次请求克隆一次不构成开销。
    http: RwLock<reqwest::Client>,
    limiter: RateLimiter,
}

impl OkxClient {
    /// 不带显式代理的客户端。
    ///
    /// 注意「不带显式代理」不等于「直连」：`reqwest` 的 `system-proxy` 特性
    /// 会让它去读系统/环境变量代理。用户填了显式代理时那条路径会被关闭
    /// （见 [`OkxClient::reconfigure`]），优先级是**显式配置 > 系统设置**。
    pub fn new() -> AppResult<Self> {
        Ok(Self {
            http: RwLock::new(build_http(None)?),
            limiter: RateLimiter::new(),
        })
    }

    /// 应用（`Some`）或清除（`None`）网络代理，并**立即**替换底层 HTTP 客户端。
    ///
    /// 之所以能热替换：`reqwest` 的 `Client` 只是连接池与配置的句柄，
    /// 已在进行中的请求继续用旧句柄跑完，新请求从下一次开始走新代理——
    /// 不会打断用户正在看的行情刷新。
    pub fn reconfigure(&self, proxy: Option<&str>) -> AppResult<()> {
        let next = build_http(proxy)?;
        *self.http.write().expect("HTTP 客户端锁被毒化") = next;
        Ok(())
    }

    /// 取出当前 HTTP 客户端（廉价克隆）。
    fn http(&self) -> reqwest::Client {
        self.http.read().expect("HTTP 客户端锁被毒化").clone()
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
        // 一次请求内固定用同一个客户端：做重试的是**同一个**代理路径，
        // 不会出现「首次直连、重试走代理」这种无法解释的日志。
        let http = self.http();
        let mut attempt: u32 = 0;

        loop {
            attempt += 1;
            self.limiter.acquire(group).await;

            let mut builder = http.get(url.clone());

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
                        // 瞬时业务错误也值得重试（见 is_retryable_business_code）
                        if is_retryable_business_code(&envelope.code) && attempt < MAX_ATTEMPTS {
                            tracing::warn!(
                                path = request_path,
                                code = %envelope.code,
                                attempt,
                                "OKX 返回瞬时业务错误码，退避后重试"
                            );
                            tokio::time::sleep(backoff(attempt)).await;
                            continue;
                        }
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

/// 按给定代理构造 HTTP 客户端。
///
/// `proxy = None` 时**不显式设置**代理，`reqwest` 的 `system-proxy` 会接管
/// （Windows 系统代理 / `HTTP(S)_PROXY` / `ALL_PROXY`）。`Some` 时显式代理会
/// 关掉系统代理路径——用户手填的地址必须赢过系统设置，否则「我明明改了代理却没用」
/// 会成为一个无法从界面上解释的现象。
fn build_http(proxy: Option<&str>) -> AppResult<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(concat!("MarketLens/", env!("CARGO_PKG_VERSION")));

    if let Some(proxy) = proxy {
        let parsed = reqwest::Proxy::all(proxy)
            .map_err(|err| AppError::Config(format!("代理地址不可用：{err}")))?;
        builder = builder.proxy(parsed);
    }

    builder
        .build()
        .map_err(|err| AppError::Http(err.to_string()))
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

/// OKX 的业务错误码里，这些是**瞬时**的，值得重试。
///
/// 大多数业务错误（参数错、权限不足）重试确实无意义，但有一类是服务端临时故障：
/// 实测遇到过 `50001 Service temporarily unavailable`——**同一个请求立刻重试就成功了**。
/// 把它一律归为「不可重试」会让用户看到一个本可自愈的失败，而错误信息里
/// 没有任何线索提示「再点一次就好」。
///
/// 列表刻意只收录**实际观察到过**的码，不按猜测扩充——猜错的代价是重试一个
/// 永远失败的业务错误，白等几秒。
fn is_retryable_business_code(code: &str) -> bool {
    matches!(code, "50001")
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

    /// 瞬时业务错误必须被识别为可重试。
    ///
    /// `50001` 是实测遇到的（服务端临时不可用，立刻重试即成功）。
    /// 参数类错误绝不能被当成可重试——否则会白等几秒再失败。
    #[test]
    fn transient_business_codes_are_retryable_but_parameter_errors_are_not() {
        assert!(is_retryable_business_code("50001"), "实测瞬时错误应可重试");
        assert!(
            !is_retryable_business_code("51000"),
            "参数错误重试无意义，不该白等"
        );
        assert!(
            !is_retryable_business_code("50113"),
            "签名错误重试也不会变好"
        );
        assert!(!is_retryable_business_code("50101"), "环境不匹配重试无意义");
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

    /// 一个最小的假 HTTP 代理：记录它收到的第一行请求，回 502 后关闭。
    ///
    /// 刻意用 `std::net` 而不是 `tokio::net`：测试只需要「有人连上来、说了什么」，
    /// 用阻塞线程实现最直接，也不必为一个测试引入额外的 tokio 特性。
    fn spawn_recording_proxy() -> (u16, std::sync::mpsc::Receiver<String>) {
        use std::io::{BufRead, BufReader, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("绑定假代理失败");
        let port = listener.local_addr().expect("读取端口失败").port();
        let (tx, rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut reader = BufReader::new(stream.try_clone().expect("克隆流失败"));
            let mut line = String::new();
            if reader.read_line(&mut line).is_ok() {
                let _ = tx.send(line.trim().to_string());
            }
            // 直接拒绝：本测试只关心「请求去了哪里」，不关心代理是否真的转发了。
            let _ = stream.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n");
        });

        (port, rx)
    }

    /// 代理必须**真的作用到请求上**。
    ///
    /// 只断言「`reconfigure` 没报错」是自证式测试：代理根本没接上时它照样通过。
    /// 所以这里起一个会记录请求行的假代理，断言 OKX 的 HTTPS 请求确实以
    /// `CONNECT www.okx.com:443` 的形式打到了代理上。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn requests_actually_travel_through_the_configured_proxy() {
        let (port, rx) = spawn_recording_proxy();

        let client = std::sync::Arc::new(OkxClient::new().expect("客户端构造失败"));
        client
            .reconfigure(Some(&format!("http://127.0.0.1:{port}")))
            .expect("应接受合法的代理地址");

        let requester = {
            let client = std::sync::Arc::clone(&client);
            tokio::spawn(async move {
                // 结果必然是失败（假代理回 502），本测试只关心请求发去了哪里。
                let _ = client
                    .get_public::<crate::okx::models::ServerTime>(
                        crate::okx::endpoints::public::TIME,
                        RateGroup::Market,
                        &[],
                    )
                    .await;
            })
        };

        let line = tokio::task::spawn_blocking(move || rx.recv_timeout(Duration::from_secs(10)))
            .await
            .expect("等待代理记录失败")
            .expect("假代理没收到任何请求——说明请求根本没走代理");

        assert!(
            line.starts_with("CONNECT www.okx.com:443"),
            "HTTPS 请求应通过 CONNECT 隧道打到 OKX；实际收到：{line}"
        );

        requester.abort();
    }

    /// `socks5://` / `socks5h://` 是本地代理最常见的形态。
    ///
    /// 这条测试守着 `reqwest` 的 `socks` 特性没被误删——删掉的话
    /// `Proxy::all("socks5h://…")` 会直接报错，而用户只会看到一个
    /// 「无法解析代理地址」的失败。
    #[test]
    fn socks_proxies_are_supported() {
        let client = OkxClient::new().expect("构造失败");

        for proxy in ["socks5://127.0.0.1:1080", "socks5h://127.0.0.1:1080"] {
            client
                .reconfigure(Some(proxy))
                .unwrap_or_else(|err| panic!("{proxy} 必须被支持：{err}"));
        }

        client.reconfigure(None).expect("清除代理应成功");
    }
}
