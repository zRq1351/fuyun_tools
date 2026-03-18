use crate::core::config::{
    ProviderConfig, DEFAULT_IMAGE_TOGGLE_SHORTCUT, DEFAULT_TOGGLE_SHORTCUT,
};
use keyring::Entry;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const LEGACY_ENCRYPTION_KEY: &[u8] = b"fuyun_tools_encryption_key_2025!"; // 32字节旧版密钥，仅用于迁移

/// 获取应用默认版本号
pub fn get_default_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppSettingsData {
    pub version: String,
    pub max_items: usize,
    #[serde(default = "default_text_max_items")]
    pub text_max_items: usize,
    #[serde(default = "default_image_max_items")]
    pub image_max_items: usize,
    #[serde(default = "default_image_disk_limit_mb")]
    pub image_disk_limit_mb: u64,
    pub hot_key: String,
    #[serde(default = "default_image_hot_key")]
    pub image_hot_key: String,
    #[serde(default)]
    pub ai_provider: String,
    /// 每个AI提供商的独立配置
    #[serde(default)]
    pub provider_configs: HashMap<String, ProviderConfig>,
    #[serde(default = "default_selection_enabled")]
    pub selection_enabled: bool,
    #[serde(default = "default_grouped_items_protected_from_limit")]
    pub grouped_items_protected_from_limit: bool,
    #[serde(default = "default_clipboard_bottom_offset")]
    pub clipboard_bottom_offset: i32,
    #[serde(default = "default_translation_prompt_template")]
    pub translation_prompt_template: String,
    #[serde(default = "default_explanation_prompt_template")]
    pub explanation_prompt_template: String,
    #[serde(default = "default_image_fill_verify_mode")]
    pub image_fill_verify_mode: String,
}

impl Default for AppSettingsData {
    fn default() -> Self {
        Self {
            version: get_default_app_version(),
            max_items: 50,
            text_max_items: default_text_max_items(),
            image_max_items: default_image_max_items(),
            image_disk_limit_mb: default_image_disk_limit_mb(),
            hot_key: DEFAULT_TOGGLE_SHORTCUT.to_string(),
            image_hot_key: default_image_hot_key(),
            ai_provider: "deepseek".to_string(),
            provider_configs: HashMap::new(),
            selection_enabled: true,
            grouped_items_protected_from_limit: default_grouped_items_protected_from_limit(),
            clipboard_bottom_offset: default_clipboard_bottom_offset(),
            translation_prompt_template: default_translation_prompt_template(),
            explanation_prompt_template: default_explanation_prompt_template(),
            image_fill_verify_mode: default_image_fill_verify_mode(),
        }
    }
}

fn default_selection_enabled() -> bool {
    true
}

fn default_text_max_items() -> usize {
    50
}

fn default_image_max_items() -> usize {
    50
}

fn default_image_disk_limit_mb() -> u64 {
    2048
}

fn default_image_hot_key() -> String {
    DEFAULT_IMAGE_TOGGLE_SHORTCUT.to_string()
}

fn default_grouped_items_protected_from_limit() -> bool {
    true
}

fn default_clipboard_bottom_offset() -> i32 {
    8
}

fn default_image_fill_verify_mode() -> String {
    "fast".to_string()
}

pub fn default_translation_prompt_template() -> String {
    "你是专业翻译助手。任务：将用户文本翻译为{target_language}。\n要求：\n1) 自动识别源语言（如已提供{source_language}且不是“自动识别”，按其处理）。\n2) 忠实原意，不遗漏、不杜撰。\n3) 保留专有名词、代码、变量、URL、邮箱、数字与单位。\n4) 保持原文段落与换行结构。\n5) 只输出译文，不要任何说明。\n\n待翻译文本：\n{text}".to_string()
}

pub fn default_explanation_prompt_template() -> String {
    "你是清晰易懂的讲解助手。请使用{target_language}解释下列内容。\n要求：\n1) 先给一句话总结，再分点说明关键点。\n2) 面向普通用户，术语给简短释义。\n3) 保持准确，不编造；不确定时直接说明。\n4) 控制在180字以内。\n5) 仅输出解释内容。\n\n待解释文本：\n{text}".to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct MigrationVersion {
    major: u32,
    minor: u32,
    patch: u32,
}

impl MigrationVersion {
    const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self { major, minor, patch }
    }
}

fn parse_migration_version(raw: &str) -> Option<MigrationVersion> {
    let trimmed = raw.trim().trim_start_matches('v');
    if trimmed.is_empty() {
        return None;
    }

    if !trimmed.contains('.') {
        if let Ok(legacy) = trimmed.parse::<u32>() {
            return Some(MigrationVersion::new(0, legacy, 0));
        }
    }

    let core = trimmed
        .split_once('-')
        .map(|(left, _)| left)
        .unwrap_or(trimmed)
        .split_once('+')
        .map(|(left, _)| left)
        .unwrap_or(trimmed);
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u32>().ok()?;
    let minor = parts.next().unwrap_or("0").parse::<u32>().ok()?;
    let patch = parts.next().unwrap_or("0").parse::<u32>().ok()?;
    Some(MigrationVersion::new(major, minor, patch))
}

impl AppSettingsData {
    /// 为指定提供商设置API密钥（存储到系统凭据管理器）
    pub fn set_provider_api_key(&mut self, provider_key: &str, api_key: &str) -> Result<(), String> {
        if let Some(config) = self.provider_configs.get_mut(provider_key) {
            config.encrypted_api_key.clear();
        }

        let service_name = "fuyun_tools";
        let user_name = format!("api_key_{}", provider_key);

        if api_key.is_empty() {
            if let Ok(entry) = Entry::new(service_name, &user_name) {
                let _ = entry.delete_credential();
            }
            log::info!("API key cleared for provider: {}", provider_key);
            return Ok(());
        }

        // 尝试创建并写入
        match Entry::new(service_name, &user_name) {
            Ok(entry) => {
                let mut last_error = String::new();
                for i in 0..3 {
                    match entry.set_password(api_key) {
                        Ok(_) => {
                            log::info!("API key saved for provider: {} (attempt {})", provider_key, i + 1);
                            return Ok(());
                        },
                        Err(e) => {
                            let _ = entry.delete_credential();
                            log::warn!("Failed to save API key (attempt {}): {}", i + 1, e);
                            last_error = e.to_string();
                            thread::sleep(Duration::from_millis(100));
                        }
                    }
                }
                Err(format!("保存API密钥失败(重试3次后): {}", last_error))
            },
            Err(e) => Err(format!("创建密钥入口失败: {}", e))
        }
    }

    /// 获取指定提供商的API密钥（从系统凭据管理器）
    pub fn get_provider_api_key(&self, provider_key: &str) -> Result<String, String> {
        let service_name = "fuyun_tools";
        let user_name = format!("api_key_{}", provider_key);

        let entry = Entry::new(service_name, &user_name)
            .map_err(|e| format!("创建密钥入口失败: {}", e))?;

        // 增加重试机制
        let mut last_error = String::new();
        for i in 0..3 {
            match entry.get_password() {
                Ok(password) => {
                    log::info!("Successfully retrieved API key for provider: {} (attempt {})", provider_key, i + 1);
                    return Ok(password);
                },
                Err(keyring::Error::NoEntry) => {
                    log::info!("No API key found in keyring for provider: {}", provider_key);
                    return Ok(String::new());
                },
                Err(e) => {
                    let error_msg = e.to_string();
                    if error_msg.contains("Element not found") || error_msg.contains("找不到元素") {
                        return Ok(String::new());
                    }

                    log::warn!("Failed to retrieve API key for provider {} (attempt {}): {}", provider_key, i + 1, e);
                    last_error = error_msg;
                    thread::sleep(Duration::from_millis(100));
                }
            }
        }

        log::error!("Failed to retrieve API key after retries for provider {}: {}", provider_key, last_error);
        Err(format!("获取API密钥失败: {}", last_error))
    }

    /// 迁移旧版加密的API密钥到系统凭据管理器
    /// 返回是否发生了迁移
    pub fn migrate_legacy_api_keys(&mut self) -> bool {
        let mut migrated = false;
        let provider_keys: Vec<String> = self.provider_configs.keys().cloned().collect();

        for provider_key in provider_keys {
            if let Some(config) = self.provider_configs.get_mut(&provider_key) {
                if !config.encrypted_api_key.is_empty() {
                    log::info!("发现旧版加密密钥，正在迁移提供商: {}", provider_key);

                    // 解密旧版密钥
                    use base64::engine::general_purpose::STANDARD;
                    use base64::Engine as _;

                    let decrypted_result = STANDARD.decode(&config.encrypted_api_key)
                        .ok()
                        .and_then(|encrypted| {
                            let decrypted: Vec<u8> = encrypted
                                .iter()
                                .enumerate()
                                .map(|(i, &b)| b ^ LEGACY_ENCRYPTION_KEY[i % LEGACY_ENCRYPTION_KEY.len()])
                                .collect();
                            String::from_utf8(decrypted).ok()
                        });

                    if let Some(api_key) = decrypted_result {
                        // 保存到系统凭据管理器
                        if let Ok(entry) = Entry::new("fuyun_tools", &format!("api_key_{}", provider_key)) {
                            if let Err(e) = entry.set_password(&api_key) {
                                log::error!("迁移密钥失败: {}", e);
                            } else {
                                log::info!("密钥迁移成功");
                                migrated = true;
                            }
                        }
                    }
                    config.encrypted_api_key.clear();
                }
            }
        }
        migrated
    }

    /// 保存当前提供商的配置
    pub fn save_current_provider_config(&mut self, api_key: &str) -> Result<(), String> {
        let provider_key = self.ai_provider.clone();  // 克隆避免借用冲突

        self.set_provider_api_key(&provider_key, api_key)?;

        Ok(())
    }

    /// 加载指定提供商的配置到当前设置
    pub fn load_provider_config_to_current(
        &mut self,
        provider_name: &str,
    ) -> Result<ProviderConfig, String> {
        let provider_key = provider_name.to_string();

        // 先获取配置的副本
        let config_copy = if let Some(config) = self.provider_configs.get(&provider_key) {
            config.clone()
        } else {
            let (default_url, default_model) = match provider_name {
                "deepseek" => (
                    "https://api.deepseek.com/v1".to_string(),
                    "deepseek-chat".to_string(),
                ),
                "qwen" => (
                    "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(),
                    "qwen-plus".to_string(),
                ),
                "xiaomimimo" => (
                    "https://api.xiaomimimo.com/v1".to_string(),
                    "mimo-v2-flash".to_string(),
                ),
                _ => {
                    (String::new(), String::new())
                }
            };
            ProviderConfig {
                api_url: default_url,
                model_name: default_model,
                encrypted_api_key: String::new(),
            }
        };

        let _ = self.get_provider_api_key(&provider_key);

        self.ai_provider = provider_name.to_string();

        if self.provider_configs.contains_key(&provider_key) {
            if let Some(decrypted_config) = self.provider_configs.get(&provider_key) {
                Ok(decrypted_config.clone())
            } else {
                Ok(config_copy)
            }
        } else {
            Ok(config_copy)
        }
    }

    /// 获取当前提供商的配置信息
    pub fn get_current_provider_config(&self) -> Option<&ProviderConfig> {
        self.provider_configs.get(&self.ai_provider)
    }

    /// 验证设置有效性
    pub fn validate(&self) -> Result<(), String> {
        if self.max_items == 0 || self.max_items > 1000 {
            return Err("max_items必须在1-1000之间".to_string());
        }
        if self.text_max_items == 0 || self.text_max_items > 1000 {
            return Err("text_max_items必须在1-1000之间".to_string());
        }
        if self.image_max_items == 0 || self.image_max_items > 1000 {
            return Err("image_max_items必须在1-1000之间".to_string());
        }
        if self.image_disk_limit_mb < 100 || self.image_disk_limit_mb > 102400 {
            return Err("image_disk_limit_mb必须在100-102400之间".to_string());
        }
        if self.image_fill_verify_mode != "strict" && self.image_fill_verify_mode != "fast" {
            return Err("image_fill_verify_mode必须是strict或fast".to_string());
        }

        Ok(())
    }

    /// 获取部分隐藏的API密钥（用于前端显示）
    pub fn get_masked_api_key(&self) -> String {
        match self.get_provider_api_key(&self.ai_provider) {
            Ok(api_key) => {
                if api_key.is_empty() {
                    return String::new();
                }

                let len = api_key.len();

                if len <= 16 {
                    return "*".repeat(len.min(30));
                }

                let prefix = &api_key[..8.min(len)];
                let suffix = &api_key[len - 8.min(len - 8)..];

                format!("{}{}{}", prefix, "*".repeat(30), suffix)
            }
            Err(_) => String::new(),
        }
    }

    /// 迁移旧版本设置
    pub fn migrate_from_old(&mut self) {
        let current_version = get_default_app_version();

        if self.version == current_version {
            log::debug!("当前已是最新版本: {}，无需迁移", self.version);
            return;
        }

        match (
            parse_migration_version(&self.version),
            parse_migration_version(&current_version),
        ) {
            (Some(old_ver), Some(new_ver)) => {
                if old_ver < new_ver {
                    log::debug!("执行版本 {} 到 {} 的迁移", self.version, current_version);
                    self.perform_version_migration(old_ver, new_ver);
                }
            }
            _ => {
                log::debug!("无法解析版本号格式，执行通用迁移");
                self.perform_generic_migration();
            }
        }

        self.version = current_version;
        log::debug!("版本迁移完成，当前版本: {}", self.version);
    }

    /// 执行具体的版本迁移逻辑
    fn perform_version_migration(&mut self, old_version: MigrationVersion, new_version: MigrationVersion) {
        log::info!("执行版本迁移: {:?} -> {:?}", old_version, new_version);
        if old_version < MigrationVersion::new(0, 3, 0)
            && new_version >= MigrationVersion::new(0, 3, 0)
        {
            log::info!("迁移至版本 3: 初始化AI提供商配置");
            self.initialize_ai_provider_configs_if_needed();
        }

        if old_version < MigrationVersion::new(0, 2, 0)
            && new_version >= MigrationVersion::new(0, 2, 0)
        {
            log::info!("迁移至版本 2: 确保基础配置完整性");
            self.ensure_basic_config_integrity();
        }
    }

    /// 执行通用迁移（当版本号无法解析时）
    fn perform_generic_migration(&mut self) {
        log::info!("执行通用配置迁移");

        self.ensure_basic_config_integrity();

        self.initialize_ai_provider_configs_if_needed();
    }

    /// 确保基础配置完整性
    fn ensure_basic_config_integrity(&mut self) {
        log::info!("开始确保基础配置完整性");
        log::debug!("迁移前 max_items: {}", self.max_items);

        if self.max_items < 10 || self.max_items > 1000 {
            let old_value = self.max_items;
            self.max_items = 50;
            log::info!("修复 max_items 从 {} 为默认值: 50", old_value);
        }
        if self.text_max_items == default_text_max_items()
            && self.image_max_items == default_image_max_items()
            && self.max_items >= 10
            && self.max_items <= 1000
        {
            self.text_max_items = self.max_items;
            self.image_max_items = self.max_items;
        }
        if self.text_max_items < 10 || self.text_max_items > 1000 {
            self.text_max_items = default_text_max_items();
        }
        if self.image_max_items < 10 || self.image_max_items > 1000 {
            self.image_max_items = default_image_max_items();
        }
        if self.image_disk_limit_mb < 100 || self.image_disk_limit_mb > 102400 {
            self.image_disk_limit_mb = default_image_disk_limit_mb();
        }
        self.max_items = self.text_max_items;

        if self.hot_key.is_empty() {
            self.hot_key = DEFAULT_TOGGLE_SHORTCUT.to_string();
            log::info!("修复 hot_key 为默认值: {}", DEFAULT_TOGGLE_SHORTCUT);
        }

        if self.image_hot_key.is_empty() {
            self.image_hot_key = default_image_hot_key();
        }

        if self.clipboard_bottom_offset < 0 || self.clipboard_bottom_offset > 400 {
            self.clipboard_bottom_offset = default_clipboard_bottom_offset();
        }

        if self.translation_prompt_template.trim().is_empty() {
            self.translation_prompt_template = default_translation_prompt_template();
        }

        if self.explanation_prompt_template.trim().is_empty() {
            self.explanation_prompt_template = default_explanation_prompt_template();
        }
        if self.image_fill_verify_mode != "strict" && self.image_fill_verify_mode != "fast" {
            self.image_fill_verify_mode = default_image_fill_verify_mode();
        }

        log::debug!(
            "迁移后 max_items: {}, text_max_items: {}, image_max_items: {}, image_disk_limit_mb: {}",
            self.max_items,
            self.text_max_items,
            self.image_max_items,
            self.image_disk_limit_mb
        );
    }

    /// 初始化AI提供商配置（如果需要）
    fn initialize_ai_provider_configs_if_needed(&mut self) {
        // 如果提供商配置为空，初始化默认配置
        if self.provider_configs.is_empty() {
            initialize_builtin_providers(self);
            log::info!("初始化内置AI提供商配置");
        }

        if !self.provider_configs.contains_key(&self.ai_provider) {
            let (default_url, default_model) = self.get_provider_default_config(&self.ai_provider);

            let config = ProviderConfig {
                api_url: default_url,
                model_name: default_model,
                encrypted_api_key: String::new(),
            };

            self.provider_configs.insert(self.ai_provider.clone(), config);
            log::info!("为提供商 {} 创建默认配置", self.ai_provider);
        }
    }

    /// 获取提供商的默认配置
    fn get_provider_default_config(&self, provider_name: &str) -> (String, String) {
        match provider_name {
            "deepseek" => (
                "https://api.deepseek.com/v1".to_string(),
                "deepseek-chat".to_string(),
            ),
            "qwen" => (
                "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(),
                "qwen-plus".to_string(),
            ),
            "xiaomimimo" => (
                "https://api.xiaomimimo.com/v1".to_string(),
                "mimo-v2-flash".to_string(),
            ),
            _ => {
                (String::new(), String::new())
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ClipboardHistoryData {
    pub items: Vec<String>,
    #[serde(default)]
    pub categories: HashMap<String, String>,
    #[serde(default)]
    pub category_list: Vec<String>,
    #[serde(default)]
    pub pinned_items: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardHistoryPageItem {
    pub position: usize,
    pub id: String,
    pub content: String,
    pub category: String,
    pub pinned: bool,
    pub updated_at: i64,
    pub snippet: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardHistoryPageData {
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub items: Vec<ClipboardHistoryPageItem>,
}
/// 获取设置文件路径
pub fn get_settings_file_path() -> PathBuf {
    let mut settings_dir = env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    settings_dir.pop();
    settings_dir.push("settings.json");
    settings_dir
}

pub fn get_history_db_path() -> PathBuf {
    let mut history_dir = env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    history_dir.pop();
    history_dir.push("history.db");
    history_dir
}

fn open_history_db() -> Result<Connection, String> {
    let db_path = get_history_db_path();
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建历史数据库目录失败: {}", e))?;
    }
    let conn = Connection::open(db_path).map_err(|e| format!("打开历史数据库失败: {}", e))?;
    conn.busy_timeout(Duration::from_millis(1200))
        .map_err(|e| format!("设置历史数据库超时失败: {}", e))?;
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA temp_store = MEMORY;
        ",
    )
    .map_err(|e| format!("设置历史数据库参数失败: {}", e))?;
    ensure_history_db_schema(&conn)?;
    Ok(conn)
}

fn ensure_history_db_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS history_items (
            position INTEGER PRIMARY KEY,
            content TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS categories (
            item TEXT PRIMARY KEY,
            category TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS category_list (
            position INTEGER PRIMARY KEY,
            category TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS pinned_items (
            position INTEGER PRIMARY KEY,
            item TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_history_items_position ON history_items(position);
        CREATE INDEX IF NOT EXISTS idx_categories_category ON categories(category);
        ",
    )
    .map_err(|e| format!("初始化历史数据库失败: {}", e))?;

    ensure_sqlite_column(conn, "history_items", "item_id", "TEXT")?;
    ensure_sqlite_column(conn, "history_items", "content_hash", "TEXT")?;
    ensure_sqlite_column(conn, "history_items", "created_at", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_sqlite_column(conn, "history_items", "updated_at", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_sqlite_column(conn, "categories", "item_id", "TEXT")?;
    ensure_sqlite_column(conn, "pinned_items", "item_id", "TEXT")?;

    conn.execute(
        "UPDATE history_items
         SET created_at = CAST(strftime('%s','now') AS INTEGER) * 1000
         WHERE created_at <= 0",
        [],
    )
    .map_err(|e| format!("初始化历史数据库失败: {}", e))?;
    conn.execute(
        "UPDATE history_items
         SET updated_at = created_at
         WHERE updated_at <= 0",
        [],
    )
    .map_err(|e| format!("初始化历史数据库失败: {}", e))?;
    conn.execute(
        "
        UPDATE categories
        SET item_id = (
            SELECT hi.item_id
            FROM history_items hi
            WHERE hi.content = categories.item
            LIMIT 1
        )
        WHERE item_id IS NULL OR item_id = ''
        ",
        [],
    )
    .map_err(|e| format!("初始化历史数据库失败: {}", e))?;
    conn.execute(
        "
        UPDATE pinned_items
        SET item_id = (
            SELECT hi.item_id
            FROM history_items hi
            WHERE hi.content = pinned_items.item
            LIMIT 1
        )
        WHERE item_id IS NULL OR item_id = ''
        ",
        [],
    )
    .map_err(|e| format!("初始化历史数据库失败: {}", e))?;
    conn.execute_batch(
        "
        CREATE INDEX IF NOT EXISTS idx_history_items_item_id ON history_items(item_id);
        CREATE INDEX IF NOT EXISTS idx_history_items_updated_at ON history_items(updated_at);
        CREATE INDEX IF NOT EXISTS idx_categories_item_id ON categories(item_id);
        CREATE INDEX IF NOT EXISTS idx_pinned_items_item_id ON pinned_items(item_id);
        ",
    )
    .map_err(|e| format!("初始化历史数据库失败: {}", e))?;
    ensure_history_fts(conn);
    Ok(())
}

fn ensure_history_fts(conn: &Connection) {
    let create_result = conn.execute_batch(
        "
        CREATE VIRTUAL TABLE IF NOT EXISTS history_items_fts USING fts5(
            item_id UNINDEXED,
            content,
            tokenize = 'unicode61'
        );
        ",
    );
    if create_result.is_err() {
        return;
    }

    let _ = conn.execute(
        "
        INSERT OR REPLACE INTO history_items_fts(rowid, item_id, content)
        SELECT position + 1, COALESCE(item_id, ''), content
        FROM history_items
        ",
        [],
    );
    let _ = conn.execute(
        "
        DELETE FROM history_items_fts
        WHERE rowid NOT IN (SELECT position + 1 FROM history_items)
        ",
        [],
    );
}

fn ensure_sqlite_column(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), String> {
    let pragma_sql = format!("PRAGMA table_info({})", table);
    let mut stmt = conn
        .prepare(&pragma_sql)
        .map_err(|e| format!("读取历史数据库结构失败: {}", e))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| format!("读取历史数据库结构失败: {}", e))?;
    let mut exists = false;
    for row in rows {
        if row.map_err(|e| format!("读取历史数据库结构失败: {}", e))? == column {
            exists = true;
            break;
        }
    }
    drop(stmt);

    if !exists {
        let sql = format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, definition);
        conn.execute(&sql, [])
            .map_err(|e| format!("升级历史数据库结构失败: {}", e))?;
    }
    Ok(())
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn stable_history_item_id(content: &str) -> String {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    let hash = hasher.finish();
    format!("{:016x}-{}", hash, content.chars().count())
}

fn stable_history_content_hash(content: &str) -> String {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn sqlite_history_has_any_data(conn: &Connection) -> Result<bool, String> {
    let history_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM history_items", [], |row| row.get(0))
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    let categories_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM categories", [], |row| row.get(0))
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    let category_list_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM category_list", [], |row| row.get(0))
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    let pinned_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM pinned_items", [], |row| row.get(0))
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    Ok(history_count + categories_count + category_list_count + pinned_count > 0)
}

fn load_history_data_from_sqlite() -> Result<Option<ClipboardHistoryData>, String> {
    let db_path = get_history_db_path();
    if !db_path.exists() {
        return Ok(None);
    }
    let conn = open_history_db()?;
    if !sqlite_history_has_any_data(&conn)? {
        return Ok(None);
    }

    let mut items_stmt = conn
        .prepare("SELECT content FROM history_items ORDER BY position ASC")
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    let items_iter = items_stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    let mut items = Vec::new();
    for row in items_iter {
        items.push(row.map_err(|e| format!("读取历史数据库失败: {}", e))?);
    }

    let mut categories_stmt = conn
        .prepare("SELECT item, category FROM categories")
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    let categories_iter = categories_stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    let mut categories = HashMap::new();
    for row in categories_iter {
        let (item, category) = row.map_err(|e| format!("读取历史数据库失败: {}", e))?;
        categories.insert(item, category);
    }

    let mut category_list_stmt = conn
        .prepare("SELECT category FROM category_list ORDER BY position ASC")
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    let category_list_iter = category_list_stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    let mut category_list = Vec::new();
    for row in category_list_iter {
        category_list.push(row.map_err(|e| format!("读取历史数据库失败: {}", e))?);
    }

    let mut pinned_stmt = conn
        .prepare("SELECT item FROM pinned_items ORDER BY position ASC")
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    let pinned_iter = pinned_stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    let mut pinned_items = Vec::new();
    for row in pinned_iter {
        pinned_items.push(row.map_err(|e| format!("读取历史数据库失败: {}", e))?);
    }

    Ok(Some(ClipboardHistoryData {
        items,
        categories,
        category_list,
        pinned_items,
    }))
}

fn load_indexed_values_from_table(
    tx: &rusqlite::Transaction<'_>,
    table: &str,
    value_column: &str,
) -> Result<Vec<String>, String> {
    let sql = format!(
        "SELECT {} FROM {} ORDER BY position ASC",
        value_column, table
    );
    let mut stmt = tx
        .prepare(&sql)
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("读取历史数据库失败: {}", e))?);
    }
    Ok(out)
}

fn sync_indexed_values_table(
    tx: &rusqlite::Transaction<'_>,
    table: &str,
    value_column: &str,
    values: &[String],
) -> Result<(), String> {
    let existing = load_indexed_values_from_table(tx, table, value_column)?;
    let shared_len = existing.len().min(values.len());
    let update_sql = format!(
        "UPDATE {} SET {} = ?1 WHERE position = ?2",
        table, value_column
    );
    let insert_sql = format!(
        "INSERT INTO {}(position, {}) VALUES(?1, ?2)",
        table, value_column
    );
    let delete_sql = format!("DELETE FROM {} WHERE position >= ?1", table);

    for idx in 0..shared_len {
        if existing[idx] != values[idx] {
            tx.execute(&update_sql, params![values[idx], idx as i64])
                .map_err(|e| format!("写入历史数据库失败: {}", e))?;
        }
    }

    if values.len() > existing.len() {
        for idx in existing.len()..values.len() {
            tx.execute(&insert_sql, params![idx as i64, values[idx]])
                .map_err(|e| format!("写入历史数据库失败: {}", e))?;
        }
    } else if existing.len() > values.len() {
        tx.execute(&delete_sql, params![values.len() as i64])
            .map_err(|e| format!("写入历史数据库失败: {}", e))?;
    }

    Ok(())
}

fn load_history_item_rows(
    tx: &rusqlite::Transaction<'_>,
) -> Result<Vec<(String, String, i64, i64)>, String> {
    let mut stmt = tx
        .prepare(
            "
            SELECT content, COALESCE(item_id, ''), COALESCE(created_at, 0), COALESCE(updated_at, 0)
            FROM history_items
            ORDER BY position ASC
            ",
        )
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("读取历史数据库失败: {}", e))?);
    }
    Ok(out)
}

fn sync_history_items_table(
    tx: &rusqlite::Transaction<'_>,
    values: &[String],
) -> Result<(), String> {
    let existing = load_history_item_rows(tx)?;
    let shared_len = existing.len().min(values.len());
    let now_ms = now_unix_ms();
    let fts_enabled = history_fts_enabled(tx)?;

    for idx in 0..shared_len {
        let new_content = &values[idx];
        let (old_content, old_id, old_created_at, _) = &existing[idx];
        if old_content == new_content {
            if old_id.is_empty() {
                let new_item_id = stable_history_item_id(new_content);
                tx.execute(
                    "UPDATE history_items
                     SET item_id = ?1, content_hash = ?2, updated_at = CASE WHEN updated_at > 0 THEN updated_at ELSE ?3 END
                     WHERE position = ?4",
                    params![
                        new_item_id,
                        stable_history_content_hash(new_content),
                        now_ms,
                        idx as i64
                    ],
                )
                .map_err(|e| format!("写入历史数据库失败: {}", e))?;
                sync_history_fts_row(tx, fts_enabled, idx as i64, &new_item_id, new_content)?;
            }
            continue;
        }
        let new_item_id = stable_history_item_id(new_content);

        tx.execute(
            "UPDATE history_items
             SET content = ?1, item_id = ?2, content_hash = ?3, created_at = ?4, updated_at = ?5
             WHERE position = ?6",
            params![
                new_content,
                new_item_id,
                stable_history_content_hash(new_content),
                if *old_created_at > 0 { *old_created_at } else { now_ms },
                now_ms,
                idx as i64
            ],
        )
        .map_err(|e| format!("写入历史数据库失败: {}", e))?;
        sync_history_fts_row(tx, fts_enabled, idx as i64, &new_item_id, new_content)?;
    }

    if values.len() > existing.len() {
        for idx in existing.len()..values.len() {
            let content = &values[idx];
            let item_id = stable_history_item_id(content);
            tx.execute(
                "INSERT INTO history_items(position, content, item_id, content_hash, created_at, updated_at)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    idx as i64,
                    content,
                    item_id,
                    stable_history_content_hash(content),
                    now_ms,
                    now_ms
                ],
            )
            .map_err(|e| format!("写入历史数据库失败: {}", e))?;
            sync_history_fts_row(tx, fts_enabled, idx as i64, &item_id, content)?;
        }
    } else if existing.len() > values.len() {
        tx.execute(
            "DELETE FROM history_items WHERE position >= ?1",
            params![values.len() as i64],
        )
        .map_err(|e| format!("写入历史数据库失败: {}", e))?;
        if fts_enabled {
            tx.execute(
                "DELETE FROM history_items_fts WHERE rowid > ?1",
                params![values.len() as i64],
            )
            .map_err(|e| format!("写入历史数据库失败: {}", e))?;
        }
    }

    Ok(())
}

fn history_fts_enabled(tx: &rusqlite::Transaction<'_>) -> Result<bool, String> {
    let count: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'history_items_fts'",
            [],
            |row| row.get(0),
        )
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    Ok(count > 0)
}

fn history_fts_enabled_conn(conn: &Connection) -> Result<bool, String> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'history_items_fts'",
            [],
            |row| row.get(0),
        )
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    Ok(count > 0)
}

fn sync_history_fts_row(
    tx: &rusqlite::Transaction<'_>,
    fts_enabled: bool,
    position: i64,
    item_id: &str,
    content: &str,
) -> Result<(), String> {
    if !fts_enabled {
        return Ok(());
    }
    tx.execute(
        "INSERT OR REPLACE INTO history_items_fts(rowid, item_id, content) VALUES(?1, ?2, ?3)",
        params![position + 1, item_id, content],
    )
    .map_err(|e| format!("写入历史数据库失败: {}", e))?;
    Ok(())
}

fn build_fts_query(keyword: &str) -> String {
    let normalized = keyword.trim().replace('"', " ");
    format!("\"{}\"", normalized)
}

fn resolve_history_sort(_sort_by: Option<String>, sort_order: Option<String>) -> String {
    let order = match sort_order.as_deref() {
        Some("desc") | Some("DESC") => "DESC",
        _ => "ASC",
    };
    format!(
        "CASE WHEN p.item IS NULL THEN 1 ELSE 0 END ASC, CASE WHEN p.item IS NOT NULL THEN hi.position END ASC, CASE WHEN p.item IS NULL THEN hi.position END {}",
        order,
    )
}

fn build_keyword_snippet(content: &str, keyword: &str) -> String {
    let text = content.trim();
    if text.is_empty() {
        return String::new();
    }
    let key = keyword.trim();
    if key.is_empty() {
        return text.chars().take(120).collect();
    }

    if let Some(pos) = text.find(key) {
        let center = text[..pos].chars().count();
        let chars: Vec<char> = text.chars().collect();
        let key_len = key.chars().count().max(1);
        let start = center.saturating_sub(36);
        let end = (center + key_len + 84).min(chars.len());
        chars[start..end].iter().collect()
    } else {
        text.chars().take(120).collect()
    }
}

fn sync_categories_table(
    tx: &rusqlite::Transaction<'_>,
    categories: &HashMap<String, String>,
    item_id_by_content: &HashMap<String, String>,
) -> Result<(), String> {
    let mut stmt = tx
        .prepare("SELECT item, category, COALESCE(item_id, '') FROM categories")
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    let mut existing: HashMap<String, (String, String)> = HashMap::new();
    for row in rows {
        let (item, category, item_id) = row.map_err(|e| format!("读取历史数据库失败: {}", e))?;
        existing.insert(item, (category, item_id));
    }
    drop(stmt);

    for (item, category) in categories {
        let new_item_id = item_id_by_content
            .get(item)
            .cloned()
            .unwrap_or_else(|| stable_history_item_id(item));
        match existing.get(item) {
            Some((current_category, current_item_id))
                if current_category == category && current_item_id == &new_item_id => {}
            Some(_) => {
                tx.execute(
                    "UPDATE categories SET category = ?1, item_id = ?2 WHERE item = ?3",
                    params![category, new_item_id, item],
                )
                .map_err(|e| format!("写入历史数据库失败: {}", e))?;
            }
            None => {
                tx.execute(
                    "INSERT INTO categories(item, category, item_id) VALUES(?1, ?2, ?3)",
                    params![item, category, new_item_id],
                )
                .map_err(|e| format!("写入历史数据库失败: {}", e))?;
            }
        }
    }

    for item in existing.keys() {
        if !categories.contains_key(item) {
            tx.execute("DELETE FROM categories WHERE item = ?1", params![item])
                .map_err(|e| format!("写入历史数据库失败: {}", e))?;
        }
    }

    Ok(())
}

fn load_pinned_item_rows(tx: &rusqlite::Transaction<'_>) -> Result<Vec<(String, String)>, String> {
    let mut stmt = tx
        .prepare("SELECT item, COALESCE(item_id, '') FROM pinned_items ORDER BY position ASC")
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    let rows = stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .map_err(|e| format!("读取历史数据库失败: {}", e))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| format!("读取历史数据库失败: {}", e))?);
    }
    Ok(out)
}

fn sync_pinned_items_table(
    tx: &rusqlite::Transaction<'_>,
    pinned_items: &[String],
    item_id_by_content: &HashMap<String, String>,
) -> Result<(), String> {
    let existing = load_pinned_item_rows(tx)?;
    let shared_len = existing.len().min(pinned_items.len());
    let update_sql = "UPDATE pinned_items SET item = ?1, item_id = ?2 WHERE position = ?3";
    let insert_sql = "INSERT INTO pinned_items(position, item, item_id) VALUES(?1, ?2, ?3)";
    let delete_sql = "DELETE FROM pinned_items WHERE position >= ?1";

    for idx in 0..shared_len {
        let item = &pinned_items[idx];
        let item_id = item_id_by_content
            .get(item)
            .cloned()
            .unwrap_or_else(|| stable_history_item_id(item));
        if existing[idx].0 != *item || existing[idx].1 != item_id {
            tx.execute(update_sql, params![item, item_id, idx as i64])
                .map_err(|e| format!("写入历史数据库失败: {}", e))?;
        }
    }

    if pinned_items.len() > existing.len() {
        for idx in existing.len()..pinned_items.len() {
            let item = &pinned_items[idx];
            let item_id = item_id_by_content
                .get(item)
                .cloned()
                .unwrap_or_else(|| stable_history_item_id(item));
            tx.execute(insert_sql, params![idx as i64, item, item_id])
                .map_err(|e| format!("写入历史数据库失败: {}", e))?;
        }
    } else if existing.len() > pinned_items.len() {
        tx.execute(delete_sql, params![pinned_items.len() as i64])
            .map_err(|e| format!("写入历史数据库失败: {}", e))?;
    }

    Ok(())
}

fn save_history_data_to_sqlite(data: &ClipboardHistoryData) -> Result<(), String> {
    let mut conn = open_history_db()?;
    let tx = conn
        .transaction()
        .map_err(|e| format!("创建历史数据库事务失败: {}", e))?;

    sync_history_items_table(&tx, &data.items)?;
    let item_id_by_content: HashMap<String, String> = data
        .items
        .iter()
        .map(|content| (content.clone(), stable_history_item_id(content)))
        .collect();
    sync_categories_table(&tx, &data.categories, &item_id_by_content)?;
    sync_indexed_values_table(&tx, "category_list", "category", &data.category_list)?;
    sync_pinned_items_table(&tx, &data.pinned_items, &item_id_by_content)?;

    tx.commit()
        .map_err(|e| format!("提交历史数据库事务失败: {}", e))
}

fn get_backup_file_path(path: &Path) -> PathBuf {
    let mut backup_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "data".to_string());
    backup_name.push_str(".bak");
    path.with_file_name(backup_name)
}

pub fn atomic_write_with_backup(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }

    let mut tmp_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "data".to_string());
    tmp_name.push_str(".tmp");
    let tmp_path = path.with_file_name(tmp_name);
    let backup_path = get_backup_file_path(path);

    fs::write(&tmp_path, bytes).map_err(|e| format!("写入临时文件失败: {}", e))?;

    if path.exists() {
        if backup_path.exists() {
            let _ = fs::remove_file(&backup_path);
        }
        fs::copy(path, &backup_path).map_err(|e| format!("创建备份文件失败: {}", e))?;
    }

    match fs::rename(&tmp_path, path) {
        Ok(_) => {
            let _ = fs::remove_file(&backup_path);
            Ok(())
        }
        Err(rename_error) => {
            let _ = fs::remove_file(&tmp_path);
            if backup_path.exists() {
                let _ = fs::copy(&backup_path, path);
            }
            Err(format!("替换目标文件失败: {}", rename_error))
        }
    }
}

pub fn read_text_with_backup(path: &Path) -> Result<String, String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(primary_error) => {
            let backup_path = get_backup_file_path(path);
            if !backup_path.exists() {
                return Err(format!("读取文件失败: {}", primary_error));
            }

            let backup_content = fs::read_to_string(&backup_path)
                .map_err(|e| format!("读取文件与备份均失败: 主文件错误: {}，备份错误: {}", primary_error, e))?;
            let _ = atomic_write_with_backup(path, backup_content.as_bytes());
            Ok(backup_content)
        }
    }
}

/// 保存设置到文件
pub fn save_settings(settings: &AppSettingsData) -> Result<(), String> {
    let settings_path = get_settings_file_path();
    let json =
        serde_json::to_string_pretty(settings).map_err(|e| format!("序列化设置失败: {}", e))?;
    atomic_write_with_backup(&settings_path, json.as_bytes())
        .map_err(|e| format!("写入设置文件失败: {}", e))?;
    Ok(())
}

/// 从文件加载设置
pub fn load_settings() -> Result<AppSettingsData, String> {
    let settings_path = get_settings_file_path();

    if !settings_path.exists() {
        log::info!("首次运行，创建默认设置文件");
        let mut default_settings = AppSettingsData::default();

        initialize_builtin_providers(&mut default_settings);

        let json = serde_json::to_string_pretty(&default_settings)
            .map_err(|e| format!("序列化默认设置失败: {}", e))?;
        atomic_write_with_backup(&settings_path, json.as_bytes())
            .map_err(|e| format!("创建设置文件失败: {}", e))?;
        return Ok(default_settings);
    }

    let contents = read_text_with_backup(&settings_path).map_err(|e| format!("读取设置文件失败: {}", e))?;

    let mut settings: AppSettingsData =
        serde_json::from_str(&contents).map_err(|e| format!("解析设置文件失败: {}", e))?;

    let keys_migrated = settings.migrate_legacy_api_keys();
    let old_version = settings.version.clone();
    settings.migrate_from_old();

    if old_version != settings.version || keys_migrated {
        log::info!("配置已更新，保存到文件");
        save_settings(&settings)?;
    }

    let _provider_key = settings.ai_provider.to_string();

    Ok(settings)
}

/// 保存剪切板历史记录到数据库
pub fn save_history(history: &[String]) -> Result<(), String> {
    let history_data = ClipboardHistoryData {
        items: history.to_vec(),
        categories: HashMap::new(),
        category_list: Vec::new(),
        pinned_items: Vec::new(),
    };
    save_history_data_with_retry(&history_data, 3)
}

/// 保存历史记录到数据库（带重试）
pub fn save_history_with_retry(history: &Vec<String>, max_retries: u32) -> Result<(), String> {
    save_history_data_with_retry(
        &ClipboardHistoryData {
            items: history.clone(),
            categories: HashMap::new(),
            category_list: Vec::new(),
            pinned_items: Vec::new(),
        },
        max_retries,
    )
}

/// 保存完整的历史数据（包含分类）到数据库（带重试）
pub fn save_history_data_with_retry(
    data: &ClipboardHistoryData,
    max_retries: u32,
) -> Result<(), String> {
    for i in 0..max_retries {
        match save_history_data_to_sqlite(data) {
            Ok(_) => return Ok(()),
            Err(e) => {
                if i == max_retries - 1 {
                    return Err(e);
                }
                log::warn!("写入历史数据库失败 (重试 {}/{}): {}", i + 1, max_retries, e);
                thread::sleep(Duration::from_millis(50));
            }
        }
    }
    Ok(())
}

/// 从数据库加载历史记录
pub fn load_history() -> Result<Vec<String>, String> {
    load_history_data().map(|data| data.items)
}

/// 从数据库加载完整的历史数据（包含分类）
pub fn load_history_data() -> Result<ClipboardHistoryData, String> {
    if let Some(sqlite_data) = load_history_data_from_sqlite()? {
        return Ok(sqlite_data);
    }
    Ok(ClipboardHistoryData::default())
}

pub fn load_history_page_data(
    offset: usize,
    limit: usize,
    category: Option<String>,
    pinned_only: bool,
    keyword: Option<String>,
    sort_by: Option<String>,
    sort_order: Option<String>,
) -> Result<ClipboardHistoryPageData, String> {
    let db_path = get_history_db_path();
    if !db_path.exists() {
        return Ok(ClipboardHistoryPageData {
            total: 0,
            offset,
            limit: limit.clamp(1, 200),
            items: Vec::new(),
        });
    }

    let conn = open_history_db()?;
    let effective_limit = limit.clamp(1, 200);
    let category_filter = category
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty() && v != "全部");
    let keyword_filter = keyword
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let fts_keyword = keyword_filter.as_ref().map(|v| build_fts_query(v));
    let pinned_flag = if pinned_only { 1 } else { 0 };
    let offset_i64 = offset as i64;
    let limit_i64 = effective_limit as i64;
    let fts_enabled = history_fts_enabled_conn(&conn)?;
    let order_clause = resolve_history_sort(sort_by, sort_order);

    let (total, items) = if fts_enabled {
        let total: i64 = conn
            .query_row(
                "
                SELECT COUNT(*)
                FROM history_items hi
                LEFT JOIN categories c ON (c.item_id = hi.item_id OR c.item = hi.content)
                LEFT JOIN pinned_items p ON (p.item_id = hi.item_id OR p.item = hi.content)
                WHERE
                  (?1 IS NULL OR (CASE WHEN c.category IS NULL OR c.category = '' THEN '未分类' ELSE c.category END) = ?1)
                  AND (?2 = 0 OR p.item IS NOT NULL)
                  AND (
                    ?3 IS NULL
                    OR EXISTS (
                        SELECT 1 FROM history_items_fts
                        WHERE history_items_fts.rowid = hi.position + 1
                          AND history_items_fts MATCH ?3
                    )
                  )
                ",
                params![category_filter.as_deref(), pinned_flag, fts_keyword.as_deref()],
                |row| row.get(0),
            )
            .map_err(|e| format!("读取历史数据库失败: {}", e))?;

        let query_sql = format!(
            "
                SELECT
                  hi.position,
                  COALESCE(hi.item_id, ''),
                  hi.content,
                  CASE WHEN c.category IS NULL OR c.category = '' THEN '未分类' ELSE c.category END,
                  CASE WHEN p.item IS NULL THEN 0 ELSE 1 END,
                  COALESCE(hi.updated_at, 0)
                FROM history_items hi
                LEFT JOIN categories c ON (c.item_id = hi.item_id OR c.item = hi.content)
                LEFT JOIN pinned_items p ON (p.item_id = hi.item_id OR p.item = hi.content)
                WHERE
                  (?1 IS NULL OR (CASE WHEN c.category IS NULL OR c.category = '' THEN '未分类' ELSE c.category END) = ?1)
                  AND (?2 = 0 OR p.item IS NOT NULL)
                  AND (
                    ?3 IS NULL
                    OR EXISTS (
                        SELECT 1 FROM history_items_fts
                        WHERE history_items_fts.rowid = hi.position + 1
                          AND history_items_fts MATCH ?3
                    )
                  )
                ORDER BY {}
                LIMIT ?4 OFFSET ?5
                ",
            order_clause
        );
        let mut stmt = conn
            .prepare(&query_sql)
            .map_err(|e| format!("读取历史数据库失败: {}", e))?;
        let rows = stmt
            .query_map(
                params![
                    category_filter.as_deref(),
                    pinned_flag,
                    fts_keyword.as_deref(),
                    limit_i64,
                    offset_i64
                ],
                |row| {
                    Ok(ClipboardHistoryPageItem {
                        position: row.get::<_, i64>(0)? as usize,
                        id: row.get::<_, String>(1)?,
                        content: row.get::<_, String>(2)?,
                        category: row.get::<_, String>(3)?,
                        pinned: row.get::<_, i64>(4)? == 1,
                        updated_at: row.get::<_, i64>(5)?,
                        snippet: None,
                    })
                },
            )
            .map_err(|e| format!("读取历史数据库失败: {}", e))?;
        let mut items = Vec::new();
        for row in rows {
            let mut item = row.map_err(|e| format!("读取历史数据库失败: {}", e))?;
            if item.id.is_empty() {
                item.id = stable_history_item_id(&item.content);
            }
            items.push(item);
        }
        (total, items)
    } else {
        let total: i64 = conn
            .query_row(
                "
                SELECT COUNT(*)
                FROM history_items hi
                LEFT JOIN categories c ON (c.item_id = hi.item_id OR c.item = hi.content)
                LEFT JOIN pinned_items p ON (p.item_id = hi.item_id OR p.item = hi.content)
                WHERE
                  (?1 IS NULL OR (CASE WHEN c.category IS NULL OR c.category = '' THEN '未分类' ELSE c.category END) = ?1)
                  AND (?2 = 0 OR p.item IS NOT NULL)
                  AND (?3 IS NULL OR hi.content LIKE '%' || ?3 || '%')
                ",
                params![category_filter.as_deref(), pinned_flag, keyword_filter.as_deref()],
                |row| row.get(0),
            )
            .map_err(|e| format!("读取历史数据库失败: {}", e))?;

        let query_sql = format!(
            "
                SELECT
                  hi.position,
                  COALESCE(hi.item_id, ''),
                  hi.content,
                  CASE WHEN c.category IS NULL OR c.category = '' THEN '未分类' ELSE c.category END,
                  CASE WHEN p.item IS NULL THEN 0 ELSE 1 END,
                  COALESCE(hi.updated_at, 0)
                FROM history_items hi
                LEFT JOIN categories c ON (c.item_id = hi.item_id OR c.item = hi.content)
                LEFT JOIN pinned_items p ON (p.item_id = hi.item_id OR p.item = hi.content)
                WHERE
                  (?1 IS NULL OR (CASE WHEN c.category IS NULL OR c.category = '' THEN '未分类' ELSE c.category END) = ?1)
                  AND (?2 = 0 OR p.item IS NOT NULL)
                  AND (?3 IS NULL OR hi.content LIKE '%' || ?3 || '%')
                ORDER BY {}
                LIMIT ?4 OFFSET ?5
                ",
            order_clause
        );
        let mut stmt = conn
            .prepare(&query_sql)
            .map_err(|e| format!("读取历史数据库失败: {}", e))?;
        let rows = stmt
            .query_map(
                params![
                    category_filter.as_deref(),
                    pinned_flag,
                    keyword_filter.as_deref(),
                    limit_i64,
                    offset_i64
                ],
                |row| {
                    Ok(ClipboardHistoryPageItem {
                        position: row.get::<_, i64>(0)? as usize,
                        id: row.get::<_, String>(1)?,
                        content: row.get::<_, String>(2)?,
                        category: row.get::<_, String>(3)?,
                        pinned: row.get::<_, i64>(4)? == 1,
                        updated_at: row.get::<_, i64>(5)?,
                        snippet: None,
                    })
                },
            )
            .map_err(|e| format!("读取历史数据库失败: {}", e))?;
        let mut items = Vec::new();
        for row in rows {
            let mut item = row.map_err(|e| format!("读取历史数据库失败: {}", e))?;
            if item.id.is_empty() {
                item.id = stable_history_item_id(&item.content);
            }
            items.push(item);
        }
        (total, items)
    };

    let items = if let Some(key) = keyword_filter.as_deref() {
        items
            .into_iter()
            .map(|mut item| {
                item.snippet = Some(build_keyword_snippet(&item.content, key));
                item
            })
            .collect()
    } else {
        items
    };

    Ok(ClipboardHistoryPageData {
        total: total as usize,
        offset,
        limit: effective_limit,
        items,
    })
}

/// 获取日志目录路径
pub fn get_logs_dir_path() -> PathBuf {
    let mut logs_dir = env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    logs_dir.pop();
    logs_dir.push("logs");
    logs_dir
}

/// 初始化内置提供商配置
fn initialize_builtin_providers(settings: &mut AppSettingsData) {
    use crate::core::config::{AIProvider, ProviderConfig};

    // 为每个内置提供商创建默认配置
    let builtin_providers = [
        AIProvider::DeepSeek,
        AIProvider::Qwen,
        AIProvider::XiaoMiMimo,
    ];

    for provider in builtin_providers {
        let provider_key = provider.to_string();
        let (default_url, default_model) = provider.get_default_config();

        let config = ProviderConfig {
            api_url: default_url,
            model_name: default_model,
            encrypted_api_key: String::new(),
        };

        settings.provider_configs.insert(provider_key, config);
    }

    log::info!("已初始化内置AI提供商配置");
}

/// 文本完整性检测结果
#[derive(Debug, Clone, PartialEq)]
pub enum TextCompleteness {
    /// 完整文本
    Complete,
    /// 缺失前段
    MissingPrefix,
    /// 缺失后段
    MissingSuffix,
    /// 缺失前后段
    MissingBoth,
    /// 无法确定
    Unknown,
}

/// 版本对比结果
#[derive(Debug, Clone)]
pub struct VersionComparison {
    /// 相似度分数 (0.0 - 1.0)
    pub similarity_score: f64,
    /// 新版本的完整性状态
    pub new_completeness: TextCompleteness,
    /// 是否应该替换旧版本
    pub should_replace: bool,
    /// 替换建议原因
    pub reason: String,
}

const LCS_MAX_CHARS_EACH: usize = 1400;
const LCS_MAX_PRODUCT: usize = 1_600_000;
const FIND_BEST_CANDIDATE_BUDGET_MS: u64 = 18;
const FIND_BEST_CANDIDATE_BUDGET_MIN_MS: u64 = 12;
const FIND_BEST_CANDIDATE_BUDGET_MAX_MS: u64 = 30;
const CANDIDATE_LEN_RATIO_MIN: f64 = 0.22;
const CANDIDATE_EDGE_MATCH_MIN: f64 = 0.06;
static FIND_BEST_CANDIDATE_DYNAMIC_BUDGET_MS: AtomicU64 =
    AtomicU64::new(FIND_BEST_CANDIDATE_BUDGET_MS);
static DEDUP_SCAN_TOTAL: AtomicU64 = AtomicU64::new(0);
static DEDUP_SCAN_TIMEOUTS: AtomicU64 = AtomicU64::new(0);
static DEDUP_SCAN_ELAPSED_TOTAL_MS: AtomicU64 = AtomicU64::new(0);
static DEDUP_SCAN_ITEMS_TOTAL: AtomicU64 = AtomicU64::new(0);
static DEDUP_SCAN_LAST_ELAPSED_MS: AtomicU64 = AtomicU64::new(0);
static DEDUP_SCAN_LAST_SCANNED_ITEMS: AtomicU64 = AtomicU64::new(0);
static DEDUP_SCAN_LAST_TIMEOUT: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize)]
pub struct DedupScanMetrics {
    pub budget_ms_current: u64,
    pub total_scans: u64,
    pub timeout_scans: u64,
    pub timeout_ratio: f64,
    pub avg_elapsed_ms: f64,
    pub avg_scanned_items: f64,
    pub last_elapsed_ms: u64,
    pub last_scanned_items: u64,
    pub last_timeout: bool,
}

pub fn get_dedup_scan_metrics() -> DedupScanMetrics {
    let total_scans = DEDUP_SCAN_TOTAL.load(Ordering::Relaxed);
    let timeout_scans = DEDUP_SCAN_TIMEOUTS.load(Ordering::Relaxed);
    let elapsed_total = DEDUP_SCAN_ELAPSED_TOTAL_MS.load(Ordering::Relaxed);
    let items_total = DEDUP_SCAN_ITEMS_TOTAL.load(Ordering::Relaxed);
    let timeout_ratio = if total_scans == 0 {
        0.0
    } else {
        timeout_scans as f64 / total_scans as f64
    };
    let avg_elapsed_ms = if total_scans == 0 {
        0.0
    } else {
        elapsed_total as f64 / total_scans as f64
    };
    let avg_scanned_items = if total_scans == 0 {
        0.0
    } else {
        items_total as f64 / total_scans as f64
    };
    DedupScanMetrics {
        budget_ms_current: FIND_BEST_CANDIDATE_DYNAMIC_BUDGET_MS.load(Ordering::Relaxed),
        total_scans,
        timeout_scans,
        timeout_ratio,
        avg_elapsed_ms,
        avg_scanned_items,
        last_elapsed_ms: DEDUP_SCAN_LAST_ELAPSED_MS.load(Ordering::Relaxed),
        last_scanned_items: DEDUP_SCAN_LAST_SCANNED_ITEMS.load(Ordering::Relaxed),
        last_timeout: DEDUP_SCAN_LAST_TIMEOUT.load(Ordering::Relaxed) == 1,
    }
}

fn prefix_match_ratio(text1: &str, text2: &str, sample_len: usize) -> f64 {
    let a: Vec<char> = text1.chars().take(sample_len).collect();
    let b: Vec<char> = text2.chars().take(sample_len).collect();
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }
    let mut same = 0usize;
    for i in 0..n {
        if a[i] == b[i] {
            same += 1;
        }
    }
    same as f64 / n as f64
}

fn suffix_match_ratio(text1: &str, text2: &str, sample_len: usize) -> f64 {
    let mut a: Vec<char> = text1.chars().rev().take(sample_len).collect();
    let mut b: Vec<char> = text2.chars().rev().take(sample_len).collect();
    a.reverse();
    b.reverse();
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }
    let mut same = 0usize;
    for i in 0..n {
        if a[i] == b[i] {
            same += 1;
        }
    }
    same as f64 / n as f64
}

fn calculate_text_similarity_fast(text1: &str, text2: &str, len1: usize, len2: usize) -> f64 {
    if text1 == text2 {
        return 1.0;
    }
    let max_len = len1.max(len2) as f64;
    let min_len = len1.min(len2) as f64;
    let length_ratio = if max_len == 0.0 { 0.0 } else { min_len / max_len };
    if text1.contains(text2) || text2.contains(text1) {
        return length_ratio.max(0.85);
    }
    let head = prefix_match_ratio(text1, text2, 256);
    let tail = suffix_match_ratio(text1, text2, 256);
    (head * 0.35 + tail * 0.35 + length_ratio * 0.30).min(1.0)
}

/// 计算两个文本的相似度
/// 使用最长公共子序列(LCS)算法计算相似度
pub fn calculate_text_similarity(text1: &str, text2: &str) -> f64 {
    if text1.is_empty() && text2.is_empty() {
        return 1.0;
    }

    if text1.is_empty() || text2.is_empty() {
        return 0.0;
    }

    let chars1: Vec<char> = text1.chars().collect();
    let chars2: Vec<char> = text2.chars().collect();
    let len1 = chars1.len();
    let len2 = chars2.len();

    log::debug!("计算相似度，长度: {} vs {}", len1, len2);

    if len1 > LCS_MAX_CHARS_EACH
        || len2 > LCS_MAX_CHARS_EACH
        || len1.saturating_mul(len2) > LCS_MAX_PRODUCT
    {
        return calculate_text_similarity_fast(text1, text2, len1, len2);
    }

    // 创建DP表
    let mut dp = vec![vec![0; len2 + 1]; len1 + 1];

    // 填充DP表
    for i in 1..=len1 {
        for j in 1..=len2 {
            if chars1[i - 1] == chars2[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }

    // 计算相似度
    let lcs_length = dp[len1][len2];
    let max_len = len1.max(len2);

    let similarity = if max_len == 0 {
        0.0
    } else {
        lcs_length as f64 / max_len as f64
    };

    log::debug!("LCS长度: {}, 最大长度: {}, 相似度: {:.4}", 
                lcs_length, max_len, similarity);

    similarity
}

fn candidate_prefilter(old_text: &str, new_text: &str) -> bool {
    if old_text.is_empty() || new_text.is_empty() {
        return true;
    }
    if old_text.contains(new_text) || new_text.contains(old_text) {
        return true;
    }
    let len_old = old_text.chars().count();
    let len_new = new_text.chars().count();
    let min_len = len_old.min(len_new) as f64;
    let max_len = len_old.max(len_new) as f64;
    if max_len > 0.0 && (min_len / max_len) < CANDIDATE_LEN_RATIO_MIN {
        return false;
    }
    let head = prefix_match_ratio(old_text, new_text, 32);
    let tail = suffix_match_ratio(old_text, new_text, 32);
    head >= CANDIDATE_EDGE_MATCH_MIN || tail >= CANDIDATE_EDGE_MATCH_MIN
}

/// 检测文本完整性
/// 分析文本是否可能是截断版本
pub fn detect_text_completeness(text: &str, reference_text: &str) -> TextCompleteness {
    let similarity = calculate_text_similarity(text, reference_text);
    detect_text_completeness_with_similarity(text, reference_text, similarity)
}

fn detect_text_completeness_with_similarity(
    text: &str,
    reference_text: &str,
    similarity: f64,
) -> TextCompleteness {
    if text.is_empty() || reference_text.is_empty() {
        return TextCompleteness::Unknown;
    }

    // 如果文本完全相同，认为是完整版本
    if text == reference_text {
        return TextCompleteness::Complete;
    }

    // 如果新文本比参考文本长，认为是完整版本
    if text.len() > reference_text.len() {
        return TextCompleteness::Complete;
    }

    // 检查是否是前缀
    if reference_text.starts_with(text) {
        return TextCompleteness::MissingSuffix;
    }

    // 检查是否是后缀
    if reference_text.ends_with(text) {
        return TextCompleteness::MissingPrefix;
    }

    // 检查是否包含在中间
    if reference_text.contains(text) && text.len() < reference_text.len() {
        return TextCompleteness::MissingBoth;
    }

    // 检查相似度，如果很高但不是上述情况，可能是部分内容缺失
    if similarity > 0.8 {
        // 通过字符位置分析判断缺失类型
        let text_chars: Vec<char> = text.chars().collect();
        let ref_chars: Vec<char> = reference_text.chars().collect();

        // 检查开头是否匹配
        let mut prefix_match = true;
        let min_len = text_chars.len().min(10); // 检查前10个字符
        for i in 0..min_len {
            if i >= ref_chars.len() || text_chars[i] != ref_chars[i] {
                prefix_match = false;
                break;
            }
        }

        // 检查结尾是否匹配
        let mut suffix_match = true;
        let min_len = text_chars.len().min(10); // 检查后10个字符
        for i in 0..min_len {
            let text_idx = text_chars.len() - 1 - i;
            let ref_idx = ref_chars.len() - 1 - i;
            if text_idx >= text_chars.len() || ref_idx >= ref_chars.len() ||
                text_chars[text_idx] != ref_chars[ref_idx] {
                suffix_match = false;
                break;
            }
        }

        match (prefix_match, suffix_match) {
            (true, false) => TextCompleteness::MissingSuffix,
            (false, true) => TextCompleteness::MissingPrefix,
            (false, false) => TextCompleteness::MissingBoth,
            (true, true) => TextCompleteness::Complete, // 可能是完全相同的短文本
        }
    } else {
        TextCompleteness::Unknown
    }
}

/// 统计文本中标点符号数量
fn count_punctuation(text: &str) -> usize {
    let punctuation_chars = ['。', '！', '？', '.', '!', '?', '；', ';', '，', ','];
    text.chars().filter(|&c| punctuation_chars.contains(&c)).count()
}

/// 判断文本是否具有更完整的句子结构
fn is_more_complete_sentence(new_text: &str, old_text: &str) -> bool {
    // 检查新文本是否有句子结束标志而旧文本没有
    let new_ends_with_period = has_sentence_endings(new_text);
    let old_ends_with_period = has_sentence_endings(old_text);

    new_ends_with_period && !old_ends_with_period
}

/// 判断文本是否以句子结束符结尾
fn has_sentence_endings(text: &str) -> bool {
    let ending_chars = ['。', '！', '？', '.', '!', '?'];
    text.trim_end().chars().last().map_or(false, |c| ending_chars.contains(&c))
}

/// 判断文本是否像是被截断的句子
fn is_truncated_sentence(text: &str) -> bool {
    let trimmed = text.trim_end();
    if trimmed.is_empty() {
        return false;
    }

    // 如果文本以某些字符结尾，可能是被截断的
    let last_char = trimmed.chars().last().unwrap();
    let truncation_indicators = ['，', ',', '、', '(', '[', '{', '"', '\''];

    truncation_indicators.contains(&last_char) ||
        // 或者以常见词汇结尾但没有句子结束符
        (!has_sentence_endings(trimmed) &&
            (trimmed.ends_with("但非") ||
                trimmed.ends_with("但是") ||
                trimmed.ends_with("而且") ||
                trimmed.ends_with("并且")))
}

/// 判断new_text是否是old_text的子集（前缀或后缀）
fn is_subset_of(new_text: &str, old_text: &str) -> bool {
    if new_text.is_empty() || old_text.is_empty() {
        return false;
    }

    // 检查是否是前缀
    if old_text.starts_with(new_text) {
        return true;
    }

    // 检查是否是后缀
    if old_text.ends_with(new_text) {
        return true;
    }

    // 检查是否包含在中间
    if old_text.contains(new_text) && new_text.len() < old_text.len() {
        return true;
    }

    false
}

/// 比较两个版本并决定是否应该替换
pub fn compare_versions(old_text: &str, new_text: &str, similarity_threshold: f64) -> VersionComparison {
    let similarity = calculate_text_similarity(old_text, new_text);
    let completeness = detect_text_completeness_with_similarity(new_text, old_text, similarity);

    log::debug!(
        "版本对比 - 长度 old={} new={}",
        old_text.chars().count(),
        new_text.chars().count()
    );
    log::debug!("相似度: {:.4}, 完整性: {:?}", similarity, completeness);

    let (should_replace, reason) = if similarity >= similarity_threshold {
        match completeness {
            TextCompleteness::Complete => {
                // 改进的完整版本判断逻辑
                if new_text.len() > old_text.len() {
                    (true, "新版本更完整，长度更长".to_string())
                } else if new_text.len() == old_text.len() {
                    // 即使长度相同，如果新版本包含更多标点符号或完整句子结构，也应该替换
                    let new_has_more_punctuation = count_punctuation(new_text) > count_punctuation(old_text);
                    let new_is_more_complete = is_more_complete_sentence(new_text, old_text);

                    if new_has_more_punctuation || new_is_more_complete {
                        (true, "新版本句子结构更完整".to_string())
                    } else {
                        (false, "版本相同，无需替换".to_string())
                    }
                } else {
                    // 新版本更短的情况 - 检查是否是已有完整版本的子集
                    if is_subset_of(new_text, old_text) {
                        (true, "新版本是已有完整版本的子集，移动完整版本到前面".to_string())
                    } else {
                        // 即使新版本稍短，但如果它更完整（如句子结束符），也可以考虑替换
                        let old_is_truncated = is_truncated_sentence(old_text);
                        let new_is_complete = has_sentence_endings(new_text);

                        if old_is_truncated && new_is_complete {
                            (true, "替换不完整的截断版本".to_string())
                        } else {
                            (false, "新版本较短，保持原版本".to_string())
                        }
                    }
                }
            },
            TextCompleteness::MissingPrefix | TextCompleteness::MissingSuffix | TextCompleteness::MissingBoth => {
                // 对于不完整版本，检查是否存在对应的完整版本
                if new_text.len() < old_text.len() && is_subset_of(new_text, old_text) {
                    // 新版本是旧版本的子集，说明是找回完整版本的情况
                    (true, "找回完整版本，将完整版本移动到前面".to_string())
                } else if new_text.len() > old_text.len() && has_sentence_endings(new_text) {
                    // 新版本更长且有句子结束符
                    (true, "新版本虽被标记为不完整但实际更完整".to_string())
                } else {
                    (false, "新版本内容不完整，保持原版本".to_string())
                }
            },
            TextCompleteness::Unknown => {
                // 当无法确定时，基于长度和句子完整性做保守判断
                if new_text.len() > old_text.len() && has_sentence_endings(new_text) && !has_sentence_endings(old_text) {
                    (true, "基于长度和句子完整性判断，新版本更完整".to_string())
                } else {
                    (false, "无法确定版本关系，保持原版本".to_string())
                }
            }
        }
    } else {
        (false, "文本相似度低于阈值，视为不同内容".to_string())
    };

    log::debug!("替换决策: {}, 原因: {}", should_replace, reason);

    VersionComparison {
        similarity_score: similarity,
        new_completeness: completeness,
        should_replace,
        reason,
    }
}

/// 在历史记录中查找相似条目并返回最佳替换候选
pub fn find_best_replacement_candidate(
    new_text: &str,
    history: &[String],
    similarity_threshold: f64,
) -> Option<(usize, VersionComparison)> {
    let mut best_candidate: Option<(usize, VersionComparison)> = None;
    let started = Instant::now();
    let budget_ms = FIND_BEST_CANDIDATE_DYNAMIC_BUDGET_MS
        .load(Ordering::Relaxed)
        .clamp(
            FIND_BEST_CANDIDATE_BUDGET_MIN_MS,
            FIND_BEST_CANDIDATE_BUDGET_MAX_MS,
        );
    let budget = Duration::from_millis(budget_ms);
    let mut scanned = 0usize;
    let mut timed_out = false;

    for (index, old_text) in history.iter().enumerate() {
        if started.elapsed() >= budget {
            timed_out = true;
            log::debug!(
                "候选扫描命中耗时预算{}ms，已扫描 {} 条，耗时 {:?}",
                budget_ms,
                scanned,
                started.elapsed()
            );
            break;
        }
        if !candidate_prefilter(old_text, new_text) {
            continue;
        }
        scanned += 1;
        let comparison = compare_versions(old_text, new_text, similarity_threshold);

        if comparison.should_replace {
            match &best_candidate {
                None => {
                    best_candidate = Some((index, comparison));
                },
                Some((_, existing_comparison)) => {
                    // 选择相似度更高或更完整的版本
                    if comparison.similarity_score > existing_comparison.similarity_score ||
                        (comparison.similarity_score == existing_comparison.similarity_score &&
                            (matches!(comparison.new_completeness, TextCompleteness::Complete) ||
                                comparison.reason.contains("更完整"))) {
                        best_candidate = Some((index, comparison));
                    }
                }
            }
        }
    }

    let elapsed_ms = started.elapsed().as_millis() as u64;
    DEDUP_SCAN_TOTAL.fetch_add(1, Ordering::Relaxed);
    DEDUP_SCAN_ELAPSED_TOTAL_MS.fetch_add(elapsed_ms, Ordering::Relaxed);
    DEDUP_SCAN_ITEMS_TOTAL.fetch_add(scanned as u64, Ordering::Relaxed);
    DEDUP_SCAN_LAST_ELAPSED_MS.store(elapsed_ms, Ordering::Relaxed);
    DEDUP_SCAN_LAST_SCANNED_ITEMS.store(scanned as u64, Ordering::Relaxed);
    DEDUP_SCAN_LAST_TIMEOUT.store(if timed_out { 1 } else { 0 }, Ordering::Relaxed);
    if timed_out {
        DEDUP_SCAN_TIMEOUTS.fetch_add(1, Ordering::Relaxed);
    }
    let next_budget_ms = if timed_out {
        (budget_ms + 2).min(FIND_BEST_CANDIDATE_BUDGET_MAX_MS)
    } else if elapsed_ms.saturating_mul(2) < budget_ms {
        budget_ms
            .saturating_sub(1)
            .max(FIND_BEST_CANDIDATE_BUDGET_MIN_MS)
    } else {
        budget_ms
    };
    if next_budget_ms != budget_ms {
        FIND_BEST_CANDIDATE_DYNAMIC_BUDGET_MS.store(next_budget_ms, Ordering::Relaxed);
    }

    best_candidate
}
