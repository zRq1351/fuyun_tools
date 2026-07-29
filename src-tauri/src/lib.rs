pub mod core;
pub mod features;
pub mod services;
pub mod sync;
pub mod ui;
pub mod utils;

use crate::core::app_state::AppState;
use crate::core::error::install_global_panic_hook;
use crate::services::ai_services::{stream_custom_prompt_text, stream_explain_text, stream_translate_text};
use crate::services::clipboard_manager::set_clipboard_listener_enabled;
use crate::services::image_clipboard_manager::{
    emit_image_history_payload, set_image_clipboard_listener_enabled,
};
use crate::sync::{lock_arc_mutex, Mutex};
use crate::ui::commands::*;
use crate::ui::commands_backup::*;
use crate::ui::commands_clipboard::*;
use crate::ui::commands_diagnostic::*;
use crate::ui::commands_document::*;
use crate::ui::commands_launcher::{
    add_custom_command, add_launcher_category, add_manual_app, batch_extract_icons,
    get_all_apps, get_launcher_config, hide_launcher, launch_app, launch_app_with_args,
    open_app_directory, open_file, remove_app_record,
    remove_custom_command, remove_launcher_category, rename_launcher_category, reorder_categories,
    scan_and_save_apps, search_launcher_items, set_app_category,
    set_launcher_view_mode, show_launcher, toggle_custom_command, toggle_launcher,
    update_app_sort_orders, update_category_icon, update_custom_command,
};
use crate::ui::commands_recording::{
    cancel_recording, check_recording_ffmpeg, download_recording_ffmpeg, get_recording_output_dir,
    get_recording_state, list_recording_audio_devices, list_recording_audio_processes,
    list_recording_system_output_devices, open_recording_folder, pause_recording,
    resize_recording_toolbar, resume_recording, run_recording_regression, show_recording_toolbar,
    start_recording, stop_recording, toggle_microphone_from_shortcut,
    toggle_recording_from_shortcut, update_recording_audio_capture,
};
use crate::ui::commands_screenshot::*;
use crate::ui::commands_vc_runtime::*;
use crate::ui::tray_menu::rebuild_tray_menu;
use crate::ui::window_manager::{
    bind_overlay_window_events, bind_standard_window_close_to_hide,
    show_clipboard_window, show_doc_manager_widget_window,
    show_image_clipboard_window, show_standard_window_by_label,
};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

static RECORDING_SHORTCUT_IN_FLIGHT: AtomicBool = AtomicBool::new(false);
static RECORDING_SHORTCUT_LAST_TRIGGER_MS: AtomicU64 = AtomicU64::new(0);
const RECORDING_SHORTCUT_MIN_INTERVAL_MS: u64 = 300;
static BACKUP_SCHEDULER_STOP: AtomicBool = AtomicBool::new(false);

fn now_unix_ms_u64() -> u64 {
    crate::utils::utils_helpers::now_unix_ms_u64()
}

fn start_auto_backup_scheduler(app_handle: AppHandle, state: Arc<Mutex<AppState>>) {
    thread::spawn(move || {
        while !BACKUP_SCHEDULER_STOP.load(Ordering::Acquire) {
            match tauri::async_runtime::block_on(crate::ui::commands_backup::run_auto_backup_tick(
                state.clone(),
            )) {
                Ok(true) => {
                    if let Err(e) = app_handle.emit(
                        "backup-run-updated",
                        serde_json::json!({
                            "status": "success",
                        }),
                    ) {
                        log::warn!("发送自动备份成功事件失败: {}", e);
                    }
                }
                Ok(false) => {}
                Err(error) => {
                    log::warn!("自动备份执行失败: {}", error);
                    if let Err(e) = app_handle.emit(
                        "backup-run-updated",
                        serde_json::json!({
                            "status": "failed",
                            "message": error,
                        }),
                    ) {
                        log::warn!("发送自动备份失败事件失败: {}", e);
                    }
                }
            }
            // 分段睡眠以便及时响应停止信号
            for _ in 0..60 {
                if BACKUP_SCHEDULER_STOP.load(Ordering::Relaxed) {
                    break;
                }
                thread::sleep(std::time::Duration::from_secs(5));
            }
        }
        log::info!("自动备份调度器已停止");
    });
}

/// 启动划词选择监听器
pub fn start_text_selection_listener(app_handle: AppHandle, state: Arc<Mutex<AppState>>) {
    let selection_enabled = {
        let state_guard = lock_arc_mutex(&state);
        state_guard.settings.selection_enabled
    };

    features::mouse_listener::set_selection_listener_enabled(app_handle, state, selection_enabled);
}

/// 清理启动时遗留的截图临时文件
/// Bug修复 (B15): 防止异常退出后临时文件累积
fn cleanup_stale_screenshot_boot_files() {
    let mut dir = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return,
    };
    dir.pop();
    dir.push("screenshot_boot");
    if !dir.exists() {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(&dir) {
        let mut count = 0usize;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with("screenshot_boot_") && name.ends_with(".png") {
                        let _ = std::fs::remove_file(&path);
                        count += 1;
                    }
                }
            }
        }
        if count > 0 {
            log::info!("启动清理: 删除 {} 个遗留截图临时文件", count);
        }
    }
}

/// 清理启动时遗留的录屏临时文件
/// 防止崩溃后 .tmp.mp4 / .sys.* / .mic.* 文件累积
fn cleanup_stale_recording_tmp_files() {
    let mut dir = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return,
    };
    dir.pop();
    let recordings_dir = dir.join("recordings");
    if !recordings_dir.exists() {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(&recordings_dir) {
        let mut count = 0usize;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };
            let is_tmp = name.ends_with(".tmp.mp4")
                || name.contains(".sys.")
                || name.contains(".mic.");
            if is_tmp {
                let _ = std::fs::remove_file(&path);
                count += 1;
            }
        }
        if count > 0 {
            log::info!("启动清理: 删除 {} 个遗留录屏临时文件", count);
        }
    }
}

/// 运行Tauri应用程序
pub fn run() {
    install_global_panic_hook();
    // 启动时清理遗留临时文件（后台线程，不阻塞启动流程）
    std::thread::spawn(|| {
        cleanup_stale_screenshot_boot_files();
        cleanup_stale_recording_tmp_files();
    });
    let initial_state = AppState::default();
    let state_arc = Arc::new(Mutex::new(initial_state));

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(state_arc.clone())
        .setup(move |app| {
            if let Some(settings_window) = app.get_webview_window("settings") {
                bind_standard_window_close_to_hide(&settings_window);
            }
            if let Some(recording_window) = app.get_webview_window("recording_toolbar") {
                bind_overlay_window_events(
                    &recording_window,
                    app.handle().clone(),
                    "recording_toolbar",
                );
            }

            let app_handle = app.handle();
            rebuild_tray_menu(app_handle, state_arc.clone());
            start_auto_backup_scheduler(app_handle.clone(), state_arc.clone());
            let state_clone = state_arc.clone();
            let app_handle_clone = app_handle.clone();
            // 优化：一次性批量获取所有需要的设置字段，减少锁获取次数
            let (
                hot_key,
                image_hot_key,
                screenshot_hot_key,
                recording_hot_key,
                recording_mic_toggle_hot_key,
                text_clipboard_enabled,
                image_clipboard_enabled,
                screenshot_enabled,
                recording_enabled,
                launcher_enabled,
                doc_manager_enabled,
                doc_manager_widget_enabled,
            ) = {
                let guard = lock_arc_mutex(&state_arc);
                (
                    guard.settings.hot_key.clone(),
                    guard.settings.image_hot_key.clone(),
                    guard.settings.screenshot_hot_key.clone(),
                    guard.settings.recording_hot_key.clone(),
                    guard.settings.recording_mic_toggle_hot_key.clone(),
                    guard.settings.text_clipboard_enabled,
                    guard.settings.image_clipboard_enabled,
                    guard.settings.screenshot_enabled,
                    guard.settings.recording_enabled,
                    guard.settings.launcher_enabled,
                    guard.settings.doc_manager_enabled,
                    guard.settings.doc_manager_widget_enabled,
                )
            };
            let mut shortcut_conflicts: Vec<String> = Vec::new();
            if text_clipboard_enabled {
                if let Err(e) = app.global_shortcut().on_shortcut(
                    hot_key.as_str(),
                    move |_app, _shortcut, event| {
                        if let ShortcutState::Pressed = event.state {
                            let state_guard = lock_arc_mutex(&state_clone);
                            if !state_guard.settings.text_clipboard_enabled {
                                return;
                            }
                            if state_guard.is_visible {
                                drop(state_guard);
                                if let Err(e) = crate::ui::window_manager::hide_overlay_window_by_label(&app_handle_clone, "clipboard") {
                                    log::warn!("隐藏文字剪贴板窗口失败: {}", e);
                                }
                                return;
                            }
                            let is_image_visible = state_guard.is_image_visible;
                            drop(state_guard);
                            if is_image_visible {
                                if let Err(e) = crate::ui::window_manager::hide_overlay_window_by_label(&app_handle_clone, "image_clipboard") {
                                    log::warn!("隐藏图片剪贴板窗口失败: {}", e);
                                }
                            }
                            crate::ui::commands_writeback::interrupt_text_fill_flow(&state_clone);
                            show_clipboard_window(
                                app_handle_clone.clone(),
                                state_clone.clone(),
                            );
                            features::mouse_listener::reset_ctrl_key_state();
                        }
                    },
                ) {
                    log::warn!("文字窗口快捷键 '{}' 注册失败: {}", hot_key, e);
                    shortcut_conflicts.push(format!("文字窗口：{}", hot_key));
                }
            }

            let state_clone_image = state_arc.clone();
            let app_handle_clone_image = app_handle.clone();
            if image_clipboard_enabled {
                if let Err(e) = app.global_shortcut().on_shortcut(
                    image_hot_key.as_str(),
                    move |_app, _shortcut, event| {
                        if let ShortcutState::Pressed = event.state {
                            let state_guard = lock_arc_mutex(&state_clone_image);
                            if !state_guard.settings.image_clipboard_enabled {
                                return;
                            }
                            if state_guard.is_image_visible {
                                drop(state_guard);
                                if let Err(e) = crate::ui::window_manager::hide_overlay_window_by_label(&app_handle_clone_image, "image_clipboard") {
                                    log::warn!("隐藏图片剪贴板窗口失败: {}", e);
                                }
                                return;
                            }
                            let is_text_visible = state_guard.is_visible;
                            drop(state_guard);
                            if is_text_visible {
                                if let Err(e) = crate::ui::window_manager::hide_overlay_window_by_label(&app_handle_clone_image, "clipboard") {
                                    log::warn!("隐藏文字剪贴板窗口失败: {}", e);
                                }
                            }
                            crate::ui::commands_writeback::interrupt_image_fill_flow(&state_clone_image);
                            show_image_clipboard_window(
                                app_handle_clone_image.clone(),
                                state_clone_image.clone(),
                            );
                        }
                    },
                ) {
                    log::warn!("图片窗口快捷键 '{}' 注册失败: {}", image_hot_key, e);
                    shortcut_conflicts.push(format!("图片窗口：{}", image_hot_key));
                }
            }

            let app_handle_clone_screenshot = app_handle.clone();
            let state_clone_screenshot = state_arc.clone();
            if screenshot_enabled {
                if let Err(e) = app.global_shortcut().on_shortcut(
                    screenshot_hot_key.as_str(),
                    move |_app, _shortcut, event| {
                        if let ShortcutState::Pressed = event.state {
                            let state_guard = lock_arc_mutex(&state_clone_screenshot);
                            if !state_guard.settings.screenshot_enabled {
                                return;
                            }
                            drop(state_guard);
                            let app_handle_inner = app_handle_clone_screenshot.clone();
                            tauri::async_runtime::spawn(async move {
                                if let Err(e) = crate::ui::commands_screenshot::open_screenshot_editor(
                                    app_handle_inner,
                                    None,
                                )
                                .await
                                {
                                    log::error!("截图失败: {}", e);
                                }
                            });
                        }
                    },
                ) {
                    log::warn!("截图快捷键 '{}' 注册失败: {}", screenshot_hot_key, e);
                    shortcut_conflicts.push(format!("截图：{}", screenshot_hot_key));
                }
            }

            let app_handle_clone_recording = app_handle.clone();
            if recording_enabled {
                if let Err(e) = app.global_shortcut().on_shortcut(
                    recording_hot_key.as_str(),
                    move |_app, _shortcut, event| {
                        if let ShortcutState::Pressed = event.state {
                            let now_ms = now_unix_ms_u64();
                            let last_ms =
                                RECORDING_SHORTCUT_LAST_TRIGGER_MS.load(Ordering::Relaxed);
                            if last_ms > 0
                                && now_ms.saturating_sub(last_ms)
                                    < RECORDING_SHORTCUT_MIN_INTERVAL_MS
                            {
                                return;
                            }
                            if RECORDING_SHORTCUT_IN_FLIGHT.swap(true, Ordering::AcqRel) {
                                return;
                            }
                            RECORDING_SHORTCUT_LAST_TRIGGER_MS.store(now_ms, Ordering::Release);
                            let app_handle_inner = app_handle_clone_recording.clone();
                            tauri::async_runtime::spawn(async move {
                                toggle_recording_from_shortcut(app_handle_inner).await;
                                RECORDING_SHORTCUT_IN_FLIGHT.store(false, Ordering::Release);
                            });
                        }
                    },
                ) {
                    log::warn!("录屏快捷键 '{}' 注册失败: {}", recording_hot_key, e);
                    shortcut_conflicts.push(format!("录屏：{}", recording_hot_key));
                }
            }

            // 注册麦克风快捷键（按住开启，松开关闭）
            let app_handle_clone_mic = app_handle.clone();
            if recording_enabled {
                if let Err(e) = app.global_shortcut().on_shortcut(
                    recording_mic_toggle_hot_key.as_str(),
                    move |_app, _shortcut, event| {
                        let app_handle_inner = app_handle_clone_mic.clone();
                        match event.state {
                            ShortcutState::Pressed => {
                                // 按下快捷键：开启麦克风
                                tauri::async_runtime::spawn(async move {
                                    toggle_microphone_from_shortcut(app_handle_inner, true).await;
                                });
                            }
                            ShortcutState::Released => {
                                // 松开快捷键：关闭麦克风
                                tauri::async_runtime::spawn(async move {
                                    toggle_microphone_from_shortcut(app_handle_inner, false).await;
                                });
                            }
                        }
                    },
                ) {
                    log::warn!(
                        "麦克风切换快捷键 '{}' 注册失败: {}",
                        recording_mic_toggle_hot_key,
                        e
                    );
                    shortcut_conflicts.push(format!("麦克风切换：{}", recording_mic_toggle_hot_key));
                }
            }

            // 注册启动器快捷键
            if launcher_enabled {
                let app_handle_clone_launcher = app_handle.clone();
                let launcher_hot_key = {
                    let guard = lock_arc_mutex(&state_arc);
                    guard.settings.launcher_hot_key.clone()
                };
                if let Err(e) = app.global_shortcut().on_shortcut(
                    launcher_hot_key.as_str(),
                    move |_app, _shortcut, event| {
                        if let ShortcutState::Pressed = event.state {
                            let app_handle_inner = app_handle_clone_launcher.clone();
                            tauri::async_runtime::spawn(async move {
                                if let Err(e) = toggle_launcher(app_handle_inner).await {
                                    log::error!("切换启动器失败: {}", e);
                                }
                            });
                        }
                    },
                ) {
                    log::warn!("启动器快捷键 '{}' 注册失败: {}", launcher_hot_key, e);
                    shortcut_conflicts.push(format!("启动器：{}", launcher_hot_key));
                }
            }

            if doc_manager_enabled {
                let app_handle_clone_doc = app_handle.clone();
                let doc_manager_hot_key_str = {
                    let guard = lock_arc_mutex(&state_arc);
                    guard.settings.doc_manager_hot_key.clone()
                };
                if let Err(e) = app.global_shortcut().on_shortcut(
                    doc_manager_hot_key_str.as_str(),
                move |_app, _shortcut, event| {
                    if let ShortcutState::Pressed = event.state {
                        let app_handle_inner = app_handle_clone_doc.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Err(e) = crate::ui::window_manager::show_standard_window_by_label(
                                &app_handle_inner,
                                "document_manager",
                            ) {
                                log::error!("显示文档管理器窗口失败: {}", e);
                            }
                        });
                    }
                },
                ) {
                    log::warn!("文档管理快捷键 '{}' 注册失败: {}", doc_manager_hot_key_str, e);
                    shortcut_conflicts.push(format!("文档管理：{}", doc_manager_hot_key_str));
                }
            }

            if !shortcut_conflicts.is_empty() {
                let payload = serde_json::json!({
                    "conflicts": shortcut_conflicts.clone()
                });
                if let Err(e) = app_handle.emit("shortcut-conflict-warning", payload.clone()) {
                    log::error!("发送快捷键冲突警告失败: {}", e);
                }

                if let Some(settings_window) = app.get_webview_window("settings") {
                    if let Err(e) = show_standard_window_by_label(&app.handle().clone(), "settings") {
                        log::error!("显示设置窗口失败: {}", e);
                    }
                    let script = format!("window.__SHORTCUT_CONFLICT__ = {};", payload);
                    if let Err(e) = settings_window.eval(&script) {
                        log::error!("注入快捷键冲突脚本失败: {}", e);
                    }
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
            crate::utils::image_clipboard::init_preview_generator_with_app_handle(
                app_handle.clone(),
            );

            emit_image_history_payload(app_handle, state_arc.clone());

            #[cfg(windows)]
            if {
                let guard = lock_arc_mutex(&state_arc);
                guard.settings.selection_enabled
            } {
                start_text_selection_listener(app_handle.clone(), state_arc.clone());
            }

            if text_clipboard_enabled {
                // 窗口由 show_clipboard_window 懒创建（首次快捷键调用时）
            }
            if image_clipboard_enabled {
                // 窗口由 show_image_clipboard_window 懒创建
            }
            // screenshot/longshot 窗口由 open_screenshot_editor 懒创建
            // recording_toolbar 窗口由 show_recording_toolbar 懒创建
            // launcher 窗口由 show_launcher 懒创建
            // document_manager 窗口由 show_standard_window 懒创建

            if doc_manager_widget_enabled && doc_manager_enabled {
                if let Err(e) = show_doc_manager_widget_window(&app_handle) {
                    log::error!("显示文档管理小部件失败: {}", e);
                }
            }

            tauri::async_runtime::spawn(async {
                crate::utils::ai_store::init_db().await;
                crate::utils::ai_store::migrate_from_old().await;
                log::info!("AI 数据存储初始化完成");
            });

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
            open_text_preview_window,
            close_text_preview_window,
            start_text_preview_window_drag,
            update_text_item,
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
            recognize_image_ocr,
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
            check_vc_runtime_dependencies,
            download_vc_runtime_installer,
            open_vc_runtime_installer,
            install_vc_runtime_and_wait,
            #[cfg(debug_assertions)]
            get_vc_runtime_debug_state,
            #[cfg(debug_assertions)]
            set_vc_runtime_debug_config,
            resize_selection_toolbar,
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
            stream_custom_prompt_text,
            get_provider_config,
            remove_ai_provider,
            get_all_configured_providers,
            preview_backup_export,
            export_backup_to_path,
            preview_backup_package,
            restore_backup_package,
            list_backup_history,
            delete_backup_history_item,
            run_manual_backup,
            get_backup_settings,
            save_backup_settings,
            get_diagnostic_overview,
            get_diagnostic_items,
            run_diagnostic_action,
            get_image_preview_by_id,
            check_previews_ready,
            copy_image_clipboard_item_to_directory,
            get_clipboard_full_snapshot,
            // 截图相关命令
            start_screenshot,
            get_screenshot_data,
            start_manual_longshot,
            pause_manual_longshot,
            resume_manual_longshot,
            cancel_manual_longshot,
            finish_manual_longshot,
            get_manual_longshot_status,
            get_manual_longshot_availability,
            recognize_image_ocr,
            capture_region,
            choose_screenshot_save_path,
            export_screenshot_to_path,
            render_screenshot_to_png_data,
            copy_screenshot_to_clipboard,
            save_screenshot,
            save_screenshot_to_path,
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
            // 启动器命令
            search_launcher_items,
            get_all_apps,
            scan_and_save_apps,
            batch_extract_icons,
            get_launcher_config,
            add_launcher_category,
            remove_launcher_category,
            rename_launcher_category,
            set_app_category,
            set_launcher_view_mode,
            remove_app_record,
            update_app_sort_orders,
            reorder_categories,
            update_category_icon,
            // 自定义命令管理
            add_custom_command,
            remove_custom_command,
            update_custom_command,
            toggle_custom_command,
            launch_app,
            launch_app_with_args,
            open_file,
            open_app_directory,
            add_manual_app,
            show_launcher,
            hide_launcher,
            toggle_launcher,
            show_standard_window_command,
            notify_result_window_ready,
            show_clipboard_window_command,
            start_screenshot_command,
            toggle_recording_command,
            add_doc_root,
            get_doc_roots,
            remove_doc_root,
            add_doc_category,
            get_doc_categories,
            remove_doc_category,
            rename_doc_category,
            reorder_doc_categories,
            reorder_doc_roots,
            reorder_doc_files,
            import_files,
            get_doc_page,
            update_doc_meta,
            delete_doc,
            move_doc,
            get_doc_stats,
            open_doc,
            open_doc_folder,
            get_doc_detail,
            scan_folder,
            get_import_history,
            undo_import,
            undo_import_item,
            get_import_files,
            detect_orphan_files,
            show_document_manager,
            show_doc_manager_widget,
            hide_doc_manager_widget,
            get_file_type_icon,
            get_file_type_icons,
            // 性能监控命令
            get_system_resources,
            get_perf_summary,
            get_metrics_by_category,
            get_startup_metrics,
            get_memory_metrics,
            get_ipc_metrics,
            // 主题命令
            get_theme,
            set_theme,
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
            app.run(move |app_handle, event| {
                match event {
                    tauri::RunEvent::ExitRequested { api, .. } => {
                        // Signal background threads to stop
                        BACKUP_SCHEDULER_STOP.store(true, Ordering::Release);
                        // 清理可能残留的长截图 FFmpeg 子进程，防止孤儿进程
                        crate::features::screenshot::longshot::kill_active_ffmpeg_child();
                        // Prevent immediate exit if recording is active
                        let has_active_recording = {
                            if let Some(state) = app_handle.try_state::<Arc<Mutex<AppState>>>() {
                                let guard = lock_arc_mutex(&state);
                                let rt = lock_arc_mutex(&guard.recording_runtime);
                                rt.phase == crate::features::recording::state::RecordingPhase::Recording
                                    || rt.phase == crate::features::recording::state::RecordingPhase::Paused
                            } else {
                                false
                            }
                        };
                        if has_active_recording {
                            log::warn!("录屏进行中，阻止应用退出");
                            api.prevent_exit();
                        }
                    }
                    tauri::RunEvent::Exit => {
                        BACKUP_SCHEDULER_STOP.store(true, Ordering::Release);
                        crate::services::clipboard_wakeup::stop_wake_dispatcher();
                        crate::features::screenshot::longshot::kill_active_ffmpeg_child();
                        log::info!("应用退出，已发送后台线程停止信号");
                    }
                    _ => {}
                }
            });
        }
        Err(e) => {
            log::error!("构建Tauri应用失败: {}", e);
        }
    }
}
