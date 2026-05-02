use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherCategory {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub app_ids: Vec<String>,
}

/// 自定义命令类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CustomCommandType {
    /// 打开窗口（设置、剪贴板等）
    OpenWindow { label: String },
    /// 执行操作（截图、录屏等）
    ExecuteAction { action: String },
    /// 复制文本到剪贴板
    CopyText { text: String },
    /// 运行外部程序
    RunProgram { path: String, args: Option<String> },
}

/// 自定义命令
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomCommand {
    pub id: String,
    pub prefix: String,          // 命令前缀，如 ":mycmd"
    pub title: String,           // 显示标题
    pub description: Option<String>, // 描述
    pub icon: String,            // 图标名称
    pub command_type: CustomCommandType, // 命令类型
    pub enabled: bool,           // 是否启用
    pub created_at: i64,         // 创建时间戳
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherConfig {
    pub view_mode: String,
    pub categories: Vec<LauncherCategory>,
    pub app_category_map: HashMap<String, String>,
    pub custom_commands: Vec<CustomCommand>,  // 自定义命令列表
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            view_mode: "list".to_string(),
            categories: Vec::new(),
            app_category_map: HashMap::new(),
            custom_commands: Vec::new(),
        }
    }
}

fn get_config_path() -> PathBuf {
    let mut path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    path.pop();
    path.push("launcher");
    std::fs::create_dir_all(&path).ok();
    path.push("config.json");
    path
}

pub fn load_launcher_config() -> LauncherConfig {
    let path = get_config_path();
    if path.exists() {
        match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => LauncherConfig::default(),
        }
    } else {
        LauncherConfig::default()
    }
}

pub fn save_launcher_config(config: &LauncherConfig) -> Result<(), String> {
    let path = get_config_path();
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

pub fn add_category(name: String, icon: String) -> Result<LauncherConfig, String> {
    let mut config = load_launcher_config();
    let id = format!("custom_{}", chrono::Local::now().timestamp_millis());
    config.categories.push(LauncherCategory {
        id,
        name,
        icon,
        app_ids: Vec::new(),
    });
    save_launcher_config(&config)?;
    Ok(config)
}

pub fn remove_category(category_id: String) -> Result<LauncherConfig, String> {
    let mut config = load_launcher_config();
    config.categories.retain(|c| c.id != category_id);
    config.app_category_map.retain(|_, v| v != &category_id);
    save_launcher_config(&config)?;
    Ok(config)
}

pub fn rename_category(category_id: String, new_name: String) -> Result<LauncherConfig, String> {
    let mut config = load_launcher_config();
    if let Some(category) = config.categories.iter_mut().find(|c| c.id == category_id) {
        category.name = new_name;
    }
    save_launcher_config(&config)?;
    Ok(config)
}

pub fn set_app_category(app_id: String, category_id: String) -> Result<LauncherConfig, String> {
    let mut config = load_launcher_config();
    if category_id.is_empty() {
        config.app_category_map.remove(&app_id);
    } else {
        config.app_category_map.insert(app_id, category_id);
    }
    save_launcher_config(&config)?;
    Ok(config)
}

pub fn set_view_mode(mode: String) -> Result<LauncherConfig, String> {
    let mut config = load_launcher_config();
    config.view_mode = mode;
    save_launcher_config(&config)?;
    Ok(config)
}

pub fn reorder_categories(category_ids: Vec<String>) -> Result<LauncherConfig, String> {
    let mut config = load_launcher_config();

    // 根据提供的ID顺序重新排列分类
    let mut new_categories = Vec::new();
    for id in &category_ids {
        if let Some(category) = config.categories.iter().find(|c| c.id == *id).cloned() {
            new_categories.push(category);
        }
    }

    // 添加未被重排序的分类（如果有的话）
    for category in &config.categories {
        if !new_categories.iter().any(|c| c.id == category.id) {
            new_categories.push(category.clone());
        }
    }

    config.categories = new_categories;
    save_launcher_config(&config)?;
    Ok(config)
}

pub fn update_category_icon(category_id: String, icon: String) -> Result<LauncherConfig, String> {
    let mut config = load_launcher_config();
    if let Some(category) = config.categories.iter_mut().find(|c| c.id == category_id) {
        category.icon = icon;
    }
    save_launcher_config(&config)?;
    Ok(config)
}

/// 添加自定义命令
pub fn add_custom_command(
    prefix: String,
    title: String,
    description: Option<String>,
    icon: String,
    command_type: CustomCommandType,
) -> Result<LauncherConfig, String> {
    let mut config = load_launcher_config();

    // 检查前缀是否已存在
    if config.custom_commands.iter().any(|c| c.prefix == prefix) {
        return Err(format!("命令前缀 '{}' 已存在", prefix));
    }

    let id = format!("cmd_{}", chrono::Local::now().timestamp_millis());
    let created_at = chrono::Local::now().timestamp();

    config.custom_commands.push(CustomCommand {
        id,
        prefix,
        title,
        description,
        icon,
        command_type,
        enabled: true,
        created_at,
    });

    save_launcher_config(&config)?;
    Ok(config)
}

/// 删除自定义命令
pub fn remove_custom_command(command_id: String) -> Result<LauncherConfig, String> {
    let mut config = load_launcher_config();
    config.custom_commands.retain(|c| c.id != command_id);
    save_launcher_config(&config)?;
    Ok(config)
}

/// 更新自定义命令
pub fn update_custom_command(
    command_id: String,
    prefix: Option<String>,
    title: Option<String>,
    description: Option<String>,
    icon: Option<String>,
    command_type: Option<CustomCommandType>,
    enabled: Option<bool>,
) -> Result<LauncherConfig, String> {
    let mut config = load_launcher_config();

    // 先检查前缀是否冲突
    if let Some(ref p) = prefix {
        if config.custom_commands.iter().any(|c| c.id != command_id && c.prefix == *p) {
            return Err(format!("命令前缀 '{}' 已存在", p));
        }
    }

    // 然后更新命令
    if let Some(command) = config.custom_commands.iter_mut().find(|c| c.id == command_id) {
        if let Some(p) = prefix {
            command.prefix = p;
        }
        if let Some(t) = title {
            command.title = t;
        }
        if let Some(d) = description {
            command.description = Some(d);
        }
        if let Some(i) = icon {
            command.icon = i;
        }
        if let Some(ct) = command_type {
            command.command_type = ct;
        }
        if let Some(e) = enabled {
            command.enabled = e;
        }
    } else {
        return Err("命令不存在".to_string());
    }

    save_launcher_config(&config)?;
    Ok(config)
}

/// 切换命令启用状态
pub fn toggle_custom_command(command_id: String) -> Result<LauncherConfig, String> {
    let mut config = load_launcher_config();

    if let Some(command) = config.custom_commands.iter_mut().find(|c| c.id == command_id) {
        command.enabled = !command.enabled;
    } else {
        return Err("命令不存在".to_string());
    }

    save_launcher_config(&config)?;
    Ok(config)
}
