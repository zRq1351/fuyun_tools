use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaddleOcrLine {
    pub text: String,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub confidence: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaddleOcrParagraph {
    pub text: String,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub lines: Vec<PaddleOcrLine>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaddleOcrResult {
    pub paragraphs: Vec<PaddleOcrParagraph>,
}

/// PaddleOCR 引擎配置
pub struct PaddleOcrConfig {
    /// 检测模型路径
    pub det_model_path: String,
    /// 识别模型路径
    pub rec_model_path: String,
    /// 字符集文件路径
    pub charset_path: String,
    /// 是否使用 GPU 加速
    pub use_gpu: bool,
    /// 识别语言（ch: 中文, en: 英文）
    pub language: String,
}

impl Default for PaddleOcrConfig {
    fn default() -> Self {
        Self {
            det_model_path: "models/PP-OCRv5_mobile_det.mnn".to_string(),
            rec_model_path: "models/PP-OCRv5_mobile_rec.mnn".to_string(),
            charset_path: "models/ppocr_keys_v5.txt".to_string(),
            use_gpu: false,
            language: "ch".to_string(),
        }
    }
}

/// PaddleOCR 识别引擎
#[cfg(feature = "paddle-ocr")]
pub struct PaddleOcrEngine {
    engine: rust_paddle_ocr::OcrEngine,
    config: PaddleOcrConfig,
}

#[cfg(feature = "paddle-ocr")]
impl PaddleOcrEngine {
    /// 创建新的 PaddleOCR 引擎实例
    pub fn new(config: PaddleOcrConfig) -> Result<Self, String> {
        log::info!("初始化 PaddleOCR 引擎...");
        log::info!("检测模型: {}", config.det_model_path);
        log::info!("识别模型: {}", config.rec_model_path);
        log::info!("字符集: {}", config.charset_path);

        let engine = rust_paddle_ocr::OcrEngine::new(
            &config.det_model_path,
            &config.rec_model_path,
            &config.charset_path,
            None, // 不使用 GPU
        )
        .map_err(|e| format!("初始化 PaddleOCR 引擎失败: {}", e))?;

        log::info!("PaddleOCR 引擎初始化成功");

        Ok(Self { engine, config })
    }

    /// 从 PNG base64 数据识别文字
    pub async fn recognize_png_base64(&self, png_base64: &str) -> Result<PaddleOcrResult, String> {
        use base64::Engine;

        // 解码 base64
        let png_bytes = base64::engine::general_purpose::STANDARD
            .decode(png_base64)
            .map_err(|e| format!("Base64 解码失败: {}", e))?;

        // 加载图片
        let image = image::load_from_memory(&png_bytes)
            .map_err(|e| format!("加载图片失败: {}", e))?;

        // 转换为 RGB
        let rgb_image = image.to_rgb8();
        let width = rgb_image.width();
        let height = rgb_image.height();

        log::debug!("开始 PaddleOCR 识别，图片尺寸: {}x{}", width, height);

        // 执行 OCR 识别
        let results = self
            .engine
            .recognize(&rgb_image)
            .map_err(|e| format!("PaddleOCR 识别失败: {}", e))?;

        log::info!("PaddleOCR 识别完成，检测到 {} 个文本框", results.len());

        // 转换结果为我们的格式
        let mut lines = Vec::new();
        for result in &results {
            let text = result.text.clone();
            if text.trim().is_empty() {
                continue;
            }

            // 获取文本框坐标
            let points = &result.points;
            if points.len() < 4 {
                continue;
            }

            // 计算包围盒
            let mut min_x = f64::MAX;
            let mut min_y = f64::MAX;
            let mut max_x = f64::MIN;
            let mut max_y = f64::MIN;

            for point in points {
                min_x = min_x.min(point[0] as f64);
                min_y = min_y.min(point[1] as f64);
                max_x = max_x.max(point[0] as f64);
                max_y = max_y.max(point[1] as f64);
            }

            lines.push(PaddleOcrLine {
                text: clean_ocr_text(&text),
                x0: min_x,
                y0: min_y,
                x1: max_x,
                y1: max_y,
                confidence: result.score,
            });
        }

        if lines.is_empty() {
            return Ok(PaddleOcrResult {
                paragraphs: Vec::new(),
            });
        }

        // 按 Y 坐标排序（从上到下）
        lines.sort_by(|a, b| a.y0.partial_cmp(&b.y0).unwrap_or(std::cmp::Ordering::Equal));

        // 将相近的行合并为段落
        let paragraphs = merge_lines_to_paragraphs(lines);

        Ok(PaddleOcrResult { paragraphs })
    }
}

/// 清理 OCR 文本中的多余空格（与 native_ocr.rs 保持一致）
fn clean_ocr_text(text: &str) -> String {
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

/// 将行合并为段落（Y 坐标相近的行合并）
fn merge_lines_to_paragraphs(lines: Vec<PaddleOcrLine>) -> Vec<PaddleOcrParagraph> {
    if lines.is_empty() {
        return Vec::new();
    }

    let mut paragraphs = Vec::new();
    let mut current_lines = vec![lines[0].clone()];
    let mut current_y = lines[0].y0;

    for line in lines.iter().skip(1) {
        // 如果 Y 坐标差距小于 20 像素，认为是同一段落
        if (line.y0 - current_y).abs() < 20.0 {
            current_lines.push(line.clone());
        } else {
            // 创建新段落
            paragraphs.push(create_paragraph(current_lines));
            current_lines = vec![line.clone()];
            current_y = line.y0;
        }
    }

    // 添加最后一个段落
    if !current_lines.is_empty() {
        paragraphs.push(create_paragraph(current_lines));
    }

    paragraphs
}

/// 创建段落
fn create_paragraph(lines: Vec<PaddleOcrLine>) -> PaddleOcrParagraph {
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    let mut texts = Vec::new();

    for line in &lines {
        min_x = min_x.min(line.x0);
        min_y = min_y.min(line.y0);
        max_x = max_x.max(line.x1);
        max_y = max_y.max(line.y1);
        texts.push(line.text.clone());
    }

    PaddleOcrParagraph {
        text: texts.join("\n"),
        x0: min_x,
        y0: min_y,
        x1: max_x,
        y1: max_y,
        lines,
    }
}

/// 非 paddle-ocr 特性时的占位实现
#[cfg(not(feature = "paddle-ocr"))]
pub struct PaddleOcrEngine;

#[cfg(not(feature = "paddle-ocr"))]
impl PaddleOcrEngine {
    pub fn new(_config: PaddleOcrConfig) -> Result<Self, String> {
        Err("PaddleOCR 功能未启用。请在编译时启用 'paddle-ocr' 特性。".to_string())
    }

    pub async fn recognize_png_base64(&self, _png_base64: &str) -> Result<PaddleOcrResult, String> {
        Err("PaddleOCR 功能未启用。请在编译时启用 'paddle-ocr' 特性。".to_string())
    }
}
