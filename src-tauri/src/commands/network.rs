//! 网络代理命令。
//!
//! 代理是**唯一一个必须在引导期就能配、且必须当场能验**的设置：
//! 受限网络下，录完凭据点「保存并测试」会失败、第 4 步拉候选标的会失败，
//! 而这两个失败都不指向真正的原因（网络到不了 OKX）。所以这里除了读写，
//! 还专门给了一个 `proxy_test`——用最轻量的公开端点把「代理通不通」和
//! 「凭据对不对」两件事分开。

use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

use crate::error::AppResult;
use crate::okx::client::OkxClient;
use crate::okx::endpoints::{RateGroup, public};
use crate::okx::models::ServerTime;
use crate::settings;
use crate::storage::Db;

/// 当前代理配置。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct ProxySettings {
    /// 已保存的代理地址。`None` = 未显式配置——此时会**沿用系统/环境变量代理**，
    /// 而不是「一定直连」。界面文案必须说清这一点，否则用户会以为没配就是直连。
    pub url: Option<String>,
}

/// `proxy_set` 的入参。
#[derive(Debug, Deserialize)]
pub struct SetProxyInput {
    /// `None` 或空串 = 清除显式配置（回到系统/环境变量代理）。
    pub url: Option<String>,
}

/// 代理连通性探测结果。
///
/// 刻意**不用 `Err` 表示探测失败**（与 `credentials_test` 一致）：连不通是
/// 用户此刻最需要看清的一种**正常结果**，不是命令执行出错。返回结构化结果，
/// 界面才能把「地址填错了」和「代理进程没开」分开提示。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct ProxyProbe {
    pub ok: bool,
    /// 往返耗时（毫秒）。给用户一个「代理慢不慢」的量级判断。
    #[ts(type = "number")]
    pub latency_ms: u64,
    /// OKX 返回的服务器时间（Unix 毫秒）；成功时才有值。
    #[ts(type = "number | null")]
    pub server_time_ms: Option<i64>,
    /// 失败原因，成功时为 `None`。
    pub error: Option<String>,
}

#[tauri::command]
pub async fn proxy_get(db: State<'_, Db>) -> AppResult<ProxySettings> {
    Ok(ProxySettings {
        url: settings::read_proxy(&db).await?,
    })
}

/// 保存代理并**立即生效**。
///
/// 落库与热重建的顺序是刻意的：**先落库、再重建**。反过来的话，重建成功而落库失败
/// 会变成「本次运行走新代理、重启后悄悄退回」——用户看到的是「保存成功」，
/// 而这正是最难排查的一类不一致。按当前顺序，最坏情况是「本次没生效、重启后生效」，
/// 用户至少能通过重启自愈。
#[tauri::command]
pub async fn proxy_set(
    client: State<'_, OkxClient>,
    db: State<'_, Db>,
    input: SetProxyInput,
) -> AppResult<ProxySettings> {
    let url = settings::write_proxy(&db, input.url.as_deref()).await?;

    client.reconfigure(url.as_deref())?;
    match url.as_deref() {
        Some(url) => tracing::info!(proxy = %settings::redact_proxy(url), "网络代理已更新"),
        None => tracing::info!("网络代理已清除，后续请求交由系统 / 环境变量代理处理"),
    }

    Ok(ProxySettings { url })
}

/// 用一次最轻量的公开请求验证代理是否真的通了。
///
/// 打的是 `/api/v5/public/time`：不需要鉴权，所以它通过只可能是**网络路径**通了，
/// 与密钥无关；失败信息也就只指向代理/网络。
#[tauri::command]
pub async fn proxy_test(client: State<'_, OkxClient>) -> AppResult<ProxyProbe> {
    let started = std::time::Instant::now();

    let result = client
        .get_public::<ServerTime>(public::TIME, RateGroup::Market, &[])
        .await;

    let latency_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;

    Ok(match result {
        Ok(rows) => ProxyProbe {
            ok: true,
            latency_ms,
            server_time_ms: rows.first().map(|time| time.ts),
            error: None,
        },
        Err(err) => ProxyProbe {
            ok: false,
            latency_ms,
            server_time_ms: None,
            error: Some(err.to_string()),
        },
    })
}
