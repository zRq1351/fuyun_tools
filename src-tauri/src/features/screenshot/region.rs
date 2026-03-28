use serde::{Deserialize, Serialize};

/// 截图区域选择结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionRegion {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl SelectionRegion {
    /// 创建新的区域
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// 从两个点创建区域（起始点和结束点）
    pub fn from_points(x1: i32, y1: i32, x2: i32, y2: i32) -> Self {
        let x = x1.min(x2);
        let y = y1.min(y2);
        let width = (x2 - x1).unsigned_abs();
        let height = (y2 - y1).unsigned_abs();

        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// 验证区域是否有效
    pub fn is_valid(&self) -> bool {
        self.width > 0 && self.height > 0
    }

    /// 获取右下角坐标
    pub fn bottom_right(&self) -> (i32, i32) {
        (
            self.x + self.width as i32,
            self.y + self.height as i32,
        )
    }

    /// 计算面积
    pub fn area(&self) -> u64 {
        self.width as u64 * self.height as u64
    }
}

/// 截图模式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ScreenshotMode {
    /// 区域选择
    Region,
    /// 全屏
    FullScreen,
    /// 窗口
    Window,
    /// 滚动截图
    Scroll,
}

impl Default for ScreenshotMode {
    fn default() -> Self {
        Self::Region
    }
}

/// 截图配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotConfig {
    /// 截图模式
    pub mode: ScreenshotMode,
    /// 延迟时间（秒）
    pub delay_seconds: u32,
    /// 是否包含鼠标指针
    pub include_cursor: bool,
    /// 保存路径
    pub save_path: Option<String>,
    /// 自动复制到剪贴板
    pub auto_copy: bool,
}

impl Default for ScreenshotConfig {
    fn default() -> Self {
        Self {
            mode: ScreenshotMode::default(),
            delay_seconds: 0,
            include_cursor: false,
            save_path: None,
            auto_copy: true,
        }
    }
}

/// 截图结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotResult {
    /// 是否成功
    pub success: bool,
    /// 图片宽度
    pub width: u32,
    /// 图片高度
    pub height: u32,
    /// PNG Base64数据（用于预览）
    pub png_base64: Option<String>,
    /// 保存路径
    pub saved_path: Option<String>,
    /// 错误信息
    pub error: Option<String>,
}

impl ScreenshotResult {
    /// 创建成功结果
    pub fn success(width: u32, height: u32, png_base64: Option<String>) -> Self {
        Self {
            success: true,
            width,
            height,
            png_base64,
            saved_path: None,
            error: None,
        }
    }

    /// 创建失败结果
    pub fn error(error: String) -> Self {
        Self {
            success: false,
            width: 0,
            height: 0,
            png_base64: None,
            saved_path: None,
            error: Some(error),
        }
    }

    /// 设置保存路径
    pub fn with_saved_path(mut self, path: String) -> Self {
        self.saved_path = Some(path);
        self
    }
}