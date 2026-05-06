use crate::core::app_state::AppState as SharedAppState;
use crate::core::error::ErrorCode;
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
use crate::ui::commands_clipboard::{frontend_error, get_image_clipboard_manager_arc, is_screenshot_feature_enabled};
use crate::ui::commands_screenshot_render::{export_screenshot_image, render_screenshot_image, ScreenshotExportRequest};
use crate::ui::window_manager::{
    bind_overlay_window_events, ensure_window_for_label, focus_overlay_window_by_label,
    hide_overlay_window_by_label, show_overlay_window_by_label,
};
use crate::utils::image_clipboard::ImageClipboardManager;
use crate::utils::utils_helpers::load_settings;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_positioner::WindowExt;

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
            return Err("源图片文件不存在".to_string());
        }
        let file_name = source
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.is_empty())
            .ok_or_else(|| "无法解析源文件名".to_string())?
            .to_string();

        let target_dir = PathBuf::from(target_directory.trim());
        if target_dir.as_os_str().is_empty() {
            return Err("目标目录不能为空".to_string());
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
            let image_path = write_screenshot_boot_image(&rgba, width, height, session_id)
                .map_err(|e| format!("写入截图源图失败: {}", e))?;
            let png_base64 = capture::rgba_to_base64_png(&rgba, width, height)
                .map_err(|e| format!("转换PNG失败: {}", e))?;

            Ok(serde_json::json!({
                "success": true,
                "width": width,
                "height": height,
                "origin_x": origin_x,
                "origin_y": origin_y,
                "png_base64": png_base64,
                "image_path": image_path
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
        return Err(frontend_error(
            ErrorCode::ValidationError,
            "截图功能已停用",
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
    png_bytes: Vec<u8>,
    engine: Option<String>,
    _app: AppHandle,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<serde_json::Value, String> {
    let started_at = std::time::Instant::now();

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
        return Err(frontend_error(
            ErrorCode::ValidationError,
            "截图功能已停用",
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
        return Err("保存路径为空".to_string());
    }

    let target_path = PathBuf::from(&output_path);
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
    let Some(screen_window) = app.get_webview_window("screenshot") else {
        let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
            anchor.x + anchor.width as i32 + 12,
            anchor.y + (anchor.height as i32 / 2) - (toolbar_h / 2),
        )));
        return;
    };
    let Ok(Some(monitor)) = screen_window.current_monitor() else {
        let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
            anchor.x + anchor.width as i32 + 12,
            anchor.y + (anchor.height as i32 / 2) - (toolbar_h / 2),
        )));
        return;
    };
    let dpi = monitor.scale_factor().max(1.0);
    let phys_w = (toolbar_w as f64 * dpi) as i32;
    let phys_h = (toolbar_h as f64 * dpi) as i32;
    let anchor_x = (anchor.x as f64 * dpi) as i32;
    let anchor_y = (anchor.y as f64 * dpi) as i32;
    let anchor_w = (anchor.width as f64 * dpi) as i32;
    let anchor_h = (anchor.height as f64 * dpi) as i32;
    let margin = (12f64 * dpi) as i32;
    let default_x = anchor_x + anchor_w + margin;
    let default_y = anchor_y + (anchor_h / 2) - (phys_h / 2);
    let mon_pos = monitor.position();
    let mon_size = monitor.size();
    let min_x = mon_pos.x + 8;
    let max_x = mon_pos.x + mon_size.width as i32 - phys_w - 8;
    let min_y = mon_pos.y + 8;
    let max_y = mon_pos.y + mon_size.height as i32 - phys_h - 8;
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
    let _ = window.set_content_protected(true);
    let _ = window.emit(
        "manual-longshot-toolbar-reset",
        serde_json::json!({
            "ts": now_unix_ms()
        }),
    );
    let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize {
        width: 260.0,
        height: 430.0,
    }));
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
    let _ = window.set_content_protected(true);
    let _ = window.set_ignore_cursor_events(true);
    // 边框窗外扩，确保边框不进入实际采集区域
    const BORDER_OUTSET: i32 = 2;
    let width = (anchor.width as i32 + BORDER_OUTSET * 2).max(2) as u32;
    let height = (anchor.height as i32 + BORDER_OUTSET * 2).max(2) as u32;
    let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize { width, height }));
    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
        anchor.x - BORDER_OUTSET,
        anchor.y - BORDER_OUTSET,
    )));
    show_overlay_window_by_label(&app, "longshot_border", false)?;
    Ok(())
}

#[tauri::command]
pub async fn hide_longshot_border(app: AppHandle) -> Result<(), String> {
    let _ = hide_overlay_window_by_label(&app, "longshot_border");
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
    let left = mon_pos.x + 8;
    let right = mon_pos.x + mon_size.width as i32 - size.width as i32 - 8;
    let top = mon_pos.y + 8;
    let threshold = 28;

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
        _ => return Err("不支持的长截图操作".to_string()),
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
            return Err(frontend_error(
                ErrorCode::ValidationError,
                "截图功能已停用",
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
            return Err(format!("截图失败: {}", e));
        }
    };

    let session_id = NEXT_SCREENSHOT_SESSION_ID.fetch_add(1, Ordering::SeqCst);
    let image_path =
        write_screenshot_boot_image(&rgba, width, height, session_id).map_err(|e| {
            capture::set_screenshot_in_progress(false);
            record_perf_metric(
                "screenshot.open_prepare",
                "截图打开准备耗时",
                started_at.elapsed().as_millis() as u64,
                false,
                Some(e.clone()),
            );
            e
        })?;
    let png_base64 = capture::rgba_to_base64_png(&rgba, width, height)
        .unwrap_or_default();

    let selection_mode = selection_mode;
    ensure_window_for_label(&app, "screenshot")?;
    if let Some(window) = app.get_webview_window("screenshot") {
        if SCREENSHOT_LIFECYCLE_BOUND_FOR_BOOT_WINDOW
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            bind_screenshot_window_lifecycle(&window, &app);
        }
        let payload = serde_json::json!({
            "png_base64": png_base64,
            "image_path": image_path,
            "width": width,
            "height": height,
            "origin_x": origin_x,
            "origin_y": origin_y,
            "session_id": session_id
        });
        let script = format!(
            "if (!window.__SCREENSHOT_BOOT_READY__) {{ throw new Error('screenshot boot not ready'); }}\
window.__SCREENSHOT_BOOT__ = window.__SCREENSHOT_BOOT__ || {{ pendingData: null, pendingStartSessionId: 0 }};\
window.__SCREENSHOT_BOOT__.pendingData = {payload};\
window.__SCREENSHOT_BOOT__.pendingStartSessionId = {session_id};\
window.__SCREENSHOT_BOOT__.pendingMode = '{selection_mode}';\
window.dispatchEvent(new CustomEvent('screenshot-data', {{ detail: {payload} }}));\
window.dispatchEvent(new CustomEvent('start-region-select', {{ detail: {{ session_id: {session_id}, mode: '{selection_mode}' }} }}));"
        );

        let app_for_window = app.clone();
        thread::spawn(move || {
            let _ = window.set_always_on_top(true);
            let _ = window.set_ignore_cursor_events(false);
            let _ = window.set_fullscreen(true);
            let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize { width, height }));
            let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
                x: origin_x,
                y: origin_y,
            }));
            let mut injected = false;
            for _attempt in 0..20 {
                if window.eval(&script).is_ok() {
                    injected = true;
                    break;
                }
                thread::sleep(Duration::from_millis(25));
            }
            if injected {
                record_perf_metric(
                    "screenshot.open_prepare",
                    "截图打开准备耗时",
                    started_at.elapsed().as_millis() as u64,
                    true,
                    None,
                );
                let _ = show_overlay_window_by_label(&app_for_window, "screenshot", true);
            } else {
                record_perf_metric(
                    "screenshot.open_prepare",
                    "截图打开准备耗时",
                    started_at.elapsed().as_millis() as u64,
                    false,
                    Some("截图窗口脚本注入失败".to_string()),
                );
                let _ = hide_overlay_window_by_label(&app_for_window, "screenshot");
                cleanup_all_screenshot_boot_images();
                capture::set_screenshot_in_progress(false);
            }
        });
    } else {
        let payload = serde_json::json!({
            "png_base64": png_base64,
            "image_path": image_path,
            "width": width,
            "height": height,
            "origin_x": origin_x,
            "origin_y": origin_y,
            "session_id": session_id
        });
        let boot_script = format!(
            "window.__SCREENSHOT_BOOT__ = window.__SCREENSHOT_BOOT__ || {{ pendingData: null, pendingStartSessionId: 0 }};\
window.__SCREENSHOT_BOOT__.pendingData = {};\
window.__SCREENSHOT_BOOT__.pendingStartSessionId = {};\
window.__SCREENSHOT_BOOT__.pendingMode = '{}';",
            payload, session_id, selection_mode
        );
        let window = tauri::WebviewWindowBuilder::new(
            &app,
            "screenshot",
            tauri::WebviewUrl::App("screenshot.html".into()),
        )
            .title("截图选择")
            .visible(false)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .inner_size(width as f64, height as f64)
            .position(origin_x as f64, origin_y as f64)
            .fullscreen(true)
            .on_page_load(move |window, _| {
                let _ = window.eval(&boot_script);
                let app_handle = window.app_handle();
                let _ = show_overlay_window_by_label(&app_handle, "screenshot", true);
            })
            .build()
            .map_err(|e| {
                cleanup_all_screenshot_boot_images();
                capture::set_screenshot_in_progress(false);
                record_perf_metric(
                    "screenshot.open_prepare",
                    "截图打开准备耗时",
                    started_at.elapsed().as_millis() as u64,
                    false,
                    Some(e.to_string()),
                );
                format!("创建截图窗口失败: {}", e)
            })?;
        bind_screenshot_window_lifecycle(&window, &app);
        let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize { width, height }));
        let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
            x: origin_x,
            y: origin_y,
        }));
        record_perf_metric(
            "screenshot.open_prepare",
            "截图打开准备耗时",
            started_at.elapsed().as_millis() as u64,
            true,
            None,
        );
    }

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
