pub mod core;
pub mod services;
pub mod ui;
pub mod utils;
pub mod features;

use crate::core::app_state::AppState;
use crate::core::config::DEFAULT_HIDE_SHORTCUT;
use crate::services::ai_services::{stream_explain_text, stream_translate_text};
use crate::services::clipboard_manager::start_clipboard_listener;
use crate::services::image_clipboard_manager::start_image_clipboard_listener;
use crate::ui::commands::*;
use crate::ui::tray_menu::rebuild_tray_menu;
use crate::ui::window_manager::{
    hide_clipboard_window, hide_image_clipboard_window, show_clipboard_window,
    show_image_clipboard_window,
};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// 启动划词选择监听器
pub fn start_text_selection_listener(app_handle: AppHandle, state: Arc<Mutex<AppState>>) {
    let selection_enabled = {
        let state_guard = state.lock().unwrap();
        state_guard.settings.selection_enabled
    };

    features::mouse_listener::set_selection_listener_enabled(
        app_handle,
        state,
        selection_enabled,
    );
}

/// 运行Tauri应用程序
pub fn run() {
    let initial_state = AppState::default();
    let state_arc = Arc::new(Mutex::new(initial_state));

    let builder = tauri::Builder::default()
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
            let hot_key = state_arc
                .lock().unwrap().settings.hot_key.clone();
            let image_hot_key = state_arc
                .lock().unwrap().settings.image_hot_key.clone();
            app.global_shortcut()
                .on_shortcut(hot_key.as_str(), move |_app, _shortcut, event| {
                    if let ShortcutState::Pressed = event.state {
                        let state_guard = state_clone.lock().unwrap();
                        if !state_guard.is_visible && !state_guard.is_image_visible && !state_guard.is_processing_selection {
                            drop(state_guard);
                            show_clipboard_window(app_handle_clone.clone(), state_clone.clone());

                            features::mouse_listener::reset_ctrl_key_state();
                        }
                    }
                })
                .map_err(|e| e.to_string())?;

            let state_clone_image = state_arc.clone();
            let app_handle_clone_image = app_handle.clone();
            app.global_shortcut()
                .on_shortcut(image_hot_key.as_str(), move |_app, _shortcut, event| {
                    if let ShortcutState::Pressed = event.state {
                        let state_guard = state_clone_image.lock().unwrap();
                        if !state_guard.is_visible && !state_guard.is_image_visible && !state_guard.is_processing_selection {
                            drop(state_guard);
                            show_image_clipboard_window(app_handle_clone_image.clone(), state_clone_image.clone());
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
                        hide_image_clipboard_window(
                            app_handle_clone_hide.clone(),
                            state_clone_hide.clone(),
                        );

                        features::mouse_listener::reset_ctrl_key_state();
                    }
                })
                .map_err(|e| e.to_string())?;

            start_clipboard_listener(app_handle.clone(), state_arc.clone());
            start_image_clipboard_listener(app_handle.clone(), state_arc.clone());

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
            remove_image_clipboard_item,
            get_clipboard_history,
            get_image_clipboard_history,
            open_image_preview_window,
            close_image_preview_window,
            warmup_image_clipboard_item,
            select_and_fill,
            select_and_fill_image,
            set_item_category,
            set_image_item_category,
            add_category,
            add_image_category,
            remove_category,
            remove_image_category,
            get_clipboard_bottom_offset,
            preview_clipboard_bottom_offset,
            save_clipboard_bottom_offset,
            window_blur,
            image_window_blur,
            selection_toolbar_blur,
            copy_text,
            get_ai_settings,
            save_app_settings,
            test_ai_connection,
            stream_translate_text,
            stream_explain_text,
            get_provider_config,
            remove_ai_provider,
            get_all_configured_providers,
        ])
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::Builder::new().build());

    // 使用统一的日志配置
    let builder = builder.plugin(core::logger::build_logger().build());

    builder
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_opener::init())
        .build(tauri::generate_context!())
        .expect("构建Tauri应用时出错")
        .run(|_app_handle, _event| {});
}
