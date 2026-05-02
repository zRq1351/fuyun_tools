use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::services::launcher_db;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherCategory {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub app_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CustomCommandType {
    OpenWindow { label: String },
    ExecuteAction { action: String },
    CopyText { text: String },
    RunProgram { path: String, args: Option<String> },
}

impl CustomCommandType {
    fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    fn from_json(s: &str) -> Option<Self> {
        serde_json::from_str(s).ok()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomCommand {
    pub id: String,
    pub prefix: String,
    pub title: String,
    pub description: Option<String>,
    pub icon: String,
    pub command_type: CustomCommandType,
    pub enabled: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherConfig {
    pub view_mode: String,
    pub categories: Vec<LauncherCategory>,
    pub app_category_map: HashMap<String, String>,
    pub custom_commands: Vec<CustomCommand>,
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

pub async fn load_launcher_config() -> LauncherConfig {
    let view_mode = launcher_db::get_config_value("view_mode")
        .await
        .unwrap_or(None)
        .unwrap_or_else(|| "list".to_string());

    let cat_rows = launcher_db::load_categories().await.unwrap_or_default();
    let mut categories = Vec::new();
    for cr in &cat_rows {
        let app_ids = launcher_db::load_category_app_ids(&cr.id).await.unwrap_or_default();
        categories.push(LauncherCategory {
            id: cr.id.clone(),
            name: cr.name.clone(),
            icon: cr.icon.clone(),
            app_ids,
        });
    }

    let map_entries = launcher_db::load_app_category_map().await.unwrap_or_default();
    let mut app_category_map = HashMap::new();
    for (app_id, cat_id) in map_entries {
        app_category_map.insert(app_id, cat_id);
    }

    let custom_commands = load_custom_commands_from_db().await;

    LauncherConfig {
        view_mode,
        categories,
        app_category_map,
        custom_commands,
    }
}
pub async fn add_category(name: String, icon: String) -> Result<LauncherConfig, String> {
    let id = format!("custom_{}", chrono::Local::now().timestamp_millis());
    let config = load_launcher_config().await;
    let mut new_categories = config.categories.clone();
    new_categories.push(LauncherCategory {
        id: id.clone(),
        name: name.clone(),
        icon: icon.clone(),
        app_ids: Vec::new(),
    });
    launcher_db::upsert_category(&id, &name, &icon, (new_categories.len() - 1) as i32).await?;
    Ok(load_launcher_config().await)
}

pub async fn remove_category(category_id: String) -> Result<LauncherConfig, String> {
    let config = load_launcher_config().await;
    config.categories.iter().find(|c| c.id == category_id).ok_or("分类不存在".to_string())?;
    launcher_db::delete_category(&category_id).await?;
    launcher_db::clear_category_app_map_by_category(&category_id).await?;
    Ok(load_launcher_config().await)
}

pub async fn rename_category(category_id: String, new_name: String) -> Result<LauncherConfig, String> {
    launcher_db::update_category_name(&category_id, &new_name).await?;
    Ok(load_launcher_config().await)
}

pub async fn set_app_category(app_id: String, category_id: String) -> Result<LauncherConfig, String> {
    if category_id.is_empty() {
        launcher_db::remove_app_category_map(&app_id).await?;
    } else {
        launcher_db::set_app_category_map(&app_id, &category_id).await?;
    }
    Ok(load_launcher_config().await)
}

pub async fn set_view_mode(mode: String) -> Result<LauncherConfig, String> {
    launcher_db::set_config_value("view_mode", &mode).await?;
    Ok(load_launcher_config().await)
}

pub async fn reorder_categories(category_ids: Vec<String>) -> Result<LauncherConfig, String> {
    launcher_db::sync_category_positions(&category_ids).await?;
    Ok(load_launcher_config().await)
}

pub async fn update_category_icon(category_id: String, icon: String) -> Result<LauncherConfig, String> {
    launcher_db::update_category_icon(&category_id, &icon).await?;
    Ok(load_launcher_config().await)
}

pub async fn add_custom_command(
    prefix: String,
    title: String,
    description: Option<String>,
    icon: String,
    command_type: CustomCommandType,
) -> Result<LauncherConfig, String> {
    let id = format!("cmd_{}", chrono::Local::now().timestamp_millis());
    let created_at = chrono::Local::now().timestamp();

    if launcher_db::check_prefix_exists(&prefix, None).await? {
        return Err(format!("命令前缀 '{}' 已存在", prefix));
    }

    launcher_db::insert_custom_command(&launcher_db::CustomCommandRow {
        id: id.clone(),
        prefix: prefix.clone(),
        title,
        description,
        icon,
        command_type: command_type.to_json(),
        enabled: true,
        created_at,
    }).await?;

    Ok(load_launcher_config().await)
}

pub async fn remove_custom_command(command_id: String) -> Result<LauncherConfig, String> {
    launcher_db::delete_custom_command(&command_id).await?;
    Ok(load_launcher_config().await)
}

pub async fn update_custom_command(
    command_id: String,
    prefix: Option<String>,
    title: Option<String>,
    description: Option<String>,
    icon: Option<String>,
    command_type: Option<CustomCommandType>,
    enabled: Option<bool>,
) -> Result<LauncherConfig, String> {
    let commands = load_custom_commands_from_db().await;
    if !commands.iter().any(|c| c.id == command_id) {
        return Err("命令不存在".to_string());
    }

    if let Some(ref p) = prefix {
        if launcher_db::check_prefix_exists(p, Some(&command_id)).await? {
            return Err(format!("命令前缀 '{}' 已存在", p));
        }
    }

    let ct_json = command_type.map(|ct| ct.to_json());

    launcher_db::update_custom_command_fields(
        &command_id,
        prefix.as_deref(),
        title.as_deref(),
        description.as_deref(),
        icon.as_deref(),
        ct_json.as_deref(),
        enabled,
    ).await?;

    Ok(load_launcher_config().await)
}

pub async fn toggle_custom_command(command_id: String) -> Result<LauncherConfig, String> {
    let commands = load_custom_commands_from_db().await;
    if !commands.iter().any(|c| c.id == command_id) {
        return Err("命令不存在".to_string());
    }
    launcher_db::toggle_custom_command_enabled(&command_id).await?;
    Ok(load_launcher_config().await)
}

async fn load_custom_commands_from_db() -> Vec<CustomCommand> {
    let rows = launcher_db::load_custom_commands().await.unwrap_or_default();
    rows.into_iter().map(|r| CustomCommand {
        id: r.id,
        prefix: r.prefix,
        title: r.title,
        description: r.description,
        icon: r.icon,
        command_type: CustomCommandType::from_json(&r.command_type)
            .unwrap_or(CustomCommandType::ExecuteAction { action: "".to_string() }),
        enabled: r.enabled,
        created_at: r.created_at,
    }).collect()
}
