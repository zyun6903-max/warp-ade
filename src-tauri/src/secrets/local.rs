use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use rand::RngCore;

use crate::error::{AppError, AppResult};
use crate::storage::db::app_data_dir;

const MASTER_KEY_FILE: &str = "master.key";
const SECRETS_FILE: &str = "secrets.enc";
const NONCE_LEN: usize = 12;

fn secrets_dir() -> AppResult<PathBuf> {
    let dir = app_data_dir()?.join("secrets");
    fs::create_dir_all(&dir)?;
    restrict_permissions(&dir)?;
    Ok(dir)
}

fn master_key_path() -> AppResult<PathBuf> {
    Ok(secrets_dir()?.join(MASTER_KEY_FILE))
}

fn secrets_file_path() -> AppResult<PathBuf> {
    Ok(secrets_dir()?.join(SECRETS_FILE))
}

fn load_or_create_master_key() -> AppResult<[u8; 32]> {
    let path = master_key_path()?;
    if path.exists() {
        let bytes = fs::read(&path)?;
        if bytes.len() != 32 {
            return Err(AppError::from("本地密钥文件损坏，请删除后重试"));
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        return Ok(key);
    }

    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    fs::write(&path, key)?;
    restrict_permissions(&path)?;
    Ok(key)
}

fn cipher() -> AppResult<Aes256Gcm> {
    let key = load_or_create_master_key()?;
    Aes256Gcm::new_from_slice(&key).map_err(|err| AppError::from(err.to_string()))
}

fn load_secrets_plain() -> AppResult<HashMap<String, String>> {
    let path = secrets_file_path()?;
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let encrypted = fs::read(&path)?;
    if encrypted.len() <= NONCE_LEN {
        return Err(AppError::from("本地密钥数据损坏"));
    }

    let (nonce_bytes, ciphertext) = encrypted.split_at(NONCE_LEN);
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher()?
        .decrypt(nonce, ciphertext)
        .map_err(|_| AppError::from("无法解密本地 API Key，密钥文件可能已损坏"))?;
    serde_json::from_slice(&plaintext).map_err(AppError::from)
}

fn save_secrets_plain(secrets: &HashMap<String, String>) -> AppResult<()> {
    let path = secrets_file_path()?;
    let plaintext = serde_json::to_vec(secrets)?;
    let mut nonce = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce);
    let ciphertext = cipher()?
        .encrypt(Nonce::from_slice(&nonce), plaintext.as_ref())
        .map_err(|err| AppError::from(err.to_string()))?;

    let mut output = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    output.extend_from_slice(&nonce);
    output.extend_from_slice(&ciphertext);
    fs::write(&path, output)?;
    restrict_permissions(&path)?;
    Ok(())
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) -> AppResult<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = if path.is_dir() { 0o700 } else { 0o600 };
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(mode));
    Ok(())
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> AppResult<()> {
    Ok(())
}

pub fn store_api_key(account: &str, key: &str) -> AppResult<()> {
    let mut secrets = load_secrets_plain()?;
    secrets.insert(account.to_string(), key.to_string());
    save_secrets_plain(&secrets)
}

pub fn has_api_key(account: &str) -> AppResult<bool> {
    let secrets = load_secrets_plain()?;
    Ok(secrets
        .get(account)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false))
}

pub fn get_api_key(account: &str) -> AppResult<String> {
    let secrets = load_secrets_plain()?;
    secrets
        .get(account)
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::from("未找到该服务的 API Key"))
}

pub fn delete_api_key(account: &str) -> AppResult<()> {
    let mut secrets = load_secrets_plain()?;
    secrets.remove(account);
    save_secrets_plain(&secrets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use uuid::Uuid;

    fn with_temp_secrets<F: FnOnce() -> AppResult<()>>(test: F) {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

        let temp_root = std::env::temp_dir().join(format!("warp-ade-secrets-test-{}", Uuid::new_v4()));
        std::env::set_var("WARP_ADE_DATA_DIR", temp_root.to_string_lossy().to_string());
        let result = test();
        let _ = fs::remove_dir_all(temp_root);
        result.unwrap();
    }

    #[test]
    fn local_secret_roundtrip() {
        with_temp_secrets(|| {
            let account = format!("provider-{}", Uuid::new_v4());
            store_api_key(&account, "sk-local-secret-test")?;
            assert!(has_api_key(&account)?);
            assert_eq!(get_api_key(&account)?, "sk-local-secret-test");
            delete_api_key(&account)?;
            assert!(!has_api_key(&account)?);
            Ok(())
        });
    }
}
