use serde::Serialize;
use std::fmt;
use std::panic;
use std::sync::OnceLock;

/// 应用程序错误代码
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub enum ErrorCode {
    /// 配置相关错误
    ConfigError,
    /// 网络/API相关错误
    NetworkError,
    /// 文件系统/IO错误
    IoError,
    /// 剪贴板操作错误
    ClipboardError,
    /// 系统/未知错误
    SystemError,
    /// 验证错误
    ValidationError,
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorCode::ConfigError => write!(f, "CONFIG_ERROR"),
            ErrorCode::NetworkError => write!(f, "NETWORK_ERROR"),
            ErrorCode::IoError => write!(f, "IO_ERROR"),
            ErrorCode::ClipboardError => write!(f, "CLIPBOARD_ERROR"),
            ErrorCode::SystemError => write!(f, "SYSTEM_ERROR"),
            ErrorCode::ValidationError => write!(f, "VALIDATION_ERROR"),
        }
    }
}

/// 统一的应用程序错误结构
#[derive(Debug, Serialize, Clone)]
pub struct AppError {
    /// 错误代码
    pub code: ErrorCode,
    /// 用户友好的错误消息
    pub message: String,
    /// 技术详情（可选，用于调试或日志）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl AppError {
    /// 创建新的 AppError
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    /// 添加技术详情
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}

/// 方便的 Result 类型别名
pub type AppResult<T> = Result<T, AppError>;

fn compact_error_details(raw: &str) -> String {
    let merged = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" | ");
    // 优化：使用字节长度代替 chars().count() 避免两次遍历
    if merged.len() <= 500 {
        return merged;
    }
    let end = merged
        .char_indices()
        .nth(500)
        .map(|(pos, _)| pos)
        .unwrap_or(merged.len());
    let mut shortened = merged[..end].to_string();
    shortened.push_str("...");
    shortened
}

pub fn to_frontend_error_string(err: AppError) -> String {
    match err.details.as_deref() {
        Some(details) if !details.trim().is_empty() => {
            format!(
                "[{}] {}；{}",
                err.code,
                err.message,
                compact_error_details(details)
            )
        }
        _ => format!("[{}] {}", err.code, err.message),
    }
}

static PANIC_HOOK_INSTALLED: OnceLock<()> = OnceLock::new();

pub fn install_global_panic_hook() {
    PANIC_HOOK_INSTALLED.get_or_init(|| {
        let default_hook = panic::take_hook();
        panic::set_hook(Box::new(move |panic_info| {
            let location = panic_info
                .location()
                .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
                .unwrap_or_else(|| "unknown".to_string());
            let payload = if let Some(msg) = panic_info.payload().downcast_ref::<&str>() {
                msg.to_string()
            } else if let Some(msg) = panic_info.payload().downcast_ref::<String>() {
                msg.clone()
            } else {
                "unknown panic payload".to_string()
            };
            let bt = std::panic::catch_unwind(std::backtrace::Backtrace::force_capture)
                .map(|bt| bt.to_string())
                .unwrap_or_else(|_| "backtrace capture failed".to_string());
            log::error!(
                "全局未捕获panic: location={}, payload={}, backtrace={}",
                location,
                payload,
                bt
            );
            default_hook(panic_info);
        }));
    });
}

// 实现从 String 到 AppError 的转换（默认为 SystemError）
impl From<String> for AppError {
    fn from(msg: String) -> Self {
        AppError::new(ErrorCode::SystemError, msg)
    }
}

// 实现从 &str 到 AppError 的转换
impl From<&str> for AppError {
    fn from(msg: &str) -> Self {
        AppError::new(ErrorCode::SystemError, msg)
    }
}

// 实现从 std::io::Error 到 AppError 的转换
impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::new(ErrorCode::IoError, err.to_string())
    }
}

// 实现从 sqlx::Error 到 AppError 的转换
impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::new(ErrorCode::SystemError, format!("数据库错误: {}", err))
    }
}

// 实现从 serde_json::Error 到 AppError 的转换
impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::new(ErrorCode::SystemError, format!("JSON 解析错误: {}", err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_display() {
        assert_eq!(ErrorCode::ConfigError.to_string(), "CONFIG_ERROR");
        assert_eq!(ErrorCode::NetworkError.to_string(), "NETWORK_ERROR");
        assert_eq!(ErrorCode::IoError.to_string(), "IO_ERROR");
        assert_eq!(ErrorCode::ClipboardError.to_string(), "CLIPBOARD_ERROR");
        assert_eq!(ErrorCode::SystemError.to_string(), "SYSTEM_ERROR");
        assert_eq!(ErrorCode::ValidationError.to_string(), "VALIDATION_ERROR");
    }

    #[test]
    fn test_app_error_new_without_details() {
        let err = AppError::new(ErrorCode::SystemError, "出错");
        assert_eq!(err.code, ErrorCode::SystemError);
        assert_eq!(err.message, "出错");
        assert!(err.details.is_none());
        assert_eq!(err.to_string(), "[SYSTEM_ERROR] 出错");
    }

    #[test]
    fn test_app_error_with_details() {
        let err = AppError::new(ErrorCode::IoError, "读取失败").with_details("path not found");
        assert_eq!(err.code, ErrorCode::IoError);
        assert_eq!(err.details.as_deref(), Some("path not found"));
    }

    #[test]
    fn test_from_string_and_str() {
        let err: AppError = "直接消息".into();
        assert_eq!(err.code, ErrorCode::SystemError);
        assert_eq!(err.message, "直接消息");

        let err2: AppError = String::from("字符串消息").into();
        assert_eq!(err2.code, ErrorCode::SystemError);
        assert_eq!(err2.message, "字符串消息");
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err: AppError = io_err.into();
        assert_eq!(err.code, ErrorCode::IoError);
        assert!(err.message.contains("file missing"));
    }

    #[test]
    fn test_from_sqlx_and_json_error() {
        let err: AppError = serde_json::from_str::<serde_json::Value>("{bad json")
            .unwrap_err()
            .into();
        assert_eq!(err.code, ErrorCode::SystemError);
        assert!(err.message.contains("JSON 解析错误"));
    }

    #[test]
    fn test_to_frontend_error_string_without_details() {
        let err = AppError::new(ErrorCode::ValidationError, "参数无效");
        assert_eq!(to_frontend_error_string(err), "[VALIDATION_ERROR] 参数无效");
    }

    #[test]
    fn test_to_frontend_error_string_with_details() {
        let err = AppError::new(ErrorCode::ClipboardError, "剪贴板失败").with_details("detail line1\n\ndetail line2");
        let s = to_frontend_error_string(err);
        assert!(s.starts_with("[CLIPBOARD_ERROR] 剪贴板失败；"));
        assert!(s.contains("detail line1"));
        assert!(s.contains("detail line2"));
        // 空行应被压缩
        assert!(!s.contains("\n\n"));
    }

    #[test]
    fn test_to_frontend_error_string_details_truncated_at_500() {
        let long = "x".repeat(600);
        let err = AppError::new(ErrorCode::SystemError, "长详情").with_details(long);
        let s = to_frontend_error_string(err);
        assert!(s.ends_with("..."));
        assert!(s.len() < 600 + 100);
    }

    #[test]
    fn test_compact_error_details_joins_and_trims() {
        assert_eq!(compact_error_details("  a \n\n  b \n"), "a | b");
        assert_eq!(compact_error_details(""), "");
    }

    #[test]
    fn test_install_global_panic_hook_idempotent() {
        install_global_panic_hook();
        install_global_panic_hook();
        // 不 panic 即通过（OnceLock 保证只安装一次）
    }
}
