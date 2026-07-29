use crate::core::app_state::AppState as SharedAppState;
use crate::core::error::ErrorCode;
use crate::core::error_codes::AppErrorKind;
use crate::core::perf_metrics::{record_perf_metric, timed_sync};
use crate::features;
use crate::sync::{lock_arc_mutex, Mutex};
use crate::ui::commands::{
    bind_screenshot_window_lifecycle,
    cleanup_all_screenshot_boot_images,
    now_unix_ms, replace_screenshot_boot_image_path, write_screenshot_boot_image,
    ManualLongshotSessionRequest, NEXT_PINNED_IMAGE_WINDOW_ID,
    NEXT_SCREENSHOT_SESSION_ID, SCREENSHOT_LIFECYCLE_BOUND_FOR_BOOT_WINDOW,
};
use crate::ui::commands_clipboard::{frontend_error, frontend_error_kind, frontend_error_kind_params, get_image_clipboard_manager_arc, is_screenshot_feature_enabled};
use crate::ui::commands_screenshot_render::{export_screenshot_image, render_screenshot_image, ScreenshotExportRequest};
use crate::ui::window_manager::{
    bind_overlay_window_events, ensure_window_for_label, focus_overlay_window_by_label,
    hide_overlay_window_by_label, show_overlay_window_by_label,
};
use crate::utils::image_clipboard::ImageClipboardManager;
use crate::utils::utils_helpers::load_settings;
use base64::Engine;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_positioner::WindowExt;

/// 截图会话数据，前端通过 `get_screenshot_data` IPC 主动拉取
struct ScreenshotSession {
    bmp_path: PathBuf,   // BMP 快速显示（无压缩，浏览器零解码）
    png_path: PathBuf,   // PNG 用于保存/复制/固定
    width: u32,
    height: u32,
    origin_x: i32,
    origin_y: i32,
    session_id: u64,
    selection_mode: String,
}

static SCREENSHOT_SESSION: std::sync::OnceLock<std::sync::Mutex<Option<ScreenshotSession>>> =
    std::sync::OnceLock::new();

fn screenshot_session_store() -> &'static std::sync::Mutex<Option<ScreenshotSession>> {
    SCREENSHOT_SESSION.get_or_init(|| std::sync::Mutex::new(None))
}

#[tauri::command]
pub async fn get_screenshot_data() -> Result<serde_json::Value, String> {
    let mut guard = screenshot_session_store()
        .lock()
        .map_err(|e| format!("锁获取失败: {}", e))?;
    match guard.take() {
        Some(session) => Ok(serde_json::json!({
            "success": true,
            "bmp_path": session.bmp_path,
            "image_path": session.png_path,
            "width": session.width,
            "height": session.height,
            "origin_x": session.origin_x,
            "origin_y": session.origin_y,
            "session_id": session.session_id,
            "selection_mode": session.selection_mode,
        })),
        None => Ok(serde_json::json!({ "success": false })),
    }
}

#[tauri::command]
pub async fn resize_selection_toolbar(
    app: AppHandle,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("selection_toolbar") {
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};
            if let Ok(hwnd) = window.hwnd() {
                unsafe {
                    let _ = SetWindowPos(
                        windows::Win32::Foundation::HWND(hwnd.0 as *mut core::ffi::c_void),
                        None,
                        x,
                        y,
                        width as i32,
                        height as i32,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = window.set_size(tauri::PhysicalSize::new(width, height));
            let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn copy_image_clipboard_item_to_directory(
    item_id: String,
    target_directory: String,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<serde_json::Value, String> {
    let state_arc = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let manager_arc = get_image_clipboard_manager_arc(&state_arc);
        let manager = lock_arc_mutex(&manager_arc);
        let source_path = manager.get_preview_image_path_by_id(&item_id)?;
        drop(manager);

        let source = PathBuf::from(&source_path);
        if !source.exists() {
            return Err(frontend_error_kind(AppErrorKind::ScreenshotSourceFileNotFound, source_path));
        }
        let file_name = source
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.is_empty())
            .ok_or_else(|| "无法解析源文件名".to_string())?
            .to_string();

        let target_dir = PathBuf::from(target_directory.trim());
        if target_dir.as_os_str().is_empty() {
            return Err(frontend_error_kind(AppErrorKind::ScreenshotTargetDirEmpty, ""));
        }

        // 防御性路径校验：拒绝包含 ".." 的路径（路径穿越攻击防护）
        if target_dir.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            return Err("不允许的路径：包含路径遍历".to_string());
        }

        fs::create_dir_all(&target_dir).map_err(|e| format!("创建目标目录失败: {}", e))?;

        let stem = Path::new(&file_name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("image");
        let ext = Path::new(&file_name)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("png");

        let mut target_path = target_dir.join(&file_name);
        // 防御性路径校验：拼接后的最终路径也不允许包含 ".."（防止文件名注入路径穿越）
        if target_path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            return Err("不允许的路径：包含路径遍历".to_string());
        }
        if target_path.exists() {
            for idx in 1..10000u32 {
                let candidate = target_dir.join(format!("{} ({idx}).{}", stem, ext));
                if !candidate.exists() {
                    target_path = candidate;
                    break;
                }
            }
        }

        fs::copy(&source, &target_path).map_err(|e| format!("复制图片失败: {}", e))?;
        Ok(serde_json::json!({
            "success": true,
            "sourcePath": source.to_string_lossy(),
            "savedPath": target_path.to_string_lossy(),
        }))
    })
        .await
        .map_err(|e| {
            frontend_error(
                ErrorCode::SystemError,
                "复制图片任务执行失败",
                e.to_string(),
            )
        })?
}

/// 开始截图（全屏）
#[tauri::command]
pub async fn start_screenshot(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<serde_json::Value, String> {
    use crate::features::screenshot::capture;
    if !is_screenshot_feature_enabled(state.inner()) {
        return Err(frontend_error(
            ErrorCode::ValidationError,
            "截图功能已停用",
            "screenshot feature disabled",
        ));
    }

    log::info!("开始全屏截图");

    match capture::capture_full_screen() {
        Ok((rgba, width, height, origin_x, origin_y)) => {
            let session_id = NEXT_SCREENSHOT_SESSION_ID.fetch_add(1, Ordering::SeqCst);
            let (image_path, png_data) = write_screenshot_boot_image(&rgba, width, height, session_id)
                .map_err(|e| frontend_error_kind_params(AppErrorKind::ScreenshotWriteSourceFailed, serde_json::json!({"error": e}), e))?;
            use base64::Engine;
            let png_base64 = base64::engine::general_purpose::STANDARD.encode(&png_data);

            Ok(serde_json::json!({
                "success": true,
                "width": width,
                "height": height,
                "origin_x": origin_x,
                "origin_y": origin_y,
                "image_path": image_path,
                "png_base64": png_base64
            }))
        }
        Err(e) => {
            log::error!("截图失败: {}", e);
            Ok(serde_json::json!({
                "success": false,
                "error": e.to_string()
            }))
        }
    }
}

#[tauri::command]
pub async fn start_manual_longshot(
    request: crate::features::screenshot::longshot::StartManualLongshotRequest,
    app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<serde_json::Value, String> {
    if !is_screenshot_feature_enabled(state.inner()) {
        return Err(frontend_error_kind(
            AppErrorKind::ScreenshotFeatureDisabled,
            "screenshot feature disabled",
        ));
    }

    let _ = hide_overlay_window_by_label(&app, "screenshot");
    let _ = hide_overlay_window_by_label(&app, "longshot_border");
    tauri::async_runtime::spawn_blocking(|| {
        std::thread::sleep(std::time::Duration::from_millis(90))
    })
        .await
        .map_err(|e| format!("等待截图窗口隐藏失败: {}", e))?;
    crate::features::screenshot::longshot::start_manual_longshot(app, request)
}

#[tauri::command]
pub async fn pause_manual_longshot(
    request: ManualLongshotSessionRequest,
    app: AppHandle,
) -> Result<(), String> {
    crate::features::screenshot::longshot::pause_manual_longshot(request.session_id, app)
}

#[tauri::command]
pub async fn resume_manual_longshot(
    request: ManualLongshotSessionRequest,
    app: AppHandle,
) -> Result<(), String> {
    crate::features::screenshot::longshot::resume_manual_longshot(request.session_id, app)
}

#[tauri::command]
pub async fn cancel_manual_longshot(
    request: ManualLongshotSessionRequest,
    app: AppHandle,
) -> Result<(), String> {
    let session_id = request.session_id;
    tauri::async_runtime::spawn_blocking(move || {
        crate::features::screenshot::longshot::cancel_manual_longshot(session_id, app)
    })
        .await
        .map_err(|e| format!("取消长截图任务执行失败: {}", e))?
}

#[tauri::command]
pub async fn finish_manual_longshot(
    request: ManualLongshotSessionRequest,
    app: AppHandle,
) -> Result<crate::features::screenshot::longshot::ManualLongshotFinishResult, String> {
    let session_id = request.session_id;
    let result = tauri::async_runtime::spawn_blocking(move || {
        crate::features::screenshot::longshot::finish_manual_longshot(session_id, app)
    })
        .await
        .map_err(|e| format!("完成长截图任务执行失败: {}", e))??;
    if !result.image_path.is_empty() {
        replace_screenshot_boot_image_path(Some(PathBuf::from(&result.image_path)));
    }
    Ok(result)
}

#[tauri::command]
pub async fn get_manual_longshot_status(
    request: ManualLongshotSessionRequest,
) -> Result<crate::features::screenshot::longshot::ManualLongshotStatus, String> {
    crate::features::screenshot::longshot::get_manual_longshot_status(request.session_id)
}

#[tauri::command]
pub async fn recognize_image_ocr(
    png_base64: String,
    engine: Option<String>,
    _app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<serde_json::Value, String> {
    let started_at = std::time::Instant::now();

    // 从 base64 解码 PNG 字节
    let png_bytes = base64::engine::general_purpose::STANDARD
        .decode(&png_base64)
        .map_err(|e| format!("base64 解码失败: {}", e))?;

    // 选择 OCR 引擎
    let engine_type = match engine.as_deref() {
        Some("ocr-rs") => {
            log::info!("使用 ocr-rs (Rust) 引擎");
            crate::services::ocr_engine::OcrEngineType::OcrRs
        }
        Some("windows-native") => {
            log::debug!("使用 Windows 原生 OCR 引擎");
            crate::services::ocr_engine::OcrEngineType::WindowsNative
        }
        None => {
            // 从设置中读取默认引擎
            let state_guard = lock_arc_mutex(state.inner());
            let ocr_engine_setting = state_guard.settings.ocr_engine.clone();
            drop(state_guard);

            if ocr_engine_setting == "ocr-rs" {
                log::info!("使用设置中的 ocr-rs (Rust) 引擎");
                crate::services::ocr_engine::OcrEngineType::OcrRs
            } else {
                log::debug!("使用设置中的 Windows 原生 OCR 引擎");
                crate::services::ocr_engine::OcrEngineType::WindowsNative
            }
        }
        _ => {
            log::debug!("使用 Windows 原生 OCR 引擎");
            crate::services::ocr_engine::OcrEngineType::WindowsNative
        }
    };

    match crate::services::ocr_engine::recognize_image(&png_bytes, engine_type, &_app).await {
        Ok(result) => {
            record_perf_metric(
                "ocr.recognize",
                "OCR识别耗时",
                started_at.elapsed().as_millis() as u64,
                true,
                None,
            );
            Ok(serde_json::json!({
                "success": true,
                "paragraphs": result.paragraphs
            }))
        }
        Err(e) => {
            record_perf_metric(
                "ocr.recognize",
                "OCR识别耗时",
                started_at.elapsed().as_millis() as u64,
                false,
                Some(e.clone()),
            );
            Ok(serde_json::json!({
                "success": false,
                "error": e
            }))
        }
    }
}

/// 捕获指定区域
#[tauri::command]
pub async fn capture_region(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<serde_json::Value, String> {
    use crate::features::screenshot::capture;
    if !is_screenshot_feature_enabled(state.inner()) {
        return Err(frontend_error_kind(
            AppErrorKind::ScreenshotFeatureDisabled,
            "screenshot feature disabled",
        ));
    }

    log::info!("捕获区域: ({}, {}) {}x{}", x, y, width, height);

    if width < 1 || height < 1 {
        return Ok(serde_json::json!({
            "success": false,
            "error": "区域尺寸无效"
        }));
    }

    match capture::capture_screen_region(x, y, width, height) {
        Ok((rgba, w, h)) => {
            let png_base64 = capture::rgba_to_base64_png(&rgba, w, h)
                .map_err(|e| format!("转换PNG失败: {}", e))?;

            Ok(serde_json::json!({
                "success": true,
                "width": w,
                "height": h,
                "png_base64": png_base64
            }))
        }
        Err(e) => {
            log::error!("区域截图失败: {}", e);
            Ok(serde_json::json!({
                "success": false,
                "error": e.to_string()
            }))
        }
    }
}

/// 保存截图到文件
#[tauri::command]
pub async fn choose_screenshot_save_path(app: AppHandle) -> Result<serde_json::Value, String> {
    let filename = format!("screenshot_{}.png", now_unix_ms());
    let (tx, rx) = mpsc::channel::<Result<Option<PathBuf>, String>>();
    let screenshot_window = app.get_webview_window("screenshot");

    if let Some(window) = screenshot_window.as_ref() {
        let _ = window.set_always_on_top(false);
        let _ = window.set_ignore_cursor_events(true);
    }

    app.dialog()
        .file()
        .add_filter("PNG图片", &["png"])
        .set_file_name(&filename)
        .save_file(move |path| {
            let result = match path {
                Some(file_path) => file_path
                    .as_path()
                    .map(|p| Some(p.to_path_buf()))
                    .ok_or_else(|| "无法获取保存路径".to_string()),
                None => Ok(None),
            };
            let _ = tx.send(result);
        });

    let selected_path_result = tauri::async_runtime::spawn_blocking(move || {
        rx.recv_timeout(Duration::from_secs(30))
    }).await;

    if let Some(window) = screenshot_window.as_ref() {
        let _ = window.set_always_on_top(true);
        let _ = window.set_ignore_cursor_events(false);
        let _ = focus_overlay_window_by_label(&app, "screenshot");
    }

    let selected_path = selected_path_result
        .map_err(|e| format!("等待保存对话框结果失败: {}", e))?
        .map_err(|e| format!("接收保存对话框结果失败: {}", e))??;

    let Some(path_buf) = selected_path else {
        return Ok(serde_json::json!({
            "success": false,
            "cancelled": true,
            "message": "用户取消保存"
        }));
    };

    Ok(serde_json::json!({
        "success": true,
        "cancelled": false,
        "path": path_buf.to_string_lossy().to_string()
    }))
}

#[tauri::command]
pub async fn save_screenshot_to_path(
    png_base64: String,
    output_path: String,
) -> Result<serde_json::Value, String> {
    use base64::Engine;

    if output_path.trim().is_empty() {
        return Err(frontend_error_kind(AppErrorKind::ScreenshotSavePathEmpty, ""));
    }

    let target_path = PathBuf::from(&output_path);

    // 防御性路径校验：拒绝包含 ".." 的路径（路径穿越攻击防护）
    if target_path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err("不允许的路径：包含路径遍历".to_string());
    }

    timed_sync("screenshot.save_file", "截图保存耗时", || {
        let png_data = base64::engine::general_purpose::STANDARD
            .decode(&png_base64)
            .map_err(|e| format!("Base64解码失败: {}", e))?;

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建保存目录失败: {}", e))?;
        }

        fs::write(&target_path, &png_data).map_err(|e| format!("写入文件失败: {}", e))?;
        Ok::<(), String>(())
    })?;

    Ok(serde_json::json!({
        "success": true,
        "path": target_path.to_string_lossy().to_string()
    }))
}

#[tauri::command]
pub async fn export_screenshot_to_path(
    request: ScreenshotExportRequest,
) -> Result<serde_json::Value, String> {
    let output_path = request.output_path.clone();

    // 防御性路径校验：拒绝包含 ".." 的路径（路径穿越攻击防护）
    let target = PathBuf::from(&output_path);
    if target.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err("不允许的路径：包含路径遍历".to_string());
    }

    tauri::async_runtime::spawn_blocking(move || export_screenshot_image(&request))
        .await
        .map_err(|e| format!("执行截图导出任务失败: {}", e))?
        .map(|_| {
            serde_json::json!({
                "success": true,
                "path": output_path
            })
        })
}

#[tauri::command]
pub async fn render_screenshot_to_png_data(
    request: ScreenshotExportRequest,
) -> Result<serde_json::Value, String> {
    let (rgba, width, height) = tauri::async_runtime::spawn_blocking(move || {
        let canvas = render_screenshot_image(&request)?;
        let width = canvas.width();
        let height = canvas.height();
        Ok::<(Vec<u8>, u32, u32), String>((canvas.into_raw(), width, height))
    })
        .await
        .map_err(|e| format!("执行截图渲染任务失败: {}", e))??;
    let png_base64 = crate::features::screenshot::capture::rgba_to_base64_png(&rgba, width, height)
        .map_err(|e| format!("转换PNG失败: {}", e))?;
    Ok(serde_json::json!({
        "success": true,
        "pngBase64": png_base64,
        "width": width,
        "height": height
    }))
}

#[tauri::command]
pub async fn copy_screenshot_to_clipboard(
    request: ScreenshotExportRequest,
    app: AppHandle,
) -> Result<serde_json::Value, String> {
    let (rgba, width, height) = tauri::async_runtime::spawn_blocking(move || {
        let canvas = render_screenshot_image(&request)?;
        let width = canvas.width();
        let height = canvas.height();
        Ok::<(Vec<u8>, u32, u32), String>((canvas.into_raw(), width, height))
    })
        .await
        .map_err(|e| format!("执行截图渲染任务失败: {}", e))??;
    let image = tauri::image::Image::new_owned(rgba, width, height);
    ImageClipboardManager::write_clipboard_image(&app, &image)?;
    Ok(serde_json::json!({
        "success": true,
        "width": width,
        "height": height
    }))
}

#[tauri::command]
pub async fn save_screenshot(
    png_base64: String,
    app: AppHandle,
) -> Result<serde_json::Value, String> {
    use base64::Engine;
    use std::time::{SystemTime, UNIX_EPOCH};
    let started_at = std::time::Instant::now();

    log::info!("保存截图到文件");

    // 解码Base64
    let png_data = base64::engine::general_purpose::STANDARD
        .decode(&png_base64)
        .map_err(|e| format!("Base64解码失败: {}", e))?;

    // 生成文件名
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    let filename = format!("screenshot_{}.png", timestamp);

    let (tx, rx) = mpsc::channel::<Result<Option<PathBuf>, String>>();

    // 获取保存路径（用户选择）
    app.dialog()
        .file()
        .add_filter("PNG图片", &["png"])
        .set_file_name(&filename)
        .save_file(move |path| {
            let result = match path {
                Some(file_path) => file_path
                    .as_path()
                    .map(|p| Some(p.to_path_buf()))
                    .ok_or_else(|| "无法获取保存路径".to_string()),
                None => Ok(None),
            };
            let _ = tx.send(result);
        });

    let selected_path = tauri::async_runtime::spawn_blocking(move || {
        rx.recv_timeout(Duration::from_secs(30))
    })
        .await
        .map_err(|e| format!("等待保存对话框结果失败: {}", e))?
        .map_err(|e| format!("接收保存对话框结果失败: {}", e))??;

    let Some(path_buf) = selected_path else {
        log::info!("用户取消保存");
        record_perf_metric(
            "screenshot.save_dialog",
            "截图保存对话框总耗时",
            started_at.elapsed().as_millis() as u64,
            true,
            None,
        );
        return Ok(serde_json::json!({
            "success": false,
            "cancelled": true,
            "message": "用户取消保存"
        }));
    };
    // 防御性路径校验：拒绝包含 ".." 的路径（路径穿越攻击防护）
    if path_buf.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err("路径包含非法字符".to_string());
    }

    fs::write(&path_buf, &png_data).map_err(|e| format!("写入文件失败: {}", e))?;
    record_perf_metric(
        "screenshot.save_dialog",
        "截图保存对话框总耗时",
        started_at.elapsed().as_millis() as u64,
        true,
        None,
    );
    log::info!("截图已保存到: {}", path_buf.display());

    Ok(serde_json::json!({
        "success": true,
        "cancelled": false,
        "path": path_buf.to_string_lossy().to_string()
    }))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinScreenshotRequest {
    png_base64: String,
    x: Option<f64>,
    y: Option<f64>,
    width: Option<f64>,
    height: Option<f64>,
}

#[tauri::command]
pub async fn pin_screenshot_on_screen(
    request: PinScreenshotRequest,
    app: AppHandle,
) -> Result<serde_json::Value, String> {
    // 限制贴图窗口数量，防止资源耗尽
    const MAX_PINNED_WINDOWS: usize = 20;
    let existing_count = app.webview_windows()
        .keys()
        .filter(|k| k.starts_with("pinned_image_"))
        .count();
    if existing_count >= MAX_PINNED_WINDOWS {
        return Err("贴图数量已达上限（20个），请先关闭部分贴图".to_string());
    }

    let label = format!(
        "pinned_image_{}",
        NEXT_PINNED_IMAGE_WINDOW_ID.fetch_add(1, Ordering::Relaxed)
    );
    let x = request.x.unwrap_or(100.0).max(0.0);
    let y = request.y.unwrap_or(100.0).max(0.0);
    let width = request.width.unwrap_or(360.0).max(1.0);
    let height = request.height.unwrap_or(240.0).max(1.0);
    let payload = serde_json::json!({
        "label": label,
        "png_base64": request.png_base64,
        "width": width,
        "height": height
    });
    let payload_init_script = format!("window.__PINNED_IMAGE_PAYLOAD__ = {};", payload);
    let window = tauri::WebviewWindowBuilder::new(
        &app,
        label.clone(),
        tauri::WebviewUrl::App("pinned_image.html".into()),
    )
        .title("固定截图")
        .visible(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(true)
        .initialization_script(&payload_init_script)
        .build()
        .map_err(|e| format!("创建固定图片窗口失败: {}", e))?;
    bind_overlay_window_events(&window, app.clone(), label.clone());

    let window_clone = window.clone();
    let _ = window_clone.set_resizable(true);
    let _ = window_clone.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x: x as i32, y: y as i32 }));
    let _ = window_clone.set_size(tauri::Size::Physical(tauri::PhysicalSize { width: width as u32, height: height as u32 }));
    let _ = show_overlay_window_by_label(&app, &label, false);
    let script = format!(
        "window.__PINNED_IMAGE_PAYLOAD__ = {}; window.dispatchEvent(new CustomEvent('pinned-image-data', {{ detail: {} }}));",
        payload, payload
    );
    let _ = window_clone.eval(script);

    Ok(serde_json::json!({ "success": true, "label": label }))
}

#[tauri::command]
pub async fn close_pinned_image_window(label: String, app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.close();
    }
    Ok(())
}

#[tauri::command]
pub async fn get_pinned_image_window_position(
    label: String,
    app: AppHandle,
) -> Result<serde_json::Value, String> {
    if let Some(window) = app.get_webview_window(&label) {
        if let Ok(pos) = window.outer_position() {
            return Ok(serde_json::json!({
                "success": true,
                "x": pos.x,
                "y": pos.y
            }));
        }
    }
    Ok(serde_json::json!({
        "success": false
    }))
}

#[tauri::command]
pub async fn move_pinned_image_window(
    label: String,
    x: i32,
    y: i32,
    app: AppHandle,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }));
    }
    Ok(())
}

/// 获取屏幕尺寸
#[tauri::command]
pub async fn get_screen_size() -> Result<serde_json::Value, String> {
    use crate::features::screenshot::capture;

    match capture::get_screen_size() {
        Ok((width, height)) => Ok(serde_json::json!({
            "success": true,
            "width": width,
            "height": height
        })),
        Err(e) => Ok(serde_json::json!({
            "success": false,
            "error": e.to_string()
        })),
    }
}

#[tauri::command]
pub async fn set_screenshot_clipboard_link_once(linked: bool) -> Result<(), String> {
    use crate::features::screenshot::capture;
    if let Ok(settings) = load_settings() {
        if !settings.screenshot_enabled {
            return Ok(());
        }
    }
    capture::set_allow_image_clipboard_once(linked);
    Ok(())
}

fn set_screenshot_window_passthrough_internal(
    app: &AppHandle,
    enabled: bool,
) -> Result<(), String> {
    let Some(window) = app.get_webview_window("screenshot") else {
        return Ok(());
    };
    window
        .set_ignore_cursor_events(enabled)
        .map_err(|e| format!("设置截图窗口输入穿透失败: {}", e))?;
    if !enabled {
        let _ = focus_overlay_window_by_label(&app, "screenshot");
    }
    Ok(())
}

fn set_screenshot_window_visibility_internal(app: &AppHandle, visible: bool) -> Result<(), String> {
    let Some(window) = app.get_webview_window("screenshot") else {
        return Ok(());
    };
    if visible {
        let _ = window.set_ignore_cursor_events(false);
        show_overlay_window_by_label(app, "screenshot", true)?;
    } else {
        let _ = window.set_ignore_cursor_events(true);
        hide_overlay_window_by_label(app, "screenshot")?;
    }
    Ok(())
}

fn ensure_longshot_toolbar_window(app: &AppHandle) -> Result<(tauri::WebviewWindow, bool), String> {
    let (window, is_new) = crate::ui::window_manager::ensure_overlay_window(
        app, "longshot_toolbar", "longshot_toolbar.html", "长截图工具栏", Some((320.0, 180.0)),
    )?;
    if is_new {
        let _ = window.set_content_protected(true);
    }
    Ok((window, is_new))
}

fn ensure_longshot_border_window(app: &AppHandle) -> Result<(tauri::WebviewWindow, bool), String> {
    let (window, is_new) = crate::ui::window_manager::ensure_overlay_window(
        app, "longshot_border", "longshot_border.html", "长截图边框", None,
    )?;
    if is_new {
        let _ = window.set_content_protected(true);
        let _ = window.set_ignore_cursor_events(true);
    }
    Ok((window, is_new))
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LongshotToolbarAnchor {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

fn place_longshot_toolbar_near_anchor(
    app: &AppHandle,
    window: &tauri::WebviewWindow,
    anchor: Option<LongshotToolbarAnchor>,
) {
    let Some(anchor) = anchor else {
        let _ = window.move_window(tauri_plugin_positioner::Position::TopRight);
        return;
    };
    let (toolbar_w, toolbar_h) = (260i32, 430i32);
    let dpi = window.scale_factor().unwrap_or(1.0).max(1.0);
    let phys_h = (toolbar_h as f64 * dpi) as i32;
    let margin = (12f64 * dpi) as i32;
    let Some(screen_window) = app.get_webview_window("screenshot") else {
        let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
            anchor.x + anchor.width as i32 + margin,
            anchor.y + (anchor.height as i32 / 2) - (phys_h / 2),
        )));
        return;
    };
    let Ok(Some(monitor)) = screen_window.current_monitor() else {
        let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
            anchor.x + anchor.width as i32 + margin,
            anchor.y + (anchor.height as i32 / 2) - (phys_h / 2),
        )));
        return;
    };
    let dpi = monitor.scale_factor().max(1.0);
    let phys_w = (toolbar_w as f64 * dpi) as i32;
    let phys_h = (toolbar_h as f64 * dpi) as i32;
    // anchor 坐标已经是物理像素，无需再乘 DPI
    let anchor_x = anchor.x;
    let anchor_y = anchor.y;
    let anchor_w = anchor.width as i32;
    let anchor_h = anchor.height as i32;
    let margin = (12f64 * dpi) as i32;
    let default_x = anchor_x + anchor_w + margin;
    let default_y = anchor_y + (anchor_h / 2) - (phys_h / 2);
    let mon_pos = monitor.position();
    let mon_size = monitor.size();
    let edge_margin = (8f64 * dpi) as i32;
    let min_x = mon_pos.x + edge_margin;
    let max_x = mon_pos.x + mon_size.width as i32 - phys_w - edge_margin;
    let min_y = mon_pos.y + edge_margin;
    let max_y = mon_pos.y + mon_size.height as i32 - phys_h - edge_margin;
    let anchor_left = anchor_x;
    let anchor_top = anchor_y;
    let anchor_right = anchor_x + anchor_w;
    let anchor_bottom = anchor_y + anchor_h;

    let clamp_candidate =
        |x: i32, y: i32| -> (i32, i32) { (x.clamp(min_x, max_x), y.clamp(min_y, max_y)) };
    let intersects_anchor = |x: i32, y: i32| -> bool {
        let right = x + phys_w;
        let bottom = y + phys_h;
        x < anchor_right && right > anchor_left && y < anchor_bottom && bottom > anchor_top
    };

    let mut candidates = vec![
        clamp_candidate(default_x, default_y),
        clamp_candidate(anchor_left - phys_w - margin, default_y),
        clamp_candidate(
            anchor_left + (anchor_w - phys_w) / 2,
            anchor_bottom + margin,
        ),
        clamp_candidate(
            anchor_left + (anchor_w - phys_w) / 2,
            anchor_top - phys_h - margin,
        ),
        (max_x, min_y),
    ];
    candidates.dedup();

    let chosen = candidates
        .iter()
        .copied()
        .find(|(x, y)| !intersects_anchor(*x, *y))
        .unwrap_or((max_x, min_y));

    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
        chosen.0, chosen.1,
    )));
}

#[tauri::command]
pub async fn set_screenshot_input_passthrough(enabled: bool, app: AppHandle) -> Result<(), String> {
    set_screenshot_window_passthrough_internal(&app, enabled)
}

#[tauri::command]
pub async fn set_screenshot_window_visible(visible: bool, app: AppHandle) -> Result<(), String> {
    set_screenshot_window_visibility_internal(&app, visible)
}

#[tauri::command]
pub async fn show_longshot_toolbar(
    app: AppHandle,
    anchor: Option<LongshotToolbarAnchor>,
) -> Result<(), String> {
    let (window, _created) = ensure_longshot_toolbar_window(&app)?;
    if let Err(e) = window.set_content_protected(true) {
        log::warn!("设置长截图工具栏内容保护失败: {}", e);
    }
    if let Err(e) = window.emit(
        "manual-longshot-toolbar-reset",
        serde_json::json!({
            "ts": now_unix_ms()
        }),
    ) {
        log::warn!("发送长截图工具栏重置事件失败: {}", e);
    }
    if let Err(e) = window.set_size(tauri::Size::Logical(tauri::LogicalSize {
        width: 260.0,
        height: 430.0,
    })) {
        log::warn!("设置长截图工具栏大小失败: {}", e);
    }
    place_longshot_toolbar_near_anchor(&app, &window, anchor);
    show_overlay_window_by_label(&app, "longshot_toolbar", true)?;
    Ok(())
}

#[tauri::command]
pub async fn show_longshot_border(
    app: AppHandle,
    anchor: LongshotToolbarAnchor,
) -> Result<(), String> {
    let (window, _created) = ensure_longshot_border_window(&app)?;
    if let Err(e) = window.set_content_protected(true) {
        log::warn!("设置长截图边框内容保护失败: {}", e);
    }
    if let Err(e) = window.set_ignore_cursor_events(true) {
        log::warn!("设置长截图边框忽略鼠标事件失败: {}", e);
    }
    // 边框窗外扩，确保边框不进入实际采集区域
    const BORDER_OUTSET: i32 = 2;
    let width = (anchor.width as i32 + BORDER_OUTSET * 2).max(2) as u32;
    let height = (anchor.height as i32 + BORDER_OUTSET * 2).max(2) as u32;
    if let Err(e) = window.set_size(tauri::Size::Physical(tauri::PhysicalSize { width, height })) {
        log::warn!("设置长截图边框大小失败: {}", e);
    }
    if let Err(e) = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
        anchor.x - BORDER_OUTSET,
        anchor.y - BORDER_OUTSET,
    ))) {
        log::warn!("设置长截图边框位置失败: {}", e);
    }
    show_overlay_window_by_label(&app, "longshot_border", false)?;
    Ok(())
}

#[tauri::command]
pub async fn hide_longshot_border(app: AppHandle) -> Result<(), String> {
    if let Err(e) = hide_overlay_window_by_label(&app, "longshot_border") {
        log::warn!("隐藏长截图边框失败: {}", e);
    }
    Ok(())
}

#[tauri::command]
pub async fn snap_longshot_toolbar_window(app: AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window("longshot_toolbar") else {
        return Ok(());
    };
    let Ok(pos) = window.outer_position() else {
        return Ok(());
    };
    let Ok(size) = window.outer_size() else {
        return Ok(());
    };
    let Ok(Some(monitor)) = window.current_monitor() else {
        return Ok(());
    };
    let mon_pos = monitor.position();
    let mon_size = monitor.size();
    let dpi = monitor.scale_factor().max(1.0);
    let edge_margin = (8f64 * dpi) as i32;
    let left = mon_pos.x + edge_margin;
    let right = mon_pos.x + mon_size.width as i32 - size.width as i32 - edge_margin;
    let top = mon_pos.y + edge_margin;
    let threshold = (28f64 * dpi) as i32;

    let mut next_x = pos.x;
    let mut next_y = pos.y;
    if (pos.x - left).abs() <= threshold {
        next_x = left;
    } else if (pos.x - right).abs() <= threshold {
        next_x = right;
    }
    if (pos.y - top).abs() <= threshold {
        next_y = top;
    }
    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
        next_x, next_y,
    )));
    Ok(())
}

#[tauri::command]
pub async fn hide_longshot_toolbar(app: AppHandle) -> Result<(), String> {
    let _ = hide_overlay_window_by_label(&app, "longshot_toolbar");
    Ok(())
}

#[tauri::command]
pub async fn longshot_toolbar_action(action: String, app: AppHandle) -> Result<(), String> {
    let Some(session_id) =
        crate::features::screenshot::longshot::active_manual_longshot_session_id()
    else {
        return Ok(());
    };
    match action.as_str() {
        "pause" => {
            crate::features::screenshot::longshot::pause_manual_longshot(session_id, app.clone())?;
        }
        "resume" => {
            crate::features::screenshot::longshot::resume_manual_longshot(session_id, app.clone())?;
        }
        "finish" => {
            let app_for_finish = app.clone();
            let result = tauri::async_runtime::spawn_blocking(move || {
                crate::features::screenshot::longshot::finish_manual_longshot(
                    session_id,
                    app_for_finish,
                )
            })
                .await
                .map_err(|e| format!("完成长截图任务执行失败: {}", e))??;
            if !result.image_path.is_empty() {
                replace_screenshot_boot_image_path(Some(PathBuf::from(&result.image_path)));
            }
            let _ = app.emit(
                "manual-longshot-shortcut-finished",
                serde_json::json!({
                    "sessionId": result.session_id,
                    "pngBase64": result.png_base64,
                    "imagePath": result.image_path,
                    "width": result.width,
                    "height": result.height,
                    "frameCount": result.frame_count,
                    "droppedFrames": result.dropped_frames,
                }),
            );
            let _ = hide_longshot_border(app.clone()).await;
            let _ = hide_longshot_toolbar(app.clone()).await;
            let _ = set_screenshot_window_visibility_internal(&app, true);
            return Ok(());
        }
        "cancel" => {
            let app_for_cancel = app.clone();
            tauri::async_runtime::spawn_blocking(move || {
                crate::features::screenshot::longshot::cancel_manual_longshot(
                    session_id,
                    app_for_cancel,
                )
            })
                .await
                .map_err(|e| format!("取消长截图任务执行失败: {}", e))??;
            let _ = app.emit(
                "manual-longshot-shortcut-canceled",
                serde_json::json!({
                    "sessionId": session_id
                }),
            );
            let _ = hide_longshot_border(app.clone()).await;
            let _ = hide_longshot_toolbar(app.clone()).await;
            let _ = set_screenshot_window_visibility_internal(&app, true);
            return Ok(());
        }
        _ => return Err(frontend_error_kind(AppErrorKind::ScreenshotUnsupportedOperation, action)),
    }
    Ok(())
}

pub async fn finish_manual_longshot_from_shortcut(app: AppHandle) -> Result<(), String> {
    let Some(session_id) =
        crate::features::screenshot::longshot::active_manual_longshot_session_id()
    else {
        return Ok(());
    };
    let app_for_finish = app.clone();
    match tauri::async_runtime::spawn_blocking(move || {
        crate::features::screenshot::longshot::finish_manual_longshot(session_id, app_for_finish)
    })
        .await
        .map_err(|e| format!("完成长截图任务执行失败: {}", e))?
    {
        Ok(result) => {
            if !result.image_path.is_empty() {
                replace_screenshot_boot_image_path(Some(PathBuf::from(&result.image_path)));
            }
            let _ = app.emit(
                "manual-longshot-shortcut-finished",
                serde_json::json!({
                    "sessionId": result.session_id,
                    "pngBase64": result.png_base64,
                    "imagePath": result.image_path,
                    "width": result.width,
                    "height": result.height,
                    "frameCount": result.frame_count,
                    "droppedFrames": result.dropped_frames,
                }),
            );
            let _ = hide_longshot_border(app.clone()).await;
            let _ = hide_longshot_toolbar(app.clone()).await;
            let _ = set_screenshot_window_visibility_internal(&app, true);
            Ok(())
        }
        Err(e) => Err(e),
    }
}

pub async fn cancel_manual_longshot_from_shortcut(app: AppHandle) -> Result<(), String> {
    let Some(session_id) =
        crate::features::screenshot::longshot::active_manual_longshot_session_id()
    else {
        return Ok(());
    };
    let app_for_cancel = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::features::screenshot::longshot::cancel_manual_longshot(session_id, app_for_cancel)
    })
        .await
        .map_err(|e| format!("取消长截图任务执行失败: {}", e))??;
    let _ = app.emit(
        "manual-longshot-shortcut-canceled",
        serde_json::json!({
            "sessionId": session_id
        }),
    );
    let _ = hide_longshot_border(app.clone()).await;
    let _ = hide_longshot_toolbar(app.clone()).await;
    let _ = set_screenshot_window_visibility_internal(&app, true);
    Ok(())
}

pub async fn toggle_manual_longshot_pause_from_shortcut(app: AppHandle) -> Result<(), String> {
    let Some(session_id) =
        crate::features::screenshot::longshot::active_manual_longshot_session_id()
    else {
        return Ok(());
    };
    let status = crate::features::screenshot::longshot::get_manual_longshot_status(session_id)?;
    if status.state == "running" {
        crate::features::screenshot::longshot::pause_manual_longshot(session_id, app.clone())?;
        let _ = app.emit(
            "manual-longshot-shortcut-paused",
            serde_json::json!({
                "sessionId": session_id
            }),
        );
        return Ok(());
    }
    if status.state == "paused" {
        crate::features::screenshot::longshot::resume_manual_longshot(session_id, app.clone())?;
        let _ = app.emit(
            "manual-longshot-shortcut-resumed",
            serde_json::json!({
                "sessionId": session_id
            }),
        );
    }
    Ok(())
}

/// 打开截图编辑窗口
#[tauri::command]
pub async fn open_screenshot_editor(app: AppHandle, mode: Option<String>) -> Result<(), String> {
    let selection_mode = mode
        .as_ref()
        .map(|m| m.to_lowercase())
        .unwrap_or_else(|| "screenshot".to_string());
    if let Ok(settings) = load_settings() {
        if !settings.screenshot_enabled && selection_mode != "recording_region" {
            return Err(frontend_error_kind(
                AppErrorKind::ScreenshotFeatureDisabled,
                "screenshot feature disabled",
            ));
        }
    }
    log::info!("打开截图编辑窗口");
    let started_at = std::time::Instant::now();

    use crate::features::screenshot::capture;
    if !capture::try_begin_screenshot() {
        log::info!("截图任务已在进行中，忽略重复触发");
        return Ok(());
    }
    let (rgba, width, height, origin_x, origin_y) = match capture::capture_full_screen() {
        Ok(data) => data,
        Err(e) => {
            capture::set_screenshot_in_progress(false);
            record_perf_metric(
                "screenshot.open_prepare",
                "截图打开准备耗时",
                started_at.elapsed().as_millis() as u64,
                false,
                Some(e.to_string()),
            );
            return Err(frontend_error_kind_params(AppErrorKind::ScreenshotFailed, serde_json::json!({"error": e.to_string()}), e.to_string()));
        }
    };

    let session_id = NEXT_SCREENSHOT_SESSION_ID.fetch_add(1, Ordering::SeqCst);

    // BMP 快速写入（~5ms，浏览器零解码显示）
    let bmp_data = capture::rgba_to_bmp_bytes(&rgba, width, height).map_err(|e| {
        capture::set_screenshot_in_progress(false);
        format!("BMP编码失败: {}", e)
    })?;
    let bmp_path = {
        let mut dir = std::env::current_exe().map_err(|e| format!("获取程序目录失败: {}", e))?;
        dir.pop();
        dir.push("screenshot_boot");
        let _ = std::fs::create_dir_all(&dir);
        dir.join(format!("screenshot_boot_{}.bmp", session_id))
    };
    std::fs::write(&bmp_path, &bmp_data).map_err(|e| format!("BMP写入失败: {}", e))?;

    // PNG 后台异步编码保存（用于导出，不阻塞热路径）
    let png_path = bmp_path.with_extension("png");
    capture::save_png_async(rgba, width, height, png_path.clone());

    let bmp_path_str = bmp_path.to_string_lossy().replace('\\', "\\\\");

    // 存储会话数据
    {
        let mut guard = screenshot_session_store()
            .lock()
            .map_err(|e| format!("锁获取失败: {}", e))?;
        *guard = Some(ScreenshotSession {
            bmp_path: bmp_path.clone(),
            png_path: png_path.clone(),
            width,
            height,
            origin_x,
            origin_y,
            session_id,
            selection_mode: selection_mode.clone(),
        });
    }

    ensure_window_for_label(&app, "screenshot")?;
    let window = app
        .get_webview_window("screenshot")
        .ok_or_else(|| "截图窗口创建失败".to_string())?;
    if SCREENSHOT_LIFECYCLE_BOUND_FOR_BOOT_WINDOW
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        bind_screenshot_window_lifecycle(&window, &app);
    }
    let _ = window.set_always_on_top(true);
    let _ = window.set_fullscreen(true);
    let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize { width, height }));
    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
        x: origin_x,
        y: origin_y,
    }));
    // 注入 BMP 路径预加载（图片加载与 Vue 挂载并行）
    let preload_script = format!(
        "window.__SCREENSHOT_PRELOAD__ = {{ bmpPath: '{}', imagePath: '{}', sessionId: {}, mode: '{}' }};\
window.dispatchEvent(new CustomEvent('screenshot-preload-ready'));",
        bmp_path_str,
        png_path.to_string_lossy().replace('\\', "\\\\"),
        session_id,
        selection_mode
    );
    let _ = window.eval(&preload_script);

    capture::set_screenshot_in_progress(false);
    record_perf_metric(
        "screenshot.open_prepare",
        "截图打开准备耗时",
        started_at.elapsed().as_millis() as u64,
        true,
        None,
    );
    Ok(())
}

/// 获取窗口列表
#[tauri::command]
pub async fn get_window_list() -> Result<serde_json::Value, String> {
    use crate::features::screenshot::window_detect;

    match window_detect::get_window_list() {
        Ok(windows) => Ok(serde_json::json!({
            "success": true,
            "windows": windows
        })),
        Err(e) => {
            log::error!("获取窗口列表失败: {}", e);
            Ok(serde_json::json!({
                "success": false,
                "error": e.to_string(),
                "windows": []
            }))
        }
    }
}

/// 关闭截图窗口并释放焦点
#[tauri::command]
pub async fn close_screenshot_window(app: AppHandle) -> Result<(), String> {
    log::info!("关闭截图窗口");
    if let Some(window) = app.get_webview_window("screenshot") {
        // 解除置顶和鼠标拦截，防止在Windows上残留透明幽灵窗口导致桌面无法点击
        let _ = window.set_always_on_top(false);
        let _ = window.set_ignore_cursor_events(true);
        let _ = window.eval(
            "window.dispatchEvent(new CustomEvent('screenshot-reset'));\
window.__SCREENSHOT_BOOT__ = window.__SCREENSHOT_BOOT__ || { pendingData: null, pendingStartSessionId: 0 };\
window.__SCREENSHOT_BOOT__.pendingData = null;\
window.__SCREENSHOT_BOOT__.pendingStartSessionId = 0;\
window.__SCREENSHOT_BOOT__.pendingMode = null;",
        );
        let _ = app.emit("screenshot-reset", ());
        let _ = hide_overlay_window_by_label(&app, "screenshot");
    }
    cleanup_all_screenshot_boot_images();
    features::screenshot::capture::set_screenshot_in_progress(false);

    Ok(())
}
