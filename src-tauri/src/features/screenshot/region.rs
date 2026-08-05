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
        (self.x + self.width as i32, self.y + self.height as i32)
    }

    /// 计算面积
    pub fn area(&self) -> u64 {
        self.width as u64 * self.height as u64
    }
}

/// 截图模式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum ScreenshotMode {
    /// 区域选择
    #[default]
    Region,
    /// 全屏
    FullScreen,
    /// 窗口
    Window,
    /// 滚动截图
    Scroll,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_region_new_and_accessors() {
        let r = SelectionRegion::new(10, 20, 100, 50);
        assert_eq!(r.x, 10);
        assert_eq!(r.y, 20);
        assert_eq!(r.width, 100);
        assert_eq!(r.height, 50);
        assert!(r.is_valid());
        assert_eq!(r.bottom_right(), (110, 70));
        assert_eq!(r.area(), 5000);
    }

    #[test]
    fn test_region_from_points_forward() {
        let r = SelectionRegion::from_points(5, 5, 105, 55);
        assert_eq!((r.x, r.y), (5, 5));
        assert_eq!((r.width, r.height), (100, 50));
        assert!(r.is_valid());
    }

    #[test]
    fn test_region_from_points_reversed() {
        // 拖拽方向相反时仍应正确归一化
        let r = SelectionRegion::from_points(105, 55, 5, 5);
        assert_eq!((r.x, r.y), (5, 5));
        assert_eq!((r.width, r.height), (100, 50));
    }

    #[test]
    fn test_region_from_points_negative_delta() {
        let r = SelectionRegion::from_points(50, 50, -10, -10);
        assert_eq!((r.x, r.y), (-10, -10));
        assert_eq!((r.width, r.height), (60, 60));
    }

    #[test]
    fn test_region_zero_size_invalid() {
        let r = SelectionRegion::new(0, 0, 0, 0);
        assert!(!r.is_valid());
        let r2 = SelectionRegion::new(0, 0, 10, 0);
        assert!(!r2.is_valid());
        let r3 = SelectionRegion::new(0, 0, 0, 10);
        assert!(!r3.is_valid());
    }

    #[test]
    fn test_region_serde_roundtrip() {
        let r = SelectionRegion::new(-5, 3, 640, 480);
        let json = serde_json::to_string(&r).unwrap();
        let back: SelectionRegion = serde_json::from_str(&json).unwrap();
        assert_eq!(back.x, -5);
        assert_eq!(back.width, 640);
    }

    #[test]
    fn test_screenshot_mode_default_and_serde() {
        assert_eq!(ScreenshotMode::default(), ScreenshotMode::Region);
        let json = serde_json::to_string(&ScreenshotMode::FullScreen).unwrap();
        assert_eq!(json, "\"FullScreen\"");
        let back: ScreenshotMode = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ScreenshotMode::FullScreen);
    }

    #[test]
    fn test_screenshot_config_default() {
        let cfg = ScreenshotConfig::default();
        assert_eq!(cfg.mode, ScreenshotMode::Region);
        assert_eq!(cfg.delay_seconds, 0);
        assert!(!cfg.include_cursor);
        assert!(cfg.save_path.is_none());
        assert!(cfg.auto_copy);
    }

    #[test]
    fn test_screenshot_result_success() {
        let res = ScreenshotResult::success(1920, 1080, Some("data".to_string()));
        assert!(res.success);
        assert_eq!(res.width, 1920);
        assert_eq!(res.height, 1080);
        assert_eq!(res.png_base64.as_deref(), Some("data"));
        assert!(res.saved_path.is_none());
        assert!(res.error.is_none());
    }

    #[test]
    fn test_screenshot_result_error() {
        let res = ScreenshotResult::error("失败".to_string());
        assert!(!res.success);
        assert_eq!(res.width, 0);
        assert_eq!(res.height, 0);
        assert_eq!(res.error.as_deref(), Some("失败"));
    }

    #[test]
    fn test_screenshot_result_with_saved_path() {
        let res = ScreenshotResult::success(100, 100, None).with_saved_path("C:/shot.png".to_string());
        assert_eq!(res.saved_path.as_deref(), Some("C:/shot.png"));
    }
}
