pub mod core;
pub mod services;
pub mod ui;
pub mod utils;
pub mod features;
pub mod sync;

use crate::core::app_state::AppState;
use crate::core::error::install_global_panic_hook;
use crate::services::ai_services::{stream_explain_text, stream_translate_text};
use crate::services::clipboard_manager::start_clipboard_listener;
use crate::services::image_clipboard_manager::{
    emit_image_history_payload, start_image_clipboard_listener,
};
use crate::sync::Mutex;
use crate::ui::commands::*;
use crate::ui::tray_menu::rebuild_tray_menu;
use crate::ui::window_manager::{show_clipboard_window, show_image_clipboard_window};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

fn lock_state<'a>(state: &'a Arc<Mutex<AppState>>) -> crate::sync::MutexGuard<'a, AppState> {
    state.lock().expect("infallible mutex lock failed")
}

/// 启动划词选择监听器
pub fn start_text_selection_listener(app_handle: AppHandle, state: Arc<Mutex<AppState>>) {
    let selection_enabled = {
        let state_guard = lock_state(&state);
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
    install_global_panic_hook();
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
            if let Some(image_window) = app.get_webview_window("image_clipboard") {
                let _ = image_window.hide();
            }

            let app_handle = app.handle();
            rebuild_tray_menu(app_handle, state_arc.clone());
            let state_clone = state_arc.clone();
            let app_handle_clone = app_handle.clone();
            let hot_key = {
                let guard = lock_state(&state_arc);
                guard.settings.hot_key.clone()
            };
            let image_hot_key = {
                let guard = lock_state(&state_arc);
                guard.settings.image_hot_key.clone()
            };
            let screenshot_hot_key = {
                let guard = lock_state(&state_arc);
                guard.settings.screenshot_hot_key.clone()
            };
            let mut shortcut_conflicts: Vec<String> = Vec::new();
            if let Err(e) = app.global_shortcut()
                .on_shortcut(hot_key.as_str(), move |_app, _shortcut, event| {
                    if let ShortcutState::Pressed = event.state {
                        let state_guard = lock_state(&state_clone);
                        if !state_guard.is_visible && !state_guard.is_image_visible {
                            drop(state_guard);
                            crate::ui::commands::interrupt_text_fill_flow(&state_clone);
                            show_clipboard_window(app_handle_clone.clone(), state_clone.clone());

                            features::mouse_listener::reset_ctrl_key_state();
                        }
                    }
                })
            {
                log::warn!("文字窗口快捷键 '{}' 注册失败: {}", hot_key, e);
                shortcut_conflicts.push(format!("文字窗口：{}", hot_key));
            }

            let state_clone_image = state_arc.clone();
            let app_handle_clone_image = app_handle.clone();
            if let Err(e) = app.global_shortcut()
                .on_shortcut(image_hot_key.as_str(), move |_app, _shortcut, event| {
                    if let ShortcutState::Pressed = event.state {
                        let state_guard = lock_state(&state_clone_image);
                        if !state_guard.is_visible && !state_guard.is_image_visible {
                            drop(state_guard);
                            crate::ui::commands::interrupt_image_fill_flow(&state_clone_image);
                            show_image_clipboard_window(app_handle_clone_image.clone(), state_clone_image.clone());
                        }
                    }
                })
            {
                log::warn!("图片窗口快捷键 '{}' 注册失败: {}", image_hot_key, e);
                shortcut_conflicts.push(format!("图片窗口：{}", image_hot_key));
            }

            let app_handle_clone_screenshot = app_handle.clone();
            if let Err(e) = app.global_shortcut()
                .on_shortcut(screenshot_hot_key.as_str(), move |_app, _shortcut, event| {
                    if let ShortcutState::Pressed = event.state {
                        let app_handle_inner = app_handle_clone_screenshot.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Err(e) = crate::ui::commands::open_screenshot_editor(app_handle_inner).await {
                                log::error!("截图失败: {}", e);
                            }
                        });
                    }
                })
            {
                log::warn!("截图快捷键 '{}' 注册失败: {}", screenshot_hot_key, e);
                shortcut_conflicts.push(format!("截图：{}", screenshot_hot_key));
            }

            if !shortcut_conflicts.is_empty() {
                let payload = serde_json::json!({
                    "conflicts": shortcut_conflicts.clone()
                });
                let _ = app_handle.emit("shortcut-conflict-warning", payload.clone());
                if let Some(settings_window) = app.get_webview_window("settings") {
                    let _ = settings_window.show();
                    let _ = settings_window.set_focus();
                    let script = format!("window.__SHORTCUT_CONFLICT__ = {};", payload);
                    let _ = settings_window.eval(&script);
                }
            }

            start_clipboard_listener(app_handle.clone(), state_arc.clone());
            start_image_clipboard_listener(app_handle.clone(), state_arc.clone());

            // 初始化异步预览生成器，支持事件通知
            crate::utils::image_clipboard::init_preview_generator_with_app_handle(app_handle.clone());

            emit_image_history_payload(app_handle, state_arc.clone());

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
            remove_image_clipboard_item_by_id,
            get_clipboard_history,
            get_clipboard_history_page,
            get_image_clipboard_history,
            get_image_clipboard_history_page,
            open_image_preview_window_by_id,
            close_image_preview_window,
            warmup_image_clipboard_item_by_id,
            warmup_multiple_images,
            select_and_fill,
            select_and_fill_image_by_id,
            set_item_category,
            set_image_item_category,
            add_category,
            add_image_category,
            remove_category,
            remove_image_category,
            set_image_item_tags,
            set_clipboard_item_pinned,
            set_image_item_pinned,
            promote_clipboard_item,
            promote_image_clipboard_item_by_id,
            clear_text_history,
            clear_image_history,
            import_image_files,
            get_clipboard_bottom_offset,
            preview_clipboard_bottom_offset,
            save_clipboard_bottom_offset,
            window_blur,
            image_window_blur,
            selection_toolbar_blur,
            show_selection_toolbar_with_text,
            show_ocr_text_window,
            copy_text,
            copy_and_paste_text,
            get_ai_settings,
            #[cfg(debug_assertions)]
            get_text_dedup_metrics,
            #[cfg(debug_assertions)]
            get_image_storage_metrics,
            save_app_settings,
            test_ai_connection,
            stream_translate_text,
            stream_explain_text,
            get_provider_config,
            remove_ai_provider,
            get_all_configured_providers,
            get_image_preview_by_id,
            check_previews_ready,
            get_clipboard_full_snapshot,
            // 截图相关命令
            start_screenshot,
            recognize_image_ocr,
            capture_region,
            save_screenshot,
            pin_screenshot_on_screen,
            close_pinned_image_window,
            get_pinned_image_window_position,
            move_pinned_image_window,
            get_screen_size,
            set_screenshot_clipboard_link_once,
            open_screenshot_editor,
            get_window_list,
            close_screenshot_window,
        ])
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::Builder::new().build());

    // 使用统一的日志配置
    let builder = builder.plugin(core::logger::build_logger().build());

    let app = builder
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_opener::init())
        .build(tauri::generate_context!());

    match app {
        Ok(app) => {
            app.run(|_app_handle, _event| {});
        }
        Err(e) => {
            log::error!("构建Tauri应用失败: {}", e);
        }
    }
}
