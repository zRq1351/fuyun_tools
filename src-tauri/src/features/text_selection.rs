use crate::services::clipboard_wakeup::subscribe_clipboard_wake_events;
use crate::sync::{lock_arc_mutex, Mutex};
use crate::ui::window_manager::{release_ctrl_key_with_fallback, ENIGO_INSTANCE};
use crate::utils::clipboard::ClipboardManager;
use enigo::{Enigo, Keyboard, Settings};
use log;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tauri::AppHandle;

fn execute_ctrl_c_with_safety(enigo: &mut Enigo) -> Result<(), String> {
    let is_console = crate::features::mouse_listener::is_foreground_window_console();

    match enigo.key(CTRL_KEY, enigo::Direction::Press) {
        Ok(_) => {}
        Err(e) => return Err(format!("按下 Ctrl 键失败: {:?}", e)),
    }

    thread::sleep(Duration::from_millis(100));

    fn release_ctrl(enigo: &mut Enigo) -> Result<(), String> {
        release_ctrl_key_with_fallback(enigo).map_err(|e| format!("释放 Ctrl 键失败: {}", e))
    }

    match enigo.key(C_KEY, enigo::Direction::Click) {
        Ok(_) => {}
        Err(e) => {
            let _ = release_ctrl(enigo);
            return Err(format!("按下 C/Insert 键失败: {:?}", e));
        }
    }

    thread::sleep(Duration::from_millis(100));

    release_ctrl(enigo)?;

    log::info!("已发送复制模拟按键 (is_console: {})", is_console);
    Ok(())
}

/// 划词捕获最大重试时长
const CAPTURE_RETRY_MAX_DURATION: Duration = Duration::from_millis(600);
/// 轮询间隔，使用序列号检测时可以更频繁
const CAPTURE_RETRY_INTERVAL: Duration = Duration::from_millis(10);
/// 模拟按键后的初始等待时间
const INITIAL_DELAY: Duration = Duration::from_millis(10);

use crate::core::app_state::AppState as SharedAppState;
use crate::core::config::{CTRL_KEY, C_KEY};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::image::Image;
use tauri::Manager;
use tauri_plugin_clipboard_manager::ClipboardExt;
#[cfg(target_os = "windows")]
use winapi::um::winuser::GetClipboardSequenceNumber;

static MANUAL_CTRL_C_TIME: AtomicU64 = AtomicU64::new(0);
static ALLOW_CLIPBOARD_LISTENER_DURING_SELECTION: AtomicBool = AtomicBool::new(false);

pub fn mark_manual_ctrl_c() {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
    MANUAL_CTRL_C_TIME.store(now, Ordering::SeqCst);
    // 允许剪贴板监听器在划词期间处理这次变化
    ALLOW_CLIPBOARD_LISTENER_DURING_SELECTION.store(true, Ordering::SeqCst);
}

pub fn should_allow_clipboard_listener() -> bool {
    ALLOW_CLIPBOARD_LISTENER_DURING_SELECTION.load(Ordering::SeqCst)
}

pub fn clear_manual_copy_flag() {
    ALLOW_CLIPBOARD_LISTENER_DURING_SELECTION.store(false, Ordering::SeqCst);
}

struct SelectionProcessingGuard {
    state: Arc<Mutex<SharedAppState>>,
    epoch: u64,
}

impl SelectionProcessingGuard {
    fn acquire(state: Arc<Mutex<SharedAppState>>) -> Option<Self> {
        let mut guard = lock_arc_mutex(&state);
        if !guard.settings.selection_enabled {
            return None;
        }
        if guard.is_processing_selection {
            return None;
        }
        guard.selection_guard_epoch = guard.selection_guard_epoch.wrapping_add(1);
        let epoch = guard.selection_guard_epoch;
        guard.is_selection_capture_active = true;
        guard.is_processing_selection = true;
        guard.is_updating_clipboard = false;
        drop(guard);
        Some(Self { state, epoch })
    }
}

impl Drop for SelectionProcessingGuard {
    fn drop(&mut self) {
        let mut state = lock_arc_mutex(&self.state);
        if state.selection_guard_epoch == self.epoch {
            state.is_selection_capture_active = false;
            state.is_processing_selection = false;
            state.is_updating_clipboard =
                state.is_text_writeback_active || state.is_image_writeback_active;
        }
        // 清除手动复制标志，避免影响后续操作
        clear_manual_copy_flag();
    }
}

enum ClipboardSnapshot {
    Text(String),
    Image {
        rgba: Vec<u8>,
        width: u32,
        height: u32,
    },
    Empty,
}

/// 获取选中的文本
pub fn get_selected_text_with_app(
    app_handle: &AppHandle,
    clipboard_manager: Arc<Mutex<ClipboardManager>>,
) -> Option<String> {
    get_selected_text_windows(app_handle, clipboard_manager)
}

/// Windows平台获取选中文本实现
fn get_selected_text_windows(
    app_handle: &AppHandle,
    clipboard_manager: Arc<Mutex<ClipboardManager>>,
) -> Option<String> {
    let state_manager = app_handle.state::<Arc<Mutex<SharedAppState>>>();
    let _processing_guard = SelectionProcessingGuard::acquire(state_manager.inner().clone())?;

    let start_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;

    // 1. 获取原始剪贴板内容（用于后续恢复）
    let original_snapshot = capture_clipboard_snapshot(&clipboard_manager, app_handle);
    let original_text = match &original_snapshot {
        ClipboardSnapshot::Text(text) => Some(text.clone()),
        _ => None,
    };
    let sequence_before_copy = get_clipboard_sequence_number();

    // 3. 模拟 Ctrl+C
    {
        let mut enigo_guard = lock_arc_mutex(&ENIGO_INSTANCE);
        if enigo_guard.is_none() {
            match Enigo::new(&Settings::default()) {
                Ok(enigo) => {
                    *enigo_guard = Some(enigo);
                }
                Err(e) => {
                    log::error!("未能初始化enigo: {}", e);
                    let mut state = lock_arc_mutex(state_manager.inner());
                    state.is_updating_clipboard = false;
                    return None;
                }
            }
        }
        if let Some(ref mut enigo) = *enigo_guard {
            if let Err(e) = execute_ctrl_c_with_safety(enigo) {
                log::error!("执行 Ctrl+C 失败: {}", e);
                crate::features::mouse_listener::reset_ctrl_key_state();
            }
        }
    }

    thread::sleep(INITIAL_DELAY);

    // 4. 等待剪贴板更新并获取新内容
    let new_content = wait_for_clipboard_update(
        &clipboard_manager,
        app_handle,
        &original_text,
        sequence_before_copy,
    );

    let sequence_after_copy = get_clipboard_sequence_number();
    let sequence_changed = sequence_after_copy != 0
        && sequence_before_copy != 0
        && sequence_after_copy != sequence_before_copy;

    let manual_c_time = MANUAL_CTRL_C_TIME.load(Ordering::SeqCst);
    let is_manual_copy = manual_c_time >= start_time;

    if new_content.is_none() && !sequence_changed {
        log::debug!("未捕获到新内容且剪贴板序列号未改变，无需恢复，避免覆盖非文本/图片格式");
    } else if is_manual_copy {
        log::info!("检测到手动 Ctrl+C，跳过剪贴板快照恢复，并主动记录到历史");
        // 如果 new_content 为空，但序列号变了，说明剪贴板确实有变化，再尝试读取一次
        let content_to_record = if new_content.is_none() && sequence_changed {
            log::info!("手动 Ctrl+C 导致序列号变化但未捕获内容，尝试再次读取");
            get_current_clipboard_content_with_manager(&clipboard_manager, app_handle)
        } else {
            new_content.clone()
        };
        
        if let Some(ref text) = content_to_record {
            if !text.trim().is_empty() {
                let manager = lock_arc_mutex(&clipboard_manager);
                manager.add_to_history(text.clone());
                log::info!("已记录手动 Ctrl+C 的文本到历史，长度: {}", text.len());
            }
        }
    } else {
        // 5. 恢复原始剪贴板内容
        restore_clipboard_snapshot(
            &clipboard_manager,
            app_handle,
            &original_snapshot,
            &new_content,
        );
    }

    match &new_content {
        Some(content) => {
            log::info!("成功捕获选中文本，长度: {}", content.len());
            new_content
        }
        None => {
            log::warn!("未能捕获选中文本");
            None
        }
    }
}

fn capture_clipboard_snapshot(
    clipboard_manager: &Arc<Mutex<ClipboardManager>>,
    app_handle: &AppHandle,
) -> ClipboardSnapshot {
    if let Some(text) = get_current_clipboard_content_with_manager(clipboard_manager, app_handle) {
        return ClipboardSnapshot::Text(text);
    }

    match crate::services::clipboard_access_guard::with_clipboard_access_lock(|| {
        app_handle.clipboard().read_image()
    }) {
        Ok(image) => {
            let width = image.width();
            let height = image.height();
            let rgba = image.rgba().to_vec();
            if width > 0 && height > 0 && !rgba.is_empty() {
                log::debug!("捕获到原始图片剪贴板: {}x{}", width, height);
                ClipboardSnapshot::Image {
                    rgba,
                    width,
                    height,
                }
            } else {
                ClipboardSnapshot::Empty
            }
        }
        Err(_) => {
            log::debug!("未捕获到原始文本/图片剪贴板内容，按空态处理");
            ClipboardSnapshot::Empty
        }
    }
}

/// 使用管理器获取当前剪贴板内容
fn get_current_clipboard_content_with_manager(
    clipboard_manager: &Arc<Mutex<ClipboardManager>>,
    app_handle: &AppHandle,
) -> Option<String> {
    let content = {
        let manager = lock_arc_mutex(clipboard_manager);
        manager.get_content(app_handle)
    };

    match &content {
        Some(text) => log::debug!("从剪贴板读取内容: {}", text),
        None => log::debug!("剪贴板中没有文本内容"),
    }

    content
}

/// 等待剪贴板更新
fn wait_for_clipboard_update(
    clipboard_manager: &Arc<Mutex<ClipboardManager>>,
    app_handle: &AppHandle,
    original_content: &Option<String>,
    sequence_before_copy: u32,
) -> Option<String> {
    let start_time = std::time::Instant::now();
    let mut attempts = 0;
    let wake_rx = subscribe_clipboard_wake_events();

    log::info!("使用事件优先+轮询兜底检测模式");

    while start_time.elapsed() < CAPTURE_RETRY_MAX_DURATION {
        attempts += 1;
        let _ = wake_rx.recv_timeout(CAPTURE_RETRY_INTERVAL);
        let current_sequence = get_clipboard_sequence_number();
        let sequence_changed = current_sequence != 0
            && sequence_before_copy != 0
            && current_sequence != sequence_before_copy;
        let should_read_content =
            sequence_changed || sequence_before_copy == 0 || attempts % 4 == 0;
        if !should_read_content {
            continue;
        }
        let current_content =
            get_current_clipboard_content_with_manager(clipboard_manager, app_handle);

        if let Some(ref current) = current_content {
            if let Some(ref original) = original_content {
                if current != original || sequence_changed {
                    log::info!(
                        "第{}次尝试成功捕获内容，耗时: {:?}",
                        attempts,
                        start_time.elapsed()
                    );
                    return current_content;
                }
            } else if !current.is_empty() {
                log::info!(
                    "第{}次尝试成功捕获新内容，耗时: {:?}",
                    attempts,
                    start_time.elapsed()
                );
                return current_content;
            }
        }
    }

    log::debug!(
        "重试{}次后仍未捕获到新内容，总耗时: {:?}",
        attempts,
        start_time.elapsed()
    );
    None
}

#[cfg(target_os = "windows")]
fn get_clipboard_sequence_number() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

#[cfg(not(target_os = "windows"))]
fn get_clipboard_sequence_number() -> u32 {
    0
}

fn restore_clipboard_snapshot(
    clipboard_manager: &Arc<Mutex<ClipboardManager>>,
    app_handle: &AppHandle,
    snapshot: &ClipboardSnapshot,
    captured_content: &Option<String>,
) {
    let current_content = get_current_clipboard_content_with_manager(clipboard_manager, app_handle);

    let still_holds_captured_text = captured_content
        .as_ref()
        .zip(current_content.as_ref())
        .is_some_and(|(captured, current)| current == captured);

    if captured_content.is_none() {
        log::debug!("未捕获到有效文本内容，直接恢复原始剪贴板");
        // 即使未捕获到有效文本，如果是因为复制了文件或图片导致剪贴板改变，也应恢复
    } else if !still_holds_captured_text {
        log::info!("检测到剪贴板在捕获后被用户更改，已放弃恢复原始内容以避免覆盖用户操作");
        return;
    }

    match snapshot {
        ClipboardSnapshot::Text(original_content) => {
            let result = {
                let manager = lock_arc_mutex(clipboard_manager);
                manager.set_clipboard_content(app_handle, original_content)
            };
            match result {
                Ok(()) => log::debug!("已恢复原始文本剪贴板内容"),
                Err(e) => log::error!("恢复文本剪贴板内容失败: {}", e),
            }
        }
        ClipboardSnapshot::Image {
            rgba,
            width,
            height,
        } => {
            let image = Image::new_owned(rgba.clone(), *width, *height);
            let result =
                crate::services::clipboard_access_guard::with_clipboard_access_lock(|| {
                    app_handle.clipboard().write_image(&image)
                });
            match result {
                Ok(()) => log::debug!("已恢复原始图片剪贴板内容"),
                Err(e) => log::error!("恢复图片剪贴板内容失败: {}", e),
            }
        }
        ClipboardSnapshot::Empty => {
            let result = {
                let manager = lock_arc_mutex(clipboard_manager);
                manager.set_clipboard_content(app_handle, "")
            };
            match result {
                Ok(()) => log::debug!("已按空态恢复剪贴板内容"),
                Err(e) => log::error!("恢复空态剪贴板内容失败: {}", e),
            }
        }
    }
}
