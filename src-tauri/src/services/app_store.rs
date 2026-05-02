use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::services::launcher_db;

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

pub async fn load_app_store() -> AppStore {
    let apps = launcher_db::load_all_apps().await.unwrap_or_default();
    let last_scan = launcher_db::get_meta("last_scan")
        .await
        .unwrap_or(None)
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);

    AppStore {
        apps: apps
            .into_iter()
            .map(|r| StoredApp {
                id: r.id,
                title: r.title,
                path: r.path,
                category: r.category,
                app_type: r.app_type,
                icon_base64: r.icon_base64,
                action: r.action,
                sort_order: r.sort_order,
            })
            .collect(),
        last_scan,
    }
}

pub async fn save_app_store(store: &AppStore) -> Result<(), String> {
    let app_rows: Vec<launcher_db::AppRow> = store
        .apps
        .iter()
        .map(|a| launcher_db::AppRow {
            id: a.id.clone(),
            title: a.title.clone(),
            path: a.path.clone(),
            category: a.category.clone(),
            app_type: a.app_type.clone(),
            icon_base64: a.icon_base64.clone(),
            action: a.action.clone(),
            sort_order: a.sort_order,
        })
        .collect();

    launcher_db::upsert_apps(&app_rows).await?;
    launcher_db::set_meta("last_scan", &store.last_scan.to_string()).await?;
    Ok(())
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

pub async fn scan_and_save_apps() -> Result<AppStore, String> {
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

    save_app_store(&store).await?;
    Ok(store)
}

pub async fn remove_app_from_store(app_id: &str) -> Result<(), String> {
    launcher_db::delete_app(app_id).await
}

pub async fn update_app_icon(app_id: &str, icon_base64: &str) {
    let _ = launcher_db::update_app_icon(app_id, icon_base64).await;
}

pub async fn batch_update_icons(icons: &HashMap<String, String>) {
    for (path, icon) in icons {
        let _ = launcher_db::update_app_icon_by_path(path, icon).await;
    }
}

pub async fn update_app_sort_orders(orders: Vec<(String, i32)>) -> Result<(), String> {
    for (app_id, sort_order) in orders {
        launcher_db::update_app_sort_order(&app_id, sort_order).await?;
    }
    Ok(())
}
