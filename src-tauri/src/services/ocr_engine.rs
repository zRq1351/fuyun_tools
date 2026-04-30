use serde::Serialize;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OcrLine {
    pub text: String,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OcrParagraph {
    pub text: String,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub lines: Vec<OcrLine>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OcrResult {
    pub paragraphs: Vec<OcrParagraph>,
}

/// 清理 OCR 文本中的多余空格
/// - 中文文本：移除所有空格
/// - 英文文本：规范化空格（多个空格合并为一个）
/// - 中英混合：智能处理
pub fn clean_ocr_text(text: &str) -> String {
    if text.is_empty() {
        return text.to_string();
    }

    // 检测是否主要为中文（中文字符占比超过50%）
    let chinese_count = text.chars().filter(|c| {
        let cp = *c as u32;
        (cp >= 0x4E00 && cp <= 0x9FFF) ||  // CJK统一汉字
        (cp >= 0x3400 && cp <= 0x4DBF) ||  // CJK扩展A
        (cp >= 0x20000 && cp <= 0x2A6DF)   // CJK扩展B
    }).count();

    let total_chars = text.chars().filter(|c| !c.is_whitespace()).count();

    if total_chars == 0 {
        return text.to_string();
    }

    let chinese_ratio = chinese_count as f64 / total_chars as f64;

    if chinese_ratio > 0.5 {
        // 主要是中文：移除所有空格
        text.chars().filter(|c| !c.is_whitespace()).collect()
    } else {
        // 主要是英文或其他语言：规范化空格
        let mut result = String::with_capacity(text.len());
        let mut prev_was_space = false;

        for c in text.chars() {
            if c.is_whitespace() {
                if !prev_was_space && !result.is_empty() {
                    result.push(' ');
                    prev_was_space = true;
                }
            } else {
                result.push(c);
                prev_was_space = false;
            }
        }

        // 移除首尾空格
        result.trim().to_string()
    }
}

/// OCR 引擎类型
#[derive(Clone, Debug)]
pub enum OcrEngineType {
    /// Windows 原生 OCR（快速，中等准确率）
    WindowsNative,
    /// ocr-rs 原生 Rust OCR（基于 PaddleOCR + MNN，高准确率）
    OcrRs,
}

impl Default for OcrEngineType {
    fn default() -> Self {
        Self::OcrRs
    }
}

/// 统一的 OCR 识别接口
pub async fn recognize_image(png_bytes: &[u8], engine_type: OcrEngineType, app_handle: &tauri::AppHandle) -> Result<OcrResult, String> {
    match engine_type {
        OcrEngineType::WindowsNative => {
            log::debug!("使用 Windows 原生 OCR 引擎");
            crate::services::native_ocr::recognize_png_bytes(png_bytes).await
        }
        OcrEngineType::OcrRs => {
            log::info!("使用 ocr-rs (Rust) 引擎");
            let paragraphs = crate::services::ocr_rs_engine::recognize_with_ocr_rs(png_bytes, app_handle).await?;
            Ok(OcrResult { paragraphs })
        }
    }
}

// TODO: PaddleOCR 集成
// 当 rust-paddle-ocr 发布到 crates.io 后，可以启用此功能
// 
// 使用示例：
// ```rust
// use lazy_static::lazy_static;
// use std::sync::Arc;
// use tokio::sync::Mutex;
//
// lazy_static! {
//     static ref PADDLE_ENGINE: Result<Arc<Mutex<PaddleOcrEngine>>, String> = {
//         let config = PaddleOcrConfig::default();
//         match PaddleOcrEngine::new(config) {
//             Ok(engine) => Ok(Arc::new(Mutex::new(engine))),
//             Err(e) => Err(format!("初始化 PaddleOCR 引擎失败: {}", e)),
//         }
//     };
// }
// ```
