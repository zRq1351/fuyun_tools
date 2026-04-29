use crate::core::app_state::AppState as SharedAppState;
use crate::core::perf_metrics::record_perf_metric;
use crate::sync::Mutex;
use crate::ui::commands_clipboard::{lock_arc_mutex, recompute_selection_related_flags};
use crate::utils::image_clipboard::is_fast_fill_verify_mode_enabled;
use serde::Serialize;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, OnceLock};
use std::sync::Mutex as StdMutex;
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

static LAST_WRITEBACK_RESULT: OnceLock<StdMutex<Option<WriteBackExecutionResult>>> =
    OnceLock::new();

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FillKind {
    Text,
    Image,
}

impl FillKind {
    fn label(self) -> &'static str {
        match self {
            Self::Text => "文本",
            Self::Image => "图片",
        }
    }

    fn window_label(self) -> &'static str {
        match self {
            Self::Text => "clipboard",
            Self::Image => "image_clipboard",
        }
    }

    fn current_seq(self, state: &SharedAppState) -> u64 {
        match self {
            Self::Text => state.text_fill_seq,
            Self::Image => state.image_fill_seq,
        }
    }
}

pub(crate) fn emit_writeback_phase(
    app: &AppHandle,
    source: &str,
    stage: &str,
    operation_id: Option<u64>,
    detail: Option<String>,
) {
    let _ = app.emit(
        "writeback-phase",
        serde_json::json!({
            "source": source,
            "stage": stage,
            "operationId": operation_id,
            "detail": detail,
        }),
    );
}

pub(crate) fn writeback_metric_source_key(source: &str) -> &'static str {
    match source {
        "文本" => "text",
        "图片" => "image",
        "结果窗" => "result_window",
        _ => "unknown",
    }
}

pub(crate) fn record_writeback_stage_metric(
    source: &str,
    stage: &str,
    label: &str,
    duration_ms: u64,
    success: bool,
    error: Option<String>,
) {
    let key = format!(
        "writeback.{}.{}",
        writeback_metric_source_key(source),
        stage
    );
    record_perf_metric(&key, label, duration_ms, success, error);
}

pub(crate) fn perf_metric_group_label(key: &str) -> &'static str {
    if key.starts_with("ocr.") || key.starts_with("ai.") {
        "交互"
    } else if key.starts_with("backup.") {
        "备份"
    } else if key.starts_with("recording.") {
        "录屏"
    } else if key.starts_with("screenshot.") {
        "截图"
    } else if key.starts_with("writeback.") {
        "回写"
    } else if key.starts_with("image.") {
        "图片"
    } else if key.starts_with("text.") {
        "文本历史"
    } else {
        "其他"
    }
}

pub(crate) fn perf_metric_group_rank(group: &str) -> usize {
    match group {
        "交互" => 0,
        "回写" => 1,
        "图片" => 2,
        "截图" => 3,
        "录屏" => 4,
        "备份" => 5,
        "文本历史" => 6,
        _ => 9,
    }
}

pub(crate) fn perf_metric_is_slow(item: &crate::core::perf_metrics::PerfMetricSnapshot) -> bool {
    let (avg_threshold, max_threshold) = if item.key.contains("first_chunk") {
        (1200.0, 2500)
    } else if item.key.contains("history_page") || item.key.contains("wait_hidden") {
        (900.0, 2000)
    } else {
        (1500.0, 3000)
    };
    item.avg_duration_ms >= avg_threshold || item.max_duration_ms >= max_threshold
}

pub(crate) fn last_writeback_result() -> Option<WriteBackExecutionResult> {
    LAST_WRITEBACK_RESULT
        .get()
        .and_then(|slot| slot.lock().ok().and_then(|guard| guard.clone()))
}

pub(crate) fn begin_fill_sequence(state: &Arc<Mutex<SharedAppState>>, kind: FillKind) -> u64 {
    let mut state_guard = lock_arc_mutex(state);
    state_guard.selection_guard_epoch = state_guard.selection_guard_epoch.wrapping_add(1);
    match kind {
        FillKind::Text => state_guard.is_text_writeback_active = true,
        FillKind::Image => state_guard.is_image_writeback_active = true,
    }
    recompute_selection_related_flags(&mut state_guard);
    match kind {
        FillKind::Text => {
            state_guard.text_fill_seq = state_guard.text_fill_seq.wrapping_add(1);
            state_guard.text_fill_seq
        }
        FillKind::Image => {
            state_guard.image_fill_seq = state_guard.image_fill_seq.wrapping_add(1);
            state_guard.image_fill_seq
        }
    }
}

pub(crate) fn is_fill_latest(state: &Arc<Mutex<SharedAppState>>, kind: FillKind, fill_seq: u64) -> bool {
    let guard = lock_arc_mutex(state);
    kind.current_seq(&guard) == fill_seq
}

pub(crate) fn finish_fill_if_latest(state: &Arc<Mutex<SharedAppState>>, kind: FillKind, fill_seq: u64) {
    let mut guard = lock_arc_mutex(state);
    if kind.current_seq(&guard) == fill_seq {
        match kind {
            FillKind::Text => guard.is_text_writeback_active = false,
            FillKind::Image => guard.is_image_writeback_active = false,
        }
        recompute_selection_related_flags(&mut guard);
    }
}

static IMAGE_PROMOTE_SENDER: OnceLock<Sender<String>> = OnceLock::new();

pub(crate) fn interrupt_text_fill_flow(state: &Arc<Mutex<SharedAppState>>) {
    let mut state_guard = lock_arc_mutex(state);
    state_guard.text_fill_seq = state_guard.text_fill_seq.wrapping_add(1);
    state_guard.is_text_writeback_active = false;
    recompute_selection_related_flags(&mut state_guard);
}

pub(crate) fn interrupt_image_fill_flow(state: &Arc<Mutex<SharedAppState>>) {
    let mut state_guard = lock_arc_mutex(state);
    state_guard.image_fill_seq = state_guard.image_fill_seq.wrapping_add(1);
    state_guard.is_image_writeback_active = false;
    recompute_selection_related_flags(&mut state_guard);
}

fn image_promote_worker(state: Arc<Mutex<SharedAppState>>, rx: Receiver<String>) {
    while let Ok(mut item_id) = rx.recv() {
        while let Ok(latest_item_id) = rx.try_recv() {
            item_id = latest_item_id;
        }
        let manager_arc = {
            let state_guard = lock_arc_mutex(&state);
            state_guard.image_clipboard_manager.clone()
        };
        let manager = lock_arc_mutex(&manager_arc);
        if let Err(e) = manager.promote_to_top_by_id(&item_id) {
            log::warn!("极速模式异步置顶图片失败: {}", e);
        } else {
            manager.sync_positions_to_store();
        }
    }
}

pub(crate) fn schedule_image_promote_to_top(state: Arc<Mutex<SharedAppState>>, item_id: String) {
    let sender = IMAGE_PROMOTE_SENDER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<String>();
        let state_for_worker = state.clone();
        thread::spawn(move || image_promote_worker(state_for_worker, rx));
        tx
    });
    if let Err(e) = sender.send(item_id) {
        log::warn!("提交极速模式异步置顶任务失败: {}", e);
    }
}

fn wait_for_fill_window_hidden(
    app: &AppHandle,
    window_label: &str,
    label: &str,
    fast_path: bool,
) -> Result<(), String> {
    let timeout_ms = if fast_path { 220 } else { 900 };
    let state_arc = app.state::<Arc<Mutex<SharedAppState>>>().inner().clone();
    crate::ui::window_manager::wait_for_window_hidden(
        app,
        &state_arc,
        window_label,
        Duration::from_millis(timeout_ms),
    )
        .map_err(|e| {
            let message = e.to_string();
            log::warn!("等待{}窗口隐藏失败: {}", label, message);
            message
        })
}

pub(crate) fn spawn_fill_task<F>(
    kind: FillKind,
    app_handle: AppHandle,
    state: Arc<Mutex<SharedAppState>>,
    fill_seq: u64,
    operation_id: u64,
    write_stage: F,
) where
    F: FnOnce(&AppHandle, &Arc<Mutex<SharedAppState>>) -> Result<(), String> + Send + 'static,
{
    thread::spawn(move || {
        let started_at = std::time::Instant::now();
        let fast_path = kind == FillKind::Image && is_fast_fill_verify_mode_enabled();
        emit_writeback_phase(
            &app_handle,
            kind.label(),
            "waiting_window_hidden",
            Some(operation_id),
            None,
        );
        let wait_started_at = std::time::Instant::now();
        let wait_result =
            wait_for_fill_window_hidden(&app_handle, kind.window_label(), kind.label(), fast_path);
        match &wait_result {
            Ok(_) => record_writeback_stage_metric(
                kind.label(),
                "wait_hidden",
                &format!("{}回写等待窗口隐藏耗时", kind.label()),
                wait_started_at.elapsed().as_millis() as u64,
                true,
                None,
            ),
            Err(error) => record_writeback_stage_metric(
                kind.label(),
                "wait_hidden",
                &format!("{}回写等待窗口隐藏耗时", kind.label()),
                wait_started_at.elapsed().as_millis() as u64,
                false,
                Some(error.clone()),
            ),
        }

        if !is_fill_latest(&state, kind, fill_seq) {
            log::info!(
                "{}回填请求过期，跳过执行: op_id={}",
                kind.label(),
                operation_id
            );
            emit_writeback_phase(
                &app_handle,
                kind.label(),
                "cancelled_stale",
                Some(operation_id),
                Some("回填请求已被更新请求替代".to_string()),
            );
            return;
        }

        let clipboard_started_at = std::time::Instant::now();
        let fill_result = write_stage(&app_handle, &state);
        if fill_result.is_ok() {
            record_writeback_stage_metric(
                kind.label(),
                "write_clipboard",
                &format!("{}回写写入剪贴板耗时", kind.label()),
                clipboard_started_at.elapsed().as_millis() as u64,
                true,
                None,
            );
            emit_writeback_phase(
                &app_handle,
                kind.label(),
                "clipboard_written",
                Some(operation_id),
                None,
            );
            if !is_fill_latest(&state, kind, fill_seq) {
                log::info!(
                    "{}回填请求被新请求替代: op_id={}",
                    kind.label(),
                    operation_id
                );
                emit_writeback_phase(
                    &app_handle,
                    kind.label(),
                    "cancelled_stale",
                    Some(operation_id),
                    Some("回填请求已被更新请求替代".to_string()),
                );
                return;
            }
            emit_writeback_phase(
                &app_handle,
                kind.label(),
                "pasting",
                Some(operation_id),
                None,
            );
            let paste_started_at = std::time::Instant::now();
            let paste_result = simulate_paste_with_retry(
                &app_handle,
                kind.label(),
                Some(operation_id),
                started_at,
                fast_path,
            );
            match paste_result {
                Ok(result) => {
                    record_writeback_stage_metric(
                        kind.label(),
                        "paste",
                        &format!("{}回写粘贴耗时", kind.label()),
                        paste_started_at.elapsed().as_millis() as u64,
                        true,
                        None,
                    );
                    record_writeback_stage_metric(
                        kind.label(),
                        "total",
                        &format!("{}回写总耗时", kind.label()),
                        started_at.elapsed().as_millis() as u64,
                        true,
                        None,
                    );
                    emit_writeback_phase(
                        &app_handle,
                        kind.label(),
                        "completed",
                        Some(operation_id),
                        Some(result.detail.clone()),
                    );
                    emit_writeback_result(&app_handle, &result)
                }
                Err(result) => {
                    record_writeback_stage_metric(
                        kind.label(),
                        "paste",
                        &format!("{}回写粘贴耗时", kind.label()),
                        paste_started_at.elapsed().as_millis() as u64,
                        false,
                        Some(result.detail.clone()),
                    );
                    record_writeback_stage_metric(
                        kind.label(),
                        "total",
                        &format!("{}回写总耗时", kind.label()),
                        started_at.elapsed().as_millis() as u64,
                        false,
                        Some(result.detail.clone()),
                    );
                    emit_writeback_phase(
                        &app_handle,
                        kind.label(),
                        "failed",
                        Some(operation_id),
                        Some(result.detail.clone()),
                    );
                    emit_writeback_result(&app_handle, &result)
                }
            }
        } else if let Err(e) = fill_result {
            log::error!(
                "{}回填失败（写入阶段）: op_id={}, {}",
                kind.label(),
                operation_id,
                e
            );
            record_writeback_stage_metric(
                kind.label(),
                "write_clipboard",
                &format!("{}回写写入剪贴板耗时", kind.label()),
                clipboard_started_at.elapsed().as_millis() as u64,
                false,
                Some(e.clone()),
            );
            record_writeback_stage_metric(
                kind.label(),
                "total",
                &format!("{}回写总耗时", kind.label()),
                started_at.elapsed().as_millis() as u64,
                false,
                Some(e.clone()),
            );
            emit_writeback_phase(
                &app_handle,
                kind.label(),
                "failed",
                Some(operation_id),
                Some(e.clone()),
            );
            emit_writeback_result(
                &app_handle,
                &WriteBackExecutionResult {
                    source: kind.label().to_string(),
                    success: false,
                    stage: "write_clipboard_failed".to_string(),
                    target_window_title: String::new(),
                    target_window_pid: 0,
                    detail: e,
                    operation_id: Some(operation_id),
                },
            );
        }

        finish_fill_if_latest(&state, kind, fill_seq);
    });
}

pub(crate) fn simulate_paste_with_retry(
    app_handle: &AppHandle,
    label: &str,
    operation_id: Option<u64>,
    started_at: std::time::Instant,
    fast_path: bool,
) -> Result<WriteBackExecutionResult, WriteBackExecutionResult> {
    let is_post_paste_ctrl_release_error = |err: &str| err.contains("释放 Ctrl");
    let mode_name = if fast_path {
        "极速模式"
    } else {
        "普通模式"
    };
    let retry_delays: &[u64] = if fast_path { &[8, 16] } else { &[22, 40, 58] };

    match crate::ui::window_manager::simulate_paste(app_handle) {
        Ok(target) => {
            if let Some(op_id) = operation_id {
                log::info!(
                    "{}回填完成: op_id={}, 耗时: {}ms",
                    label,
                    op_id,
                    started_at.elapsed().as_millis()
                );
            } else {
                log::info!(
                    "{}回填完成，耗时: {}ms",
                    label,
                    started_at.elapsed().as_millis()
                );
            }
            Ok(WriteBackExecutionResult {
                source: label.to_string(),
                success: true,
                stage: "pasted".to_string(),
                target_window_title: target.title,
                target_window_pid: target.pid,
                detail: String::new(),
                operation_id,
            })
        }
        Err(first_error) => {
            if is_post_paste_ctrl_release_error(&first_error) {
                log::warn!(
                    "{}回填检测到粘贴后Ctrl释放异常，跳过二次粘贴以避免重复输入: {}",
                    label,
                    first_error
                );
                if let Err(release_error) = crate::ui::window_manager::force_release_ctrl_key() {
                    log::warn!("{}回填粘贴后Ctrl异常兜底释放失败: {}", label, release_error);
                }
                return Err(WriteBackExecutionResult {
                    source: label.to_string(),
                    success: false,
                    stage: "paste_ctrl_release_failed".to_string(),
                    target_window_title: String::new(),
                    target_window_pid: 0,
                    detail: first_error,
                    operation_id,
                });
            }
            let mut final_error = first_error.clone();
            for delay in retry_delays {
                thread::sleep(Duration::from_millis(*delay));
                if let Err(release_error) = crate::ui::window_manager::force_release_ctrl_key() {
                    log::warn!(
                        "{}回填{}重试前释放Ctrl失败: {}",
                        label,
                        mode_name,
                        release_error
                    );
                }
                match crate::ui::window_manager::simulate_paste(app_handle) {
                    Ok(target) => {
                        if let Some(op_id) = operation_id {
                            log::warn!(
                                "{}回填{}首次粘贴失败，状态驱动重试成功: op_id={}, {}，总耗时: {}ms",
                                label,
                                mode_name,
                                op_id,
                                first_error,
                                started_at.elapsed().as_millis()
                            );
                        } else {
                            log::warn!(
                                "{}回填{}首次粘贴失败，状态驱动重试成功: {}，总耗时: {}ms",
                                label,
                                mode_name,
                                first_error,
                                started_at.elapsed().as_millis()
                            );
                        }
                        return Ok(WriteBackExecutionResult {
                            source: label.to_string(),
                            success: true,
                            stage: "pasted_after_retry".to_string(),
                            target_window_title: target.title,
                            target_window_pid: target.pid,
                            detail: format!("首次失败后重试成功: {}", first_error),
                            operation_id,
                        });
                    }
                    Err(next_error) => {
                        final_error = next_error;
                        if is_post_paste_ctrl_release_error(&final_error) {
                            log::warn!(
                                "{}回填{}检测到粘贴后Ctrl释放异常，停止后续重试: {}",
                                label,
                                mode_name,
                                final_error
                            );
                            if let Err(release_error) =
                                crate::ui::window_manager::force_release_ctrl_key()
                            {
                                log::warn!(
                                    "{}回填粘贴后Ctrl异常兜底释放失败: {}",
                                    label,
                                    release_error
                                );
                            }
                            return Err(WriteBackExecutionResult {
                                source: label.to_string(),
                                success: false,
                                stage: "paste_ctrl_release_failed".to_string(),
                                target_window_title: String::new(),
                                target_window_pid: 0,
                                detail: final_error,
                                operation_id,
                            });
                        }
                    }
                }
            }
            if let Err(release_error) = crate::ui::window_manager::force_release_ctrl_key() {
                log::warn!(
                    "{}回填{}最终兜底释放Ctrl失败: {}",
                    label,
                    mode_name,
                    release_error
                );
            }
            if let Some(op_id) = operation_id {
                log::error!(
                    "{}回填{}粘贴失败: op_id={}, 首次错误: {}，最终错误: {}",
                    label,
                    mode_name,
                    op_id,
                    first_error,
                    final_error
                );
            } else {
                log::error!(
                    "{}回填{}粘贴失败，首次错误: {}，最终错误: {}",
                    label,
                    mode_name,
                    first_error,
                    final_error
                );
            }
            Err(WriteBackExecutionResult {
                source: label.to_string(),
                success: false,
                stage: "paste_failed".to_string(),
                target_window_title: String::new(),
                target_window_pid: 0,
                detail: format!("首次错误: {}，最终错误: {}", first_error, final_error),
                operation_id,
            })
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteBackExecutionResult {
    pub source: String,
    pub success: bool,
    pub stage: String,
    pub target_window_title: String,
    pub target_window_pid: u32,
    pub detail: String,
    pub operation_id: Option<u64>,
}

pub(crate) fn emit_writeback_result(app: &AppHandle, result: &WriteBackExecutionResult) {
    let slot = LAST_WRITEBACK_RESULT.get_or_init(|| StdMutex::new(None));
    if let Ok(mut guard) = slot.lock() {
        *guard = Some(result.clone());
    }
    let _ = app.emit("writeback-result", result);
}
