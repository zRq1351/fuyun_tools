use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredApp {
    pub id: String,
    pub title: String,
    pub path: String,
    pub category: String,
    pub app_type: String,
    pub icon_base64: Option<String>,
    #[serde(default = "default_action")]
    pub action: String,
    #[serde(default)]
    pub sort_order: i32,
}

fn default_action() -> String {
    "launch_app".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppStore {
    pub apps: Vec<StoredApp>,
    pub last_scan: i64,
}

fn get_store_path() -> PathBuf {
    let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("fuyun_tools");
    std::fs::create_dir_all(&path).ok();
    path.push("launcher_apps.json");
    path
}

pub fn load_app_store() -> AppStore {
    let path = get_store_path();
    if path.exists() {
        match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or(AppStore { apps: Vec::new(), last_scan: 0 }),
            Err(_) => AppStore { apps: Vec::new(), last_scan: 0 },
        }
    } else {
        AppStore { apps: Vec::new(), last_scan: 0 }
    }
}

pub fn save_app_store(store: &AppStore) -> Result<(), String> {
    let path = get_store_path();
    let json = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

fn is_system_app(title: &str, path: &str) -> bool {
    let title_lower = title.to_lowercase();
    let path_lower = path.to_lowercase();

    let system_titles = [
        "控制面板",
        "命令提示符",
        "windows powershell",
        "记事本",
        "画图",
        "计算器",
        "截图工具",
        "远程桌面",
        "任务管理器",
        "磁盘清理",
        "碎片整理",
        "资源监视器",
        "系统信息",
        "字符映射表",
        "步骤记录器",
        "windows defender",
        "windows 传真",
        "internet explorer",
        "windows media player",
        "windows 辅助功能",
        "windows 附件",
        "系统工具",
        "辅助功能",
        "维护",
    ];

    if system_titles.iter().any(|k| title_lower.contains(k)) {
        return true;
    }

    let system_path_patterns = [
        "\\start menu\\programs\\accessories",
        "\\start menu\\programs\\administrative tools",
        "\\start menu\\programs\\maintenance",
        "\\start menu\\programs\\system tools",
        "\\start menu\\programs\\windows powershell",
        "\\start menu\\programs\\启动",
    ];

    system_path_patterns.iter().any(|p| path_lower.contains(p))
}

pub fn scan_and_save_apps() -> Result<AppStore, String> {
    let categories = crate::services::app_scanner::scan_apps_by_category();
    let mut apps = Vec::new();

    for category in categories {
        for app in category.apps {
            if let Some(path) = &app.path {
                let app_type = if is_system_app(&app.title, path) {
                    "system".to_string()
                } else {
                    "third_party".to_string()
                };
                apps.push(StoredApp {
                    id: app.id,
                    title: app.title,
                    path: path.clone(),
                    category: app.category,
                    app_type,
                    icon_base64: None,
                    action: "launch_app".to_string(),
                    sort_order: 0,
                });
            }
        }
    }

    apps.sort_by(|a, b| {
        if a.app_type == b.app_type {
            a.title.cmp(&b.title)
        } else {
            a.app_type.cmp(&b.app_type)
        }
    });

    let store = AppStore {
        apps,
        last_scan: chrono::Utc::now().timestamp(),
    };

    save_app_store(&store)?;
    Ok(store)
}

pub fn remove_app_from_store(app_id: &str) -> Result<(), String> {
    let mut store = load_app_store();
    store.apps.retain(|a| a.id != app_id);
    save_app_store(&store)
}

pub fn update_app_icon(app_id: &str, icon_base64: &str) {
    let mut store = load_app_store();
    if let Some(app) = store.apps.iter_mut().find(|a| a.id == app_id) {
        app.icon_base64 = Some(icon_base64.to_string());
        let _ = save_app_store(&store);
    }
}

pub fn batch_update_icons(icons: &HashMap<String, String>) {
    let mut store = load_app_store();
    let mut changed = false;
    for (path, icon) in icons {
        if let Some(app) = store.apps.iter_mut().find(|a| &a.path == path) {
            app.icon_base64 = Some(icon.clone());
            changed = true;
        }
    }
    if changed {
        let _ = save_app_store(&store);
    }
}

pub fn update_app_sort_orders(orders: Vec<(String, i32)>) -> Result<(), String> {
    let mut store = load_app_store();
    for (app_id, sort_order) in orders {
        if let Some(app) = store.apps.iter_mut().find(|a| a.id == app_id) {
            app.sort_order = sort_order;
        }
    }
    save_app_store(&store)
}
