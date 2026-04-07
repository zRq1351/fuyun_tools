use crate::features::recording::ffmpeg_runner::resolve_ffmpeg_path;
use base64::Engine;
use image::ImageEncoder;
use opencv::core::{self, Mat, MatTraitConst, Point};
use opencv::imgproc;
use opencv::prelude::*;
use serde::{Deserialize, Serialize};
use std::io::Read;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
static NEXT_LONGSHOT_SESSION_ID: AtomicU64 = AtomicU64::new(1);
static MANUAL_LONGSHOT_RUNTIME: OnceLock<StdMutex<Option<ManualLongshotRuntime>>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LongshotRegion {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartManualLongshotRequest {
    pub region: LongshotRegion,
    #[serde(default = "default_longshot_fps")]
    pub fps: u32,
    #[serde(default = "default_longshot_min_confidence")]
    pub min_confidence: f32,
    #[serde(default = "default_longshot_max_duration_sec")]
    pub max_duration_sec: u32,
    #[serde(default = "default_longshot_preview_interval_ms")]
    pub preview_interval_ms: u32,
}

fn default_longshot_fps() -> u32 {
    10
}

fn default_longshot_min_confidence() -> f32 {
    0.82
}

fn default_longshot_max_duration_sec() -> u32 {
    90
}

fn default_longshot_preview_interval_ms() -> u32 {
    300
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualLongshotStatus {
    pub session_id: u64,
    pub state: String,
    pub region: LongshotRegion,
    pub frame_count: u64,
    pub dropped_frames: u64,
    pub stitched_height: u32,
    pub stitched_width: u32,
    pub last_confidence: f32,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualLongshotFinishResult {
    pub session_id: u64,
    pub width: u32,
    pub height: u32,
    pub png_base64: String,
    pub frame_count: u64,
    pub dropped_frames: u64,
}

struct ManualLongshotControl {
    stop: AtomicBool,
    paused: AtomicBool,
    status: StdMutex<ManualLongshotStatus>,
    result: StdMutex<Option<ManualLongshotFinishResult>>,
}

struct ManualLongshotRuntime {
    session_id: u64,
    control: Arc<ManualLongshotControl>,
    worker: Option<JoinHandle<()>>,
}

#[derive(Debug, Clone, Copy)]
struct AlignEstimate {
    overlap_rows: i32,
    unique_rows: i32,
    confidence: f32,
    phase_unique_rows: i32,
    phase_response: f32,
    seam_cost: f32,
}

fn runtime_slot() -> &'static StdMutex<Option<ManualLongshotRuntime>> {
    MANUAL_LONGSHOT_RUNTIME.get_or_init(|| StdMutex::new(None))
}

fn suppress_console_window(command: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }
}

pub fn start_manual_longshot(
    app: AppHandle,
    mut request: StartManualLongshotRequest,
) -> Result<serde_json::Value, String> {
    if request.region.width < 64 || request.region.height < 64 {
        return Err("长截图区域太小，最小为 64x64".to_string());
    }
    request.fps = request.fps.clamp(4, 24);
    request.min_confidence = request.min_confidence.clamp(0.5, 0.99);
    request.max_duration_sec = request.max_duration_sec.clamp(10, 300);
    request.preview_interval_ms = request.preview_interval_ms.clamp(120, 1000);

    let mut slot = runtime_slot()
        .lock()
        .map_err(|_| "长截图会话锁不可用".to_string())?;
    if let Some(existing) = slot.as_ref() {
        let status = existing
            .control
            .status
            .lock()
            .map_err(|_| "长截图状态锁不可用".to_string())?;
        if status.state == "running" || status.state == "paused" {
            return Err("已有长截图会话进行中，请先完成或取消".to_string());
        }
    }

    let session_id = NEXT_LONGSHOT_SESSION_ID.fetch_add(1, Ordering::SeqCst);
    let control = Arc::new(ManualLongshotControl {
        stop: AtomicBool::new(false),
        paused: AtomicBool::new(false),
        status: StdMutex::new(ManualLongshotStatus {
            session_id,
            state: "running".to_string(),
            region: request.region.clone(),
            frame_count: 0,
            dropped_frames: 0,
            stitched_height: 0,
            stitched_width: request.region.width,
            last_confidence: 0.0,
            last_error: None,
        }),
        result: StdMutex::new(None),
    });

    let worker_control = control.clone();
    let worker_app = app.clone();
    let worker = thread::spawn(move || {
        run_longshot_worker(worker_app, worker_control, request);
    });

    *slot = Some(ManualLongshotRuntime {
        session_id,
        control: control.clone(),
        worker: Some(worker),
    });
    drop(slot);

    let _ = app.emit(
        "manual-longshot-lifecycle",
        serde_json::json!({
            "sessionId": session_id,
            "state": "started",
        }),
    );

    Ok(serde_json::json!({
        "success": true,
        "sessionId": session_id
    }))
}

pub fn pause_manual_longshot(session_id: u64, app: AppHandle) -> Result<(), String> {
    with_runtime(session_id, |runtime| {
        runtime.control.paused.store(true, Ordering::Release);
        if let Ok(mut status) = runtime.control.status.lock() {
            status.state = "paused".to_string();
        }
        let _ = app.emit(
            "manual-longshot-lifecycle",
            serde_json::json!({
                "sessionId": session_id,
                "state": "paused",
            }),
        );
        Ok(())
    })
}

pub fn resume_manual_longshot(session_id: u64, app: AppHandle) -> Result<(), String> {
    with_runtime(session_id, |runtime| {
        runtime.control.paused.store(false, Ordering::Release);
        if let Ok(mut status) = runtime.control.status.lock() {
            status.state = "running".to_string();
        }
        let _ = app.emit(
            "manual-longshot-lifecycle",
            serde_json::json!({
                "sessionId": session_id,
                "state": "resumed",
            }),
        );
        Ok(())
    })
}

pub fn cancel_manual_longshot(session_id: u64, app: AppHandle) -> Result<(), String> {
    let mut slot = runtime_slot()
        .lock()
        .map_err(|_| "长截图会话锁不可用".to_string())?;
    let Some(mut runtime) = slot.take() else {
        return Err("未找到进行中的长截图会话".to_string());
    };
    if runtime.session_id != session_id {
        *slot = Some(runtime);
        return Err("长截图会话 ID 不匹配".to_string());
    }

    runtime.control.stop.store(true, Ordering::Release);
    runtime.control.paused.store(false, Ordering::Release);
    if let Ok(mut status) = runtime.control.status.lock() {
        status.state = "canceled".to_string();
    }
    let worker = runtime.worker.take();
    drop(slot);
    if let Some(handle) = worker {
        let _ = handle.join();
    }

    let _ = app.emit(
        "manual-longshot-lifecycle",
        serde_json::json!({
            "sessionId": session_id,
            "state": "canceled",
        }),
    );
    Ok(())
}

pub fn finish_manual_longshot(
    session_id: u64,
    app: AppHandle,
) -> Result<ManualLongshotFinishResult, String> {
    let mut slot = runtime_slot()
        .lock()
        .map_err(|_| "长截图会话锁不可用".to_string())?;
    let Some(mut runtime) = slot.take() else {
        return Err("未找到进行中的长截图会话".to_string());
    };
    if runtime.session_id != session_id {
        *slot = Some(runtime);
        return Err("长截图会话 ID 不匹配".to_string());
    }

    runtime.control.stop.store(true, Ordering::Release);
    runtime.control.paused.store(false, Ordering::Release);
    if let Ok(mut status) = runtime.control.status.lock() {
        status.state = "finishing".to_string();
    }
    let worker = runtime.worker.take();
    let control = runtime.control.clone();
    drop(slot);

    if let Some(handle) = worker {
        let _ = handle.join();
    }
    let result = control
        .result
        .lock()
        .map_err(|_| "长截图结果锁不可用".to_string())?
        .clone();
    let Some(final_result) = result else {
        let status = control
            .status
            .lock()
            .map_err(|_| "长截图状态锁不可用".to_string())?
            .clone();
        return Err(
            status
                .last_error
                .unwrap_or_else(|| "长截图结束失败，未生成结果图片".to_string()),
        );
    };

    let _ = app.emit(
        "manual-longshot-lifecycle",
        serde_json::json!({
            "sessionId": session_id,
            "state": "ended",
            "width": final_result.width,
            "height": final_result.height,
        }),
    );
    Ok(final_result)
}

pub fn get_manual_longshot_status(session_id: u64) -> Result<ManualLongshotStatus, String> {
    with_runtime(session_id, |runtime| {
        runtime
            .control
            .status
            .lock()
            .map_err(|_| "长截图状态锁不可用".to_string())
            .map(|s| s.clone())
    })
}

pub fn active_manual_longshot_session_id() -> Option<u64> {
    let slot = runtime_slot().lock().ok()?;
    let runtime = slot.as_ref()?;
    let status = runtime.control.status.lock().ok()?;
    if status.state == "running" || status.state == "paused" || status.state == "finishing" {
        Some(runtime.session_id)
    } else {
        None
    }
}

fn with_runtime<T, F>(session_id: u64, f: F) -> Result<T, String>
where
    F: FnOnce(&ManualLongshotRuntime) -> Result<T, String>,
{
    let slot = runtime_slot()
        .lock()
        .map_err(|_| "长截图会话锁不可用".to_string())?;
    let Some(runtime) = slot.as_ref() else {
        return Err("未找到进行中的长截图会话".to_string());
    };
    if runtime.session_id != session_id {
        return Err("长截图会话 ID 不匹配".to_string());
    }
    f(runtime)
}

fn run_longshot_worker(
    app: AppHandle,
    control: Arc<ManualLongshotControl>,
    request: StartManualLongshotRequest,
) {
    let session_id = {
        let status = control.status.lock();
        match status {
            Ok(s) => s.session_id,
            Err(_) => 0,
        }
    };
    let run_result = run_longshot_worker_inner(&app, &control, &request);
    if let Err(err) = run_result {
        if let Ok(mut status) = control.status.lock() {
            status.state = "error".to_string();
            status.last_error = Some(err.clone());
        }
        let _ = app.emit(
            "manual-longshot-lifecycle",
            serde_json::json!({
                "sessionId": session_id,
                "state": "error",
                "message": err,
            }),
        );
    }
}

fn run_longshot_worker_inner(
    app: &AppHandle,
    control: &Arc<ManualLongshotControl>,
    request: &StartManualLongshotRequest,
) -> Result<(), String> {
    let ffmpeg = resolve_ffmpeg_path()?;
    let frame_width = request.region.width as usize;
    let frame_height = request.region.height as usize;
    let frame_bytes = frame_width
        .checked_mul(frame_height)
        .ok_or_else(|| "长截图区域尺寸过大".to_string())?;

    let mut command = Command::new(ffmpeg);
    suppress_console_window(&mut command);
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-f")
        .arg("gdigrab")
        .arg("-framerate")
        .arg(request.fps.to_string())
        .arg("-offset_x")
        .arg(request.region.x.to_string())
        .arg("-offset_y")
        .arg(request.region.y.to_string())
        .arg("-video_size")
        .arg(format!("{}x{}", request.region.width, request.region.height))
        .arg("-i")
        .arg("desktop")
        .arg("-vf")
        .arg("format=gray")
        .arg("-pix_fmt")
        .arg("gray")
        .arg("-f")
        .arg("rawvideo")
        .arg("pipe:1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|e| format!("启动 ffmpeg 长截图采样失败: {}", e))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "无法读取 ffmpeg stdout".to_string())?;

    let start_at = Instant::now();
    let mut last_progress_emit = Instant::now();
    let mut frame_buf = vec![0u8; frame_bytes];
    // 仅使用“已成功拼接到 stitched 的最后一帧”作为对齐基准，
    // 避免丢弃帧污染基准导致累计重叠/错位。
    let mut anchor_frame: Option<Mat> = None;
    let mut stitched: Option<Mat> = None;
    let mut consecutive_drops: u32 = 0;
    let mut finishing_drain_left: Option<u32> = None;
    let mut ended_by_finishing = false;

    loop {
        if control.stop.load(Ordering::Acquire) {
            let finishing = control
                .status
                .lock()
                .map(|s| s.state == "finishing")
                .unwrap_or(false);
            if !finishing {
                // cancel 等场景立即停
                break;
            }
            ended_by_finishing = true;
            if finishing_drain_left.is_none() {
                // finish 时短暂排空 ffmpeg 缓冲，避免尾段丢失
                finishing_drain_left = Some(request.fps.clamp(6, 24));
            } else if finishing_drain_left == Some(0) {
                break;
            }
        }
        if start_at.elapsed() > Duration::from_secs(request.max_duration_sec as u64) {
            break;
        }
        if control.paused.load(Ordering::Acquire) {
            thread::sleep(Duration::from_millis(40));
            continue;
        }

        if stdout.read_exact(&mut frame_buf).is_err() {
            break;
        }
        let frame = frame_from_gray_bytes(&frame_buf, frame_height as i32)?;

        if stitched.is_none() {
            stitched = Some(frame.try_clone().map_err(to_cv_err)?);
            anchor_frame = Some(frame);
            let mut first_frame_session_id = 0u64;
            if let Ok(mut status) = control.status.lock() {
                first_frame_session_id = status.session_id;
                status.frame_count = 1;
                status.stitched_width = request.region.width;
                status.stitched_height = request.region.height;
            }
            let _ = app.emit(
                "manual-longshot-first-frame",
                serde_json::json!({
                    "sessionId": first_frame_session_id,
                }),
            );
            continue;
        }

        let Some(prev) = anchor_frame.as_ref() else {
            anchor_frame = Some(frame);
            continue;
        };

        let estimate = estimate_overlap(prev, &frame)?;
        let adaptive_min_conf = if consecutive_drops >= 10 {
            (request.min_confidence * 0.55).clamp(0.35, 0.95)
        } else if consecutive_drops >= 6 {
            (request.min_confidence * 0.70).clamp(0.45, 0.95)
        } else if consecutive_drops >= 3 {
            (request.min_confidence * 0.85).clamp(0.50, 0.95)
        } else {
            request.min_confidence
        };
        let mut dropped = false;
        let finishing_mode = finishing_drain_left.is_some();
        if finishing_mode {
            // 收尾阶段放宽门控，优先避免“最后一段漏拼”
            let tail_motion_ok = estimate.unique_rows >= 3
                && estimate.phase_unique_rows >= 2
                && (estimate.phase_unique_rows - estimate.unique_rows).abs() <= 44
                && estimate.seam_cost <= 18.0
                && estimate.confidence >= 0.28;
            if !tail_motion_ok {
                dropped = true;
            }
        } else {
            // 常规阶段严格门控：必须检测到足够稳定的垂直位移
            let motion_ok = estimate.unique_rows >= 8
                && estimate.phase_unique_rows >= 6
                && (estimate.phase_unique_rows - estimate.unique_rows).abs() <= 28
                && (estimate.phase_response >= 0.010 || estimate.confidence >= 0.78);
            if !motion_ok || estimate.unique_rows <= 6 || estimate.confidence < adaptive_min_conf {
                dropped = true;
            }
        }
        if !dropped {
            if let Some(existing) = stitched.take() {
            let append = frame
                .row_range(&core::Range::new(estimate.overlap_rows, frame.rows()).map_err(to_cv_err)?)
                .map_err(to_cv_err)?
                .try_clone()
                .map_err(to_cv_err)?;
            stitched = Some(vconcat_mats(&existing, &append)?);
            // 只有真正拼接成功后，才更新基准帧
            anchor_frame = Some(frame.try_clone().map_err(to_cv_err)?);
            consecutive_drops = 0;
            }
        }
        if dropped {
            consecutive_drops = consecutive_drops.saturating_add(1);
        }
        if let Ok(mut status) = control.status.lock() {
            status.frame_count = status.frame_count.saturating_add(1);
            if dropped {
                status.dropped_frames = status.dropped_frames.saturating_add(1);
            }
            status.last_confidence = estimate.confidence;
            if let Some(st) = stitched.as_ref() {
                status.stitched_height = st.rows().max(0) as u32;
                status.stitched_width = st.cols().max(0) as u32;
            }
            if status.state != "paused" {
                status.state = "running".to_string();
            }
        }

        if last_progress_emit.elapsed().as_millis() >= request.preview_interval_ms as u128 {
            let mut current_session_id = 0u64;
            if let Ok(status) = control.status.lock() {
                current_session_id = status.session_id;
                let _ = app.emit(
                    "manual-longshot-progress",
                    serde_json::json!({
                        "sessionId": status.session_id,
                        "state": status.state,
                        "frameCount": status.frame_count,
                        "droppedFrames": status.dropped_frames,
                        "stitchedHeight": status.stitched_height,
                        "stitchedWidth": status.stitched_width,
                        "captureHeight": status.region.height,
                        "captureWidth": status.region.width,
                        "lastConfidence": status.last_confidence,
                    }),
                );
            }
            if let Some(stitched_mat) = stitched.as_ref() {
                if let Ok(preview_base64) = gray_mat_to_preview_base64(stitched_mat, 300, 110) {
                    let _ = app.emit(
                        "manual-longshot-preview-updated",
                        serde_json::json!({
                            "sessionId": current_session_id,
                            "previewBase64": preview_base64,
                        }),
                    );
                }
            }
            last_progress_emit = Instant::now();
        }

        if let Some(left) = finishing_drain_left.as_mut() {
            if *left > 0 {
                *left -= 1;
            }
            if *left == 0 {
                break;
            }
        }
    }

    let _ = child.kill();
    let _ = child.wait();

    // 根因级收尾：finish 触发后再抓取一帧“最终静态画面”参与拼接，
    // 避免最后一次滚动发生在流式采样间隙而被整体漏掉。
    if ended_by_finishing {
        if let Ok(final_frame) = capture_single_gray_frame(request) {
            if let Some(prev) = anchor_frame.as_ref() {
                if let Ok(estimate) = estimate_overlap(prev, &final_frame) {
                    let tail_ok = estimate.unique_rows >= 2
                        && estimate.phase_unique_rows >= 2
                        && (estimate.phase_unique_rows - estimate.unique_rows).abs() <= 52
                        && estimate.seam_cost <= 22.0
                        && estimate.confidence >= 0.20;
                    if tail_ok {
                        if let Some(existing) = stitched.take() {
                            let append = final_frame
                                .row_range(
                                    &core::Range::new(estimate.overlap_rows, final_frame.rows())
                                        .map_err(to_cv_err)?,
                                )
                                .map_err(to_cv_err)?
                                .try_clone()
                                .map_err(to_cv_err)?;
                            stitched = Some(vconcat_mats(&existing, &append)?);
                            anchor_frame = Some(final_frame.try_clone().map_err(to_cv_err)?);
                            if let Ok(mut status) = control.status.lock() {
                                status.frame_count = status.frame_count.saturating_add(1);
                                status.last_confidence = estimate.confidence;
                                if let Some(st) = stitched.as_ref() {
                                    status.stitched_height = st.rows().max(0) as u32;
                                    status.stitched_width = st.cols().max(0) as u32;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let final_mat = stitched.ok_or_else(|| "长截图没有采集到有效画面".to_string())?;
    let width = final_mat.cols().max(0) as u32;
    let height = final_mat.rows().max(0) as u32;
    let png_base64 = gray_mat_to_png_base64(&final_mat)?;
    let status_snapshot = control
        .status
        .lock()
        .map_err(|_| "长截图状态锁不可用".to_string())?
        .clone();

    let finish = ManualLongshotFinishResult {
        session_id: status_snapshot.session_id,
        width,
        height,
        png_base64,
        frame_count: status_snapshot.frame_count,
        dropped_frames: status_snapshot.dropped_frames,
    };

    if let Ok(mut result) = control.result.lock() {
        *result = Some(finish);
    }
    if let Ok(mut status) = control.status.lock() {
        if status.state != "canceled" {
            status.state = "ended".to_string();
        }
    }
    Ok(())
}

fn capture_single_gray_frame(request: &StartManualLongshotRequest) -> Result<Mat, String> {
    let ffmpeg = resolve_ffmpeg_path()?;
    let frame_bytes = (request.region.width as usize)
        .checked_mul(request.region.height as usize)
        .ok_or_else(|| "长截图区域尺寸过大".to_string())?;
    let mut command = Command::new(ffmpeg);
    suppress_console_window(&mut command);
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-f")
        .arg("gdigrab")
        .arg("-framerate")
        .arg("30")
        .arg("-offset_x")
        .arg(request.region.x.to_string())
        .arg("-offset_y")
        .arg(request.region.y.to_string())
        .arg("-video_size")
        .arg(format!("{}x{}", request.region.width, request.region.height))
        .arg("-i")
        .arg("desktop")
        .arg("-frames:v")
        .arg("1")
        .arg("-vf")
        .arg("format=gray")
        .arg("-pix_fmt")
        .arg("gray")
        .arg("-f")
        .arg("rawvideo")
        .arg("pipe:1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|e| format!("收尾抓取最终帧失败: {}", e))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "收尾抓取无法读取 ffmpeg stdout".to_string())?;
    let mut frame_buf = vec![0u8; frame_bytes];
    stdout
        .read_exact(&mut frame_buf)
        .map_err(|e| format!("收尾抓取读取最终帧失败: {}", e))?;
    let _ = child.kill();
    let _ = child.wait();
    frame_from_gray_bytes(&frame_buf, request.region.height as i32)
}

fn frame_from_gray_bytes(bytes: &[u8], height: i32) -> Result<Mat, String> {
    let mat_1d = Mat::from_slice(bytes).map_err(to_cv_err)?;
    let reshaped = mat_1d.reshape(1, height).map_err(to_cv_err)?;
    reshaped.try_clone().map_err(to_cv_err)
}

fn estimate_overlap(prev: &Mat, curr: &Mat) -> Result<AlignEstimate, String> {
    let rows = prev.rows();
    if rows <= 16 {
        return Err("长截图帧高度过小".to_string());
    }
    let tpl_h = (rows / 4).clamp(64, 220);
    let template = curr
        .row_range(&core::Range::new(0, tpl_h).map_err(to_cv_err)?)
        .map_err(to_cv_err)?;

    let mut result = Mat::default();
    imgproc::match_template(
        prev,
        &template,
        &mut result,
        imgproc::TM_CCOEFF_NORMED,
        &core::no_array(),
    )
    .map_err(to_cv_err)?;
    let mut min_val = 0.0f64;
    let mut max_val = 0.0f64;
    let mut min_loc = Point::new(0, 0);
    let mut max_loc = Point::new(0, 0);
    core::min_max_loc(
        &result,
        Some(&mut min_val),
        Some(&mut max_val),
        Some(&mut min_loc),
        Some(&mut max_loc),
        &core::no_array(),
    )
    .map_err(to_cv_err)?;

    let template_overlap = (rows - max_loc.y).clamp(1, rows - 1);

    let mut prev32 = Mat::default();
    let mut curr32 = Mat::default();
    prev.convert_to(&mut prev32, core::CV_32F, 1.0, 0.0)
        .map_err(to_cv_err)?;
    curr.convert_to(&mut curr32, core::CV_32F, 1.0, 0.0)
        .map_err(to_cv_err)?;
    let mut response = 0.0f64;
    let phase_shift = imgproc::phase_correlate(&prev32, &curr32, &core::no_array(), &mut response)
        .map_err(to_cv_err)?;
    let phase_unique = phase_shift.y.abs().round() as i32;
    let phase_overlap = (rows - phase_unique).clamp(1, rows - 1);

    // 先融合 template 与 phase 估计，再做一轮 seam 代价精修，降低重复段风险
    let mut base_overlap = template_overlap;
    if response > 0.02 {
        if (phase_overlap - template_overlap).abs() <= 48 {
            // 在两个估计接近时偏向更大 overlap，避免拼接重复
            base_overlap = template_overlap.max(phase_overlap);
        } else if response > 0.12 {
            base_overlap = phase_overlap;
        }
    }
    let overlap_rows = refine_overlap_rows(prev, curr, base_overlap)?;
    let unique_rows = rows - overlap_rows;
    let diff = (phase_unique - unique_rows).abs();
    let phase_factor = if response > 0.02 && diff <= 48 { 1.0 } else { 0.85 };
    let seam_cost = overlap_sad_cost(prev, curr, overlap_rows).unwrap_or(255.0);
    let seam_factor = if seam_cost < 6.0 {
        1.0
    } else if seam_cost < 12.0 {
        0.95
    } else if seam_cost < 20.0 {
        0.85
    } else {
        0.7
    };
    let confidence = (max_val as f32 * phase_factor as f32 * seam_factor as f32).clamp(0.0, 1.0);

    Ok(AlignEstimate {
        overlap_rows,
        unique_rows,
        confidence,
        phase_unique_rows: phase_unique,
        phase_response: response as f32,
        seam_cost: seam_cost as f32,
    })
}

fn refine_overlap_rows(prev: &Mat, curr: &Mat, seed_overlap: i32) -> Result<i32, String> {
    let rows = prev.rows();
    let seed = seed_overlap.clamp(1, rows - 1);
    let min_overlap = (seed - 40).max(8);
    let max_overlap = (seed + 40).min(rows - 1);
    let mut best_overlap = seed;
    let mut best_cost = f64::INFINITY;
    for overlap in min_overlap..=max_overlap {
        let cost = overlap_sad_cost(prev, curr, overlap)?;
        if cost < best_cost {
            best_cost = cost;
            best_overlap = overlap;
        }
    }
    Ok(best_overlap)
}

fn overlap_sad_cost(prev: &Mat, curr: &Mat, overlap_rows: i32) -> Result<f64, String> {
    let rows = prev.rows();
    if overlap_rows <= 1 || overlap_rows >= rows {
        return Ok(f64::INFINITY);
    }
    let probe_h = overlap_rows.clamp(12, 72);
    if overlap_rows < probe_h {
        return Ok(f64::INFINITY);
    }
    let prev_tail = prev
        .row_range(&core::Range::new(rows - probe_h, rows).map_err(to_cv_err)?)
        .map_err(to_cv_err)?;
    let curr_tail = curr
        .row_range(&core::Range::new(overlap_rows - probe_h, overlap_rows).map_err(to_cv_err)?)
        .map_err(to_cv_err)?;
    let mut diff = Mat::default();
    core::absdiff(&prev_tail, &curr_tail, &mut diff).map_err(to_cv_err)?;
    let avg = core::mean(&diff, &core::no_array()).map_err(to_cv_err)?;
    Ok(avg[0])
}

fn vconcat_mats(top: &Mat, bottom: &Mat) -> Result<Mat, String> {
    let mut mats = core::Vector::<Mat>::new();
    mats.push(top.try_clone().map_err(to_cv_err)?);
    mats.push(bottom.try_clone().map_err(to_cv_err)?);
    let mut out = Mat::default();
    core::vconcat(&mats, &mut out).map_err(to_cv_err)?;
    Ok(out)
}

fn gray_mat_to_png_base64(gray: &Mat) -> Result<String, String> {
    if gray.cols() <= 0 || gray.rows() <= 0 {
        return Err("长截图结果为空".to_string());
    }
    let width = gray.cols() as u32;
    let height = gray.rows() as u32;
    let mut packed = Mat::default();
    gray.copy_to(&mut packed).map_err(to_cv_err)?;
    let bytes = packed.data_bytes().map_err(to_cv_err)?.to_vec();
    let image = image::GrayImage::from_raw(width, height, bytes)
        .ok_or_else(|| "长截图像素数据构建失败".to_string())?;
    let mut png = Vec::<u8>::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut png);
    encoder
        .write_image(
            image.as_raw(),
            width,
            height,
            image::ExtendedColorType::L8,
        )
        .map_err(|e| format!("长截图 PNG 编码失败: {}", e))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(png))
}

fn gray_mat_to_preview_base64(gray: &Mat, max_width: i32, max_height: i32) -> Result<String, String> {
    if gray.cols() <= 0 || gray.rows() <= 0 {
        return Err("预览图为空".to_string());
    }
    let src_w = gray.cols();
    let src_h = gray.rows();
    let scale_w = max_width as f64 / src_w as f64;
    let scale_h = max_height as f64 / src_h as f64;
    let scale = scale_w.min(scale_h).min(1.0).max(0.01);
    let dst_w = ((src_w as f64) * scale).round().max(1.0) as i32;
    let dst_h = ((src_h as f64) * scale).round().max(1.0) as i32;

    let mut resized = Mat::default();
    imgproc::resize(
        gray,
        &mut resized,
        core::Size {
            width: dst_w,
            height: dst_h,
        },
        0.0,
        0.0,
        imgproc::INTER_AREA,
    )
    .map_err(to_cv_err)?;
    gray_mat_to_png_base64(&resized)
}

fn to_cv_err<E: std::fmt::Display>(e: E) -> String {
    format!("OpenCV 错误: {}", e)
}
