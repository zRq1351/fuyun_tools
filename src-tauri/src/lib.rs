pub mod ai_client;
pub mod clipboard;
pub mod config;
pub mod mouse_listener;
pub mod text_selection;
pub mod utils; // 添加新的AI客户端模块

use crate::config::{CTRL_KEY, DEFAULT_HIDE_SHORTCUT, DEFAULT_TOGGLE_SHORTCUT};
use crate::utils::get_logs_dir_path;
use clipboard::ClipboardManager;
use config::CLIPBOARD_POLL_INTERVAL;
use enigo::{Enigo, Key, Keyboard, Settings};
use std::env;

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::menu::{CheckMenuItem, CheckMenuItemBuilder, Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_positioner::{Position, WindowExt};
use tauri_plugin_updater::UpdaterExt;
use utils::{load_settings, save_settings, AppSettingsData};

use crate::ai_client::{AIClient, AIConfig};
use lazy_static::lazy_static;

// 类型别名定义
pub type SharedAppState = AppState;

lazy_static! {
    static ref ENIGO_INSTANCE: Arc<Mutex<Option<Enigo>>> = Arc::new(Mutex::new(None));
}

#[derive(Clone)]
struct TrayMenuItems {
    autostart_item: CheckMenuItem<tauri::Wry>,
}

pub struct AppState {
    clipboard_manager: Arc<Mutex<ClipboardManager>>,
    is_visible: bool,
    selected_index: usize,
    settings: AppSettingsData,
    is_updating_clipboard: bool,
    is_processing_selection: bool,
    tray_menu_items: Option<TrayMenuItems>,
    ai_client: Arc<Mutex<Option<AIClient>>>, // 新增AI客户端缓存
}

impl Clone for AppState {
    fn clone(&self) -> Self {
        Self {
            clipboard_manager: self.clipboard_manager.clone(),
            is_visible: self.is_visible,
            selected_index: self.selected_index,
            settings: self.settings.clone(),
            is_updating_clipboard: self.is_updating_clipboard,
            is_processing_selection: self.is_processing_selection,
            tray_menu_items: None,
            ai_client: Arc::new(Mutex::new((*self.ai_client.lock().unwrap()).clone())), // 复制AI客户端
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        let saved_settings = load_settings().unwrap_or_default();

        Self {
            clipboard_manager: Arc::new(Mutex::new(ClipboardManager::new(
                saved_settings.max_items,
            ))),
            is_visible: false,
            selected_index: 0,
            settings: saved_settings,
            is_updating_clipboard: false,
            is_processing_selection: false,
            tray_menu_items: None,
            ai_client: Arc::new(Mutex::new(None)), // 初始化为None
        }
    }
}

pub fn run() {
    let initial_state = AppState::default();
    let state_arc = Arc::new(Mutex::new(initial_state));
    tauri::Builder::default()
        .manage(state_arc.clone())
        .setup(move |app| {
            let instance =
                single_instance::SingleInstance::new("fuyun_tools").expect("未能创建单实例锁");

            if !instance.is_single() {
                app.dialog()
                    .message("软件已运行，请观察系统托盘！")
                    .title("提示")
                    .blocking_show();
                std::process::exit(0);
            }

            if let Some(settings_window) = app.get_webview_window("settings") {
                let settings_window_clone = settings_window.clone();
                settings_window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = settings_window_clone.hide();
                    }
                });
            }

            let app_handle = app.handle();
            rebuild_tray_menu(&app_handle, state_arc.clone());
            // 注册全局快捷键监听
            let state_clone = state_arc.clone();
            let app_handle_clone = app_handle.clone();
            app.global_shortcut()
                .on_shortcut(DEFAULT_TOGGLE_SHORTCUT, move |_app, _shortcut, event| {
                    if let ShortcutState::Pressed = event.state {
                        let state_guard = state_clone.lock().unwrap();
                        if !state_guard.is_visible && !state_guard.is_processing_selection {
                            drop(state_guard);
                            show_clipboard_window(app_handle_clone.clone(), state_clone.clone());

                            mouse_listener::reset_ctrl_key_state();
                        }
                    }
                })
                .map_err(|e| e.to_string())?;

            let state_clone_hide = state_arc.clone();
            let app_handle_clone_hide = app_handle.clone();
            app.global_shortcut()
                .on_shortcut(DEFAULT_HIDE_SHORTCUT, move |_app, _shortcut, event| {
                    if let ShortcutState::Pressed = event.state {
                        hide_clipboard_window(
                            app_handle_clone_hide.clone(),
                            state_clone_hide.clone(),
                        );

                        mouse_listener::reset_ctrl_key_state();
                    }
                })
                .map_err(|e| e.to_string())?;

            start_clipboard_listener(app_handle.clone(), state_arc.clone());

            start_text_selection_listener(app_handle.clone(), state_arc.clone());

            #[cfg(desktop)]
            app_handle
                .plugin(tauri_plugin_updater::Builder::new().build())
                .map_err(|e| e.to_string())?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            remove_clipboard_item,
            get_clipboard_history,
            select_and_fill,
            window_blur,
            selection_toolbar_blur,
            copy_text,
            get_ai_settings,
            save_ai_settings,
            test_ai_connection,
            stream_translate_text,
            stream_explain_text,
        ])
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Warn)
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::Folder {
                        path: get_logs_dir_path(),
                        file_name: Some(String::from("fuyun_log")),
                    },
                ))
                .max_file_size(1000000) // 1MB
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepAll)
                .build(),
        )
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_positioner::init())
        .build(tauri::generate_context!())
        .expect("构建Tauri应用时出错")
        .run(|_app_handle, _event| {});
}
/// 启动划词选择监听器
pub fn start_text_selection_listener(app_handle: AppHandle, state: Arc<Mutex<AppState>>) {
    mouse_listener::MouseListener::start_mouse_listener(app_handle, state);
}

/// 打开划词工具栏
fn show_selection_toolbar_impl(app_handle: AppHandle, selected_text: String) {
    if let Some(toolbar_window) = app_handle.get_webview_window("selection_toolbar") {
        set_toolbar_window(&toolbar_window);
        if toolbar_window.show().is_ok() {
            if let Err(e) = app_handle.emit("selected-text", selected_text) {
                log::error!("未能发送选择文本到前端:{}", e);
            }
        }
    }
}

/// 设置工具栏窗口位置
fn set_toolbar_window(window: &tauri::WebviewWindow) {
    let _ = window.set_size(tauri::LogicalSize::new(50, 130));
    let _ = window.move_window(Position::RightCenter);
}

/// 隐藏工具栏窗口
fn hide_selection_toolbar_impl(app_handle: AppHandle) {
        if let Some(toolbar_window) = app_handle.get_webview_window("selection_toolbar") {
        if let Ok(is_visible) = toolbar_window.is_visible() {
            if is_visible {
                if let Ok(has_focus) = toolbar_window.is_focused() {
                    if !has_focus {
                        let _ = toolbar_window.hide();
                    }
                }
            }
        }
    }
}

///
/// 该函数会创建一个新线程，每隔100毫秒检查一次剪贴板内容，
/// 当检测到新内容时将其添加到历史记录。
///
/// # 参数
///
/// * `app_handle` - Tauri应用程序句柄
fn start_clipboard_listener(app_handle: AppHandle, state: Arc<Mutex<AppState>>) {
    thread::spawn(move || {
        let mut last_content = String::new();
        let mut check_interval = CLIPBOARD_POLL_INTERVAL;
        let mut last_check_time = std::time::Instant::now();

        loop {
            let elapsed = last_check_time.elapsed();
            if elapsed < check_interval {
                thread::sleep(check_interval - elapsed);
            }
            last_check_time = std::time::Instant::now();

            let is_updating = {
                let state_guard = state.lock().unwrap();
                state_guard.is_updating_clipboard || state_guard.is_processing_selection
            };

            if is_updating {
                continue;
            }

            let state_guard = state.lock().unwrap();
            let manager = state_guard.clipboard_manager.lock().unwrap();

            if let Some(current_content) = manager.get_content(&app_handle) {
                if !current_content.is_empty() && current_content != last_content {
                    let current_content_clone = current_content.clone();
                    drop(manager);
                    drop(state_guard);

                    add_to_clipboard_history(current_content_clone.clone(), state.clone());
                    last_content = current_content_clone.clone();

                    check_interval = Duration::from_millis(50);
                    log::info!("检测到剪贴板内容变化，已添加到历史记录");
                } else {
                    check_interval = CLIPBOARD_POLL_INTERVAL;
                }
            } else {
                check_interval = CLIPBOARD_POLL_INTERVAL;
            }
        }
    });
}

fn show_clipboard_window(app_handle: AppHandle, state: Arc<Mutex<AppState>>) {
    {
        let state_guard = state.lock().unwrap();
        if state_guard.is_visible {
            return;
        }
    }

    {
        let mut state_guard = state.lock().unwrap();
        state_guard.is_visible = true;
    }

    let selected_index = {
        let state_guard = state.lock().unwrap();
        state_guard.selected_index
    };

    let history = {
        let state_guard = state.lock().unwrap();
        let manager = state_guard.clipboard_manager.lock().unwrap();
        manager.get_history()
    };

    if let Some(_window) = app_handle.get_webview_window("clipboard") {
        let app_handle_clone = app_handle.clone();
        let history_clone = history.clone();
        thread::spawn(move || {
            if let Some(window) = app_handle_clone.get_webview_window("clipboard") {
                set_window_position(&window);
                if window.show().is_ok() {
                    let _ = window.set_focus();
                    let payload = serde_json::json!({
                        "history": history_clone,
                        "selectedIndex": selected_index
                    });
                    let _ = app_handle_clone.emit("show-window", payload);
                }
            }
        });
    }
}

fn hide_clipboard_window(app_handle: AppHandle, state: Arc<Mutex<AppState>>) {
    let is_visible = {
        let state_guard = state.lock().unwrap();
        state_guard.is_visible
    };

    if !is_visible {
        return;
    }

    if let Some(window) = app_handle.get_webview_window("clipboard") {
        let _ = window.hide();
    }
    {
        let mut state_guard = state.lock().unwrap();
        state_guard.is_visible = false;
        state_guard.selected_index = 0;
        state_guard.is_processing_selection = false;
    }
}

/// 设置窗口位置和大小
///
/// 根据指定的位置参数，将窗口定位在屏幕的相应位置。
/// 目前只支持固定在屏幕底部。
///
/// # 参数
///
/// * `window` - 要设置位置的窗口引用
/// * `_position` - 位置字符串（目前未使用，窗口始终固定在底部）
fn set_window_position(window: &tauri::WebviewWindow) {
    if let Some(monitor) = window.current_monitor().unwrap() {
        let screen_size = monitor.size();

        let window_width = screen_size.width;
        let window_height = 250u32;

        let _ = window.set_size(tauri::LogicalSize::new(window_width, window_height));

        let _ = window.move_window(Position::BottomLeft);
    }
}

/// 打开日志目录
fn open_log_directory() -> Result<(), Box<dyn std::error::Error>> {
    let log_dir = get_logs_dir_path();

    if !log_dir.exists() {
        return Ok(());
    }
    opener::open(log_dir)?;
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

fn rebuild_tray_menu(app_handle: &AppHandle, state: Arc<Mutex<AppState>>) {
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
        let check_update_item = create_menu_item("check_update", "检查更新");
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
            tauri::menu::Submenu::with_items(app_handle, "清除", true, &clear_submenu_items)
                .expect("未能创建清除子菜单");

        let menu_items: [&dyn tauri::menu::IsMenuItem<tauri::Wry>; 6] = [
            &autostart_item,
            &clear_submenu,
            &open_logs_item,
            &settings_item,
            &check_update_item,
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
                            if let Err(e) = open_log_directory() {
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
                        "check_update" => {
                            handle_check_update_event(app);
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

fn open_settings(app: &AppHandle) {
    if let Some(settings_window) = app.get_webview_window("settings") {
        let _ = settings_window.show();
    }
}

/// 添加到剪贴板历史记录
fn add_to_clipboard_history(content: String, state: Arc<Mutex<AppState>>) {
    if content.trim().is_empty() {
        return;
    }

    {
        let state_guard = state.lock().unwrap();
        if state_guard.is_processing_selection {
            log::debug!("正在进行划词操作，跳过添加到历史记录");
            return;
        }
    }

    let state_guard = state.lock().unwrap();
    let manager = state_guard.clipboard_manager.lock().unwrap();
    manager.add_to_history(content);
}

#[tauri::command]
async fn get_clipboard_history(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<String>, String> {
    let state_guard = state.lock().unwrap();
    let manager = state_guard.clipboard_manager.lock().unwrap();
    Ok(manager.get_history())
}

#[tauri::command]
async fn select_and_fill(
    index: usize,
    state: State<'_, Arc<Mutex<AppState>>>,
    app: AppHandle,
) -> Result<String, String> {
    let item = {
        let state_guard = state.lock().unwrap();
        let manager = state_guard.clipboard_manager.lock().unwrap();
        let history = manager.get_history();

        if let Some(item) = history.get(index) {
            Some(item.clone())
        } else {
            let error_msg = format!("索引 {} 超出范围", index);
            log::info!("{}", error_msg);
            return Err(error_msg);
        }
    };

    {
        let mut state_guard = state.lock().unwrap();
        state_guard.is_updating_clipboard = true;
        state_guard.is_processing_selection = true;
    }

    let item_content = item.as_ref().unwrap().clone();
    let result = {
        let state_guard = state.lock().unwrap();
        let manager = state_guard.clipboard_manager.lock().unwrap();
        manager.set_clipboard_content(&app, &item_content)
    };

    {
        let mut state_guard = state.lock().unwrap();
        state_guard.is_updating_clipboard = false;
    }

    let app_handle = app.clone();
    let state_clone = state.inner().clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        hide_clipboard_window(app_handle, state_clone.clone());
    });
    match result {
        Ok(_) => {
            let value = item_content.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(100));
                simulate_paste();
            });

            Ok(value)
        }
        Err(e) => {
            let error_msg = format!("复制到剪贴板失败: {}", e);
            log::info!("{}", error_msg);
            {
                let state_guard = state.lock().unwrap();
                let mut state_guard = state_guard;
                state_guard.is_processing_selection = false;
            }
            Err(error_msg)
        }
    }
}

fn simulate_paste() {
    {
        let mut enigo_guard = ENIGO_INSTANCE.lock().unwrap();
        if enigo_guard.is_none() {
            *enigo_guard = Some(Enigo::new(&Settings::default()).expect("未能初始化enigo"));
        }

        if let Some(ref mut enigo) = *enigo_guard {
            let _ = enigo.key(CTRL_KEY, enigo::Direction::Press);
            let _ = enigo.key(Key::Unicode('v'), enigo::Direction::Click);
            let _ = enigo.key(CTRL_KEY, enigo::Direction::Release);
        }
    }
}

#[tauri::command]
async fn remove_clipboard_item(
    index: usize,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    log::info!("删除剪贴板项目，索引: {}", index);
    let state_guard = state.lock().unwrap();
    let manager = state_guard.clipboard_manager.lock().unwrap();
    manager.remove_from_history(index)?;
    Ok(())
}

#[tauri::command]
async fn window_blur(state: State<'_, Arc<Mutex<AppState>>>, app: AppHandle) -> Result<(), String> {
    let is_visible = {
        let state_guard = state.lock().unwrap();
        state_guard.is_visible
    };
    if is_visible {
        let state_clone = state.inner().clone();
        hide_clipboard_window(app, state_clone);
    }
    Ok(())
}

#[tauri::command]
async fn selection_toolbar_blur(app: AppHandle) -> Result<(), String> {
    if let Some(toolbar_window) = app.get_webview_window("selection_toolbar") {
        let _ = toolbar_window.hide();
    }
    Ok(())
}

/// 处理退出事件
fn handle_quit_event(app: &AppHandle) {
    log::info!("退出应用");
    app.exit(0);
}

/// 处理自启动设置事件
fn handle_autostart_event(app: &AppHandle, state: &Arc<Mutex<AppState>>) {
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
fn handle_clear_history_event(state: &Arc<Mutex<AppState>>) {
    let state_guard = state.lock().unwrap();
    let manager = state_guard.clipboard_manager.lock().unwrap();
    manager.clear_history();
}

/// 处理检查更新事件
fn handle_check_update_event(app: &AppHandle) {
    log::info!("检查更新");

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        match check_for_updates(app_handle.clone()).await {
            Ok(has_update) => {
                if has_update {
                    log::info!("发现新版本并已开始下载安装");
                } else {
                    log::info!("已是最新版本");

                    let _ = app_handle
                        .notification()
                        .builder()
                        .title("更新")
                        .body("应用已是最新版本")
                        .show();
                }
            }
            Err(e) => {
                log::error!("检查更新失败: {}", e);

                let _ = app_handle
                    .notification()
                        .builder()
                        .title("更新错误")
                        .body(&format!("检查更新失败: {}", e))
                        .show();
            }
        }
    });
}

#[tauri::command]
async fn check_for_updates(app: AppHandle) -> Result<bool, String> {
    match app.updater().map_err(|e| e.to_string()) {
        Ok(updater) => match updater.check().await {
            Ok(update_option) => {
                if let Some(update) = update_option {
                    let should_update = app
                        .dialog()
                        .message(format!(
                            "发现新版本 {}，是否立即更新？\n\n更新内容:\n{}",
                            update.version,
                            update.body.as_ref().unwrap_or(&"".to_string())
                        ))
                        .title("发现更新")
                        .blocking_show();

                    if should_update {
                        update
                            .download_and_install(
                                |progress, total| {
                                    let percentage = if let Some(total) = total {
                                        (progress as f64 / total as f64 * 100.0).round() as u32
                                    } else {
                                        0
                                    };
                                    log::info!(
                                        "更新下载进度: {}% ({} bytes)",
                                        percentage,
                                        progress
                                    );

                                    let _ = app
                                        .notification()
                                        .builder()
                                        .title("更新下载进度")
                                        .body(&format!("下载进度: {}%", percentage))
                                        .show();
                                },
                                || {
                                    log::info!("更新下载完成，准备安装...");

                                    let _ = app
                                        .notification()
                                        .builder()
                                        .title("更新下载完成")
                                        .body("更新下载完成，准备安装...")
                                        .show();
                                },
                            )
                            .await
                            .map_err(|e| e.to_string())?;
                        Ok(true)
                    } else {
                        Ok(false)
                    }
                } else {
                    app.dialog()
                        .message("已是最新版本")
                        .title("更新")
                        .blocking_show();
                    Ok(false)
                }
            }
            Err(e) => Err(e.to_string()),
        },
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
async fn get_ai_settings() -> Result<AppSettingsData, String> {
    load_settings()
}

#[tauri::command]
async fn save_ai_settings(
    max_items: usize,
    ai_api_url: String,
    ai_model_name: String,
    ai_api_key: String,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let settings = AppSettingsData {
        max_items,
        ai_api_url,
        ai_model_name,
        ai_api_key,
    };

    save_settings(&settings).map_err(|e| e.to_string())?;

    {
        let mut state_guard = state.lock().unwrap();
        state_guard.settings = settings;
    }

    Ok(())
}

#[tauri::command]
async fn test_ai_connection(
    ai_api_url: String,
    ai_model_name: String,
    ai_api_key: String,
) -> Result<String, String> {
    use crate::ai_client::{AIClient, AIConfig};

    let config = AIConfig {
        api_key: ai_api_key,
        base_url: ai_api_url,
        model: ai_model_name,
    };

    let client = AIClient::new(config).map_err(|e| format!("客户端初始化失败: {}", e))?;

    match client.test_connection().await {
        Ok(success) => {
            if success {
                Ok("连接成功".to_string())
            } else {
                Err("连接测试未返回预期结果".to_string())
            }
        }
        Err(e) => {
            log::error!("AI连接测试失败: {}", e);
            Err(format!("连接测试失败: {}", e))
        }
    }
}

#[tauri::command]
async fn copy_text(text: String, app: AppHandle) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;

    match app.clipboard().write_text(text) {
        Ok(()) => {
            log::info!("文本已复制到剪贴板");
            Ok(())
        }
        Err(e) => {
            let error_msg = format!("复制文本失败: {}", e);
            log::error!("{}", error_msg);
            Err(error_msg)
        }
    }
}

async fn show_result_window(
    title: String,
    content: String,
    window_type: String,
    original: String,
    app: AppHandle,
) -> Result<(), String> {
    let window_label = format!("result_{}", window_type);

    if let Some(existing_window) = app.get_webview_window(&window_label) {
        let _ = existing_window.show();
        let _ = existing_window.set_focus();
        return Ok(());
    }

    let window = tauri::WebviewWindowBuilder::new(
        &app,
        &window_label,
        WebviewUrl::App("result_display.html".into()),
    )
    .title(&title)
    .visible(false)
    .inner_size(480.0, 300.0)
    .resizable(true)
    .decorations(true)
    .on_page_load(move |window, _| {
        let payload = serde_json::json!({
            "type": window_type.clone(),
            "original": original.clone(),
            "content": content.clone()
        });
        let script = format!("window.__INITIAL_DATA__ = {};", payload);
        let _ = window.eval(&script);
    })
    .build()
    .map_err(|e| format!("创建窗口失败: {}", e))?;

    let _ = window.move_window(Position::RightCenter);
    let _ = window.show();
    let _ = window.set_focus();
    Ok(())
}

async fn update_result_window(
    content: String,
    window_type: String,
    app: AppHandle,
) -> Result<(), String> {
    use tauri::Manager;
    let window_label = format!("result_{}", window_type);
    if let Some(window) = app.get_webview_window(&window_label) {
        let payload = serde_json::json!({
            "content": content
        });
        match window.emit("result-update", payload) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("发送数据失败: {}", e)),
        }
    } else {
        log::error!("{}窗口不存在", &window_type);
        Err("窗口不存在".to_string())
    }
}

async fn get_or_create_ai_client(state: Arc<Mutex<SharedAppState>>) -> Result<AIClient, String> {
    let current_config = {
        let state_guard = state.lock().unwrap();
        let settings = &state_guard.settings;
        let cached_client_exists_and_valid = {
            if let Some(ref client) = *state_guard.ai_client.lock().unwrap() {
                settings.ai_api_key == client.config.api_key &&
                settings.ai_api_url == client.config.base_url &&
                settings.ai_model_name == client.config.model
            } else {
                false
            }
        };
        
        if cached_client_exists_and_valid {
            if let Some(client) = (*state_guard.ai_client.lock().unwrap()).as_ref() {
                return Ok(client.clone());
            }
        }
        
        AIConfig {
            api_key: settings.ai_api_key.clone(),
            base_url: settings.ai_api_url.clone(),
            model: settings.ai_model_name.clone(),
        }
    };

    let client = AIClient::new(current_config).map_err(|e| format!("客户端初始化失败: {}", e))?;

    {
        let state_guard = state.lock().unwrap();
        *state_guard.ai_client.lock().unwrap() = Some(client.clone());
    }

    Ok(client)
}

#[tauri::command]
async fn stream_translate_text(
    text: String,
    source_language: String,
    target_language: String,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    use crate::ai_client::{ChatCompletionRequest, Message};

    let client: AIClient = get_or_create_ai_client(state.inner().clone()).await?;
    let model = {
        let state_guard = state.lock().unwrap();
        state_guard.settings.ai_model_name.clone()
    };

    show_result_window(
        "翻译结果".to_string(),
        "正在翻译...".to_string(),
        "translation".to_string(),
        text.clone(),
        app.clone(),
    )
    .await?;

    // 直接使用传入的中文语言名称
    let source_language_name = source_language;
    let target_language_name = target_language;

    let messages = vec![Message {
        role: "user".to_string(),
        content: format!(
            "请翻译这段话不要过多解释，最好根据文字直接翻译,由{}翻译为:{}。：\n\n{}",
            source_language_name, target_language_name, text
        ),
    }];

    let request = ChatCompletionRequest {
        model: model.clone(),
        messages,
        temperature: Some(0.7),
        max_tokens: None,
        max_completion_tokens: None,
        top_p: Some(1.0),
        frequency_penalty: Some(0.0),
        presence_penalty: Some(0.0),
        stream: Some(true), // 启用流式响应
    };

    let result = client
        .chat_completion_stream(&request, |content_chunk| {
            let app_clone = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) =
                    update_result_window(content_chunk, "translation".to_string(), app_clone).await
                {
                    log::error!("发送数据失败:{}", e);
                }
            });
        })
        .await;

    match result {
        Ok(()) => {
            log::info!("翻译完成");
        }
        Err(e) => {
            let error_msg = format!("翻译失败: {}", e);
            update_result_window(error_msg.clone(), "translation".to_string(), app).await?;
            log::error!("翻译失败: {}", error_msg);
        }
    }

    Ok(())
}

#[tauri::command]
async fn stream_explain_text(
    text: String,
    target_language: String,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    use crate::ai_client::{ChatCompletionRequest, Message};

    let client: AIClient = get_or_create_ai_client(state.inner().clone()).await?;
    let model = {
        let state_guard = state.lock().unwrap();
        state_guard.settings.ai_model_name.clone()
    };

    show_result_window(
        "解释结果".to_string(),
        "正在解释...".to_string(),
        "explanation".to_string(),
        text.clone(),
        app.clone(),
    )
    .await?;
    let target_language_name = target_language;

    let messages = vec![Message {
        role: "user".to_string(),
        content: format!(
            "请用{}200字内解释这段话：\n\n{}",
            target_language_name, text
        ),
    }];

    let request = ChatCompletionRequest {
        model: model.clone(),
        messages,
        temperature: Some(0.7),
        top_p: Some(1.0),
        max_tokens: None,
        max_completion_tokens: None,
        frequency_penalty: Some(0.0),
        presence_penalty: Some(0.0),
        stream: Some(true), // 启用流式响应
    };

    let result = client
        .chat_completion_stream(&request, |content_chunk| {
            let app_clone = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) =
                    update_result_window(content_chunk, "explanation".to_string(), app_clone).await
                {
                    log::error!("更新解释结果窗口失败: {}", e);
                }
            });
        })
        .await;

    match result {
        Ok(()) => {
            log::info!("解释完成");
        }
        Err(e) => {
            let error_msg = format!("解释失败: {}", e);
            update_result_window(error_msg, "explanation".to_string(), app).await?;
        }
    }

    Ok(())
}
