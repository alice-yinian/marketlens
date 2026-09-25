//! 密钥库相关命令。

use serde::Serialize;
use tauri::State;
use ts_rs::TS;

use crate::error::AppResult;
use crate::vault::Vault;

/// 密钥库状态。界面据此决定进入「创建」还是「解锁」。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "types.ts")]
pub struct VaultStatus {
    /// 密钥库文件是否已存在
    pub exists: bool,
    pub unlocked: bool,
}

impl VaultStatus {
    fn of(vault: &Vault) -> Self {
        Self {
            exists: vault.exists(),
            unlocked: vault.is_unlocked(),
        }
    }
}

#[tauri::command]
pub fn vault_status(vault: State<'_, Vault>) -> AppResult<VaultStatus> {
    Ok(VaultStatus::of(&vault))
}

/// 解锁；密钥库不存在时等同于创建。
#[tauri::command]
pub fn vault_unlock(vault: State<'_, Vault>, password: String) -> AppResult<VaultStatus> {
    if password.is_empty() {
        return Err(crate::error::AppError::Config("主密码不能为空".to_string()));
    }
    vault.unlock(&password)?;
    Ok(VaultStatus::of(&vault))
}

#[tauri::command]
pub fn vault_lock(vault: State<'_, Vault>) -> AppResult<VaultStatus> {
    vault.lock()?;
    Ok(VaultStatus::of(&vault))
}
