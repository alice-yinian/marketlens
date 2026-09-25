//! 凭据管理命令。
//!
//! 注意这里**没有任何返回明文密钥的命令**——前端只能看到掩码。
//! 明文只在 Rust 内部从密钥库取出、用于发请求，然后随作用域结束被丢弃。

use tauri::State;

use crate::credentials::{self, CredentialMeta, CredentialProbe, SaveCredentialInput};
use crate::error::AppResult;
use crate::okx::client::OkxClient;
use crate::storage::Db;
use crate::vault::Vault;

#[tauri::command]
pub async fn credentials_list(db: State<'_, Db>) -> AppResult<Vec<CredentialMeta>> {
    db.list_credentials().await
}

#[tauri::command]
pub async fn credentials_save(
    db: State<'_, Db>,
    vault: State<'_, Vault>,
    input: SaveCredentialInput,
) -> AppResult<CredentialMeta> {
    credentials::save(&db, &vault, input).await
}

#[tauri::command]
pub async fn credentials_delete(
    db: State<'_, Db>,
    vault: State<'_, Vault>,
    id: String,
) -> AppResult<()> {
    // 先删密文再删元数据：反过来的话，若密文删除失败就会留下一条
    // 「列表里看不见、但密文还在」的孤儿记录，比反过来难排查得多。
    vault.delete_credential(&id)?;
    db.delete_credential_meta(&id).await?;
    Ok(())
}

/// 探测凭据：调用 `account/config` 读出权限与持仓模式。
///
/// 非只读密钥会返回 `read_only = false`，界面必须据此给出红色警示。
#[tauri::command]
pub async fn credentials_test(
    client: State<'_, OkxClient>,
    db: State<'_, Db>,
    vault: State<'_, Vault>,
    id: String,
) -> AppResult<CredentialProbe> {
    credentials::probe(&client, &db, &vault, &id).await
}
