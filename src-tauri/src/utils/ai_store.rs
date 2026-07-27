use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, AeadCore, Key, Nonce,
};
use aes_gcm::aead::rand_core::RngCore;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

static CIPHER: OnceLock<Aes256Gcm> = OnceLock::new();
static POOL: OnceLock<SqlitePool> = OnceLock::new();

fn db_path() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    exe.parent().unwrap_or_else(|| std::path::Path::new(".")).join("ai_config.db")
}

async fn get_pool() -> &'static SqlitePool {
    POOL.get_or_init(|| {
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_lazy(&format!("sqlite:{}?mode=rwc", db_path().display()))
            .expect("open ai_config.db")
    })
}

async fn ensure_schema() {
    let pool = get_pool().await;
    for sql in [
        "CREATE TABLE IF NOT EXISTS ai_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS ai_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
        "CREATE TABLE IF NOT EXISTS provider_configs (\
            provider_key TEXT PRIMARY KEY,\
            api_url TEXT NOT NULL DEFAULT '',\
            model_name TEXT NOT NULL DEFAULT '',\
            encrypted_api_key TEXT NOT NULL DEFAULT ''\
        )",
    ] {
        sqlx::query(sql).execute(pool).await.ok();
    }
    let _ = ensure_cipher().await;
}

async fn ensure_cipher() -> &'static Aes256Gcm {
    if let Some(c) = CIPHER.get() {
        return c;
    }
    let pool = get_pool().await;
    let key_b64: Result<String, _> =
        sqlx::query_scalar("SELECT value FROM ai_meta WHERE key = 'encryption_key'")
            .fetch_one(pool)
            .await;
    let key_b64 = match key_b64 {
        Ok(k) if !k.is_empty() => k,
        _ => {
            let mut key_bytes = [0u8; 32];
            OsRng.fill_bytes(&mut key_bytes);
            let k = base64::engine::general_purpose::STANDARD.encode(&key_bytes);
            sqlx::query("INSERT OR IGNORE INTO ai_meta (key, value) VALUES ('encryption_key', ?1)")
                .bind(&k)
                .execute(pool)
                .await
                .ok();
            k
        }
    };
    let key_bytes = base64::engine::general_purpose::STANDARD.decode(&key_b64).expect("decode key");
    CIPHER.get_or_init(|| Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes)))
}

async fn encrypt(plain: &str) -> String {
    let cipher = ensure_cipher().await;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ct = cipher.encrypt(&nonce, plain.as_bytes()).expect("encrypt");
    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ct);
    base64::engine::general_purpose::STANDARD.encode(&combined)
}

async fn decrypt(encoded: &str) -> Result<String, String> {
    if encoded.is_empty() { return Ok(String::new()); }
    let cipher = ensure_cipher().await;
    let combined = base64::engine::general_purpose::STANDARD
        .decode(encoded).map_err(|e| format!("base64: {e}"))?;
    if combined.len() < 12 { return Err("too short".into()); }
    let nonce = Nonce::from_slice(&combined[..12]);
    let plain = cipher.decrypt(nonce, &combined[12..]).map_err(|e| format!("decrypt: {e}"))?;
    String::from_utf8(plain).map_err(|e| format!("utf8: {e}"))
}

// ── init ──

pub async fn init_db() {
    ensure_schema().await;
}

// ── current provider ──

pub async fn get_current_provider() -> String {
    ensure_schema().await;
    let pool = get_pool().await;
    sqlx::query_scalar("SELECT value FROM ai_settings WHERE key = 'ai_provider'")
        .fetch_optional(pool).await
        .ok().flatten()
        .unwrap_or_default()
}

pub async fn set_current_provider(key: &str) -> Result<(), String> {
    ensure_schema().await;
    let pool = get_pool().await;
    sqlx::query("INSERT OR REPLACE INTO ai_settings (key, value) VALUES ('ai_provider', ?1)")
        .bind(key).execute(pool).await
        .map_err(|e| format!("save ai_provider: {e}"))?;
    Ok(())
}

// ── provider config ──

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfigFull {
    pub api_url: String,
    pub model_name: String,
    pub api_key: String,
}

pub async fn get_provider_config(provider_key: &str) -> Option<ProviderConfigFull> {
    ensure_schema().await;
    let pool = get_pool().await;
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT api_url, model_name, encrypted_api_key FROM provider_configs WHERE provider_key = ?1"
    )
        .bind(provider_key)
        .fetch_optional(pool).await.ok().flatten();
    if let Some((u, m, ek)) = row {
        Some(ProviderConfigFull {
            api_url: u,
            model_name: m,
            api_key: decrypt(&ek).await.unwrap_or_default(),
        })
    } else {
        None
    }
}

pub async fn save_provider_config(key: &str, api_url: &str, model_name: &str, api_key: &str) -> Result<(), String> {
    ensure_schema().await;
    let pool = get_pool().await;
    let encrypted = if api_key.is_empty() { String::new() } else { encrypt(api_key).await };
    sqlx::query(
        "INSERT OR REPLACE INTO provider_configs (provider_key, api_url, model_name, encrypted_api_key) VALUES (?1, ?2, ?3, ?4)"
    )
        .bind(key).bind(api_url).bind(model_name).bind(&encrypted)
        .execute(pool).await
        .map_err(|e| format!("save provider: {e}"))?;
    Ok(())
}

pub async fn remove_provider(provider_key: &str) -> Result<(), String> {
    ensure_schema().await;
    let pool = get_pool().await;
    sqlx::query("DELETE FROM provider_configs WHERE provider_key = ?1")
        .bind(provider_key).execute(pool).await
        .map_err(|e| format!("remove provider: {e}"))?;
    Ok(())
}

pub async fn get_all_providers() -> HashMap<String, ProviderConfigFull> {
    ensure_schema().await;
    let pool = get_pool().await;
    let rows: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT provider_key, api_url, model_name, encrypted_api_key FROM provider_configs"
    )
        .fetch_all(pool).await.unwrap_or_default();
    let mut map = HashMap::new();
    for (k, u, m, ek) in rows {
        map.insert(k, ProviderConfigFull { api_url: u, model_name: m, api_key: decrypt(&ek).await.unwrap_or_default() });
    }
    map
}

/// 向后兼容：只存 Key，保持 settings_model 旧 API 可用
pub async fn set_api_key(provider_key: &str, api_key: &str) -> Result<(), String> {
    ensure_schema().await;
    let pool = get_pool().await;
    let existing: Option<(String, String, String)> = sqlx::query_as(
        "SELECT api_url, model_name, encrypted_api_key FROM provider_configs WHERE provider_key = ?1"
    ).bind(provider_key).fetch_optional(pool).await.ok().flatten();
    let (url, model) = existing
        .map(|(u, m, _)| (u, m))
        .unwrap_or_default();
    save_provider_config(provider_key, &url, &model, api_key).await
}

pub async fn get_api_key(provider_key: &str) -> Result<String, String> {
    get_provider_config(provider_key).await
        .map(|c| c.api_key)
        .ok_or_else(|| "provider not found".to_string())
}

// ── migration from old settings.json + credential manager ──

pub async fn migrate_from_old() {
    ensure_schema().await;
    let pool = get_pool().await;

    let has_providers: bool = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM provider_configs"
    ).fetch_one(pool).await.map(|c| c > 0).unwrap_or(false);

    if has_providers {
        return;
    }

    let settings = crate::utils::system_utils::load_settings().unwrap_or_default();
    let old_providers = settings.provider_configs.clone();

    for (key, cfg) in &old_providers {
        let mut api_key = String::new();
        #[cfg(windows)]
        {
            let target = format!("fuyun_tools/api_key_{}", key);
            if let Ok(k) = crate::utils::settings_model::read_windows_credential(&target) {
                if !k.is_empty() {
                    api_key = k;
                    crate::utils::settings_model::delete_windows_credential(&target);
                }
            }
        }
        save_provider_config(key, &cfg.api_url, &cfg.model_name, &api_key).await.ok();
    }

    if !settings.ai_provider.is_empty() {
        set_current_provider(&settings.ai_provider).await.ok();
    }

    if !old_providers.is_empty() {
        log::info!("AI 配置已从旧 settings.json 迁移（{} 个提供商）", old_providers.len());
    }
}
