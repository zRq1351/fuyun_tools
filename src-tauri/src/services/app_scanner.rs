use crate::utils::icon_extractor;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherItem {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub icon: String,
    pub icon_base64: Option<String>,
    pub item_type: String,
    pub action: String,
    pub path: Option<String>,
    pub shortcut: Option<String>,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppCategory {
    pub name: String,
    pub apps: Vec<LauncherItem>,
}

pub fn scan_apps_by_category() -> Vec<AppCategory> {
    let mut category_map: std::collections::HashMap<String, Vec<LauncherItem>> =
        std::collections::HashMap::new();

    let start_menu = PathBuf::from(r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs");
    if start_menu.exists() {
        scan_dir_by_category(&start_menu, "其他", &mut category_map);
    }

    if let Some(app_data) = dirs::data_dir() {
        let user_menu = app_data.join("Microsoft").join("Windows").join("Start Menu").join("Programs");
        if user_menu.exists() {
            scan_dir_by_category(&user_menu, "其他", &mut category_map);
        }
    }

    let mut categories: Vec<AppCategory> = category_map
        .into_iter()
        .filter(|(_, apps)| !apps.is_empty())
        .map(|(name, mut apps)| {
            apps.sort_by(|a, b| a.title.cmp(&b.title));
            apps.dedup_by(|a, b| a.title == b.title);
            AppCategory { name, apps }
        })
        .collect();

    categories.sort_by(|a, b| a.name.cmp(&b.name));

    let others = categories.iter().position(|c| c.name == "其他");
    if let Some(idx) = others {
        let other = categories.remove(idx);
        categories.push(other);
    }

    categories
}

fn scan_dir_by_category(
    dir: &PathBuf,
    default_category: &str,
    category_map: &mut std::collections::HashMap<String, Vec<LauncherItem>>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            let folder_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(default_category)
                .to_string();

            let skip_folders = ["启动", "Startup", "Maintenance", "Windows 系统", "Windows System"];
            if skip_folders.iter().any(|s| folder_name.contains(s)) {
                continue;
            }

            scan_dir_flat(&path, &folder_name, category_map);
        } else if path.extension().map_or(false, |e| e == "lnk") {
            if let Some(item) = parse_shortcut(&path, default_category) {
                category_map.entry(default_category.to_string()).or_default().push(item);
            }
        }
    }
}

fn scan_dir_flat(
    dir: &PathBuf,
    category: &str,
    category_map: &mut std::collections::HashMap<String, Vec<LauncherItem>>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            scan_dir_flat(&path, category, category_map);
        } else if path.extension().map_or(false, |e| e == "lnk") {
            if let Some(item) = parse_shortcut(&path, category) {
                category_map.entry(category.to_string()).or_default().push(item);
            }
        }
    }
}

fn parse_shortcut(path: &PathBuf, category: &str) -> Option<LauncherItem> {
    let file_name = path.file_stem()?.to_str()?;
    let path_str = path.to_str()?;

    let parent_dir = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase()
        .replace(' ', "_");

    let hidden = ["卸载", "Uninstall", "帮助", "Help", "readme", "说明"];
    if hidden.iter().any(|p| file_name.to_lowercase().contains(&p.to_lowercase())) {
        return None;
    }

    let stem = file_name.to_lowercase().replace(' ', "_");
    let id = if parent_dir.is_empty() {
        format!("app_{}", stem)
    } else {
        format!("app_{}_{}", parent_dir, stem)
    };

    Some(LauncherItem {
        id,
        title: file_name.to_string(),
        description: Some(path_str.to_string()),
        icon: "app".to_string(),
        icon_base64: None,
        item_type: "应用".to_string(),
        action: "launch_app".to_string(),
        path: Some(path_str.to_string()),
        shortcut: None,
        category: category.to_string(),
    })
}

pub fn search_apps(query: &str, limit: usize) -> Vec<LauncherItem> {
    let categories = scan_apps_by_category();
    let all_apps: Vec<LauncherItem> = categories.into_iter().flat_map(|c| c.apps).collect();
    let query_lower = query.to_lowercase();

    let mut results: Vec<LauncherItem> = all_apps
        .into_iter()
        .filter(|item| item.title.to_lowercase().contains(&query_lower))
        .take(limit)
        .collect();

    results.sort_by(|a, b| {
        let a_lower = a.title.to_lowercase();
        let b_lower = b.title.to_lowercase();
        let a_starts = a_lower.starts_with(&query_lower) as i32;
        let b_starts = b_lower.starts_with(&query_lower) as i32;
        b_starts.cmp(&a_starts)
    });

    results
}

/// 使用 ShellExecuteExW 打开文件/快捷方式（不显示 Windows 错误弹窗）
#[cfg(target_os = "windows")]
fn shell_execute_open(path: &str, args: Option<&str>) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::UI::Shell::{ShellExecuteExW, SHELLEXECUTEINFOW, SEE_MASK_FLAG_NO_UI};
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOW;
    use windows::Win32::Foundation::{HWND, HINSTANCE};
    use windows::core::PCWSTR;

    let wide_path: Vec<u16> = std::ffi::OsStr::new(path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let wide_verb: Vec<u16> = "open\0".encode_utf16().collect();

    let args_vec: Option<Vec<u16>> = args.map(|a| {
        std::ffi::OsStr::new(a)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    });

    let args_ptr = args_vec.as_ref()
        .map_or(PCWSTR::null(), |v| PCWSTR(v.as_ptr()));

    let mut sei = SHELLEXECUTEINFOW::default();
    sei.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    sei.fMask = SEE_MASK_FLAG_NO_UI;
    sei.hwnd = HWND::default();
    sei.lpVerb = PCWSTR(wide_verb.as_ptr());
    sei.lpFile = PCWSTR(wide_path.as_ptr());
    sei.lpParameters = args_ptr;
    sei.lpDirectory = PCWSTR::null();
    sei.nShow = SW_SHOW.0;
    sei.hInstApp = HINSTANCE::default();

    let result = unsafe { ShellExecuteExW(&mut sei) };
    if result.is_ok() {
        Ok(())
    } else {
        let err = std::io::Error::last_os_error();
        Err(format!("启动失败: {}", err))
    }
}

#[cfg(not(target_os = "windows"))]
fn shell_execute_open(_path: &str, _args: Option<&str>) -> Result<(), String> {
    Err("非 Windows 平台不支持 ShellExecute".to_string())
}

pub fn launch_app(path: &str) -> Result<(), String> {
    log::info!("[launch_app] 尝试启动程序: {}", path);
    let path_buf = PathBuf::from(path);
    if !path_buf.exists() {
        log::error!("[launch_app] 文件不存在: {}", path);
        return Err(format!("文件不存在: {}", path));
    }

    // 检查是否是快捷方式 (.lnk)
    let is_shortcut = path.to_lowercase().ends_with(".lnk");

    if is_shortcut {
        log::info!("[launch_app] 检测到快捷方式，使用 ShellExecute 启动");
        match shell_execute_open(path, None) {
            Ok(()) => {
                log::info!("[launch_app] 快捷方式启动成功");
                Ok(())
            }
            Err(e) => {
                log::error!("[launch_app] 快捷方式启动失败: {}", e);
                Err(e)
            }
        }
    } else {
        log::info!("[launch_app] 文件存在，开始启动...");
        // 直接使用 std::process::Command 启动程序，模拟双击行为
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            match std::process::Command::new(path)
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
            {
                Ok(child) => {
                    log::info!("[launch_app] 启动成功, PID: {:?}", child.id());
                    Ok(())
                }
                Err(e) => {
                    log::error!("[launch_app] 启动失败: {}", e);
                    Err(format!("启动失败: {}", e))
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            match std::process::Command::new(path).spawn() {
                Ok(child) => {
                    log::info!("[launch_app] 启动成功, PID: {:?}", child.id());
                    Ok(())
                }
                Err(e) => {
                    log::error!("[launch_app] 启动失败: {}", e);
                    Err(format!("启动失败: {}", e))
                }
            }
        }
    }
}

pub fn launch_app_with_args(path: &str, args: Option<&str>) -> Result<(), String> {
    log::info!("[launch_app_with_args] 尝试启动程序: {}, args: {:?}", path, args);
    let path_buf = PathBuf::from(path);
    if !path_buf.exists() {
        log::error!("[launch_app_with_args] 文件不存在: {}", path);
        return Err(format!("文件不存在: {}", path));
    }

    // 检查是否是快捷方式 (.lnk)
    let is_shortcut = path.to_lowercase().ends_with(".lnk");

    if is_shortcut {
        log::info!("[launch_app_with_args] 检测到快捷方式，使用 ShellExecute 启动");
        match shell_execute_open(path, args) {
            Ok(()) => {
                log::info!("[launch_app_with_args] 快捷方式启动成功");
                Ok(())
            }
            Err(e) => {
                log::error!("[launch_app_with_args] 快捷方式启动失败: {}", e);
                Err(e)
            }
        }
    } else {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            let mut command = std::process::Command::new(path);
            command.creation_flags(CREATE_NO_WINDOW);

            if let Some(arguments) = args {
                // 解析参数并添加
                for arg in arguments.split_whitespace() {
                    command.arg(arg);
                }
            }

            match command.spawn() {
                Ok(child) => {
                    log::info!("[launch_app_with_args] 启动成功, PID: {:?}", child.id());
                    Ok(())
                }
                Err(e) => {
                    log::error!("[launch_app_with_args] 启动失败: {}", e);
                    Err(format!("启动失败: {}", e))
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let mut command = std::process::Command::new(path);

            if let Some(arguments) = args {
                // 解析参数并添加
                for arg in arguments.split_whitespace() {
                    command.arg(arg);
                }
            }

            match command.spawn() {
                Ok(child) => {
                    log::info!("[launch_app_with_args] 启动成功, PID: {:?}", child.id());
                    Ok(())
                }
                Err(e) => {
                    log::error!("[launch_app_with_args] 启动失败: {}", e);
                    Err(format!("启动失败: {}", e))
                }
            }
        }
    }
}

pub fn open_file(path: &str) -> Result<(), String> {
    if !PathBuf::from(path).exists() {
        return Err(format!("文件不存在: {}", path));
    }
    shell_execute_open(path, None)
}

pub fn batch_extract_icons(paths: &[String]) -> std::collections::HashMap<String, String> {
    icon_extractor::batch_extract_icons(paths)
}
