use crate::core::app_state::{AppState, TrayMenuItems};
use crate::sync::{lock_arc_mutex, Mutex};
use crate::ui::window_manager::{cleanup_enigo_instance, show_standard_window_by_label};
#[cfg(debug_assertions)]
use crate::utils::utils_helpers::get_logs_dir_path;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{menu::CheckMenuItemBuilder, AppHandle};
use tauri_plugin_autostart::ManagerExt;
#[cfg(debug_assertions)]
use tauri_plugin_opener::OpenerExt;

/// 重建托盘菜单
pub fn rebuild_tray_menu(app_handle: &AppHandle, state: Arc<Mutex<AppState>>) {
    let mut state_guard = lock_arc_mutex(&state);
    let tray_menu_items = &mut state_guard.tray_menu_items;
    if let Some(ref mut items) = *tray_menu_items {
        match app_handle.autolaunch().is_enabled() {
            Ok(autostart_enabled) => {
                let _ = items.autostart_item.set_checked(autostart_enabled);
                log::info!("设置自启动状态: {}", autostart_enabled);
            }
            Err(e) => {
                log::error!("自启动功能可能不支持当前平台: {}", e);
            }
        }
    } else {
        let create_menu_item = |id: &str, label: &str| -> Option<MenuItem<tauri::Wry>> {
            match MenuItem::with_id(app_handle, id, label, true, None::<&str>) {
                Ok(item) => Some(item),
                Err(e) => {
                    log::error!("创建菜单项失败: id={}, label={}, error={}", id, label, e);
                    None
                }
            }
        };

        let quit_item = match create_menu_item("quit", "退出") {
            Some(item) => item,
            None => return,
        };
        let settings_item = match create_menu_item("settings", "设置") {
            Some(item) => item,
            None => return,
        };
        #[cfg(debug_assertions)]
        let clear_logs_item = match create_menu_item("clear_logs", "清除日志") {
            Some(item) => item,
            None => return,
        };
        #[cfg(debug_assertions)]
        let open_logs_item = match create_menu_item("open_logs", "打开日志目录") {
            Some(item) => item,
            None => return,
        };
        let autostart_enabled = app_handle.autolaunch().is_enabled().unwrap_or(false);
        let autostart_item = match CheckMenuItemBuilder::with_id("autostart", "开机自启")
            .checked(autostart_enabled)
            .build(app_handle)
        {
            Ok(item) => item,
            Err(e) => {
                log::error!("创建开机自启菜单项失败: {}", e);
                return;
            }
        };

        *tray_menu_items = Some(TrayMenuItems {
            autostart_item: autostart_item.clone(),
        });

        let mut menu_items: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = vec![&autostart_item];

        #[cfg(debug_assertions)]
        menu_items.push(&clear_logs_item);

        #[cfg(debug_assertions)]
        menu_items.push(&open_logs_item);

        menu_items.push(&settings_item);
        menu_items.push(&quit_item);

        let menu = match Menu::with_items(app_handle, &menu_items) {
            Ok(menu) => menu,
            Err(e) => {
                log::error!("创建主菜单失败: {}", e);
                return;
            }
        };

        if let Some(_old_tray) = app_handle.tray_by_id("main") {
            let _ = app_handle.remove_tray_by_id("main");
        }
        let version = app_handle.package_info().version.clone();
        let icon = match app_handle.default_window_icon() {
            Some(icon) => icon.clone(),
            None => {
                log::error!("创建托盘图标失败: 默认窗口图标不存在");
                return;
            }
        };
        let tray_builder = TrayIconBuilder::with_id("main")
            .icon(icon)
            .tooltip(format!("fy_tools v{}", version))
            .menu(&menu);

        tray_builder
            .on_menu_event({
                let state_for_events = state.clone();
                move |app, event| {
                    let event_id = event.id().as_ref();
                    match event_id {
                        "quit" => {
                            handle_quit_event(app);
                        }
                        "autostart" => {
                            handle_autostart_event(app, &state_for_events);
                        }
                        "settings" => {
                            open_settings(app);
                        }
                        #[cfg(debug_assertions)]
                        "open_logs" => {
                            if let Err(e) = open_log_directory(app) {
                                log::error!("打开日志目录失败: {}", e);
                            }
                        }
                        #[cfg(debug_assertions)]
                        "clear_logs" => {
                            if let Err(e) = clear_log_files() {
                                log::error!("清除日志文件失败: {}", e);
                            }
                        }
                        _ => {
                            log::info!("未知的菜单事件: {}", event_id);
                        }
                    }
                }
            })
            .build(app_handle)
            .map_err(|e| {
                log::error!("创建托盘图标失败: {}", e);
                e
            })
            .ok();
    }
}

/// 打开设置窗口
pub fn open_settings(app: &AppHandle) {
    let _ = show_standard_window_by_label(app, "settings");
}

/// 处理退出事件
pub fn handle_quit_event(app: &AppHandle) {
    log::info!("退出应用");
    cleanup_enigo_instance();
    app.exit(0);
}

/// 处理自启动设置事件
pub fn handle_autostart_event(app: &AppHandle, state: &Arc<Mutex<AppState>>) {
    log::info!("切换开机自启状态");

    let is_enabled = app.autolaunch().is_enabled().unwrap_or(false);

    let result = if is_enabled {
        match app.autolaunch().disable() {
            Ok(()) => {
                log::info!("已禁用开机自启");
                true
            }
            Err(e) => {
                log::error!("禁用开机自启失败: {}", e);
                eprintln!("禁用开机自启失败: {}", e);
                false
            }
        }
    } else {
        match app.autolaunch().enable() {
            Ok(()) => {
                log::info!("已启用开机自启");
                true
            }
            Err(e) => {
                log::error!("启用开机自启失败: {}", e);
                eprintln!("启用开机自启失败: {}", e);
                false
            }
        }
    };

    if result {
        let app_handle = app.clone();
        let state_clone = state.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            rebuild_tray_menu(&app_handle, state_clone);
        });
    }
}

/// 打开日志目录
#[cfg(debug_assertions)]
fn open_log_directory(app_handle: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let log_dir = get_logs_dir_path();
    if !log_dir.exists() {
        return Ok(());
    }
    let log_dir_string = log_dir.to_string_lossy().to_string();
    app_handle
        .opener()
        .open_path(log_dir_string, None::<&str>)?;
    Ok(())
}

/// 清除日志文件
#[cfg(debug_assertions)]
fn clear_log_files() -> Result<(), Box<dyn std::error::Error>> {
    let log_dir = get_logs_dir_path();

    if !log_dir.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(&log_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "log") {
            std::fs::remove_file(&path)?;
            log::info!("删除日志文件: {:?}", path);
        }
    }

    Ok(())
}
