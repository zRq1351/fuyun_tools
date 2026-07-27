use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::services::launcher_db;

#[cfg(target_os = "windows")]
use windows::{
    core::{Interface, PCWSTR},
    Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER, STGM},
    Win32::UI::Shell::{IShellLinkW, ShellLink},
};
#[cfg(target_os = "windows")]
use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};
#[cfg(target_os = "windows")]
use std::os::windows::ffi::OsStrExt;

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
    #[serde(default = "default_source")]
    pub source: String,
}

fn default_source() -> String {
    "scan".to_string()
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
                source: r.source,
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
            source: a.source.clone(),
        })
        .collect();

    launcher_db::replace_scan_apps(&app_rows).await?;
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
        "注册表编辑器",
        "设备管理器",
        "事件查看器",
        "磁盘管理",
        "dx",
        "directx",
        "windows 安全",
        "microsoft edge",
        "xbox",
        "cortana",
        "onedrive",
        "microsoft store",
        "windows terminal",
        "终端",
        "windows 备份",
        "windows 更新",
        "windows 迁移",
        "windows 工具",
        "凭据管理器",
        "防火墙",
        "服务",
        "组策略",
        "本地安全策略",
        "打印机",
        "默认程序",
        "语音识别",
        "放大镜",
        "讲述人",
        "屏幕键盘",
        "camera",
        "相机",
        "日历",
        "时钟",
        "天气",
        "地图",
        "照片",
        "视频编辑器",
        "录音机",
        "便签",
        "截图与草图",
        "获取帮助",
        "快速助手",
        "wordpad",
        "写字板",
        "snipping tool",
        "powershell ise",
        "subsystem",
        "恶意软件",
        "windows 管理",
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
        "\\start menu\\programs\\windows 系统",
        "\\start menu\\programs\\windows system",
        "\\start menu\\programs\\windows 附件",
        "\\start menu\\programs\\windows accessories",
        "\\start menu\\programs\\windows 辅助功能",
        "\\start menu\\programs\\windows accessibility",
        "\\start menu\\programs\\windows 轻松使用",
        "\\start menu\\programs\\windows ease of access",
        "\\start menu\\programs\\windows 管理工具",
        "\\start menu\\programs\\windows administrative tools",
        "\\start menu\\programs\\入门",
        "\\start menu\\programs\\维护",
        "\\start menu\\programs\\附件",
        "\\start menu\\programs\\管理工具",
        "\\start menu\\programs\\系统工具",
    ];

    if system_path_patterns.iter().any(|p| path_lower.contains(p)) {
        return true;
    }

    resolve_lnk_target(path).is_some_and(|target| is_microsoft_app(&target))
}

#[cfg(target_os = "windows")]
fn is_microsoft_app(target: &str) -> bool {
    if is_windows_system_path(target) {
        return true;
    }
    if is_microsoft_company_exe(target) {
        return true;
    }
    is_microsoft_related_target(target)
}

#[cfg(not(target_os = "windows"))]
fn is_microsoft_app(_target: &str) -> bool {
    false

}

#[cfg(target_os = "windows")]
pub fn resolve_lnk_target(lnk_path: &str) -> Option<String> {
    unsafe {
        let shell_link: IShellLinkW =
            CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let persist_file: windows::Win32::System::Com::IPersistFile = shell_link.cast().ok()?;

        let wide_path: Vec<u16> = std::ffi::OsStr::new(lnk_path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        persist_file
            .Load(PCWSTR(wide_path.as_ptr()), STGM(0))
            .ok()?;

        let mut buffer = vec![0u16; 260];
        shell_link.GetPath(&mut buffer, std::ptr::null_mut(), 0).ok()?;

        let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        if len == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buffer[..len]))
    }
}

#[cfg(not(target_os = "windows"))]
pub fn resolve_lnk_target(_lnk_path: &str) -> Option<String> {
    None
}

#[cfg(target_os = "windows")]
fn is_windows_system_path(target: &str) -> bool {
    let target_lower = target.to_lowercase();
    let windir = std::env::var("WINDIR")
        .unwrap_or_else(|_| "C:\\Windows".to_string())
        .to_lowercase();

    target_lower.starts_with(&windir)
}

#[cfg(target_os = "windows")]
fn is_microsoft_related_target(target: &str) -> bool {
    let target_lower = target.to_lowercase();
    let ms_indicators = [
        "microsoft.com",
        "visual studio",
        "\\microsoft ",
        "\\microsoft\\",
        "windows kits",
        "microsoft sdk",
        "microsoft visual",
        "microsoft.net",
        "msbuild",
        "msdn",
        "docs.microsoft",
        "learn.microsoft",
        "technet.microsoft",
    ];
    ms_indicators.iter().any(|k| target_lower.contains(k))
}

/// 通过 PE 文件版本信息中的 CompanyName 字段判断是否为微软产品。
/// 注意：这不是数字签名验证（非 WinVerifyTrust），仅用于启动器分类展示。
fn is_microsoft_company_exe(target: &str) -> bool {
    unsafe {
        let wide_target: Vec<u16> = std::ffi::OsStr::new(target)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let size = GetFileVersionInfoSizeW(PCWSTR(wide_target.as_ptr()), None);
        if size == 0 {
            return false;
        }

        let mut buffer: Vec<u8> = vec![0; size as usize];
        if GetFileVersionInfoW(
            PCWSTR(wide_target.as_ptr()),
            None,
            size,
            buffer.as_mut_ptr() as *mut _,
        )
        .is_err()
        {
            return false;
        }

        let translation_query: Vec<u16> = "\\VarFileInfo\\Translation"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut translation_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut translation_len: u32 = 0;

        let b = VerQueryValueW(
            buffer.as_ptr() as *const _,
            PCWSTR(translation_query.as_ptr()),
            &mut translation_ptr,
            &mut translation_len,
        );
        if !b.as_bool() {
            return false;
        }

        if translation_len < 4 || translation_ptr.is_null() {
            return false;
        }

        let lang_id = *(translation_ptr as *const u16);
        let codepage = *(translation_ptr as *const u16).add(1);

        let query = format!(
            "\\StringFileInfo\\{:04x}{:04x}\\CompanyName",
            lang_id, codepage
        );
        let wide_query: Vec<u16> = query.encode_utf16().chain(std::iter::once(0)).collect();

        let mut value_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut value_len: u32 = 0;

        let b = VerQueryValueW(
            buffer.as_ptr() as *const _,
            PCWSTR(wide_query.as_ptr()),
            &mut value_ptr,
            &mut value_len,
        );
        if !b.as_bool() {
            return false;
        }

        if value_ptr.is_null() || value_len == 0 {
            return false;
        }

        let company_name =
            String::from_utf16_lossy(std::slice::from_raw_parts(
                value_ptr as *const u16,
                value_len.saturating_sub(1) as usize,
            ));
        company_name.to_lowercase().contains("microsoft")
    }
}

#[cfg(not(target_os = "windows"))]
fn is_windows_system_path(_target: &str) -> bool {
    false
}

pub async fn scan_and_save_apps() -> Result<AppStore, String> {
    // Clear the in-memory cache before scanning
    crate::services::app_scanner::clear_app_cache();
    
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
                    source: "scan".to_string(),
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
    if let Err(e) = launcher_db::update_app_icon(app_id, icon_base64).await {
        log::warn!("更新应用图标失败 app_id={}: {}", app_id, e);
    }
}

pub async fn batch_update_icons(icons: &HashMap<String, String>) {
    for (path, icon) in icons {
        if let Err(e) = launcher_db::update_app_icon_by_path(path, icon).await {
            log::warn!("批量更新应用图标失败 path={}: {}", path, e);
        }
    }
}

pub async fn update_app_sort_orders(orders: Vec<(String, i32)>) -> Result<(), String> {
    launcher_db::batch_update_app_sort_orders(&orders).await
}

pub async fn add_manual_app(id: &str, title: &str, path: &str) -> Result<StoredApp, String> {
    launcher_db::insert_manual_app(id, title, path).await?;

    let icons = crate::services::app_scanner::batch_extract_icons(&[path.to_string()]);
    let icon_base64 = icons.get(path).cloned();

    if let Some(ref icon) = icon_base64 {
        if let Err(e) = launcher_db::update_app_icon(id, icon).await {
            log::warn!("添加手动应用后更新图标失败 id={}: {}", id, e);
        }
    }

    Ok(StoredApp {
        id: id.to_string(),
        title: title.to_string(),
        path: path.to_string(),
        category: String::new(),
        app_type: "third_party".to_string(),
        icon_base64,
        action: "launch_app".to_string(),
        sort_order: 0,
        source: "manual".to_string(),
    })
}
