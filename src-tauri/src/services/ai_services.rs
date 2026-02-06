use crate::core::app_state::AppState as SharedAppState;
use crate::services::ai_client::{AIClient, AIConfig};
use crate::ui::window_manager::{hide_selection_toolbar_impl, show_result_window, update_result_window};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

/// 验证AI提供商配置
fn validate_provider_config(state: &Arc<Mutex<SharedAppState>>) -> Result<(), String> {
    let state_guard = state.lock().unwrap();
    let settings = &state_guard.settings;

    // 检查是否选择了提供商
    if settings.ai_provider.is_empty() {
        return Err("未配置AI提供商，请在设置中选择提供商".to_string());
    }

    // 检查提供商配置是否存在
    if !settings.provider_configs.contains_key(&settings.ai_provider) {
        return Err(format!("未找到提供商 '{}' 的配置，请在设置中配置API信息", settings.ai_provider));
    }

    // 获取当前提供商配置
    let provider_config = settings.get_current_provider_config()
        .ok_or_else(|| format!("未找到提供商 '{}' 的配置，请在设置中配置API信息", settings.ai_provider))?;

    // 检查必要的配置项
    if provider_config.api_url.is_empty() {
        return Err("API地址不能为空，请在设置中填写正确的API地址".to_string());
    }

    if provider_config.model_name.is_empty() {
        return Err("模型名称不能为空，请在设置中填写正确的模型名称".to_string());
    }

    // 检查API密钥
    let api_key = settings.decrypt_provider_api_key(&settings.ai_provider)
        .unwrap_or_default();
    if api_key.is_empty() {
        return Err("API密钥未配置或无效，请在设置中填写正确的API密钥".to_string());
    }

    // 简单的URL格式验证
    if !provider_config.api_url.starts_with("http://") && !provider_config.api_url.starts_with("https://") {
        return Err("API地址格式不正确，请确保以 http:// 或 https:// 开头".to_string());
    }

    Ok(())
}

/// 获取或创建AI客户端
pub async fn get_or_create_ai_client(state: Arc<Mutex<SharedAppState>>) -> Result<AIClient, String> {
    // 先验证配置
    validate_provider_config(&state)?;
    
    let current_config = {
        let state_guard = state.lock().unwrap();
        let api_key = state_guard
            .settings
            .decrypt_provider_api_key(&state_guard.settings.ai_provider)
            .unwrap_or_default();
        let provider_config = state_guard.settings.get_current_provider_config()
            .ok_or("获取当前提供商配置失败")?;
        AIConfig {
            api_key,
            base_url: provider_config.api_url.clone(),
            model: provider_config.model_name.clone(),
        }
    };
    let client = AIClient::new(current_config).map_err(|e| format!("客户端初始化失败: {}", e))?;
    Ok(client)
}

#[tauri::command]
pub async fn stream_translate_text(
    text: String,
    source_language: String,
    target_language: String,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let client: AIClient = get_or_create_ai_client(state.inner().clone()).await?;

    show_result_window(
        "翻译结果".to_string(),
        "".to_string(),
        "translation".to_string(),
        text.clone(),
        app.clone(),
    )
        .await?;
    hide_selection_toolbar_impl(app.clone());
    // 直接使用传入的中文语言名称
    let source_language_name = source_language;
    let target_language_name = target_language;

    let messages = format!(
        "直接翻译下面的文字由{}翻译为:{}，不要有任何额外的内容输出文字输出。需要翻译内容为：\n\n{}",
        source_language_name, target_language_name, text
    );
    if let Some(window) = app.clone().get_webview_window("result_translation") {
        let _ = window.emit("result-clean", "");
    }
    let result = client
        .generate_text_stream(messages.as_str(), Some(1000), |content_chunk| {
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
pub async fn stream_explain_text(
    text: String,
    target_language: String,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<(), String> {
    let client: AIClient = get_or_create_ai_client(state.inner().clone()).await?;
    show_result_window(
        "解释结果".to_string(),
        "".to_string(),
        "explanation".to_string(),
        text.clone(),
        app.clone(),
    )
        .await?;
    hide_selection_toolbar_impl(app.clone());
    let target_language_name = target_language;

    let messages = format!(
        "请用{}200字内解释这段话：\n\n{}",
        target_language_name, text
    );
    if let Some(window) = app.clone().get_webview_window("result_explanation") {
        let _ = window.emit("result-clean", "");
    }
    let result = client
        .generate_text_stream(messages.as_str(), Some(1000), |content_chunk| {
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