//! 凭据管理服务：协调密钥库（明文）与数据库（元数据）。
//!
//! 分工是刻意的：**明文只进 Stronghold，数据库只存掩码与探测结果**。
//! 因此即使有人拿到了 `marketlens.db`，里面也没有任何可直接使用的密钥。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, AppResult};
use crate::okx::client::OkxClient;
use crate::okx::credentials::{Credentials, StoredCredential, mask_secret};
use crate::okx::endpoints::{RateGroup, private};
use crate::okx::models::AccountConfig;
use crate::storage::Db;
use crate::vault::Vault;

/// 凭据元数据（可安全落库、可安全传给前端）。
#[derive(Debug, Clone, Serialize, TS, sqlx::FromRow)]
#[ts(export, export_to = "types.ts")]
pub struct CredentialMeta {
    pub id: String,
    pub label: String,
    /// `live` | `demo`
    pub env: String,
    /// 形如 `abcd****ef12`。**明文永不入库。**
    pub api_key_masked: String,
    /// OKX 返回的权限串，如 `read_only,trade`
    pub permissions: Option<String>,
    pub uid_masked: Option<String>,
    #[ts(type = "number | null")]
    pub last_ok_at: Option<i64>,
    pub last_error: Option<String>,
    #[ts(type = "number")]
    pub created_at: i64,
}

/// 新增/更新凭据的入参。
#[derive(Debug, Deserialize)]
pub struct SaveCredentialInput {
    /// 为空表示新建；有值表示覆盖同一条
    #[serde(default)]
    pub id: Option<String>,
    pub label: String,
    pub api_key: String,
    pub secret_key: String,
    pub passphrase: String,
    /// `true` = 模拟盘（请求需带 `x-simulated-trading: 1`）
    pub demo: bool,
}

/// 凭据探测结果（`credentials_test` 的返回）。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "types.ts")]
pub struct CredentialProbe {
    pub ok: bool,
    pub permissions: Option<String>,
    pub uid_masked: Option<String>,
    /// `net_mode` | `long_short_mode`
    pub pos_mode: Option<String>,
    /// 是否**只读**。为 `false` 时界面必须给出红色警示。
    pub read_only: bool,
    pub error: Option<String>,
}

/// 保存凭据：明文进 vault，元数据进库。
///
/// 两个存储的写入顺序与回滚是刻意的：`credentials_list` 以数据库为准，
/// 所以**先写 vault 再写库**——反过来会短暂出现「列表里有、密钥却读不出来」的状态。
/// 若写库失败，则把刚写入的 vault 条目删掉，避免留下一条永远用不到的密文。
pub async fn save(db: &Db, vault: &Vault, input: SaveCredentialInput) -> AppResult<CredentialMeta> {
    validate(&input)?;

    let id = input.id.clone().unwrap_or_else(new_credential_id);
    let env = if input.demo { "demo" } else { "live" };

    let stored = StoredCredential {
        api_key: input.api_key,
        secret_key: input.secret_key,
        passphrase: input.passphrase,
        demo: input.demo,
    };

    vault.put_credential(&id, &stored)?;

    let meta = CredentialMeta {
        id: id.clone(),
        label: input.label,
        env: env.to_string(),
        api_key_masked: mask_secret(&stored.api_key),
        permissions: None,
        uid_masked: None,
        last_ok_at: None,
        last_error: None,
        created_at: crate::storage::now_ms(),
    };

    if let Err(err) = db.upsert_credential(&meta).await {
        if let Err(rollback_err) = vault.delete_credential(&id) {
            tracing::error!(%rollback_err, "写库失败后回滚密钥库条目也失败，可能残留无用密文");
        }
        return Err(err);
    }

    Ok(meta)
}

/// 校验入参。
///
/// 抽成独立函数是为了能直接测试，而不必构造 vault 与数据库。
fn validate(input: &SaveCredentialInput) -> AppResult<()> {
    if input.api_key.trim().is_empty() {
        return Err(AppError::Config("API Key 不能为空".to_string()));
    }
    if input.secret_key.trim().is_empty() {
        return Err(AppError::Config("Secret Key 不能为空".to_string()));
    }
    if input.passphrase.is_empty() {
        return Err(AppError::Config("Passphrase 不能为空".to_string()));
    }
    if input.label.trim().is_empty() {
        return Err(AppError::Config("备注名不能为空".to_string()));
    }
    Ok(())
}

/// 读取某条凭据的明文（仅在需要发请求时调用）。
pub fn credentials_for(vault: &Vault, id: &str) -> AppResult<Credentials> {
    vault
        .credential(id)?
        .ok_or_else(|| AppError::Config(format!("凭据 {id} 不存在")))
}

/// 探测凭据：调用 `account/config`，读出权限与持仓模式。
///
/// **非只读密钥不会导致失败**——但会把 `read_only = false` 返回给界面，
/// 由界面给出红色警示。应用本身不实现任何私有 POST 端点，所以即使密钥可交易，
/// 它也没有下单的能力（ADR #6）。
pub async fn probe(
    client: &OkxClient,
    db: &Db,
    vault: &Vault,
    id: &str,
) -> AppResult<CredentialProbe> {
    let credentials = match vault.credential(id)? {
        Some(creds) => creds,
        None => {
            return Ok(CredentialProbe {
                ok: false,
                permissions: None,
                uid_masked: None,
                pos_mode: None,
                read_only: true,
                error: Some(format!("凭据 {id} 不在密钥库中")),
            });
        }
    };

    let result = client
        .get_private::<AccountConfig>(
            &credentials,
            private::ACCOUNT_CONFIG,
            RateGroup::Account,
            &[],
        )
        .await;

    match result {
        Ok(rows) => {
            let Some(config) = rows.first() else {
                return Ok(CredentialProbe {
                    ok: false,
                    permissions: None,
                    uid_masked: None,
                    pos_mode: None,
                    read_only: true,
                    error: Some("account/config 返回了空数据".to_string()),
                });
            };

            let uid_masked = mask_secret(&config.uid);
            let read_only = config.is_read_only();

            db.record_credential_probe(id, Some(&config.perm), Some(&uid_masked), None)
                .await?;

            if !read_only {
                tracing::warn!(credential = id, perm = %config.perm, "凭据包含交易权限，非只读");
            }

            Ok(CredentialProbe {
                ok: true,
                permissions: Some(config.perm.clone()),
                uid_masked: Some(uid_masked),
                pos_mode: Some(config.pos_mode.clone()),
                read_only,
                error: None,
            })
        }
        Err(err) => {
            // 探测失败也要落库：用户下次打开设置页能看到上次为什么失败
            db.record_credential_probe(id, None, None, Some(&err.to_string()))
                .await?;
            Ok(CredentialProbe {
                ok: false,
                permissions: None,
                uid_masked: None,
                pos_mode: None,
                read_only: true,
                error: Some(err.to_string()),
            })
        }
    }
}

/// 生成凭据 id。
///
/// 用毫秒时间戳加进程内计数器，而不是引入 uuid 依赖：凭据是人手工录入的，
/// 同一毫秒内创建两条在实际使用中不会发生，而计数器让这一点在理论上也成立。
fn new_credential_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    format!("cred-{}-{seq}", crate::storage::now_ms())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> SaveCredentialInput {
        SaveCredentialInput {
            id: None,
            label: "T1".to_string(),
            api_key: "00000000-1111-2222-3333-444444444444".to_string(),
            secret_key: "DEADBEEFDEADBEEFDEADBEEFDEADBEEF".to_string(),
            passphrase: "pass".to_string(),
            demo: true,
        }
    }

    #[test]
    fn ids_are_unique_within_the_same_millisecond() {
        let first = new_credential_id();
        let second = new_credential_id();
        assert_ne!(first, second, "同一毫秒内连续创建也必须得到不同 id");
        assert!(first.starts_with("cred-"));
    }

    #[test]
    fn validate_accepts_a_complete_input() {
        assert!(validate(&input()).is_ok());
    }

    #[test]
    fn validate_rejects_blank_required_fields() {
        let blank_key = SaveCredentialInput {
            api_key: "   ".to_string(),
            ..input()
        };
        let err = validate(&blank_key).expect_err("空白 API Key 应被拒绝");
        assert!(
            err.to_string().contains("API Key"),
            "错误信息应指明字段：{err}"
        );

        let blank_secret = SaveCredentialInput {
            secret_key: String::new(),
            ..input()
        };
        assert!(validate(&blank_secret).is_err());

        let no_passphrase = SaveCredentialInput {
            passphrase: String::new(),
            ..input()
        };
        assert!(
            validate(&no_passphrase).is_err(),
            "OKX 的签名需要 passphrase，为空时必须提前拦下"
        );

        let blank_label = SaveCredentialInput {
            label: "  ".to_string(),
            ..input()
        };
        assert!(
            validate(&blank_label).is_err(),
            "备注名用于区分多条凭据，不能为空"
        );
    }
}
