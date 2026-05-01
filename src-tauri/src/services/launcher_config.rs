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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherConfig {
    pub view_mode: String,
    pub categories: Vec<LauncherCategory>,
    pub app_category_map: HashMap<String, String>,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            view_mode: "list".to_string(),
            categories: Vec::new(),
            app_category_map: HashMap::new(),
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
        if let Some(pos) = config.categories.iter().position(|c| c.id == *id) {
            new_categories.push(config.categories.remove(pos));
        }
    }

    // 添加未被重排序的分类（如果有的话）
    new_categories.append(&mut config.categories);

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
