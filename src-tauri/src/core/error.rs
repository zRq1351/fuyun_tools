use serde::Serialize;
use std::fmt;

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
