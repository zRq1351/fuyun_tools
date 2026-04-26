use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrLine {
    pub text: String,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrParagraph {
    pub text: String,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub lines: Vec<OcrLine>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrResult {
    pub paragraphs: Vec<OcrParagraph>,
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
            let result = crate::services::native_ocr::recognize_png_bytes(png_bytes).await?;
            
            // 转换为统一格式
            Ok(OcrResult {
                paragraphs: result.paragraphs.into_iter().map(|p| OcrParagraph {
                    text: p.text,
                    x0: p.x0,
                    y0: p.y0,
                    x1: p.x1,
                    y1: p.y1,
                    lines: p.lines.into_iter().map(|l| OcrLine {
                        text: l.text,
                        x0: l.x0,
                        y0: l.y0,
                        x1: l.x1,
                        y1: l.y1,
                    }).collect(),
                }).collect(),
            })
        }
        OcrEngineType::OcrRs => {
            log::info!("使用 ocr-rs (Rust) 引擎");
            
            // 调用 ocr-rs
            let paragraphs = crate::services::ocr_rs_engine::recognize_with_ocr_rs(png_bytes, app_handle).await?;
            
            // 转换为统一格式
            Ok(OcrResult {
                paragraphs: paragraphs.into_iter().map(|p| OcrParagraph {
                    text: p.text,
                    x0: p.x0,
                    y0: p.y0,
                    x1: p.x1,
                    y1: p.y1,
                    lines: p.lines.into_iter().map(|l| OcrLine {
                        text: l.text,
                        x0: l.x0,
                        y0: l.y0,
                        x1: l.x1,
                        y1: l.y1,
                    }).collect(),
                }).collect(),
            })
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
