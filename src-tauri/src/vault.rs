//! 密钥库：API 凭据明文的**唯一**落盘位置（设计文档 §6.5）。
//!
//! 用 Stronghold（argon2 派生的加密快照）。安全上的两个刻意选择：
//!
//! 1. **只从 Rust 侧调用**——刻意**不注册**插件的 JS 命令。设计文档原本的措辞是
//!    「不在 capabilities 里放行这些命令」，而现在它们根本不存在：WebView 连
//!    「尝试读写密钥」这个能力都没有。
//! 2. **锁定即丢弃**——`lock()` 直接 drop 掉已解锁的 Stronghold，密钥材料不留在内存里
//!    等着被下一次读取。

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use iota_stronghold::{Client, ClientError, Store};
use tauri::{AppHandle, Manager};
use tauri_plugin_stronghold::kdf::KeyDerivation;
use tauri_plugin_stronghold::stronghold::Stronghold;

use crate::error::{AppError, AppResult};
use crate::okx::credentials::{Credentials, StoredCredential};

/// vault 内的客户端名。一个就够——凭据按 key 区分。
const CLIENT_NAME: &str = "marketlens";

/// 凭据条目在 store 里的键前缀。
const CREDENTIAL_PREFIX: &str = "cred:";

pub struct Vault {
    vault_path: PathBuf,
    salt_path: PathBuf,
    /// 解锁后才有值；锁定即置空。
    unlocked: Mutex<Option<Stronghold>>,
}

impl Vault {
    pub fn new(app: &AppHandle) -> AppResult<Self> {
        let dir = app
            .path()
            .app_local_data_dir()
            .map_err(|err| AppError::AppDataDir(err.to_string()))?;
        Self::at(dir)
    }

    /// 在指定目录上构造。
    ///
    /// 测试直接传临时目录——**落盘行为正是要验证的东西**，用内存实现反而测不到
    /// 「重新解锁后凭据还在不在」这个最关键的性质。
    fn at(dir: PathBuf) -> AppResult<Self> {
        std::fs::create_dir_all(&dir)?;

        Ok(Self {
            vault_path: dir.join("vault.hold"),
            // salt 供 argon2 派生使用；文件本身不含密钥
            salt_path: dir.join("vault-salt.txt"),
            unlocked: Mutex::new(None),
        })
    }

    /// 密钥库文件是否已存在（区分「首次使用」与「需要解锁」）。
    pub fn exists(&self) -> bool {
        self.vault_path.exists()
    }

    pub fn is_unlocked(&self) -> bool {
        self.unlocked
            .lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false)
    }

    /// 解锁；密钥库不存在时等同于创建。
    ///
    /// 立刻 `save()` 一次，让「密钥库已存在」这一事实尽快成立——否则用户设完密码、
    /// 还没存任何凭据就退出，下次会被当成首次使用。
    pub fn unlock(&self, password: &str) -> AppResult<()> {
        let key = KeyDerivation::argon2(password, &self.salt_path);
        let stronghold = Stronghold::new(&self.vault_path, key)
            .map_err(|err| AppError::Vault(err.to_string()))?;
        stronghold
            .save()
            .map_err(|err| AppError::Vault(err.to_string()))?;

        *self.state_lock()? = Some(stronghold);
        tracing::info!(path = %self.vault_path.display(), "密钥库已解锁");
        Ok(())
    }

    /// 锁定：丢弃已解锁的密钥库。
    pub fn lock(&self) -> AppResult<()> {
        if self.state_lock()?.take().is_some() {
            tracing::info!("密钥库已锁定");
        }
        Ok(())
    }

    /// 写入（或覆盖）一组凭据。
    pub fn put_credential(&self, id: &str, credential: &StoredCredential) -> AppResult<()> {
        let payload = serde_json::to_vec(credential)
            .map_err(|err| AppError::Vault(format!("凭据序列化失败：{err}")))?;

        self.mutate(|store| store.insert(credential_key(id), payload, None).map(|_| ()))
    }

    /// 读取一组凭据。`None` 表示该 id 在 vault 里不存在。
    pub fn credential(&self, id: &str) -> AppResult<Option<Credentials>> {
        let raw = self.with_store(|store| {
            store
                .get(&credential_key(id))
                .map_err(|err| AppError::Vault(err.to_string()))
        })?;

        let Some(raw) = raw else {
            return Ok(None);
        };

        let stored: StoredCredential = serde_json::from_slice(&raw)
            .map_err(|err| AppError::Vault(format!("凭据解析失败：{err}")))?;
        Ok(Some(stored.into()))
    }

    pub fn delete_credential(&self, id: &str) -> AppResult<()> {
        self.mutate(|store| store.delete(&credential_key(id)).map(|_| ()))
    }

    /// 只读访问 store。
    fn with_store<T>(&self, action: impl FnOnce(&Store) -> AppResult<T>) -> AppResult<T> {
        let guard = self.state_lock()?;
        let client = unlocked_client(&guard)?;
        action(&client.store())
    }

    /// 变更 store，并**在返回前落盘**。
    ///
    /// 两个步骤缺一不可：
    ///   1. `write_client` 把该 client 的状态写进**内存快照**
    ///   2. `save` 把快照**提交到磁盘**
    ///
    /// 少了第 1 步，`save()` 提交的是一个不含该 client 的空快照——表现为
    /// 「保存返回成功，重新解锁后凭据却凭空消失」。这个 bug 由真实文件系统的
    /// 往返测试抓到，纯内存实现测不出来。
    fn mutate(&self, action: impl FnOnce(&Store) -> Result<(), ClientError>) -> AppResult<()> {
        let guard = self.state_lock()?;
        let stronghold = guard.as_ref().ok_or(AppError::VaultLocked)?;
        let client = ensure_client(stronghold)?;

        action(&client.store()).map_err(|err| AppError::Vault(err.to_string()))?;

        stronghold
            .write_client(CLIENT_NAME)
            .map_err(|err| AppError::Vault(err.to_string()))?;
        stronghold
            .save()
            .map_err(|err| AppError::Vault(err.to_string()))?;
        Ok(())
    }

    fn state_lock(&self) -> AppResult<MutexGuard<'_, Option<Stronghold>>> {
        self.unlocked
            .lock()
            .map_err(|_| AppError::Vault("密钥库状态锁已中毒".to_string()))
    }
}

/// 从状态里取出已解锁的 vault，并确保 client 存在。
fn unlocked_client(guard: &MutexGuard<'_, Option<Stronghold>>) -> AppResult<Client> {
    let stronghold = guard.as_ref().ok_or(AppError::VaultLocked)?;
    ensure_client(stronghold)
}

/// 取得（必要时创建）Stronghold client。
///
/// **三步的顺序是关键**，尤其不能把 `create_client` 提前：
///
///   1. `get_client` —— 只查**内存**中的会话 client（源码原话：*in session client,
///      not being persisted in a Snapshot*）
///   2. `load_client` —— 从**快照**加载（重新解锁时走这一支）
///   3. `create_client` —— 只有快照里确实没有时才新建
///
/// 少了第 2 步，重新解锁后第 1 步必然失败，于是第 3 步会**新建一个空 client**
/// 顶掉快照里的那个——表现为「凭据保存成功，重新解锁后凭空消失」。
/// 这个 bug 由真实文件系统的往返测试抓到。
fn ensure_client(stronghold: &Stronghold) -> AppResult<Client> {
    if let Ok(client) = stronghold.get_client(CLIENT_NAME) {
        return Ok(client);
    }
    if let Ok(client) = stronghold.load_client(CLIENT_NAME) {
        return Ok(client);
    }
    stronghold
        .create_client(CLIENT_NAME)
        .map_err(|err| AppError::Vault(err.to_string()))
}

fn credential_key(id: &str) -> Vec<u8> {
    format!("{CREDENTIAL_PREFIX}{id}").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> StoredCredential {
        StoredCredential {
            api_key: "00000000-1111-2222-3333-444444444444".to_string(),
            secret_key: "DEADBEEFDEADBEEFDEADBEEFDEADBEEF".to_string(),
            passphrase: "pass".to_string(),
            demo: true,
        }
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "marketlens-vault-{tag}-{}",
            crate::storage::now_ms()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// 密钥库的完整往返：创建 → 写入 → 锁定 → 重新解锁 → 读回。
    ///
    /// 这个测试的价值在于它**真的落盘再读回**：加密快照、argon2 派生、
    /// 以及「忘记 save() 导致重启后凭据消失」这类问题，只有走真实文件系统才测得出来。
    #[test]
    fn vault_roundtrip_persists_credentials() {
        let dir = temp_dir("roundtrip");
        let vault = Vault::at(dir.clone()).expect("构造失败");

        assert!(!vault.exists(), "新建时密钥库文件还不存在");
        assert!(!vault.is_unlocked());

        vault
            .unlock("correct horse battery staple")
            .expect("创建失败");
        assert!(vault.exists(), "解锁后应当已落盘");
        assert!(vault.is_unlocked());

        vault.put_credential("c1", &sample()).expect("写入失败");

        // 锁定后必须读不到
        vault.lock().expect("锁定失败");
        assert!(!vault.is_unlocked());
        assert!(
            matches!(vault.credential("c1"), Err(AppError::VaultLocked)),
            "锁定后读取必须返回 VaultLocked，而不是 panic 或空值"
        );

        // 用同一密码重新打开：凭据必须还在
        vault
            .unlock("correct horse battery staple")
            .expect("重新解锁失败");
        let read = vault
            .credential("c1")
            .expect("读取失败")
            .expect("凭据在重新解锁后丢失——说明没有真正落盘");
        assert_eq!(read.api_key, sample().api_key);
        assert_eq!(read.secret_key, sample().secret_key);
        assert!(read.demo);

        // 不存在的 id 返回 None，而不是报错
        assert!(vault.credential("nope").expect("查询失败").is_none());

        // 删除后读不到
        vault.delete_credential("c1").expect("删除失败");
        assert!(vault.credential("c1").expect("查询失败").is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 错误密码必须解不开——否则加密形同虚设。
    #[test]
    fn wrong_password_cannot_open_the_vault() {
        let dir = temp_dir("wrong-password");

        {
            let vault = Vault::at(dir.clone()).expect("构造失败");
            vault.unlock("the-right-password").expect("创建失败");
            vault.put_credential("c1", &sample()).expect("写入失败");
        }

        let reopened = Vault::at(dir.clone()).expect("构造失败");
        let result = reopened.unlock("the-wrong-password");

        assert!(result.is_err(), "错误密码必须失败，实际却解锁成功了");
        assert!(!reopened.is_unlocked(), "解锁失败后不应处于已解锁状态");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 覆盖同一条凭据时不应留下旧值。
    #[test]
    fn overwriting_a_credential_replaces_it() {
        let dir = temp_dir("overwrite");
        let vault = Vault::at(dir.clone()).expect("构造失败");
        vault.unlock("pw").expect("创建失败");

        vault.put_credential("c1", &sample()).expect("首次写入失败");

        let updated = StoredCredential {
            api_key: "new-key".to_string(),
            ..sample()
        };
        vault.put_credential("c1", &updated).expect("覆盖失败");

        let read = vault.credential("c1").expect("读取失败").expect("应存在");
        assert_eq!(read.api_key, "new-key");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
