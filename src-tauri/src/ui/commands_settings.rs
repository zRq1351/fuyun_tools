//! 设置与 AI 相关命令：设置持久化（含各功能开关联动）、AI 设置/连接测试/提供方管理。
//!
//! 从 `ui/commands.rs` 机械拆出；`save_app_settings` 历史较长（含快捷键/托盘/窗口联动），
//! 内部若再拆建议按设置域做纯函数化并补测试后再动。

use crate::core::app_state::AppState as SharedAppState;
use crate::core::error_codes::AppErrorKind;
use crate::features;
use crate::services::ai_client::{AIClient, AIConfig};
use crate::services::ai_services::invalidate_ai_client_cache;
use crate::services::clipboard_manager::set_clipboard_listener_enabled;
use crate::services::image_clipboard_manager::set_image_clipboard_listener_enabled;
use crate::sync::{lock_arc_mutex, Mutex};
use crate::ui::commands_clipboard::*;
use crate::ui::commands_recording::toggle_microphone_from_shortcut;
use crate::ui::commands_screenshot::close_screenshot_window;
use crate::ui::window_manager::{
    destroy_window_by_label, ensure_window_for_label,
};
use crate::utils::image_clipboard::set_image_fill_verify_mode;
use crate::utils::utils_helpers::{
    default_explanation_prompt_template, default_translation_prompt_template,
    save_settings,
};
use std::sync::Arc;
use std::sync::OnceLock;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use crate::ui::commands::{register_recording_shortcut, register_mic_toggle_shortcut};

static SETTINGS_SAVE_MUTEX: OnceLock<tauri::async_runtime::Mutex<()>> = OnceLock::new();


#[tauri::command]
pub async fn get_ai_settings(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<serde_json::Value, String> {
    let settings = {
        let state_guard = lock_arc_mutex(state.inner());
        state_guard.settings.clone()
    };
    let mut settings_json = serde_json::to_value(&settings)
        .map_err(|e| frontend_error_kind(AppErrorKind::JsonError, e.to_string()))?;

    let providers = crate::utils::ai_store::get_all_providers().await;
    let mut provider_configs_map = serde_json::Map::new();
    for (key, cfg) in &providers {
        // 只回掩码，避免明文密钥进入 webview；保存路径已识别 "********" 为不更新
        let masked_key = if cfg.api_key.is_empty() {
            String::new()
        } else {
            "********".to_string()
        };
        provider_configs_map.insert(
            key.clone(),
            serde_json::json!({
                "api_url": cfg.api_url,
                "model_name": cfg.model_name,
                "api_key": masked_key,
                "api_key_set": !cfg.api_key.is_empty(),
            }),
        );
    }

    if let Some(obj) = settings_json.as_object_mut() {
        obj.insert(
            "ai_provider".to_string(),
            serde_json::Value::String(crate::utils::ai_store::get_current_provider().await),
        );
        obj.insert(
            "provider_configs".to_string(),
            serde_json::Value::Object(provider_configs_map),
        );
    }

    Ok(settings_json)
}
pub(crate) fn effective_key(new: &Option<String>, fallback: &str) -> String {
    match new {
        Some(val) => val.clone(),
        None => fallback.to_string(),
    }
}
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn save_app_settings(
    text_max_items: Option<usize>,
    image_max_items: Option<usize>,
    image_disk_limit_mb: Option<u64>,
    ai_provider: Option<String>,
    ai_api_url: Option<String>,
    ai_model_name: Option<String>,
    ai_api_key: Option<String>,
    hot_key: Option<String>,
    image_hot_key: Option<String>,
    screenshot_hot_key: Option<String>,
    recording_hot_key: Option<String>,
    recording_mic_toggle_hot_key: Option<String>,
    launcher_hot_key: Option<String>,
    text_clipboard_enabled: Option<bool>,
    image_clipboard_enabled: Option<bool>,
    screenshot_enabled: Option<bool>,
    recording_enabled: Option<bool>,
    launcher_enabled: Option<bool>,
    doc_manager_hot_key: Option<String>,
    doc_manager_enabled: Option<bool>,
    doc_manager_widget_enabled: Option<bool>,
    selection_enabled: Option<bool>,
    selection_modifier_key: Option<String>,
    selection_custom_prompts: Option<Vec<crate::utils::settings_model::CustomPrompt>>,
    selection_web_search_enabled: Option<bool>,
    selection_web_search_engine: Option<String>,
    grouped_items_protected_from_limit: Option<bool>,
    logging_enabled: Option<bool>,
    translation_prompt_template: Option<String>,
    explanation_prompt_template: Option<String>,
    image_fill_verify_mode: Option<String>,
    ocr_engine: Option<String>,
    recording_default_fps: Option<u32>,
    recording_default_video_bitrate_kbps: Option<u32>,
    recording_default_audio_bitrate_kbps: Option<u32>,
    recording_quality_preset: Option<String>,
    recording_capture_cursor: Option<bool>,
    recording_capture_system_audio: Option<bool>,
    recording_capture_microphone: Option<bool>,
    recording_microphone_device_id: Option<String>,
    recording_system_audio_device_id: Option<String>,
    recording_output_dir: Option<String>,
    recording_auto_open_folder: Option<bool>,
    recording_toolbar_content_protected: Option<bool>,
    recording_max_duration_minutes: Option<u32>,
    recording_file_name_template: Option<String>,
    recording_ffmpeg_download_url: Option<String>,
    recording_window_audio_sync_advance_ms: Option<u32>,
    dev_force_ffmpeg_window_capture: Option<bool>,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let version = app.package_info().version.to_string();

    // 防止并发保存设置导致数据丢失
    let _settings_guard = SETTINGS_SAVE_MUTEX
        .get_or_init(|| tauri::async_runtime::Mutex::new(()))
        .lock()
        .await;

    // 注意：此处克隆后释放锁，修改期间（~800行）并发命令读取到的是旧值。
    // 这是一个已知的设计限制，完整修复需要重构为 write-through 模式。
    let mut settings = {
        let state_guard = lock_arc_mutex(state.inner());
        state_guard.settings.clone()
    };

    settings.version = version;

    if let Some(val) = text_max_items {
        settings.max_items = val;
        settings.text_max_items = val;
    }
    if let Some(val) = image_max_items {
        settings.image_max_items = val;
    }
    if let Some(val) = image_disk_limit_mb {
        settings.image_disk_limit_mb = val;
    }
    if let Some(val) = text_clipboard_enabled {
        settings.text_clipboard_enabled = val;
    }
    if let Some(val) = image_clipboard_enabled {
        settings.image_clipboard_enabled = val;
    }
    if let Some(val) = screenshot_enabled {
        settings.screenshot_enabled = val;
    }
    if let Some(val) = recording_enabled {
        settings.recording_enabled = val;
    }
    if let Some(val) = launcher_enabled {
        settings.launcher_enabled = val;
    }
    if let Some(val) = doc_manager_enabled {
        settings.doc_manager_enabled = val;
    }

    if let Some(enabled) = doc_manager_enabled {
        if enabled {
            if !app
                .global_shortcut()
                .is_registered(settings.doc_manager_hot_key.as_str())
            {
                let app_handle_for_doc = app.clone();
                if let Err(e) = app.global_shortcut().on_shortcut(
                    settings.doc_manager_hot_key.as_str(),
                    move |_app, _shortcut, event| {
                        if let ShortcutState::Pressed = event.state {
                            let _ = crate::ui::window_manager::show_standard_window_by_label(
                                &app_handle_for_doc,
                                "document_manager",
                            );
                        }
                    },
                ) {
                    log::warn!(
                        "注册文档管理快捷键 '{}' 失败: {}",
                        settings.doc_manager_hot_key,
                        e
                    );
                }
            }
        } else if let Err(e) = app
            .global_shortcut()
            .unregister(settings.doc_manager_hot_key.as_str())
        {
            log::warn!(
                "注销文档管理快捷键 '{}' 失败: {}",
                settings.doc_manager_hot_key,
                e
            );
        }
    }
    if let Some(val) = doc_manager_widget_enabled {
        let old = settings.doc_manager_widget_enabled;
        settings.doc_manager_widget_enabled = val;
        if val && !old && settings.doc_manager_enabled {
            let _ = crate::ui::window_manager::show_doc_manager_widget_window(&app);
        } else if !val && old {
            let _ = crate::ui::window_manager::hide_doc_manager_widget_window(&app);
        }
    }
    if let Some(val) = selection_enabled {
        settings.selection_enabled = val;
    }
    if let Some(val) = selection_modifier_key {
        settings.selection_modifier_key = val;
    }
    if let Some(val) = selection_custom_prompts {
        settings.selection_custom_prompts = val;
    }
    if let Some(val) = selection_web_search_enabled {
        settings.selection_web_search_enabled = val;
    }
    if let Some(val) = selection_web_search_engine {
        settings.selection_web_search_engine = val;
    }
    if let Some(val) = grouped_items_protected_from_limit {
        settings.grouped_items_protected_from_limit = val;
    }
    if let Some(val) = logging_enabled {
        settings.logging_enabled = val;
    }
    if let Some(val) = translation_prompt_template {
        settings.translation_prompt_template = if val.trim().is_empty() {
            default_translation_prompt_template()
        } else {
            val
        };
    }
    if let Some(val) = explanation_prompt_template {
        settings.explanation_prompt_template = if val.trim().is_empty() {
            default_explanation_prompt_template()
        } else {
            val
        };
    }
    if let Some(val) = image_fill_verify_mode {
        settings.image_fill_verify_mode = if val == "fast" {
            "fast".to_string()
        } else {
            "strict".to_string()
        };
    }
    if let Some(val) = ocr_engine {
        settings.ocr_engine = if val == "windows-native" {
            "windows-native".to_string()
        } else {
            "ocr-rs".to_string()
        };
    }
    if let Some(val) = recording_default_fps {
        settings.recording_default_fps = val.clamp(1, 120);
    }
    if let Some(val) = recording_default_video_bitrate_kbps {
        settings.recording_default_video_bitrate_kbps = val.clamp(500, 50000);
    }
    if let Some(val) = recording_default_audio_bitrate_kbps {
        settings.recording_default_audio_bitrate_kbps = val.clamp(32, 512);
    }
    if let Some(val) = recording_quality_preset {
        settings.recording_quality_preset = val.trim().to_string();
    }
    if let Some(val) = recording_capture_cursor {
        settings.recording_capture_cursor = val;
    }
    if let Some(val) = recording_capture_system_audio {
        settings.recording_capture_system_audio = val;
    }
    if let Some(val) = recording_capture_microphone {
        settings.recording_capture_microphone = val;
    }
    if let Some(val) = recording_microphone_device_id {
        settings.recording_microphone_device_id = val.trim().to_string();
    }
    if let Some(val) = recording_system_audio_device_id {
        settings.recording_system_audio_device_id = val.trim().to_string();
    }
    if let Some(val) = recording_output_dir {
        settings.recording_output_dir = val.trim().to_string();
    }
    if let Some(val) = recording_auto_open_folder {
        settings.recording_auto_open_folder = val;
    }
    if let Some(val) = recording_toolbar_content_protected {
        settings.recording_toolbar_content_protected = val;
    }
    if let Some(val) = recording_max_duration_minutes {
        settings.recording_max_duration_minutes = val.clamp(1, 1440);
    }
    if let Some(val) = recording_file_name_template {
        settings.recording_file_name_template = if val.trim().is_empty() {
            "{timestamp}".to_string()
        } else {
            val
        };
    }
    if let Some(val) = recording_ffmpeg_download_url {
        settings.recording_ffmpeg_download_url = if val.trim().is_empty() {
            settings.recording_ffmpeg_download_url
        } else {
            val.trim().to_string()
        };
    }
    if let Some(val) = recording_window_audio_sync_advance_ms {
        settings.recording_window_audio_sync_advance_ms = val.clamp(0, 500);
    }
    if let Some(val) = dev_force_ffmpeg_window_capture {
        settings.dev_force_ffmpeg_window_capture = val;
    }

    if let Some(ref hot_key_val) = hot_key {
        if hot_key_val.is_empty() {
            return Err(frontend_error_kind(
                AppErrorKind::SettingsHotkeyEmpty,
                "hot_key is empty",
            ));
        }

        if hot_key_val != &settings.hot_key {
            // 与其它功能已存储热键比对（即使功能当前禁用，键串仍可能冲突）
            if hot_key_val == &settings.image_hot_key
                || hot_key_val == &settings.screenshot_hot_key
                || hot_key_val == &settings.recording_hot_key
                || hot_key_val == &settings.recording_mic_toggle_hot_key
                || hot_key_val == &settings.launcher_hot_key
                || hot_key_val == &settings.doc_manager_hot_key
            {
                return Err(frontend_error_kind(
                    AppErrorKind::SettingsHotkeysIdentical,
                    format!("hot_key={} 与其它功能快捷键相同", hot_key_val),
                ));
            }
            if settings.text_clipboard_enabled
                && app.global_shortcut().is_registered(hot_key_val.as_str())
            {
                return Err(frontend_error_kind_params(
                    AppErrorKind::SettingsHotkeyConflict,
                    serde_json::json!({"key": hot_key_val}),
                    "hot_key global shortcut already registered",
                ));
            }
            let old_hot_key = settings.hot_key.clone();
            if let Err(e) = app.global_shortcut().unregister(old_hot_key.as_str()) {
                log::warn!(
                    "注销旧文字窗口快捷键 '{}' 失败 (可能从未注册成功): {}",
                    old_hot_key,
                    e
                );
            }
            if settings.text_clipboard_enabled {
                if let Err(e) =
                    register_text_shortcut(&app, state.inner().clone(), hot_key_val.as_str())
                {
                    log::warn!(
                        "注册新文字窗口快捷键 '{}' 失败, 尝试恢复旧快捷键: {}",
                        hot_key_val,
                        e
                    );
                    if let Err(e2) =
                        register_text_shortcut(&app, state.inner().clone(), old_hot_key.as_str())
                    {
                        log::error!("恢复旧文字窗口快捷键 '{}' 也失败: {}", old_hot_key, e2);
                    }
                    return Err(e);
                }
            }
            settings.hot_key = hot_key_val.clone();
        }
    }

    if let Some(ref image_hot_key_val) = image_hot_key {
        if image_hot_key_val.is_empty() {
            return Err(frontend_error_kind(
                AppErrorKind::SettingsHotkeyEmpty,
                "image_hot_key is empty",
            ));
        }

        if image_hot_key_val != &settings.image_hot_key {
            if let Some(ref hot_key_val) = hot_key {
                if image_hot_key_val == hot_key_val {
                    return Err(frontend_error_kind(
                        AppErrorKind::SettingsHotkeysIdentical,
                        format!(
                            "hot_key={}, image_hot_key={}",
                            hot_key_val, image_hot_key_val
                        ),
                    ));
                }
            } else if image_hot_key_val == &settings.hot_key {
                return Err(frontend_error_kind(
                    AppErrorKind::SettingsHotkeysIdentical,
                    format!(
                        "hot_key={}, image_hot_key={}",
                        settings.hot_key, image_hot_key_val
                    ),
                ));
            }

            if settings.image_clipboard_enabled
                && app
                    .global_shortcut()
                    .is_registered(image_hot_key_val.as_str())
            {
                return Err(frontend_error_kind_params(
                    AppErrorKind::SettingsHotkeyConflict,
                    serde_json::json!({"key": image_hot_key_val}),
                    "image_hot_key global shortcut already registered",
                ));
            }
            let old_image_hot_key = settings.image_hot_key.clone();
            if let Err(e) = app.global_shortcut().unregister(old_image_hot_key.as_str()) {
                log::warn!(
                    "注销旧图片窗口快捷键 '{}' 失败 (可能从未注册成功): {}",
                    old_image_hot_key,
                    e
                );
            }
            if settings.image_clipboard_enabled {
                if let Err(e) =
                    register_image_shortcut(&app, state.inner().clone(), image_hot_key_val.as_str())
                {
                    log::warn!(
                        "注册新图片窗口快捷键 '{}' 失败, 尝试恢复旧快捷键: {}",
                        image_hot_key_val,
                        e
                    );
                    if let Err(e2) = register_image_shortcut(
                        &app,
                        state.inner().clone(),
                        old_image_hot_key.as_str(),
                    ) {
                        log::error!(
                            "恢复旧图片窗口快捷键 '{}' 也失败: {}",
                            old_image_hot_key,
                            e2
                        );
                    }
                    return Err(e);
                }
            }
            settings.image_hot_key = image_hot_key_val.clone();
        }
    }

    if let Some(ref screenshot_hot_key_val) = screenshot_hot_key {
        if screenshot_hot_key_val.is_empty() {
            return Err(frontend_error_kind(
                AppErrorKind::SettingsHotkeyEmpty,
                "screenshot_hot_key is empty",
            ));
        }

        if screenshot_hot_key_val != &settings.screenshot_hot_key {
            let effective_hot_key = effective_key(&hot_key, &settings.hot_key);
            let effective_image_hot_key = effective_key(&image_hot_key, &settings.image_hot_key);
            let effective_recording =
                effective_key(&recording_hot_key, &settings.recording_hot_key);
            let effective_mic = effective_key(
                &recording_mic_toggle_hot_key,
                &settings.recording_mic_toggle_hot_key,
            );
            let effective_launcher = effective_key(&launcher_hot_key, &settings.launcher_hot_key);
            let effective_doc = effective_key(&doc_manager_hot_key, &settings.doc_manager_hot_key);
            if screenshot_hot_key_val == &effective_hot_key
                || screenshot_hot_key_val == &effective_image_hot_key
                || screenshot_hot_key_val == &effective_recording
                || screenshot_hot_key_val == &effective_mic
                || screenshot_hot_key_val == &effective_launcher
                || screenshot_hot_key_val == &effective_doc
            {
                return Err(frontend_error_kind(
                    AppErrorKind::SettingsHotkeysIdentical,
                    format!(
                        "screenshot_hot_key={} 与剪贴板/录屏/麦克风/启动器/文档快捷键冲突",
                        screenshot_hot_key_val
                    ),
                ));
            }

            if app
                .global_shortcut()
                .is_registered(screenshot_hot_key_val.as_str())
            {
                return Err(frontend_error_kind_params(
                    AppErrorKind::SettingsHotkeyConflict,
                    serde_json::json!({"key": screenshot_hot_key_val}),
                    "screenshot global shortcut already registered",
                ));
            }

            let old_screenshot_hot_key = settings.screenshot_hot_key.clone();
            if let Err(e) = app
                .global_shortcut()
                .unregister(old_screenshot_hot_key.as_str())
            {
                log::warn!(
                    "注销旧截图快捷键 '{}' 失败 (可能从未注册成功): {}",
                    old_screenshot_hot_key,
                    e
                );
            }
            if settings.screenshot_enabled {
                if let Err(e) = register_screenshot_shortcut(&app, screenshot_hot_key_val.as_str())
                {
                    log::warn!(
                        "注册新截图快捷键 '{}' 失败, 尝试恢复旧快捷键: {}",
                        screenshot_hot_key_val,
                        e
                    );
                    if let Err(e2) =
                        register_screenshot_shortcut(&app, old_screenshot_hot_key.as_str())
                    {
                        log::error!(
                            "恢复旧截图快捷键 '{}' 也失败: {}",
                            old_screenshot_hot_key,
                            e2
                        );
                    }
                    return Err(e);
                }
            }
            settings.screenshot_hot_key = screenshot_hot_key_val.clone();
        }
    }

    if let Some(ref recording_hot_key_val) = recording_hot_key {
        if recording_hot_key_val.is_empty() {
            return Err(frontend_error_kind(
                AppErrorKind::SettingsHotkeyEmpty,
                "recording_hot_key is empty",
            ));
        }
        if recording_hot_key_val != &settings.recording_hot_key {
            let effective_hot_key = effective_key(&hot_key, &settings.hot_key);
            let effective_image_hot_key = effective_key(&image_hot_key, &settings.image_hot_key);
            let effective_screenshot_hot_key =
                effective_key(&screenshot_hot_key, &settings.screenshot_hot_key);
            let effective_mic_toggle_hot_key = effective_key(
                &recording_mic_toggle_hot_key,
                &settings.recording_mic_toggle_hot_key,
            );
            let effective_launcher_hot_key =
                effective_key(&launcher_hot_key, &settings.launcher_hot_key);
            let effective_doc_manager_hot_key =
                effective_key(&doc_manager_hot_key, &settings.doc_manager_hot_key);
            if recording_hot_key_val == &effective_hot_key
                || recording_hot_key_val == &effective_image_hot_key
                || recording_hot_key_val == &effective_screenshot_hot_key
                || recording_hot_key_val == &effective_mic_toggle_hot_key
                || recording_hot_key_val == &effective_launcher_hot_key
                || recording_hot_key_val == &effective_doc_manager_hot_key
            {
                return Err(frontend_error_kind(
                    AppErrorKind::SettingsHotkeysIdentical,
                    format!(
                        "hot_key={}, image_hot_key={}, screenshot_hot_key={}, recording_hot_key={}",
                        effective_hot_key,
                        effective_image_hot_key,
                        effective_screenshot_hot_key,
                        recording_hot_key_val
                    ),
                ));
            }

            if app
                .global_shortcut()
                .is_registered(recording_hot_key_val.as_str())
            {
                return Err(frontend_error_kind_params(
                    AppErrorKind::SettingsHotkeyConflict,
                    serde_json::json!({"key": recording_hot_key_val}),
                    "recording global shortcut already registered",
                ));
            }

            let old_recording_hot_key = settings.recording_hot_key.clone();
            if let Err(e) = app
                .global_shortcut()
                .unregister(old_recording_hot_key.as_str())
            {
                log::warn!(
                    "注销旧录屏快捷键 '{}' 失败 (可能从未注册成功): {}",
                    old_recording_hot_key,
                    e
                );
            }
            if settings.recording_enabled {
                if let Err(e) = register_recording_shortcut(
                    &app,
                    state.inner().clone(),
                    recording_hot_key_val.as_str(),
                ) {
                    log::warn!(
                        "注册新录屏快捷键 '{}' 失败, 尝试恢复旧快捷键: {}",
                        recording_hot_key_val,
                        e
                    );
                    if let Err(e2) = register_recording_shortcut(
                        &app,
                        state.inner().clone(),
                        old_recording_hot_key.as_str(),
                    ) {
                        log::error!(
                            "恢复旧录屏快捷键 '{}' 也失败: {}",
                            old_recording_hot_key,
                            e2
                        );
                    }
                    return Err(e);
                }
            }
            settings.recording_hot_key = recording_hot_key_val.clone();
        }
    }

    if let Some(ref mic_toggle_hot_key_val) = recording_mic_toggle_hot_key {
        if mic_toggle_hot_key_val.is_empty() {
            return Err(frontend_error_kind(
                AppErrorKind::SettingsHotkeyEmpty,
                "recording_mic_toggle_hot_key is empty",
            ));
        }
        if mic_toggle_hot_key_val != &settings.recording_mic_toggle_hot_key {
            let effective_hot_key = effective_key(&hot_key, &settings.hot_key);
            let effective_image_hot_key = effective_key(&image_hot_key, &settings.image_hot_key);
            let effective_screenshot_hot_key =
                effective_key(&screenshot_hot_key, &settings.screenshot_hot_key);
            let effective_recording_hot_key =
                effective_key(&recording_hot_key, &settings.recording_hot_key);
            let effective_launcher_hot_key =
                effective_key(&launcher_hot_key, &settings.launcher_hot_key);
            let effective_doc_manager_hot_key =
                effective_key(&doc_manager_hot_key, &settings.doc_manager_hot_key);

            if mic_toggle_hot_key_val == &effective_hot_key
                || mic_toggle_hot_key_val == &effective_image_hot_key
                || mic_toggle_hot_key_val == &effective_screenshot_hot_key
                || mic_toggle_hot_key_val == &effective_recording_hot_key
                || mic_toggle_hot_key_val == &effective_launcher_hot_key
                || mic_toggle_hot_key_val == &effective_doc_manager_hot_key
            {
                return Err(frontend_error_kind(
                    AppErrorKind::SettingsHotkeysIdentical,
                    format!(
                        "hot_key={}, image_hot_key={}, screenshot_hot_key={}, recording_hot_key={}, mic_toggle_hot_key={}",
                        effective_hot_key,
                        effective_image_hot_key,
                        effective_screenshot_hot_key,
                        effective_recording_hot_key,
                        mic_toggle_hot_key_val
                    ),
                ));
            }

            if app
                .global_shortcut()
                .is_registered(mic_toggle_hot_key_val.as_str())
            {
                return Err(frontend_error_kind_params(
                    AppErrorKind::SettingsHotkeyConflict,
                    serde_json::json!({"key": mic_toggle_hot_key_val}),
                    "mic toggle global shortcut already registered",
                ));
            }

            let old_mic_toggle_hot_key = settings.recording_mic_toggle_hot_key.clone();
            if let Err(e) = app
                .global_shortcut()
                .unregister(old_mic_toggle_hot_key.as_str())
            {
                log::warn!(
                    "注销旧麦克风切换快捷键 '{}' 失败: {}",
                    old_mic_toggle_hot_key,
                    e
                );
            }

            if settings.recording_enabled {
                let app_handle_for_mic = app.clone();
                let old_mic_key_for_rollback = old_mic_toggle_hot_key.clone();
                // PTT 事件串行队列：消除 Pressed/Released 并发顺序不确定（#14）
                let (mic_event_tx, mic_event_rx) = std::sync::mpsc::channel::<bool>();
                {
                    let app_handle_inner = app_handle_for_mic.clone();
                    std::thread::spawn(move || {
                        while let Ok(enable) = mic_event_rx.recv() {
                            toggle_microphone_from_shortcut(app_handle_inner.clone(), enable);
                        }
                    });
                }
                if let Err(e) =
                    app.global_shortcut()
                        .on_shortcut(mic_toggle_hot_key_val.as_str(), {
                            let tx = mic_event_tx.clone();
                            move |_app, _shortcut, event| {
                                // 按到达顺序入队，由单一消费线程串行执行（#14）
                                let _ = tx.send(event.state == ShortcutState::Pressed);
                            }
                        })
                {
                    log::warn!(
                        "注册麦克风切换快捷键 '{}' 失败, 尝试恢复旧快捷键: {}",
                        mic_toggle_hot_key_val,
                        e
                    );
                    if let Err(e2) =
                        app.global_shortcut()
                            .on_shortcut(old_mic_key_for_rollback.as_str(), {
                                let tx = mic_event_tx.clone();
                                move |_app, _shortcut, event| {
                                    let _ = tx.send(event.state == ShortcutState::Pressed);
                                }
                            })
                    {
                        log::error!(
                            "恢复旧麦克风切换快捷键 '{}' 也失败: {}",
                            old_mic_key_for_rollback,
                            e2
                        );
                    }
                    return Err(frontend_error_kind_params(
                        AppErrorKind::ClipboardHotkeyRegisterFailed,
                        serde_json::json!({"key": mic_toggle_hot_key_val}),
                        e.to_string(),
                    ));
                }
            }

            settings.recording_mic_toggle_hot_key = mic_toggle_hot_key_val.clone();
        }
    }

    if let Some(ref launcher_hot_key_val) = launcher_hot_key {
        if launcher_hot_key_val.is_empty() {
            return Err(frontend_error_kind(
                AppErrorKind::SettingsHotkeyEmpty,
                "launcher_hot_key is empty",
            ));
        }

        if launcher_hot_key_val != &settings.launcher_hot_key {
            let effective_hot_key = effective_key(&hot_key, &settings.hot_key);
            let effective_image_hot_key = effective_key(&image_hot_key, &settings.image_hot_key);
            let effective_screenshot_hot_key =
                effective_key(&screenshot_hot_key, &settings.screenshot_hot_key);
            let effective_recording_hot_key =
                effective_key(&recording_hot_key, &settings.recording_hot_key);
            let effective_mic_toggle_hot_key = effective_key(
                &recording_mic_toggle_hot_key,
                &settings.recording_mic_toggle_hot_key,
            );
            let effective_doc_manager_hot_key =
                effective_key(&doc_manager_hot_key, &settings.doc_manager_hot_key);

            if launcher_hot_key_val == &effective_hot_key
                || launcher_hot_key_val == &effective_image_hot_key
                || launcher_hot_key_val == &effective_screenshot_hot_key
                || launcher_hot_key_val == &effective_recording_hot_key
                || launcher_hot_key_val == &effective_mic_toggle_hot_key
                || launcher_hot_key_val == &effective_doc_manager_hot_key
            {
                return Err(frontend_error_kind(
                    AppErrorKind::SettingsHotkeysIdentical,
                    format!(
                        "hot_key={}, image_hot_key={}, screenshot_hot_key={}, recording_hot_key={}, mic_toggle_hot_key={}, launcher_hot_key={}",
                        effective_hot_key,
                        effective_image_hot_key,
                        effective_screenshot_hot_key,
                        effective_recording_hot_key,
                        effective_mic_toggle_hot_key,
                        launcher_hot_key_val
                    ),
                ));
            }

            if app
                .global_shortcut()
                .is_registered(launcher_hot_key_val.as_str())
            {
                return Err(frontend_error_kind_params(
                    AppErrorKind::SettingsHotkeyConflict,
                    serde_json::json!({"key": launcher_hot_key_val}),
                    "launcher global shortcut already registered",
                ));
            }

            let old_launcher_hot_key = settings.launcher_hot_key.clone();
            if let Err(e) = app
                .global_shortcut()
                .unregister(old_launcher_hot_key.as_str())
            {
                log::warn!(
                    "注销旧启动器快捷键 '{}' 失败 (可能从未注册成功): {}",
                    old_launcher_hot_key,
                    e
                );
            }
            if settings.launcher_enabled {
                let app_handle_for_launcher = app.clone();
                let old_launcher_key_for_rollback = old_launcher_hot_key.clone();
                if let Err(e) = app.global_shortcut().on_shortcut(
                    launcher_hot_key_val.as_str(),
                    move |_app, _shortcut, event| {
                        if let ShortcutState::Pressed = event.state {
                            let app_handle = app_handle_for_launcher.clone();
                            tauri::async_runtime::spawn(async move {
                                let _ =
                                    crate::ui::commands_launcher::toggle_launcher(app_handle).await;
                            });
                        }
                    },
                ) {
                    log::warn!(
                        "注册启动器快捷键 '{}' 失败, 尝试恢复旧快捷键: {}",
                        launcher_hot_key_val,
                        e
                    );
                    let app_handle_for_rollback = app.clone();
                    if let Err(e2) = app.global_shortcut().on_shortcut(
                        old_launcher_key_for_rollback.as_str(),
                        move |_app, _shortcut, event| {
                            if let ShortcutState::Pressed = event.state {
                                let app_handle = app_handle_for_rollback.clone();
                                tauri::async_runtime::spawn(async move {
                                    let _ =
                                        crate::ui::commands_launcher::toggle_launcher(app_handle)
                                            .await;
                                });
                            }
                        },
                    ) {
                        log::error!(
                            "恢复旧启动器快捷键 '{}' 也失败: {}",
                            old_launcher_key_for_rollback,
                            e2
                        );
                    }
                    return Err(frontend_error_kind_params(
                        AppErrorKind::ClipboardHotkeyRegisterFailed,
                        serde_json::json!({"key": launcher_hot_key_val}),
                        e.to_string(),
                    ));
                }
            }
            settings.launcher_hot_key = launcher_hot_key_val.clone();
        }
    }

    if let Some(ref doc_manager_hot_key_val) = doc_manager_hot_key {
        if doc_manager_hot_key_val.is_empty() {
            return Err(frontend_error_kind(
                AppErrorKind::SettingsHotkeyEmpty,
                "doc_manager_hot_key is empty",
            ));
        }

        if doc_manager_hot_key_val != &settings.doc_manager_hot_key {
            let effective_hot_key = effective_key(&hot_key, &settings.hot_key);
            let effective_image_hot_key = effective_key(&image_hot_key, &settings.image_hot_key);
            let effective_screenshot_hot_key =
                effective_key(&screenshot_hot_key, &settings.screenshot_hot_key);
            let effective_recording_hot_key =
                effective_key(&recording_hot_key, &settings.recording_hot_key);
            let effective_mic_toggle_hot_key = effective_key(
                &recording_mic_toggle_hot_key,
                &settings.recording_mic_toggle_hot_key,
            );
            let effective_launcher_hot_key =
                effective_key(&launcher_hot_key, &settings.launcher_hot_key);

            if doc_manager_hot_key_val == &effective_hot_key
                || doc_manager_hot_key_val == &effective_image_hot_key
                || doc_manager_hot_key_val == &effective_screenshot_hot_key
                || doc_manager_hot_key_val == &effective_recording_hot_key
                || doc_manager_hot_key_val == &effective_mic_toggle_hot_key
                || doc_manager_hot_key_val == &effective_launcher_hot_key
            {
                return Err(frontend_error_kind(
                    AppErrorKind::SettingsHotkeysIdentical,
                    format!(
                        "doc_manager_hot_key={} conflicts with existing shortcut",
                        doc_manager_hot_key_val
                    ),
                ));
            }

            if app
                .global_shortcut()
                .is_registered(doc_manager_hot_key_val.as_str())
            {
                return Err(frontend_error_kind_params(
                    AppErrorKind::SettingsHotkeyConflict,
                    serde_json::json!({"key": doc_manager_hot_key_val}),
                    "doc_manager global shortcut already registered",
                ));
            }

            let old_doc_manager_hot_key = settings.doc_manager_hot_key.clone();
            if let Err(e) = app
                .global_shortcut()
                .unregister(old_doc_manager_hot_key.as_str())
            {
                log::warn!(
                    "注销旧文档管理快捷键 '{}' 失败: {}",
                    old_doc_manager_hot_key,
                    e
                );
            }
            if settings.doc_manager_enabled {
                let app_handle_for_doc = app.clone();
                let old_doc_key_for_rollback = old_doc_manager_hot_key.clone();
                if let Err(e) = app.global_shortcut().on_shortcut(
                    doc_manager_hot_key_val.as_str(),
                    move |_app, _shortcut, event| {
                        if let ShortcutState::Pressed = event.state {
                            let _ = crate::ui::window_manager::show_standard_window_by_label(
                                &app_handle_for_doc,
                                "document_manager",
                            );
                        }
                    },
                ) {
                    log::warn!(
                        "注册文档管理快捷键 '{}' 失败, 尝试恢复旧快捷键: {}",
                        doc_manager_hot_key_val,
                        e
                    );
                    let app_handle_for_rollback = app.clone();
                    if let Err(e2) = app.global_shortcut().on_shortcut(
                        old_doc_key_for_rollback.as_str(),
                        move |_app, _shortcut, event| {
                            if let ShortcutState::Pressed = event.state {
                                let _ = crate::ui::window_manager::show_standard_window_by_label(
                                    &app_handle_for_rollback,
                                    "document_manager",
                                );
                            }
                        },
                    ) {
                        log::error!(
                            "恢复旧文档管理快捷键 '{}' 也失败: {}",
                            old_doc_key_for_rollback,
                            e2
                        );
                    }
                    return Err(frontend_error_kind_params(
                        AppErrorKind::ClipboardHotkeyRegisterFailed,
                        serde_json::json!({"key": doc_manager_hot_key_val}),
                        e.to_string(),
                    ));
                }
            }
            settings.doc_manager_hot_key = doc_manager_hot_key_val.clone();
        }
    }

    if let Some(enabled) = text_clipboard_enabled {
        if enabled {
            if !app
                .global_shortcut()
                .is_registered(settings.hot_key.as_str())
            {
                register_text_shortcut(&app, state.inner().clone(), settings.hot_key.as_str())?;
            }
        } else if let Err(e) = app.global_shortcut().unregister(settings.hot_key.as_str()) {
            log::warn!("注销文字快捷键 '{}' 失败: {}", settings.hot_key, e);
        }
    }

    if let Some(enabled) = image_clipboard_enabled {
        if enabled {
            if !app
                .global_shortcut()
                .is_registered(settings.image_hot_key.as_str())
            {
                register_image_shortcut(
                    &app,
                    state.inner().clone(),
                    settings.image_hot_key.as_str(),
                )?;
            }
        } else if let Err(e) = app
            .global_shortcut()
            .unregister(settings.image_hot_key.as_str())
        {
            log::warn!("注销图片快捷键 '{}' 失败: {}", settings.image_hot_key, e);
        }
    }

    if let Some(enabled) = screenshot_enabled {
        if enabled {
            if !app
                .global_shortcut()
                .is_registered(settings.screenshot_hot_key.as_str())
            {
                register_screenshot_shortcut(&app, settings.screenshot_hot_key.as_str())?;
            }
        } else {
            if let Err(e) = app
                .global_shortcut()
                .unregister(settings.screenshot_hot_key.as_str())
            {
                log::warn!(
                    "注销截图快捷键 '{}' 失败: {}",
                    settings.screenshot_hot_key,
                    e
                );
            }
        }
    }

    if let Some(enabled) = recording_enabled {
        if enabled {
            if !app
                .global_shortcut()
                .is_registered(settings.recording_hot_key.as_str())
            {
                register_recording_shortcut(
                    &app,
                    state.inner().clone(),
                    settings.recording_hot_key.as_str(),
                )?;
            }
            // 麦克风切换快捷键同样需要随启用即时注册（否则重启前不生效）
            if !app
                .global_shortcut()
                .is_registered(settings.recording_mic_toggle_hot_key.as_str())
            {
                register_mic_toggle_shortcut(
                    &app,
                    state.inner().clone(),
                    settings.recording_mic_toggle_hot_key.as_str(),
                )?;
            }
        } else {
            if let Err(e) = app
                .global_shortcut()
                .unregister(settings.recording_hot_key.as_str())
            {
                log::warn!(
                    "注销录屏快捷键 '{}' 失败: {}",
                    settings.recording_hot_key,
                    e
                );
            }
            if app
                .global_shortcut()
                .is_registered(settings.recording_mic_toggle_hot_key.as_str())
            {
                if let Err(e) = app
                    .global_shortcut()
                    .unregister(settings.recording_mic_toggle_hot_key.as_str())
                {
                    log::warn!(
                        "注销麦克风切换快捷键 '{}' 失败: {}",
                        settings.recording_mic_toggle_hot_key,
                        e
                    );
                }
            }
        }
    }

    if let Some(enabled) = launcher_enabled {
        if enabled {
            // 检查是否有快捷键变更
            if let Some(ref launcher_hot_key_val) = launcher_hot_key {
                if launcher_hot_key_val != &settings.launcher_hot_key {
                    // 快捷键发生变更，先注销旧的
                    if let Err(e) = app
                        .global_shortcut()
                        .unregister(settings.launcher_hot_key.as_str())
                    {
                        log::warn!(
                            "注销旧启动器快捷键 '{}' 失败 (可能从未注册成功): {}",
                            settings.launcher_hot_key,
                            e
                        );
                    }

                    // 注册新的快捷键
                    let app_handle_for_launcher = app.clone();
                    let new_hot_key = launcher_hot_key_val.clone();
                    if let Err(e) = app.global_shortcut().on_shortcut(
                        launcher_hot_key_val.as_str(),
                        move |_app, _shortcut, event| {
                            if let ShortcutState::Pressed = event.state {
                                let app_handle = app_handle_for_launcher.clone();
                                tauri::async_runtime::spawn(async move {
                                    let _ =
                                        crate::ui::commands_launcher::toggle_launcher(app_handle)
                                            .await;
                                });
                            }
                        },
                    ) {
                        log::warn!("注册启动器快捷键 '{}' 失败: {}", new_hot_key, e);
                    }
                    settings.launcher_hot_key = launcher_hot_key_val.clone();
                } else if !app
                    .global_shortcut()
                    .is_registered(settings.launcher_hot_key.as_str())
                {
                    // 快捷键未变更但未注册，重新注册
                    let app_handle_for_launcher = app.clone();
                    if let Err(e) = app.global_shortcut().on_shortcut(
                        settings.launcher_hot_key.as_str(),
                        move |_app, _shortcut, event| {
                            if let ShortcutState::Pressed = event.state {
                                let app_handle = app_handle_for_launcher.clone();
                                tauri::async_runtime::spawn(async move {
                                    let _ =
                                        crate::ui::commands_launcher::toggle_launcher(app_handle)
                                            .await;
                                });
                            }
                        },
                    ) {
                        log::warn!(
                            "注册启动器快捷键 '{}' 失败: {}",
                            settings.launcher_hot_key.as_str(),
                            e
                        );
                    }
                }
            } else if !app
                .global_shortcut()
                .is_registered(settings.launcher_hot_key.as_str())
            {
                // 没有传入新快捷键，但需要确保已注册
                let app_handle_for_launcher = app.clone();
                if let Err(e) = app.global_shortcut().on_shortcut(
                    settings.launcher_hot_key.as_str(),
                    move |_app, _shortcut, event| {
                        if let ShortcutState::Pressed = event.state {
                            let app_handle = app_handle_for_launcher.clone();
                            tauri::async_runtime::spawn(async move {
                                let _ =
                                    crate::ui::commands_launcher::toggle_launcher(app_handle).await;
                            });
                        }
                    },
                ) {
                    log::warn!(
                        "注册启动器快捷键 '{}' 失败: {}",
                        settings.launcher_hot_key.as_str(),
                        e
                    );
                }
            }
        } else if let Err(e) = app
            .global_shortcut()
            .unregister(settings.launcher_hot_key.as_str())
        {
            log::warn!(
                "注销启动器快捷键 '{}' 失败: {}",
                settings.launcher_hot_key,
                e
            );
        }
    }

    if let Some(ref ai_provider_val) = ai_provider {
        if ai_provider_val.is_empty() {
            return Err(frontend_error_kind(
                AppErrorKind::SettingsProviderNameEmpty,
                "ai_provider is empty",
            ));
        }
        crate::utils::ai_store::set_current_provider(ai_provider_val)
            .await
            .map_err(|e| frontend_error_kind(AppErrorKind::SettingsSaveFailed, e))?;
    }

    // 解析当前要操作的提供商
    let provider_key = match ai_provider {
        Some(ref p) if !p.is_empty() => p.clone(),
        _ => crate::utils::ai_store::get_current_provider().await,
    };
    if !provider_key.is_empty() {
        let existing = crate::utils::ai_store::get_provider_config(&provider_key).await;
        let is_new = existing.is_none();
        let mut config = existing.unwrap_or_default();
        let mut changed = is_new;

        if let Some(ref api_url) = ai_api_url {
            if !api_url.is_empty() && config.api_url != *api_url {
                config.api_url = api_url.clone();
                changed = true;
            }
        }
        if let Some(ref model_name) = ai_model_name {
            if !model_name.is_empty() && config.model_name != *model_name {
                config.model_name = model_name.clone();
                changed = true;
            }
        }
        if let Some(ref api_key) = ai_api_key {
            if api_key != "********" && config.api_key != *api_key {
                config.api_key = api_key.clone();
                changed = true;
                if api_key.trim().is_empty() {
                    log::info!("提供商 {} 的API密钥已清空", provider_key);
                }
            }
        }

        if changed {
            crate::utils::ai_store::save_provider_config(
                &provider_key,
                &config.api_url,
                &config.model_name,
                &config.api_key,
            )
            .await
            .map_err(|e| frontend_error_kind(AppErrorKind::SettingsSaveProviderFailed, e))?;
        }
    }

    invalidate_ai_client_cache();

    settings
        .validate()
        .map_err(|e| frontend_error_kind(AppErrorKind::SettingsValidationFailed, e))?;

    save_settings(&settings)
        .map_err(|e| frontend_error_kind(AppErrorKind::SettingsSaveFailed, e))?;
    set_image_fill_verify_mode(&settings.image_fill_verify_mode);
    // 日志开关即时生效，无需重启
    crate::core::logger::set_logging_enabled(settings.logging_enabled);

    let selection_enabled = settings.selection_enabled;
    let text_clipboard_feature_enabled = settings.text_clipboard_enabled;
    let image_clipboard_feature_enabled = settings.image_clipboard_enabled;
    let screenshot_feature_enabled = settings.screenshot_enabled;
    let recording_feature_enabled = settings.recording_enabled;
    let launcher_feature_enabled = settings.launcher_enabled;
    let doc_manager_feature_enabled = settings.doc_manager_enabled;
    let (clipboard_manager_arc, image_manager_arc) = {
        let mut state_guard = lock_arc_mutex(state.inner());
        state_guard.settings = settings.clone();
        (
            state_guard.clipboard_manager.clone(),
            state_guard.image_clipboard_manager.clone(),
        )
    };
    {
        let mut manager = lock_arc_mutex(&clipboard_manager_arc);
        if let Some(val) = text_max_items {
            manager.set_max_items(val);
        }
        if let Some(val) = grouped_items_protected_from_limit {
            manager.set_grouped_items_protected_from_limit(val);
        }
    }
    {
        let mut manager = lock_arc_mutex(&image_manager_arc);
        if let Some(val) = image_max_items {
            manager.set_max_items(val);
        }
        if let Some(val) = image_disk_limit_mb {
            manager.set_disk_limit_mb(val);
        }
        if let Some(val) = grouped_items_protected_from_limit {
            manager.set_grouped_items_protected_from_limit(val);
        }
    }

    features::mouse_listener::set_selection_listener_enabled(
        app.clone(),
        state.inner().clone(),
        selection_enabled,
    );
    set_clipboard_listener_enabled(
        app.clone(),
        state.inner().clone(),
        text_clipboard_feature_enabled,
    );
    set_image_clipboard_listener_enabled(
        app.clone(),
        state.inner().clone(),
        image_clipboard_feature_enabled,
    );
    if screenshot_feature_enabled {
        let _ = ensure_window_for_label(&app, "screenshot");
        let _ = ensure_window_for_label(&app, "longshot_toolbar");
        let _ = ensure_window_for_label(&app, "longshot_border");
    } else {
        let _ = close_screenshot_window(app.clone()).await;
        destroy_window_by_label(&app, "screenshot");
        destroy_window_by_label(&app, "longshot_toolbar");
        destroy_window_by_label(&app, "longshot_border");
    }
    if recording_feature_enabled {
        let _ = ensure_window_for_label(&app, "recording_toolbar");
    } else {
        let _ = crate::features::recording::recorder_service::cancel_recording(
            &app,
            state.inner().clone(),
            crate::features::recording::types::SessionRequest { session_id: None },
        );
        destroy_window_by_label(&app, "recording_toolbar");
    }
    if text_clipboard_feature_enabled {
        let _ = ensure_window_for_label(&app, "clipboard");
        let _ = ensure_window_for_label(&app, "text_preview");
    } else {
        log::info!("[设置] 文本剪切板已禁用，开始销毁窗口");
        destroy_window_by_label(&app, "clipboard");
        destroy_window_by_label(&app, "text_preview");
    }
    if image_clipboard_feature_enabled {
        let _ = ensure_window_for_label(&app, "image_clipboard");
        let _ = ensure_window_for_label(&app, "image_preview");
    } else {
        log::info!("[设置] 图片剪切板已禁用，开始销毁窗口");
        destroy_window_by_label(&app, "image_clipboard");
        destroy_window_by_label(&app, "image_preview");
    }
    if selection_enabled {
        let _ = ensure_window_for_label(&app, "selection_toolbar");
    } else {
        destroy_window_by_label(&app, "selection_toolbar");
    }
    if launcher_feature_enabled {
        let _ = ensure_window_for_label(&app, "launcher");
    } else {
        destroy_window_by_label(&app, "launcher");
    }
    if doc_manager_feature_enabled {
        let _ = ensure_window_for_label(&app, "document_manager");
    } else {
        destroy_window_by_label(&app, "document_manager");
        let _ = crate::ui::window_manager::hide_doc_manager_widget_window(&app);
    }
    if let Some(content_protected) = recording_toolbar_content_protected {
        if let Some(window) = app.get_webview_window("recording_toolbar") {
            let _ = window.set_content_protected(content_protected);
        }
    }

    log::info!("设置保存成功（部分更新）");
    Ok(())
}
#[tauri::command]
pub async fn test_ai_connection(
    ai_provider: Option<String>,
    ai_api_url: String,
    ai_model_name: String,
    ai_api_key: String,
) -> Result<String, String> {
    let mut real_api_key = ai_api_key;

    if real_api_key == "********" {
        let provider = match ai_provider {
            Some(p) if !p.is_empty() => p,
            _ => crate::utils::ai_store::get_current_provider().await,
        };
        match crate::utils::ai_store::get_api_key(&provider).await {
            Ok(key) if !key.is_empty() => real_api_key = key,
            _ => {
                return Err(frontend_error_kind(
                    AppErrorKind::SettingsLocalKeyNotFound,
                    "real api key not found",
                ))
            }
        }
    }

    let config = AIConfig {
        api_key: real_api_key,
        base_url: ai_api_url,
        model: ai_model_name,
    };

    let client = AIClient::new(config)
        .map_err(|e| frontend_error_kind(AppErrorKind::AiClientInitFailed, e.to_string()))?;

    match client.test_connection().await {
        Ok(success) => {
            if success {
                Ok("连接成功".to_string())
            } else {
                Err(frontend_error_kind(
                    AppErrorKind::AiConnectionTestNoResponse,
                    "test_connection returned false",
                ))
            }
        }
        Err(e) => {
            log::error!("AI连接测试失败: {}", e);
            Err(frontend_error_kind(
                AppErrorKind::AiConnectionTestFailed,
                e.to_string(),
            ))
        }
    }
}
#[tauri::command]
pub async fn remove_ai_provider(
    provider: String,
    _state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    if provider.is_empty() {
        return Err(frontend_error_kind(
            AppErrorKind::SettingsProviderNameEmpty,
            "provider is empty",
        ));
    }

    crate::utils::ai_store::remove_provider(&provider)
        .await
        .map_err(|e| frontend_error_kind(AppErrorKind::AiProviderNotFound, e))?;

    let current = crate::utils::ai_store::get_current_provider().await;
    if current == provider {
        crate::utils::ai_store::set_current_provider("").await.ok();
    }

    invalidate_ai_client_cache();
    Ok(())
}
#[tauri::command]
pub async fn get_all_configured_providers() -> Result<Vec<(String, String)>, String> {
    let providers = crate::utils::ai_store::get_all_providers().await;
    Ok(providers.into_keys().map(|k| (k.clone(), k)).collect())
}

