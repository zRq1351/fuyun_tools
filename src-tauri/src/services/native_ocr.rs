use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeOcrLine {
    pub text: String,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeOcrParagraph {
    pub text: String,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub lines: Vec<NativeOcrLine>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeOcrResult {
    pub paragraphs: Vec<NativeOcrParagraph>,
}

#[cfg(target_os = "windows")]
pub async fn recognize_png_base64(png_base64: &str) -> Result<NativeOcrResult, String> {
    use base64::Engine;
    use image::imageops::FilterType;
    use image::{DynamicImage, ImageFormat};
    use windows::Globalization::Language;
    use windows::Graphics::Imaging::BitmapDecoder;
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

    let png_bytes = base64::engine::general_purpose::STANDARD
        .decode(png_base64)
        .map_err(|e| format!("OCR Base64解码失败: {}", e))?;

    fn preprocess_png_bytes(input: &[u8]) -> Result<Vec<u8>, String> {
        let image =
            image::load_from_memory(input).map_err(|e| format!("OCR 预处理加载图片失败: {}", e))?;
        let target_w = (image.width().max(1) * 2).min(4096);
        let target_h = (image.height().max(1) * 2).min(4096);
        let resized = image.resize_exact(target_w, target_h, FilterType::Lanczos3);
        let grayscale = resized.grayscale();
        let mut rgba = grayscale.to_rgba8();
        for px in rgba.pixels_mut() {
            let v = px[0];
            let nv = if v < 168 { 0 } else { 255 };
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

    async fn run_windows_ocr(
        png_bytes: &[u8],
        language_tag: Option<&str>,
    ) -> Result<NativeOcrResult, String> {
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
            let line_text = text.to_string().trim().to_string();
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
            paragraph_lines.push(NativeOcrLine {
                text: line_text,
                x0: min_x,
                y0: min_y,
                x1: max_x,
                y1: max_y,
            });
        }

        if paragraph_lines.is_empty() {
            return Ok(NativeOcrResult {
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

        Ok(NativeOcrResult {
            paragraphs: vec![NativeOcrParagraph {
                text: text_lines.join("\n"),
                x0: min_x,
                y0: min_y,
                x1: max_x,
                y1: max_y,
                lines: paragraph_lines,
            }],
        })
    }

    fn score(result: &NativeOcrResult) -> usize {
        result
            .paragraphs
            .iter()
            .map(|p| {
                let chars = p.text.chars().filter(|c| !c.is_whitespace()).count();
                chars + p.lines.len() * 8
            })
            .sum()
    }

    let enhanced_png = preprocess_png_bytes(&png_bytes).ok();
    let mut best_result = NativeOcrResult {
        paragraphs: Vec::new(),
    };
    let mut best_score = 0usize;
    let mut last_error = String::new();

    let mut attempts: Vec<(&[u8], Option<&str>)> = vec![
        (&png_bytes, Some("zh-Hans")),
        (&png_bytes, Some("en-US")),
        (&png_bytes, None),
    ];
    if let Some(enhanced) = enhanced_png.as_ref() {
        attempts.push((enhanced.as_slice(), Some("zh-Hans")));
        attempts.push((enhanced.as_slice(), Some("en-US")));
        attempts.push((enhanced.as_slice(), None));
    }

    for (bytes, lang) in attempts {
        match run_windows_ocr(bytes, lang).await {
            Ok(result) => {
                let current_score = score(&result);
                if current_score > best_score {
                    best_score = current_score;
                    best_result = result;
                }
            }
            Err(e) => {
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
pub async fn recognize_png_base64(_png_base64: &str) -> Result<NativeOcrResult, String> {
    Err("当前平台暂不支持本地原生OCR".to_string())
}
