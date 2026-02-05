use crate::core::app_state::{AppState, TrayMenuItems};
use crate::ui::window_manager::cleanup_enigo_instance;
use crate::utils::utils_helpers::get_logs_dir_path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::menu::{Menu, MenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{menu::CheckMenuItemBuilder, AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_opener::OpenerExt;

/// 重建托盘菜单
pub fn rebuild_tray_menu(app_handle: &AppHandle, state: Arc<Mutex<AppState>>) {
    let mut state_guard = state.lock().unwrap();
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
        let create_menu_item = |id: &str, label: &str| -> MenuItem<tauri::Wry> {
            MenuItem::with_id(app_handle, id, label, true, None::<&str>)
                .unwrap_or_else(|_| panic!("创建菜单项 '{}' 失败", label))
        };

        let quit_item = create_menu_item("quit", "退出");
        let clear_history_item = create_menu_item("clear_history", "清除记录");
        let clear_logs_item = create_menu_item("clear_logs", "清除日志");
        let open_logs_item = create_menu_item("open_logs", "打开日志目录");
        let settings_item = create_menu_item("settings", "设置");
        let autostart_enabled = app_handle.autolaunch().is_enabled().unwrap_or(false);
        let autostart_item = CheckMenuItemBuilder::with_id("autostart", "开机自启")
            .checked(autostart_enabled)
            .build(app_handle)
            .expect("创建开机自启菜单项失败");

        *tray_menu_items = Some(TrayMenuItems {
            autostart_item: autostart_item.clone(),
        });

        let clear_submenu_items: [&dyn tauri::menu::IsMenuItem<tauri::Wry>; 2] =
            [&clear_history_item, &clear_logs_item];

        let clear_submenu =
            Submenu::with_items(app_handle, "清除", true, &clear_submenu_items)
                .expect("未能创建清除子菜单");

        let menu_items: [&dyn tauri::menu::IsMenuItem<tauri::Wry>; 5] = [
            &autostart_item,
            &clear_submenu,
            &open_logs_item,
            &settings_item,
            &quit_item,
        ];

        let menu = Menu::with_items(app_handle, &menu_items).expect("创建主菜单失败");

        if let Some(_old_tray) = app_handle.tray_by_id("main") {
            let _ = app_handle.remove_tray_by_id("main");
        }
        let version = app_handle.package_info().version.clone();
        let tray_builder = TrayIconBuilder::with_id("main")
            .icon(app_handle.default_window_icon().unwrap().clone())
            .tooltip(&format!("fy_tools v{}", version))
            .menu(&menu);

        tray_builder
            .on_menu_event({
                let state_for_events = state.clone();
                move |app, event| {
                    let event_id = event.id().as_ref();
                    match event_id {
                        "quit" => {
                            handle_quit_event(&app);
                        }
                        "autostart" => {
                            handle_autostart_event(&app, &state_for_events);
                        }
                        "open_logs" => {
                            if let Err(e) = open_log_directory(&app) {
                                log::error!("打开日志目录失败: {}", e);
                            }
                        }
                        "clear_history" => {
                            handle_clear_history_event(&state_for_events);
                        }
                        "clear_logs" => {
                            if let Err(e) = clear_log_files() {
                                log::error!("清除日志文件失败: {}", e);
                            }
                        }
                        "settings" => {
                            open_settings(app);
                        }
                        _ => {
                            log::info!("未知的菜单事件: {}", event_id);
                        }
                    }
                }
            })
            .build(app_handle)
            .expect("创建托盘图标失败");
    }
}

/// 打开设置窗口
pub fn open_settings(app: &AppHandle) {
    if let Some(settings_window) = app.get_webview_window("settings") {
        let _ = settings_window.show();
    }
}

/// 处理退出事件
pub fn handle_quit_event(app: &AppHandle) {
    log::info!("退出应用");
    // 清理资源
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

/// 处理清除历史记录事件
pub fn handle_clear_history_event(state: &Arc<Mutex<AppState>>) {
    let state_guard = state.lock().unwrap();
    let manager = state_guard.clipboard_manager.lock().unwrap();
    manager.clear_history();
}

/// 打开日志目录
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
fn clear_log_files() -> Result<(), Box<dyn std::error::Error>> {
    let log_dir = get_logs_dir_path();

    if !log_dir.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(&log_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().map_or(false, |ext| ext == "log") {
            std::fs::remove_file(&path)?;
            log::info!("删除日志文件: {:?}", path);
        }
    }

    Ok(())
}