pub mod core;
pub mod services;
pub mod ui;
pub mod utils;
pub mod features;
pub mod sync;

use crate::core::app_state::AppState;
use crate::core::error::install_global_panic_hook;
use crate::services::ai_services::{stream_explain_text, stream_translate_text};
use crate::services::clipboard_manager::set_clipboard_listener_enabled;
use crate::services::image_clipboard_manager::{
    emit_image_history_payload, set_image_clipboard_listener_enabled,
};
use crate::sync::Mutex;
use crate::ui::commands::*;
use crate::ui::commands_recording::{
    cancel_recording, check_recording_ffmpeg, download_recording_ffmpeg, get_recording_output_dir, get_recording_state,
    list_recording_audio_devices, list_recording_audio_processes, list_recording_system_output_devices, open_recording_folder, pause_recording,
    resize_recording_toolbar, resume_recording, run_recording_regression, show_recording_toolbar, start_recording,
    stop_recording, toggle_recording_from_shortcut, update_recording_audio_capture,
};
use crate::ui::tray_menu::rebuild_tray_menu;
use crate::ui::window_manager::{show_clipboard_window, show_image_clipboard_window};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

static RECORDING_SHORTCUT_IN_FLIGHT: AtomicBool = AtomicBool::new(false);
static RECORDING_SHORTCUT_LAST_TRIGGER_MS: AtomicU64 = AtomicU64::new(0);
const RECORDING_SHORTCUT_MIN_INTERVAL_MS: u64 = 300;

fn lock_state<'a>(state: &'a Arc<Mutex<AppState>>) -> crate::sync::MutexGuard<'a, AppState> {
    state.lock().unwrap_or_else(|never| match never {})
}

fn now_unix_ms_u64() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
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
            let recording_hot_key = {
                let guard = lock_state(&state_arc);
                guard.settings.recording_hot_key.clone()
            };
            let text_clipboard_enabled = {
                let guard = lock_state(&state_arc);
                guard.settings.text_clipboard_enabled
            };
            let image_clipboard_enabled = {
                let guard = lock_state(&state_arc);
                guard.settings.image_clipboard_enabled
            };
            let screenshot_enabled = {
                let guard = lock_state(&state_arc);
                guard.settings.screenshot_enabled
            };
            let recording_enabled = {
                let guard = lock_state(&state_arc);
                guard.settings.recording_enabled
            };
            let mut shortcut_conflicts: Vec<String> = Vec::new();
            if text_clipboard_enabled {
                if let Err(e) = app.global_shortcut()
                    .on_shortcut(hot_key.as_str(), move |_app, _shortcut, event| {
                        if let ShortcutState::Pressed = event.state {
                            let state_guard = lock_state(&state_clone);
                            if !state_guard.settings.text_clipboard_enabled {
                                return;
                            }
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
            }

            let state_clone_image = state_arc.clone();
            let app_handle_clone_image = app_handle.clone();
            if image_clipboard_enabled {
                if let Err(e) = app.global_shortcut()
                    .on_shortcut(image_hot_key.as_str(), move |_app, _shortcut, event| {
                        if let ShortcutState::Pressed = event.state {
                            let state_guard = lock_state(&state_clone_image);
                            if !state_guard.settings.image_clipboard_enabled {
                                return;
                            }
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
            }

            let app_handle_clone_screenshot = app_handle.clone();
            let state_clone_screenshot = state_arc.clone();
            if screenshot_enabled {
                if let Err(e) = app.global_shortcut()
                    .on_shortcut(screenshot_hot_key.as_str(), move |_app, _shortcut, event| {
                        if let ShortcutState::Pressed = event.state {
                            let state_guard = lock_state(&state_clone_screenshot);
                            if !state_guard.settings.screenshot_enabled {
                                return;
                            }
                            drop(state_guard);
                            let app_handle_inner = app_handle_clone_screenshot.clone();
                            tauri::async_runtime::spawn(async move {
                                if let Err(e) = crate::ui::commands::open_screenshot_editor(app_handle_inner, None).await {
                                    log::error!("截图失败: {}", e);
                                }
                            });
                        }
                    })
                {
                    log::warn!("截图快捷键 '{}' 注册失败: {}", screenshot_hot_key, e);
                    shortcut_conflicts.push(format!("截图：{}", screenshot_hot_key));
                }
            }

            let app_handle_clone_recording = app_handle.clone();
            let state_clone_recording = state_arc.clone();
            if recording_enabled {
                if let Err(e) = app.global_shortcut()
                    .on_shortcut(recording_hot_key.as_str(), move |_app, _shortcut, event| {
                        if let ShortcutState::Pressed = event.state {
                            let now_ms = now_unix_ms_u64();
                            let last_ms = RECORDING_SHORTCUT_LAST_TRIGGER_MS.load(Ordering::Relaxed);
                            if last_ms > 0 && now_ms.saturating_sub(last_ms) < RECORDING_SHORTCUT_MIN_INTERVAL_MS {
                                return;
                            }
                            if RECORDING_SHORTCUT_IN_FLIGHT.swap(true, Ordering::AcqRel) {
                                return;
                            }
                            RECORDING_SHORTCUT_LAST_TRIGGER_MS.store(now_ms, Ordering::Relaxed);
                            let app_handle_inner = app_handle_clone_recording.clone();
                            let state_inner = state_clone_recording.clone();
                            tauri::async_runtime::spawn(async move {
                                toggle_recording_from_shortcut(app_handle_inner, state_inner).await;
                                RECORDING_SHORTCUT_IN_FLIGHT.store(false, Ordering::Release);
                            });
                        }
                    })
                {
                    log::warn!("录屏快捷键 '{}' 注册失败: {}", recording_hot_key, e);
                    shortcut_conflicts.push(format!("录屏：{}", recording_hot_key));
                }
            }

            let app_handle_clone_longshot_finish = app_handle.clone();
            if let Err(e) = app
                .global_shortcut()
                .on_shortcut("Ctrl+Alt+Enter", move |_app, _shortcut, event| {
                    if let ShortcutState::Pressed = event.state {
                        let app_handle_inner = app_handle_clone_longshot_finish.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = crate::ui::commands::finish_manual_longshot_from_shortcut(
                                app_handle_inner,
                            )
                            .await;
                        });
                    }
                })
            {
                log::warn!("长截图完成快捷键注册失败: {}", e);
            }

            let app_handle_clone_longshot_cancel = app_handle.clone();
            if let Err(e) = app
                .global_shortcut()
                .on_shortcut("Ctrl+Alt+Backspace", move |_app, _shortcut, event| {
                    if let ShortcutState::Pressed = event.state {
                        let app_handle_inner = app_handle_clone_longshot_cancel.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = crate::ui::commands::cancel_manual_longshot_from_shortcut(
                                app_handle_inner,
                            )
                            .await;
                        });
                    }
                })
            {
                log::warn!("长截图取消快捷键注册失败: {}", e);
            }

            let app_handle_clone_longshot_pause = app_handle.clone();
            if let Err(e) = app
                .global_shortcut()
                .on_shortcut("Ctrl+Alt+P", move |_app, _shortcut, event| {
                    if let ShortcutState::Pressed = event.state {
                        let app_handle_inner = app_handle_clone_longshot_pause.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = crate::ui::commands::toggle_manual_longshot_pause_from_shortcut(
                                app_handle_inner,
                            )
                            .await;
                        });
                    }
                })
            {
                log::warn!("长截图暂停/恢复快捷键注册失败: {}", e);
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

            set_clipboard_listener_enabled(
                app_handle.clone(),
                state_arc.clone(),
                text_clipboard_enabled,
            );
            set_image_clipboard_listener_enabled(
                app_handle.clone(),
                state_arc.clone(),
                image_clipboard_enabled,
            );

            // 初始化异步预览生成器，支持事件通知
            crate::utils::image_clipboard::init_preview_generator_with_app_handle(app_handle.clone());

            emit_image_history_payload(app_handle, state_arc.clone());

            #[cfg(windows)]
            if {
                let guard = lock_state(&state_arc);
                guard.settings.selection_enabled
            } {
                start_text_selection_listener(app_handle.clone(), state_arc.clone());
            }

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
            start_image_preview_window_drag,
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
            count_import_image_files,
            import_image_files,
            get_clipboard_bottom_offset,
            preview_clipboard_bottom_offset,
            save_clipboard_bottom_offset,
            window_blur,
            image_window_blur,
            selection_toolbar_blur,
            open_settings_window,
            show_selection_toolbar_with_text,
            show_ocr_text_window,
            copy_text,
            copy_and_paste_text,
            get_ai_settings,
            #[cfg(debug_assertions)]
            get_text_dedup_metrics,
            #[cfg(debug_assertions)]
            get_image_storage_metrics,
            #[cfg(debug_assertions)]
            get_image_persist_queue_metrics,
            #[cfg(debug_assertions)]
            get_copy_paste_dedup_debug_state,
            #[cfg(debug_assertions)]
            set_copy_paste_dedup_debug_config,
            save_app_settings,
            test_ai_connection,
            stream_translate_text,
            stream_explain_text,
            get_provider_config,
            remove_ai_provider,
            get_all_configured_providers,
            get_image_preview_by_id,
            check_previews_ready,
            copy_image_clipboard_item_to_directory,
            get_clipboard_full_snapshot,
            // 截图相关命令
            start_screenshot,
            start_manual_longshot,
            pause_manual_longshot,
            resume_manual_longshot,
            cancel_manual_longshot,
            finish_manual_longshot,
            get_manual_longshot_status,
            recognize_image_ocr,
            capture_region,
            save_screenshot,
            pin_screenshot_on_screen,
            close_pinned_image_window,
            get_pinned_image_window_position,
            move_pinned_image_window,
            get_screen_size,
            set_screenshot_clipboard_link_once,
            set_screenshot_input_passthrough,
            set_screenshot_window_visible,
            show_longshot_toolbar,
            hide_longshot_toolbar,
            show_longshot_border,
            hide_longshot_border,
            longshot_toolbar_action,
            snap_longshot_toolbar_window,
            open_screenshot_editor,
            notify_recording_region_selected,
            get_window_list,
            close_screenshot_window,
            start_recording,
            pause_recording,
            resume_recording,
            update_recording_audio_capture,
            stop_recording,
            cancel_recording,
            get_recording_state,
            get_recording_output_dir,
            list_recording_audio_devices,
            list_recording_system_output_devices,
            list_recording_audio_processes,
            open_recording_folder,
            run_recording_regression,
            show_recording_toolbar,
            resize_recording_toolbar,
            check_recording_ffmpeg,
            download_recording_ffmpeg,
        ])
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::Builder::new().build());

    // 仅调试模式启用日志插件；发布版不注册，避免任何日志落盘
    #[cfg(debug_assertions)]
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
