use crate::services::ocr_engine::{OcrLine, OcrParagraph, OcrResult, clean_ocr_text};

#[cfg(target_os = "windows")]
pub async fn recognize_png_bytes(png_bytes: &[u8]) -> Result<OcrResult, String> {
    use image::imageops::FilterType;
    use image::{DynamicImage, ImageFormat};
    use windows::Globalization::Language;
    use windows::Graphics::Imaging::BitmapDecoder;
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

    fn preprocess_png_bytes(input: &[u8]) -> Result<Vec<u8>, String> {
        let image =
            image::load_from_memory(input).map_err(|e| format!("OCR 预处理加载图片失败: {}", e))?;
        
        // 策略1：适度放大（2倍），平衡清晰度和性能
        let target_w = (image.width().max(1) * 2).min(4096);
        let target_h = (image.height().max(1) * 2).min(4096);
        let resized = image.resize_exact(target_w, target_h, FilterType::Lanczos3);
        
        // 转换为灰度图
        let grayscale = resized.grayscale();
        
        // 增强对比度：使用直方图均衡化的简化版本
        let mut rgba = grayscale.to_rgba8();
        
        // 计算最小和最大像素值用于对比度拉伸
        let mut min_val = 255u8;
        let mut max_val = 0u8;
        for px in rgba.pixels() {
            let v = px[0];
            if v < min_val { min_val = v; }
            if v > max_val { max_val = v; }
        }
        
        // 对比度拉伸：将[min_val, max_val]映射到[0, 255]
        let range = if max_val > min_val { max_val - min_val } else { 1 };
        for px in rgba.pixels_mut() {
            let v = px[0];
            // 对比度拉伸
            let stretched = ((v as u32 - min_val as u32) * 255 / range as u32) as u8;
            
            // 自适应二值化：根据局部统计调整阈值
            // 对于较暗的图片使用较低阈值，较亮的图片使用较高阈值
            let threshold = if stretched < 128 { 140 } else { 168 };
            let nv = if stretched < threshold { 0 } else { 255 };
            
            px[0] = nv;
            px[1] = nv;
            px[2] = nv;
            px[3] = 255;
        }
        
        let mut out = Vec::new();
        DynamicImage::ImageRgba8(rgba)
            .write_to(&mut std::io::Cursor::new(&mut out), ImageFormat::Png)
            .map_err(|e| format!("OCR 预处理编码失败: {}", e))?;
        Ok(out)
    }
    
    /// 轻度预处理：仅放大和灰度化，不进行二值化
    /// 适用于已经清晰的图片
    fn preprocess_png_bytes_light(input: &[u8]) -> Result<Vec<u8>, String> {
        let image =
            image::load_from_memory(input).map_err(|e| format!("OCR 轻度预处理加载图片失败: {}", e))?;
        
        // 仅放大1.5倍，保持更多细节
        let target_w = (image.width().max(1) * 3 / 2).min(4096);
        let target_h = (image.height().max(1) * 3 / 2).min(4096);
        let resized = image.resize_exact(target_w, target_h, FilterType::Lanczos3);
        
        // 转换为灰度图但不二值化
        let grayscale = resized.grayscale();
        let rgba = grayscale.to_rgba8();
        
        let mut out = Vec::new();
        DynamicImage::ImageRgba8(rgba)
            .write_to(&mut std::io::Cursor::new(&mut out), ImageFormat::Png)
            .map_err(|e| format!("OCR 轻度预处理编码失败: {}", e))?;
        Ok(out)
    }

    async fn run_windows_ocr(
        png_bytes: &[u8],
        language_tag: Option<&str>,
    ) -> Result<OcrResult, String> {
        let stream =
            InMemoryRandomAccessStream::new().map_err(|e| format!("OCR 创建内存流失败: {}", e))?;
        let writer = DataWriter::CreateDataWriter(&stream)
            .map_err(|e| format!("OCR 创建DataWriter失败: {}", e))?;
        writer
            .WriteBytes(png_bytes)
            .map_err(|e| format!("OCR 写入PNG字节失败: {}", e))?;
        writer
            .StoreAsync()
            .map_err(|e| format!("OCR StoreAsync失败: {}", e))?
            .await
            .map_err(|e| format!("OCR StoreAsync执行失败: {}", e))?;
        writer
            .FlushAsync()
            .map_err(|e| format!("OCR FlushAsync失败: {}", e))?
            .await
            .map_err(|e| format!("OCR FlushAsync执行失败: {}", e))?;
        stream
            .Seek(0)
            .map_err(|e| format!("OCR 流定位失败: {}", e))?;

        let decoder = BitmapDecoder::CreateAsync(&stream)
            .map_err(|e| format!("OCR 创建BitmapDecoder失败: {}", e))?
            .await
            .map_err(|e| format!("OCR BitmapDecoder执行失败: {}", e))?;
        let software_bitmap = decoder
            .GetSoftwareBitmapAsync()
            .map_err(|e| format!("OCR 获取SoftwareBitmap失败: {}", e))?
            .await
            .map_err(|e| format!("OCR 获取SoftwareBitmap执行失败: {}", e))?;

        let engine = if let Some(tag) = language_tag {
            let language = Language::CreateLanguage(&windows::core::HSTRING::from(tag))
                .map_err(|e| format!("OCR 创建语言对象失败: {}", e))?;
            OcrEngine::TryCreateFromLanguage(&language)
                .or_else(|_| OcrEngine::TryCreateFromUserProfileLanguages())
                .map_err(|e| format!("OCR 创建识别引擎失败: {}", e))?
        } else {
            OcrEngine::TryCreateFromUserProfileLanguages()
                .map_err(|e| format!("OCR 创建用户语言引擎失败: {}", e))?
        };

        let ocr_result = engine
            .RecognizeAsync(&software_bitmap)
            .map_err(|e| format!("OCR 识别任务创建失败: {}", e))?
            .await
            .map_err(|e| format!("OCR 识别执行失败: {}", e))?;

        let lines = ocr_result
            .Lines()
            .map_err(|e| format!("OCR 读取行结果失败: {}", e))?;
        let mut paragraph_lines = Vec::new();
        for i in 0..lines.Size().unwrap_or(0) {
            let line = lines
                .GetAt(i)
                .map_err(|e| format!("OCR 读取行失败: {}", e))?;
            let text = line
                .Text()
                .map_err(|e| format!("OCR 读取行文本失败: {}", e))?;
            let line_text = clean_ocr_text(&text.to_string());
            if line_text.is_empty() {
                continue;
            }
            let words = line
                .Words()
                .map_err(|e| format!("OCR 读取行词失败: {}", e))?;
            let mut min_x = f64::MAX;
            let mut min_y = f64::MAX;
            let mut max_x: f64 = 0.0;
            let mut max_y: f64 = 0.0;
            for wi in 0..words.Size().unwrap_or(0) {
                let word = words
                    .GetAt(wi)
                    .map_err(|e| format!("OCR 读取词失败: {}", e))?;
                let rect = word
                    .BoundingRect()
                    .map_err(|e| format!("OCR 读取词框失败: {}", e))?;
                min_x = min_x.min(rect.X as f64);
                min_y = min_y.min(rect.Y as f64);
                max_x = max_x.max((rect.X + rect.Width) as f64);
                max_y = max_y.max((rect.Y + rect.Height) as f64);
            }
            if !min_x.is_finite() || !min_y.is_finite() || max_x <= min_x || max_y <= min_y {
                min_x = 0.0;
                min_y = 0.0;
                max_x = (line_text.len() as f64 * 12.0).max(12.0);
                max_y = 22.0;
            }
            paragraph_lines.push(OcrLine {
                text: line_text,
                x0: min_x,
                y0: min_y,
                x1: max_x,
                y1: max_y,
                confidence: None,
            });
        }

        if paragraph_lines.is_empty() {
            return Ok(OcrResult {
                paragraphs: Vec::new(),
            });
        }

        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut max_x: f64 = 0.0;
        let mut max_y: f64 = 0.0;
        let mut text_lines = Vec::new();
        for line in &paragraph_lines {
            text_lines.push(line.text.clone());
            min_x = min_x.min(line.x0);
            min_y = min_y.min(line.y0);
            max_x = max_x.max(line.x1);
            max_y = max_y.max(line.y1);
        }

        Ok(OcrResult {
            paragraphs: vec![OcrParagraph {
                text: text_lines.join("\n"),
                x0: min_x,
                y0: min_y,
                x1: max_x,
                y1: max_y,
                lines: paragraph_lines,
            }],
        })
    }

    /// 增强的评分函数：考虑文本质量和长度
    fn score(result: &OcrResult) -> usize {
        result
            .paragraphs
            .iter()
            .map(|p| {
                let chars = p.text.chars().filter(|c| !c.is_whitespace()).count();
                let lines = p.lines.len();
                
                // 基础分数：字符数 + 行数奖励
                let base_score = chars + lines * 8;
                
                // 质量奖励：更长连续文本通常质量更高
                let quality_bonus = if chars > 50 { 20 } else if chars > 20 { 10 } else { 0 };
                
                // 惩罚：如果行数太多但字符很少，可能是识别错误
                let penalty = if lines > 0 && chars / lines < 3 { lines * 5 } else { 0 };
                
                base_score + quality_bonus - penalty
            })
            .sum()
    }

    let mut best_result = OcrResult {
        paragraphs: Vec::new(),
    };
    let mut best_score = 0usize;
    let mut last_error = String::new();

    // 优先尝试原图策略
    let original_attempts: Vec<(Option<&str>, &str)> = vec![
        (Some("zh-Hans"), "original-zh"),
        (Some("en-US"), "original-en"),
        (None, "original-auto"),
    ];

    for (lang, strategy_name) in original_attempts {
        match run_windows_ocr(&png_bytes, lang).await {
            Ok(result) => {
                let current_score = score(&result);
                log::debug!("OCR策略 {} 得分: {}", strategy_name, current_score);
                if current_score > best_score {
                    best_score = current_score;
                    best_result = result;
                    log::info!("OCR采用策略: {}, 得分: {}", strategy_name, current_score);
                }
            }
            Err(e) => {
                log::debug!("OCR策略 {} 失败: {}", strategy_name, e);
                last_error = e;
            }
        }
    }

    // 快速返回机制：如果原图识别得分较高，直接返回，避免耗时的图像增强
    if best_score >= 30 {
        log::info!("原图识别得分 {} >= 30，触发快速返回", best_score);
        return Ok(best_result);
    }

    // 如果原图效果不佳，在后台线程中执行耗时的图像增强
    let png_bytes_owned = png_bytes.to_vec();
    let png_bytes_clone1 = png_bytes_owned.clone();
    let enhanced_task = tokio::task::spawn_blocking(move || preprocess_png_bytes(&png_bytes_clone1).ok());
    
    let png_bytes_clone2 = png_bytes_owned.clone();
    let light_enhanced_task = tokio::task::spawn_blocking(move || preprocess_png_bytes_light(&png_bytes_clone2).ok());

    let enhanced_png = enhanced_task.await.unwrap_or(None);
    let light_enhanced_png = light_enhanced_task.await.unwrap_or(None);

    let mut attempts: Vec<(&[u8], Option<&str>, &str)> = Vec::new();

    // 增强版本（二值化）
    if let Some(enhanced) = enhanced_png.as_ref() {
        attempts.push((enhanced.as_slice(), Some("zh-Hans"), "enhanced-zh"));
        attempts.push((enhanced.as_slice(), Some("en-US"), "enhanced-en"));
        attempts.push((enhanced.as_slice(), None, "enhanced-auto"));
    }
    
    // 轻度增强版本（保留更多细节）
    if let Some(light) = light_enhanced_png.as_ref() {
        attempts.push((light.as_slice(), Some("zh-Hans"), "light-zh"));
        attempts.push((light.as_slice(), Some("en-US"), "light-en"));
        attempts.push((light.as_slice(), None, "light-auto"));
    }

    for (bytes, lang, strategy_name) in attempts {
        match run_windows_ocr(bytes, lang).await {
            Ok(result) => {
                let current_score = score(&result);
                log::debug!("OCR策略 {} 得分: {}", strategy_name, current_score);
                if current_score > best_score {
                    best_score = current_score;
                    best_result = result;
                    log::info!("OCR采用策略: {}, 得分: {}", strategy_name, current_score);
                }
            }
            Err(e) => {
                log::debug!("OCR策略 {} 失败: {}", strategy_name, e);
                last_error = e;
            }
        }
    }

    if best_score == 0 && !last_error.is_empty() {
        return Err(last_error);
    }
    Ok(best_result)
}

#[cfg(not(target_os = "windows"))]
pub async fn recognize_png_bytes(_png_bytes: &[u8]) -> Result<OcrResult, String> {
    Err("当前平台暂不支持本地原生OCR".to_string())
}
