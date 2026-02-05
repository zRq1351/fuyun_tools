pub mod core;
pub mod services;
pub mod ui;
pub mod utils;
pub mod features;

use crate::core::app_state::AppState;
use crate::core::config::{DEFAULT_HIDE_SHORTCUT, DEFAULT_TOGGLE_SHORTCUT};
use crate::services::ai_services::{stream_explain_text, stream_translate_text};
use crate::services::clipboard_manager::start_clipboard_listener;
use crate::ui::commands::*;
use crate::ui::tray_menu::rebuild_tray_menu;
use crate::ui::window_manager::{hide_clipboard_window, show_clipboard_window};
use crate::utils::utils::get_logs_dir_path;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// 根据编译环境获取日志级别
fn get_log_level() -> log::LevelFilter {
    // 根据编译环境自动设置日志级别
    if cfg!(debug_assertions) {
        // 开发环境使用Debug级别
        log::LevelFilter::Debug
    } else {
        // 生产环境使用Warn级别
        log::LevelFilter::Warn
    }
}

/// 启动划词选择监听器
pub fn start_text_selection_listener(app_handle: AppHandle, state: Arc<Mutex<AppState>>) {
    features::mouse_listener::MouseListener::start_mouse_listener(app_handle, state);
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_basic_compilation() {
        // 基本编译测试
        assert!(true);
    }
}

pub fn run() {
    let initial_state = AppState::default();
    let state_arc = Arc::new(Mutex::new(initial_state));

    tauri::Builder::default()
        .manage(state_arc.clone())
        .setup(move |app| {
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
            let state_clone = state_arc.clone();
            let app_handle_clone = app_handle.clone();
            app.global_shortcut()
                .on_shortcut(DEFAULT_TOGGLE_SHORTCUT, move |_app, _shortcut, event| {
                    if let ShortcutState::Pressed = event.state {
                        let state_guard = state_clone.lock().unwrap();
                        if !state_guard.is_visible && !state_guard.is_processing_selection {
                            drop(state_guard);
                            show_clipboard_window(app_handle_clone.clone(), state_clone.clone());

                            features::mouse_listener::reset_ctrl_key_state();
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

                        features::mouse_listener::reset_ctrl_key_state();
                    }
                })
                .map_err(|e| e.to_string())?;

            start_clipboard_listener(app_handle.clone(), state_arc.clone());

            #[cfg(windows)]
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
            save_app_settings,
            test_ai_connection,
            stream_translate_text,
            stream_explain_text,
            check_for_updates,
            get_provider_config,
            get_all_configured_providers,
        ])
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(get_log_level())
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
        .plugin(tauri_plugin_opener::init())
        .build(tauri::generate_context!())
        .expect("构建Tauri应用时出错")
        .run(|_app_handle, _event| {});
}