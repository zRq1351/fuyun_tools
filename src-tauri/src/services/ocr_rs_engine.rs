use crate::services::ocr_engine::{OcrLine, OcrParagraph, clean_ocr_text};
use ocr_rs::{OcrEngine, OcrEngineConfig};
use std::sync::{Mutex, OnceLock};

static ENGINE_CACHE: OnceLock<Mutex<Option<OcrEngine>>> = OnceLock::new();

/// 确保引擎已初始化（首次调用完成模型加载，后续调用直接返回）
fn ensure_engine(app_handle: &tauri::AppHandle) -> Result<(), String> {
    let cache = ENGINE_CACHE.get_or_init(|| Mutex::new(None));
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = Some(init_ocr_engine(app_handle)?);
        log::info!("OCR 引擎初始化完成（已缓存）");
    }
    Ok(())
}

fn init_ocr_engine(app_handle: &tauri::AppHandle) -> Result<OcrEngine, String> {
    use tauri::Manager;
    let resource_dir = app_handle.path().resource_dir()
        .map_err(|e| format!("获取资源目录失败: {}", e))?;

    let det_model = resource_dir.join("models").join("PP-OCRv5_mobile_det.mnn");
    let rec_model = resource_dir.join("models").join("PP-OCRv5_mobile_rec.mnn");
    let charset = resource_dir.join("models").join("ppocr_keys_v5.txt");

    if !det_model.exists() {
        return Err(format!("检测模型不存在: {:?}", det_model));
    }
    if !rec_model.exists() {
        return Err(format!("识别模型不存在: {:?}", rec_model));
    }
    if !charset.exists() {
        return Err(format!("字符集文件不存在: {:?}", charset));
    }

    let config = OcrEngineConfig::fast()
        .with_min_result_confidence(0.5);

    OcrEngine::new(
        det_model.to_str().ok_or("检测模型路径无效")?,
        rec_model.to_str().ok_or("识别模型路径无效")?,
        charset.to_str().ok_or("字符集路径无效")?,
        Some(config),
    )
        .map_err(|e| format!("初始化 OCR 引擎失败: {}", e))
}

/// 使用 ocr-rs 进行 OCR 识别（使用缓存的引擎）
pub async fn recognize_with_ocr_rs(image_data: &[u8], app_handle: &tauri::AppHandle) -> Result<Vec<OcrParagraph>, String> {
    log::info!("开始 OCR 识别（使用缓存引擎）...");

    // 加载图片
    let img = image::load_from_memory(image_data)
        .map_err(|e| format!("图片加载失败: {}", e))?;

    // 确保引擎已初始化（首次加载模型，后续跳过）
    ensure_engine(app_handle)?;

    // 执行 OCR（在阻塞线程中运行）
    let result = tokio::task::spawn_blocking(move || -> Result<Vec<OcrParagraph>, String> {
        let cache = ENGINE_CACHE.get().ok_or("引擎缓存未初始化")?;
        let guard = cache.lock().unwrap_or_else(|e| e.into_inner());
        let engine = guard.as_ref().ok_or("引擎未初始化")?;

        // 执行识别
        let ocr_results = engine.recognize(&img)
            .map_err(|e| format!("OCR识别失败: {}", e))?;
        
        log::info!("检测到 {} 个文本区域", ocr_results.len());
        
        // 转换为我们的格式
        let mut lines = Vec::new();
        
        for result in ocr_results {
            let bbox = &result.bbox;
            let text = result.text.clone();
            if text.trim().is_empty() {
                continue;
            }
            
            lines.push(OcrLine {
                text: clean_ocr_text(&text),
                x0: bbox.rect.left() as f64,
                y0: bbox.rect.top() as f64,
                x1: (bbox.rect.left() + bbox.rect.width() as i32) as f64,
                y1: (bbox.rect.top() + bbox.rect.height() as i32) as f64,
                confidence: Some(result.confidence),
            });
        }
        
        if lines.is_empty() {
            return Ok(Vec::new());
        }

        // 按 Y 坐标排序（从上到下）
        lines.sort_by(|a, b| a.y0.partial_cmp(&b.y0).unwrap_or(std::cmp::Ordering::Equal));

        // 将相近的行合并为段落
        let paragraphs = merge_lines_to_paragraphs(lines);
        
        Ok(paragraphs)
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
    .map_err(|e| format!("OCR处理失败: {}", e))?;
    
    log::info!("ocr-rs 识别完成，检测到 {} 个段落", result.len());
    
    Ok(result)
}

/// 将行合并为段落（Y 坐标相近的行合并）
fn merge_lines_to_paragraphs(lines: Vec<OcrLine>) -> Vec<OcrParagraph> {
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
fn create_paragraph(lines: Vec<OcrLine>) -> OcrParagraph {
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

    OcrParagraph {
        text: texts.join("\n"),
        x0: min_x,
        y0: min_y,
        x1: max_x,
        y1: max_y,
        lines,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, y0: f64) -> OcrLine {
        OcrLine {
            text: text.to_string(),
            x0: 0.0,
            y0,
            x1: 100.0,
            y1: y0 + 15.0,
            confidence: None,
        }
    }

    #[test]
    fn test_merge_empty_lines() {
        assert!(merge_lines_to_paragraphs(vec![]).is_empty());
    }

    #[test]
    fn test_merge_close_lines_into_one_paragraph() {
        // 与段落首行 y 比较：10→25 差距 15 < 20 合并；40-10=30 >= 20 拆段
        let lines = vec![line("第一行", 10.0), line("第二行", 25.0), line("第三行", 40.0)];
        let paragraphs = merge_lines_to_paragraphs(lines);
        assert_eq!(paragraphs.len(), 2);
        assert_eq!(paragraphs[0].text, "第一行\n第二行");
        assert_eq!(paragraphs[0].lines.len(), 2);
        assert_eq!(paragraphs[0].y0, 10.0);
        assert_eq!(paragraphs[0].y1, 40.0);
        assert_eq!(paragraphs[1].text, "第三行");
    }

    #[test]
    fn test_merge_all_close_to_first_line() {
        // 各行与首行 10 的差距都 < 20 → 单段
        let lines = vec![line("A", 10.0), line("B", 25.0), line("C", 29.0)];
        let paragraphs = merge_lines_to_paragraphs(lines);
        assert_eq!(paragraphs.len(), 1);
        assert_eq!(paragraphs[0].lines.len(), 3);
    }

    #[test]
    fn test_merge_far_lines_split_paragraphs() {
        let lines = vec![line("标题", 10.0), line("正文", 100.0), line("结尾", 110.0)];
        let paragraphs = merge_lines_to_paragraphs(lines);
        assert_eq!(paragraphs.len(), 2);
        assert_eq!(paragraphs[0].text, "标题");
        assert_eq!(paragraphs[1].text, "正文\n结尾");
    }

    #[test]
    fn test_merge_boundary_gap_20px_split() {
        // 差距恰好 20 像素：严格小于 20 才合并，20 应拆段
        let lines = vec![line("A", 10.0), line("B", 30.0)];
        let paragraphs = merge_lines_to_paragraphs(lines);
        assert_eq!(paragraphs.len(), 2);

        // 差距 19 像素应合并
        let lines2 = vec![line("A", 10.0), line("B", 29.0)];
        let paragraphs2 = merge_lines_to_paragraphs(lines2);
        assert_eq!(paragraphs2.len(), 1);
    }

    #[test]
    fn test_merge_single_line() {
        let lines = vec![line("只有一行", 5.0)];
        let paragraphs = merge_lines_to_paragraphs(lines);
        assert_eq!(paragraphs.len(), 1);
        assert_eq!(paragraphs[0].text, "只有一行");
        assert_eq!(paragraphs[0].x0, 0.0);
        assert_eq!(paragraphs[0].x1, 100.0);
    }

    #[test]
    fn test_merge_sorts_text_by_join() {
        let lines = vec![line("乙", 10.0), line("甲", 20.0)];
        let paragraphs = merge_lines_to_paragraphs(lines);
        // 保持输入顺序连接
        assert_eq!(paragraphs[0].text, "乙\n甲");
    }
}
