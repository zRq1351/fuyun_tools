use crate::core::app_state::AppState as SharedAppState;
use crate::core::config::{AIProvider, ProviderConfig};
use crate::features;
use crate::services::ai_client::{AIClient, AIConfig};
use crate::ui::window_manager::{hide_clipboard_window, show_clipboard_window};
use crate::utils::utils_helpers::{load_settings, save_settings};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_updater::UpdaterExt;

#[tauri::command]
pub async fn get_clipboard_history(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<Vec<String>, String> {
    let state_guard = state.lock().unwrap();
    let manager = state_guard.clipboard_manager.lock().unwrap();
    Ok(manager.get_history())
}

#[tauri::command]
pub async fn select_and_fill(
    index: usize,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    app: AppHandle,
) -> Result<String, String> {
    // 获取要选择的项目内容
    let item_content = {
        let state_guard = state.lock().unwrap();
        let manager = state_guard.clipboard_manager.lock().unwrap();
        let history = manager.get_history();

        if let Some(item) = history.get(index) {
            item.clone()
        } else {
            let error_msg = format!("索引 {} 超出范围", index);
            log::info!("{}", error_msg);
            return Err(error_msg);
        }
    };

    // 设置处理状态
    {
        let mut state_guard = state.lock().unwrap();
        state_guard.is_updating_clipboard = true;
        state_guard.is_processing_selection = true;
    }

    // 尝试设置剪贴板内容
    let result = {
        let state_guard = state.lock().unwrap();
        let manager = state_guard.clipboard_manager.lock().unwrap();
        manager.set_clipboard_content(&app, &item_content)
    };

    // 清理状态 - 确保无论如何都会清理
    {
        let mut state_guard = state.lock().unwrap();
        state_guard.is_updating_clipboard = false;
        // 注意：is_processing_selection将在成功路径中保持为true直到粘贴完成
    }

    match result {
        Ok(_) => {
            log::info!("成功复制内容到剪贴板");

            // 隐藏窗口
            let app_handle = app.clone();
            let state_clone = state.inner().clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(50));
                hide_clipboard_window(app_handle, state_clone.clone());
            });

            // 模拟粘贴操作
            let app_handle = app.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(100));
                crate::ui::window_manager::simulate_paste();

                // 在粘贴完成后最终清理状态
                if let Some(state_guard) = app_handle.try_state::<Arc<Mutex<SharedAppState>>>() {
                    if let Ok(mut guard) = state_guard.lock() {
                        guard.is_processing_selection = false;
                        log::debug!("已完成粘贴操作，清理处理状态");
                    }
                }
            });

            Ok(item_content)
        }
        Err(e) => {
            let error_msg = format!("复制到剪贴板失败: {}", e);
            log::error!("{}", error_msg);

            // 在错误路径中也要清理所有状态
            {
                let mut state_guard = state.lock().unwrap();
                state_guard.is_processing_selection = false;
                log::debug!("复制失败，已清理处理状态");
            }

            Err(error_msg)
        }
    }
}

#[tauri::command]
pub async fn remove_clipboard_item(
    index: usize,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    log::info!("删除剪贴板项目，索引: {}", index);
    let state_guard = state.lock().unwrap();
    let manager = state_guard.clipboard_manager.lock().unwrap();
    manager.remove_from_history(index)?;
    Ok(())
}

#[tauri::command]
pub async fn window_blur(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
    app: AppHandle,
) -> Result<(), String> {
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
pub async fn selection_toolbar_blur(app: AppHandle) -> Result<(), String> {
    if let Some(toolbar_window) = app.get_webview_window("selection_toolbar") {
        let _ = toolbar_window.hide();
    }
    Ok(())
}

#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<bool, String> {
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
                    let mut downloaded = 0;
                    if should_update {
                        update
                            .download_and_install(
                                |chunk_length, content_length| {
                                    downloaded += chunk_length;
                                    println!("已下载 {downloaded} / {content_length:?}");
                                },
                                || {
                                    println!("下载结束");
                                },
                            )
                            .await
                            .map_err(|e| e.to_string())?;
                        Ok(true)
                    } else {
                        Ok(false)
                    }
                } else {
                    Ok(false)
                }
            }
            Err(e) => Err(e.to_string()),
        },
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn get_ai_settings() -> Result<HashMap<String, serde_json::Value>, String> {
    let settings = load_settings()?;

    // 转换为HashMap格式，便于前端处理
    let mut result = HashMap::new();

    // 添加基本设置
    result.insert(
        "version".to_string(),
        serde_json::Value::String(settings.version.clone()),
    );
    result.insert(
        "max_items".to_string(),
        serde_json::Value::Number(serde_json::Number::from(settings.max_items)),
    );
    result.insert(
        "ai_provider".to_string(),
        serde_json::Value::String(settings.ai_provider.clone()),
    );
    result.insert(
        "hot_key".to_string(),
        serde_json::Value::String(settings.hot_key.clone()),
    );

    // 处理provider_configs，将encrypted_api_key替换为解密后的api_key
    let mut provider_configs_map: HashMap<String, serde_json::Value> = HashMap::new();

    // 先收集所有提供商键名，避免借用冲突
    let provider_keys: Vec<String> = settings.provider_configs.keys().cloned().collect();

    for provider_key in provider_keys.iter() {
        // 解密API密钥
        if let Ok(api_key) = settings.decrypt_provider_api_key(provider_key) {
            if let Some(decrypted_config) = settings.provider_configs.get(provider_key) {
                let mut config_map = HashMap::new();
                config_map.insert(
                    "api_url".to_string(),
                    serde_json::Value::String(decrypted_config.api_url.clone()),
                );
                config_map.insert(
                    "model_name".to_string(),
                    serde_json::Value::String(decrypted_config.model_name.clone()),
                );
                config_map.insert("api_key".to_string(), serde_json::Value::String(api_key));
                // 注意：这里不再包含encrypted_api_key字段

                provider_configs_map.insert(
                    provider_key.clone(),
                    serde_json::Value::Object(config_map.into_iter().collect()),
                );
            }
        }
    }

    result.insert(
        "provider_configs".to_string(),
        serde_json::Value::Object(provider_configs_map.into_iter().collect()),
    );

    Ok(result)
}

#[tauri::command]
pub async fn save_app_settings(
    max_items: usize,
    ai_provider: String,
    ai_api_url: String,
    ai_model_name: String,
    ai_api_key: String,
    hot_key: String,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let version = app.package_info().version.to_string();

    let mut settings = {
        let state_guard = state.lock().unwrap();
        state_guard.settings.clone()
    };

    settings.version = version;
    settings.max_items = max_items;

    if hot_key.is_empty() {
        return Err("快捷键不能为空".to_string());
    }

    if ai_provider.is_empty() {
        return Err("提供商名称不能为空".to_string());
    }
    if hot_key != settings.hot_key {
        if app.global_shortcut().is_registered(hot_key.as_str()) {
            return Err("快捷键冲突".to_string());
        }

        app.global_shortcut()
            .unregister(settings.hot_key.as_str())
            .map_err(|e| format!("保存配置失败: {}", e.to_string()))?;
        let app_clone = app.clone();
        let state_clone = state.inner().clone();
        app.global_shortcut()
            .on_shortcut(hot_key.as_str(), move |_app, _shortcut, event| {
                if let ShortcutState::Pressed = event.state {
                    let sg = state_clone.lock().unwrap();
                    if !sg.is_visible && !sg.is_processing_selection {
                        let state_for_window = state_clone.clone();
                        drop(sg);
                        show_clipboard_window(app_clone.clone(), state_for_window);
                        features::mouse_listener::reset_ctrl_key_state();
                    }
                }
            })
            .map_err(|e| e.to_string())?;
    }

    settings.hot_key = hot_key;
    settings.ai_provider = ai_provider.clone();

    settings.migrate_from_old();

    let config = settings
        .provider_configs
        .entry(ai_provider.clone())
        .or_insert_with(|| ProviderConfig::default());

    config.api_url = ai_api_url;
    config.model_name = ai_model_name;
    // api_key 不再存储在config中，直接用于加密

    settings
        .save_current_provider_config(&ai_api_key)
        .map_err(|e| format!("保存提供商配置失败: {}", e))?;

    settings
        .validate()
        .map_err(|e| format!("设置验证失败: {}", e))?;

    save_settings(&settings).map_err(|e| e.to_string())?;

    {
        let mut state_guard = state.lock().unwrap();
        state_guard.settings = settings;
    }

    log::info!(
        "设置保存成功: max_items={}, provider={}",
        max_items,
        ai_provider
    );
    Ok(())
}

#[tauri::command]
pub async fn test_ai_connection(
    ai_api_url: String,
    ai_model_name: String,
    ai_api_key: String,
) -> Result<String, String> {
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
pub async fn copy_text(text: String, app: AppHandle) -> Result<(), String> {
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

#[tauri::command]
pub async fn get_provider_config(provider: AIProvider) -> Result<(String, String), String> {
    let (url, model) = provider.get_default_config();
    Ok((url, model))
}

/// 获取所有已配置的提供商列表（包括自定义提供商）
#[tauri::command]
pub async fn get_all_configured_providers(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<Vec<(String, String)>, String> {
    let state_guard = state.lock().unwrap();
    let settings = &state_guard.settings;

    let mut providers: Vec<(String, String)> = Vec::new();

    for (provider_key, _) in &settings.provider_configs {
        providers.push((provider_key.clone(), provider_key.clone()));
    }

    Ok(providers)
}
