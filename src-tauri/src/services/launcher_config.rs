use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::core::error_codes::AppErrorKind;
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

/// 首次启动时从旧 JSON 文件迁移数据到 SQLite
async fn try_migrate_old_data() {
    let Ok(is_empty) = launcher_db::is_db_empty().await else { return };
    if !is_empty { return }

    let exe_dir = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let launcher_dir = exe_dir.parent().unwrap_or(&exe_dir).join("launcher");

    let config_path = launcher_dir.join("config.json");
    if let Ok(content) = std::fs::read_to_string(&config_path) {
        if let Ok(config) = serde_json::from_str::<LauncherConfig>(&content) {
            let _ = launcher_db::set_config_value("view_mode", &config.view_mode).await;
            for (i, cat) in config.categories.iter().enumerate() {
                let _ = launcher_db::upsert_category(&cat.id, &cat.name, &cat.icon, i as i32).await;
                let _ = launcher_db::sync_category_apps(&cat.id, &cat.app_ids).await;
            }
            for (app_id, cat_id) in &config.app_category_map {
                let _ = launcher_db::set_app_category_map(app_id, cat_id).await;
            }
            for cmd in &config.custom_commands {
                let _ = launcher_db::insert_custom_command(&launcher_db::CustomCommandRow {
                    id: cmd.id.clone(),
                    prefix: cmd.prefix.clone(),
                    title: cmd.title.clone(),
                    description: cmd.description.clone(),
                    icon: cmd.icon.clone(),
                    command_type: CustomCommandType::to_json(&cmd.command_type),
                    enabled: cmd.enabled,
                    created_at: cmd.created_at,
                }).await;
            }
            // 迁移成功后删除旧文件
            let _ = std::fs::remove_file(&config_path);
        }
    }

    let apps_path = launcher_dir.join("apps.json");
    if let Ok(content) = std::fs::read_to_string(&apps_path) {
        #[derive(serde::Deserialize)]
        struct OldAppStore { apps: Vec<OldApp>, last_scan: i64 }
        #[derive(serde::Deserialize)]
        struct OldApp {
            id: String, title: String, path: String, category: String,
            app_type: String, icon_base64: Option<String>, action: String, sort_order: i32,
        }
        if let Ok(store) = serde_json::from_str::<OldAppStore>(&content) {
            let app_rows: Vec<launcher_db::AppRow> = store.apps.iter().map(|a| launcher_db::AppRow {
                id: a.id.clone(), title: a.title.clone(), path: a.path.clone(),
                category: a.category.clone(), app_type: a.app_type.clone(),
                icon_base64: a.icon_base64.clone(), action: a.action.clone(),
                sort_order: a.sort_order,
                source: "scan".to_string(),
            }).collect();
            let _ = launcher_db::replace_scan_apps(&app_rows).await;
            if store.last_scan > 0 {
                let _ = launcher_db::set_meta("last_scan", &store.last_scan.to_string()).await;
            }
            let _ = std::fs::remove_file(&apps_path);
        }
    }
}

async fn ensure_default_categories() {
    let Ok(cats) = launcher_db::load_categories().await else { return };
    if !cats.is_empty() { return }

    let defaults = [
        ("default_media", "影音娱乐", "VideoCamera"),
        ("default_office", "办公学习", "Reading"),
        ("default_chat", "聊天", "ChatDotSquare"),
        ("default_tools", "工具", "Tools"),
        ("default_other", "其他", "Folder"),
    ];

    for (i, (id, name, icon)) in defaults.iter().enumerate() {
        let _ = launcher_db::upsert_category(id, name, icon, i as i32).await;
    }
}

pub async fn load_launcher_config() -> LauncherConfig {
    try_migrate_old_data().await;
    ensure_default_categories().await;
    let view_mode = launcher_db::get_config_value("view_mode")
        .await
        .unwrap_or(None)
        .unwrap_or_else(|| "list".to_string());

    // 使用单次查询加载所有分类及其应用ID（避免N+1查询）
    let categories_with_apps = launcher_db::load_all_categories_with_app_ids()
        .await
        .unwrap_or_default();
    
    let mut categories = Vec::new();
    for (cr, app_ids) in categories_with_apps {
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
    let mut config = load_launcher_config().await;
    config.categories.push(LauncherCategory {
        id: id.clone(),
        name: name.clone(),
        icon: icon.clone(),
        app_ids: Vec::new(),
    });
    launcher_db::upsert_category(&id, &name, &icon, (config.categories.len() - 1) as i32).await?;
    Ok(config)
}

pub async fn remove_category(category_id: String) -> Result<LauncherConfig, String> {
    let mut config = load_launcher_config().await;
    config.categories.iter().find(|c| c.id == category_id).ok_or("分类不存在".to_string())?;
    config.categories.retain(|c| c.id != category_id);
    config.app_category_map.retain(|_, v| v != &category_id);
    launcher_db::delete_category(&category_id).await?;
    launcher_db::clear_category_app_map_by_category(&category_id).await?;
    Ok(config)
}

pub async fn rename_category(category_id: String, new_name: String) -> Result<LauncherConfig, String> {
    launcher_db::update_category_name(&category_id, &new_name).await?;
    let mut config = load_launcher_config().await;
    if let Some(cat) = config.categories.iter_mut().find(|c| c.id == category_id) {
        cat.name = new_name;
    }
    Ok(config)
}

pub async fn set_app_category(app_id: String, category_id: String) -> Result<LauncherConfig, String> {
    if category_id.is_empty() {
        launcher_db::remove_app_category_map(&app_id).await?;
    } else {
        launcher_db::set_app_category_map(&app_id, &category_id).await?;
    }
    let mut config = load_launcher_config().await;
    if category_id.is_empty() {
        config.app_category_map.remove(&app_id);
    } else {
        config.app_category_map.insert(app_id, category_id);
    }
    Ok(config)
}

pub async fn set_view_mode(mode: String) -> Result<LauncherConfig, String> {
    launcher_db::set_config_value("view_mode", &mode).await?;
    let mut config = load_launcher_config().await;
    config.view_mode = mode;
    Ok(config)
}

pub async fn reorder_categories(category_ids: Vec<String>) -> Result<LauncherConfig, String> {
    launcher_db::sync_category_positions(&category_ids).await?;
    let mut config = load_launcher_config().await;
    let mut ordered = Vec::new();
    for id in &category_ids {
        if let Some(pos) = config.categories.iter().position(|c| &c.id == id) {
            ordered.push(config.categories.remove(pos));
        }
    }
    config.categories = ordered;
    Ok(config)
}

pub async fn update_category_icon(category_id: String, icon: String) -> Result<LauncherConfig, String> {
    launcher_db::update_category_icon(&category_id, &icon).await?;
    let mut config = load_launcher_config().await;
    if let Some(cat) = config.categories.iter_mut().find(|c| c.id == category_id) {
        cat.icon = icon;
    }
    Ok(config)
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
        return Err(AppErrorKind::LauncherCommandPrefixExists.to_frontend_json_with_details(format!("{}", prefix)));
    }

    launcher_db::insert_custom_command(&launcher_db::CustomCommandRow {
        id: id.clone(),
        prefix: prefix.clone(),
        title: title.clone(),
        description: description.clone(),
        icon: icon.clone(),
        command_type: command_type.to_json(),
        enabled: true,
        created_at,
    }).await?;

    let mut config = load_launcher_config().await;
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
    Ok(config)
}

pub async fn remove_custom_command(command_id: String) -> Result<LauncherConfig, String> {
    launcher_db::delete_custom_command(&command_id).await?;
    let mut config = load_launcher_config().await;
    config.custom_commands.retain(|c| c.id != command_id);
    Ok(config)
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
    let mut config = load_launcher_config().await;
    if !config.custom_commands.iter().any(|c| c.id == command_id) {
        return Err(AppErrorKind::LauncherCommandNotFound.to_frontend_json());
    }

    if let Some(ref p) = prefix {
        if launcher_db::check_prefix_exists(p, Some(&command_id)).await? {
            return Err(AppErrorKind::LauncherCommandPrefixExists.to_frontend_json_with_details(format!("{}", p)));
        }
    }

    let ct_json = command_type.as_ref().map(|ct| ct.to_json());

    launcher_db::update_custom_command_fields(
        &command_id,
        prefix.as_deref(),
        title.as_deref(),
        description.as_deref(),
        icon.as_deref(),
        ct_json.as_deref(),
        enabled,
    ).await?;

    if let Some(cmd) = config.custom_commands.iter_mut().find(|c| c.id == command_id) {
        if let Some(ref p) = prefix { cmd.prefix = p.clone(); }
        if let Some(ref t) = title { cmd.title = t.clone(); }
        if let Some(ref d) = description { cmd.description = Some(d.clone()); }
        if let Some(ref i) = icon { cmd.icon = i.clone(); }
        if let Some(ct) = command_type { cmd.command_type = ct; }
        if let Some(e) = enabled { cmd.enabled = e; }
    }

    Ok(config)
}

pub async fn toggle_custom_command(command_id: String) -> Result<LauncherConfig, String> {
    let mut config = load_launcher_config().await;
    if !config.custom_commands.iter().any(|c| c.id == command_id) {
        return Err(AppErrorKind::LauncherCommandNotFound.to_frontend_json());
    }
    launcher_db::toggle_custom_command_enabled(&command_id).await?;
    if let Some(cmd) = config.custom_commands.iter_mut().find(|c| c.id == command_id) {
        cmd.enabled = !cmd.enabled;
    }
    Ok(config)
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
