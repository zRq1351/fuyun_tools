pub mod clipboard;
pub mod config;
pub mod utils;

pub mod text_selection;
pub mod mouse_listener;

use crate::config::{CTRL_KEY, DEFAULT_HIDE_SHORTCUT, DEFAULT_TOGGLE_SHORTCUT};
use crate::utils::get_logs_dir_path;
use clipboard::ClipboardManager;
use config::{CLIPBOARD_POLL_INTERVAL, MAX_ITEMS_OPTIONS};
use enigo::{Enigo, Key, Keyboard, Settings};
// use rdev::{listen, Button, EventType}; // 仅在需要鼠标键盘监听时使用
use std::collections::HashMap;
use std::env;

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::menu::{CheckMenuItem, CheckMenuItemBuilder, Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_positioner::{Position, WindowExt};
use tauri_plugin_updater::UpdaterExt;
use utils::{load_settings, save_settings, AppSettingsData};

// 添加必要的导入
use lazy_static::lazy_static;

lazy_static! {
    static ref ENIGO_INSTANCE: Arc<Mutex<Option<Enigo>>> = Arc::new(Mutex::new(None));
}

#[derive(Clone)]
struct TrayMenuItems {
    max_items_map: HashMap<String, CheckMenuItem<tauri::Wry>>,
    autostart_item: CheckMenuItem<tauri::Wry>,
}

#[derive(Clone)]
struct AppSettings {
    #[allow(dead_code)] // 允许该字段未被读取
    position: String,
    max_items: usize,
}

pub struct AppState {
    clipboard_manager: Arc<Mutex<ClipboardManager>>,
    is_visible: bool,
    selected_index: usize,
    settings: AppSettings,
    is_updating_clipboard: bool,   // 添加标志位，标识是否正在更新剪贴板
    is_processing_selection: bool, // 添加标志位，标识是否正在处理选择
    tray_menu_items: Option<TrayMenuItems>,
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
            settings: AppSettings {
                position: "bottom".to_string(),
                max_items: saved_settings.max_items,
            },
            is_updating_clipboard: false,
            is_processing_selection: false,
            tray_menu_items: None,
        }
    }
}

pub fn run() {
    let initial_state = AppState::default();
    let state_arc = Arc::new(Mutex::new(initial_state));
    tauri::Builder::default()
        .manage(state_arc.clone())
        .setup(move |app| {
            let instance = single_instance::SingleInstance::new("fuyun_tools")
                .expect("未能创建单实例锁");
            
            if !instance.is_single() {
                app.dialog()
                    .message("软件已运行，请观察系统托盘！")
                    .title("提示")
                    .blocking_show();
                std::process::exit(0);
            }
            
            let app_handle = app.handle();
            let current_max_items = {
                let state_guard = state_arc.lock().unwrap();
                state_guard.settings.max_items
            };

            rebuild_tray_menu(&app_handle, Some(current_max_items), state_arc.clone());

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
                            
                            // 重置全局Ctrl键状态，防止状态不一致
                            crate::mouse_listener::reset_ctrl_key_state();
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
                        
                        // 重置全局Ctrl键状态，防止状态不一致
                        crate::mouse_listener::reset_ctrl_key_state();
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
            translate_text,
            explain_text,
            copy_text,
        ])
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Warn)
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::Folder {
                        path: get_logs_dir_path(),
                        file_name: Some(String::from("share_log")),
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
        if let Ok(_) = toolbar_window.is_focused(){
            toolbar_window.hide().unwrap();
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

/// 更新最大记录数设置
fn update_max_items_setting(app: &AppHandle, max_items: usize, state: Arc<Mutex<AppState>>) {
    log::info!("更新最大记录数设置: {}", max_items);
    update_app_settings(&state, max_items);

    rebuild_tray_menu(app, Some(max_items), state);
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

fn rebuild_tray_menu(
    app_handle: &AppHandle,
    selected_max_items: Option<usize>,
    state: Arc<Mutex<AppState>>,
) {
    let current_max_items = selected_max_items.unwrap_or_else(|| {
        let state_guard = state.lock().unwrap();
        state_guard.settings.max_items
    });

    let mut state_guard = state.lock().unwrap();
    let tray_menu_items = &mut state_guard.tray_menu_items;
    if let Some(ref mut items) = *tray_menu_items {
        for &select in MAX_ITEMS_OPTIONS {
            if let Some(menu_item) = items.max_items_map.get(&select.to_string()) {
                let _ = menu_item.set_checked(current_max_items == select);
                log::info!(
                    "设置{}条记录选中状态: {}",
                    select,
                    current_max_items == select
                );
            }
        }

        match app_handle.autolaunch().is_enabled() {
            Ok(autostart_enabled) => {
                let _ = items.autostart_item.set_checked(autostart_enabled);
                log::info!("设置自启动状态: {}", autostart_enabled);
            }
            Err(e) => {
                log::error!("获取自启动状态失败: {}", e);
                // 在某些平台上可能不支持，记录警告但不中断程序
                log::warn!("自启动功能可能不支持当前平台");
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
        let check_update_item = create_menu_item("check_update", "检查更新");
        let autostart_enabled = app_handle.autolaunch().is_enabled().unwrap_or(false);
        let autostart_item = CheckMenuItemBuilder::with_id("autostart", "开机自启")
            .checked(autostart_enabled)
            .build(app_handle)
            .expect("创建开机自启菜单项失败");

        let mut max_items_map = HashMap::new();
        let mut max_items_menu_items = Vec::new();

        for &select in MAX_ITEMS_OPTIONS {
            let label = format!("{}条", select);
            let menu_item = CheckMenuItemBuilder::with_id(select, &label)
                .checked(current_max_items == select)
                .build(app_handle)
                .expect(&format!("未能创建{}菜单项", label));

            max_items_map.insert(select.to_string(), menu_item.clone());
            max_items_menu_items.push(menu_item);
        }

        *tray_menu_items = Some(TrayMenuItems {
            max_items_map,
            autostart_item: autostart_item.clone(),
        });

        let mut max_items_menu_items_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
            Vec::new();
        for item in &max_items_menu_items {
            max_items_menu_items_refs.push(item);
        }

        let items_submenu = tauri::menu::Submenu::with_items(
            app_handle,
            "记录数",
            true,
            &max_items_menu_items_refs,
        )
        .expect("未能创建记录数子菜单");

        let clear_submenu_items: [&dyn tauri::menu::IsMenuItem<tauri::Wry>; 2] =
            [&clear_history_item, &clear_logs_item];

        let clear_submenu =
            tauri::menu::Submenu::with_items(app_handle, "清除", true, &clear_submenu_items)
                .expect("未能创建清除子菜单");

        let menu_items: [&dyn tauri::menu::IsMenuItem<tauri::Wry>; 6] = [
            &autostart_item,
            &items_submenu,
            &clear_submenu,
            &open_logs_item,
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
            .tooltip(&format!("剪贴板工具 v{}", version))
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
                        id if id.parse::<i32>().is_ok() => {
                            update_max_items_setting(
                                app,
                                id.parse().unwrap(),
                                state_for_events.clone(),
                            );
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
                        _ => {
                            log::info!("未知的菜单事件: {}", event_id);
                        }
                    }
                }
            })
            .build(app_handle)
            .expect("未能创建托盘图标");
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
fn update_app_settings(state: &Arc<Mutex<AppState>>, max_items: usize) {
    {
        let mut state_guard = state.lock().unwrap();
        state_guard.settings.max_items = max_items;

        let mut manager = state_guard.clipboard_manager.lock().unwrap();
        manager.set_max_items(max_items);
    }

    let settings = AppSettingsData { max_items };

    if let Err(e) = save_settings(&settings) {
        log::error!("保存设置失败: {}", e);
        eprintln!("保存设置失败: {}", e);
    }
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
            // 执行粘贴操作
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
        let _ = toolbar_window.close();
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
            rebuild_tray_menu(&app_handle, None, state_clone);
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
                                    // 显示下载进度
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
async fn translate_text(text: String) -> Result<String, String> {
    Ok(format!("Translation for: {}", text))
}

#[tauri::command]
async fn explain_text(text: String) -> Result<String, String> {
    Ok(format!("Explanation for: {}", text))
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
