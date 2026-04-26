use ocr_rs::{OcrEngine, OcrEngineConfig};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct OcrLine {
    pub text: String,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub confidence: f32,
}

#[derive(Serialize, Deserialize)]
pub struct OcrParagraph {
    pub text: String,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub lines: Vec<OcrLine>,
}

/// 使用 ocr-rs 进行 OCR 识别
pub async fn recognize_with_ocr_rs(png_base64: &str, app_handle: &tauri::AppHandle) -> Result<Vec<OcrParagraph>, String> {
    use base64::Engine;
    use tauri::Manager;
    
    log::info!("初始化 ocr-rs 引擎...");
    
    // 解码 base64
    let image_data = base64::engine::general_purpose::STANDARD
        .decode(png_base64)
        .map_err(|e| format!("Base64解码失败: {}", e))?;
    
    // 加载图片
    let img = image::load_from_memory(&image_data)
        .map_err(|e| format!("图片加载失败: {}", e))?;
    
    log::info!("开始 OCR 识别...");
    
    let resource_dir = app_handle.path().resource_dir().map_err(|e| format!("获取资源目录失败: {}", e))?;

    // 执行 OCR（在阻塞线程中运行）
    let result = tokio::task::spawn_blocking(move || {
        // 获取模型路径（从资源目录）
        let base_dir = resource_dir;

        let det_model = base_dir.join("models").join("PP-OCRv5_mobile_det.mnn");
        let rec_model = base_dir.join("models").join("PP-OCRv5_mobile_rec.mnn");
        let charset = base_dir.join("models").join("ppocr_keys_v5.txt");

        // 检查模型文件是否存在
        if !det_model.exists() {
            return Err(format!("检测模型不存在: {:?}", det_model));
        }
        if !rec_model.exists() {
            return Err(format!("识别模型不存在: {:?}", rec_model));
        }
        if !charset.exists() {
            return Err(format!("字符集文件不存在: {:?}", charset));
        }
        
        // 创建 OCR 配置（使用快速模式）
        let config = OcrEngineConfig::fast()
            .with_min_result_confidence(0.5);  // 最低置信度阈值
        
        // 创建 OCR 引擎
        let engine = OcrEngine::new(
            det_model.to_str().unwrap(),
            rec_model.to_str().unwrap(),
            charset.to_str().unwrap(),
            Some(config),
        )
        .map_err(|e| format!("初始化 OCR 引擎失败: {}", e))?;
        
        // 执行识别
        let ocr_results = engine.recognize(&img)
            .map_err(|e| format!("OCR识别失败: {}", e))?;
        
        log::info!("检测到 {} 个文本区域", ocr_results.len());
        
        // 转换为我们的格式
        let mut paragraphs = Vec::new();
        
        for result in ocr_results {
            let bbox = &result.bbox;
            
            // 创建段落（每个检测结果作为一个段落）
            paragraphs.push(OcrParagraph {
                text: result.text.clone(),
                x0: bbox.rect.left() as f64,
                y0: bbox.rect.top() as f64,
                x1: (bbox.rect.left() + bbox.rect.width() as i32) as f64,
                y1: (bbox.rect.top() + bbox.rect.height() as i32) as f64,
                lines: vec![OcrLine {
                    text: result.text,
                    x0: bbox.rect.left() as f64,
                    y0: bbox.rect.top() as f64,
                    x1: (bbox.rect.left() + bbox.rect.width() as i32) as f64,
                    y1: (bbox.rect.top() + bbox.rect.height() as i32) as f64,
                    confidence: result.confidence,
                }],
            });
        }
        
        Ok(paragraphs)
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
    .map_err(|e| format!("OCR处理失败: {}", e))?;
    
    log::info!("ocr-rs 识别完成，检测到 {} 个段落", result.len());
    
    Ok(result)
}
