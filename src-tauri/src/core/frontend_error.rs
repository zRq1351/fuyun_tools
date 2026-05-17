use crate::core::error::AppError;
use crate::core::error_codes::AppErrorKind;
use serde::Serialize;

/// 发送给前端的结构化错误（JSON 字符串格式）
/// 前端通过 JSON.parse 解析错误，根据 code 查找 i18n 翻译
#[derive(Debug, Serialize)]
pub struct FrontendErrorPayload {
    /// 机器可读错误码，用于 i18n 查找（例如 "E_HOTKEY_CONFLICT"）
    pub code: String,
    /// 错误分类码，用于兜底分组（CONFIG_ERROR, NETWORK_ERROR 等）
    pub category: String,
    /// 默认中文消息，作为 i18n 缺失时的兜底文本
    pub message: String,
    /// 可选的 i18n 消息模板参数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
    /// 技术详情（仅用于调试）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

/// 创建带错误码的前端 JSON 错误字符串
pub fn to_frontend_error_json(kind: AppErrorKind) -> String {
    let category = kind.category().to_string();
    let code = format!("E_{}", kind.to_key());
    let message = kind.default_message().to_string();
    let payload = FrontendErrorPayload {
        code,
        category,
        message,
        params: None,
        details: None,
    };
    serde_json::to_string(&payload).unwrap_or_else(|_| {
        format!(r#"{{"code":"E_UNKNOWN","category":"SYSTEM_ERROR","message":"{}"}}"#, kind.default_message())
    })
}

/// 创建带错误码和参数的前端 JSON 错误字符串
pub fn to_frontend_error_json_with_params(kind: AppErrorKind, params: serde_json::Value) -> String {
    let category = kind.category().to_string();
    let code = format!("E_{}", kind.to_key());
    let message = kind.default_message().to_string();
    let payload = FrontendErrorPayload {
        code,
        category,
        message,
        params: Some(params),
        details: None,
    };
    serde_json::to_string(&payload).unwrap_or_else(|_| {
        format!(r#"{{"code":"E_UNKNOWN","category":"SYSTEM_ERROR","message":"{}"}}"#, kind.default_message())
    })
}

/// 创建带错误码、参数和技术详情的前端 JSON 错误字符串
pub fn to_frontend_error_json_with_details(
    kind: AppErrorKind,
    params: Option<serde_json::Value>,
    details: String,
) -> String {
    let category = kind.category().to_string();
    let code = format!("E_{}", kind.to_key());
    let message = kind.default_message().to_string();
    let payload = FrontendErrorPayload {
        code,
        category,
        message,
        params,
        details: if details.is_empty() { None } else { Some(details) },
    };
    serde_json::to_string(&payload).unwrap_or_else(|_| {
        format!(r#"{{"code":"E_UNKNOWN","category":"SYSTEM_ERROR","message":"{}"}}"#, kind.default_message())
    })
}

/// 将 AppError 转换为新的 JSON 格式，失败时回退到旧格式
/// 用于迁移期间的向后兼容
pub fn app_error_to_frontend_json(err: AppError) -> String {
    let details = err.details.clone();
    let message = err.message.clone();
    // 对没有明确 AppErrorKind 的错误，包装为通用错误
    let payload = FrontendErrorPayload {
        code: format!("E_{}", err.code.to_string()),
        category: err.code.to_string(),
        message,
        params: None,
        details: details.filter(|d| !d.is_empty()),
    };
    serde_json::to_string(&payload).unwrap_or_else(|_| {
        crate::core::error::to_frontend_error_string(err)
    })
}

/// 检查字符串是否为有效的前端错误 JSON
pub fn is_frontend_error_json(s: &str) -> bool {
    s.starts_with("{\"code\":\"E_")
}
