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
    let err_clone = err.clone();
    let code_str = err.code.to_string();
    let details = err.details.filter(|d| !d.is_empty());
    // 对没有明确 AppErrorKind 的错误，包装为通用错误
    let payload = FrontendErrorPayload {
        code: format!("E_{}", code_str),
        category: code_str,
        message: err.message,
        params: None,
        details,
    };
    serde_json::to_string(&payload).unwrap_or_else(|_| {
        crate::core::error::to_frontend_error_string(err_clone)
    })
}

/// 检查字符串是否为有效的前端错误 JSON
pub fn is_frontend_error_json(s: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(s)
        .ok()
        .map(|v| v.get("code").is_some())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::error::ErrorCode;
    use crate::core::error_codes::AppErrorKind;

    #[test]
    fn test_to_frontend_error_json_has_code_and_message() {
        let s = to_frontend_error_json(AppErrorKind::ClipboardItemNotFound);
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["code"], "E_CLIPBOARD_ITEM_NOT_FOUND");
        assert_eq!(v["category"], "CLIPBOARD_ERROR");
        assert_eq!(v["message"], "找不到目标项目");
        assert!(v.get("details").is_none());
        assert!(v.get("params").is_none());
    }

    #[test]
    fn test_to_frontend_error_json_with_params() {
        let s = to_frontend_error_json_with_params(
            AppErrorKind::SettingsHotkeyConflict,
            serde_json::json!({"key": "Alt+R"}),
        );
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["code"], "E_SETTINGS_HOTKEY_CONFLICT");
        assert_eq!(v["params"]["key"], "Alt+R");
    }

    #[test]
    fn test_to_frontend_error_json_with_details() {
        let s = to_frontend_error_json_with_details(
            AppErrorKind::DatabaseError,
            None,
            "db locked".to_string(),
        );
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["details"], "db locked");
    }

    #[test]
    fn test_to_frontend_error_json_empty_details_omitted() {
        let s = to_frontend_error_json_with_details(AppErrorKind::IoError, None, String::new());
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert!(v.get("details").is_none());
    }

    #[test]
    fn test_app_error_to_frontend_json() {
        let err = AppError::new(ErrorCode::SystemError, "系统出错");
        let s = app_error_to_frontend_json(err);
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["code"], "E_SYSTEM_ERROR");
        assert_eq!(v["category"], "SYSTEM_ERROR");
        assert_eq!(v["message"], "系统出错");
    }

    #[test]
    fn test_app_error_to_frontend_json_with_details() {
        let err = AppError::new(ErrorCode::NetworkError, "网络错误").with_details("timeout");
        let s = app_error_to_frontend_json(err);
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["details"], "timeout");
    }

    #[test]
    fn test_is_frontend_error_json() {
        assert!(is_frontend_error_json(&to_frontend_error_json(AppErrorKind::Unknown)));
        assert!(!is_frontend_error_json("plain text"));
        assert!(!is_frontend_error_json(""));
        assert!(!is_frontend_error_json("{\"noCode\": 1}"));
    }

    #[test]
    fn test_frontend_error_payload_serializes_camel_case() {
        let payload = FrontendErrorPayload {
            code: "E_TEST".to_string(),
            category: "SYSTEM_ERROR".to_string(),
            message: "msg".to_string(),
            params: Some(serde_json::json!({"a": 1})),
            details: None,
        };
        let s = serde_json::to_string(&payload).unwrap();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["code"], "E_TEST");
        assert!(v.get("details").is_none());
    }
}
