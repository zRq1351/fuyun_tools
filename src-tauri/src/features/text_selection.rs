use crate::services::clipboard_wakeup::subscribe_clipboard_wake_events;
use crate::sync::{lock_arc_mutex, Mutex};
use crate::utils::clipboard::ClipboardManager;
use log;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tauri::AppHandle;

#[cfg(target_os = "windows")]
fn send_safe_ctrl_c() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_CONTROL;

    unsafe {
        let ctrl_was_pressed = crate::features::mouse_listener::is_ctrl_pressed_by_os();
        let mut inputs = std::mem::zeroed::<[INPUT; 4]>();
        let mut count = 0;

        if !ctrl_was_pressed {
            inputs[count] = INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_CONTROL,
                        wScan: 0,
                        dwFlags: KEYBD_EVENT_FLAGS(0),
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };
            count += 1;
        }

        let vk_insert = windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(0x2D);
        inputs[count] = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk_insert,
                    wScan: 0,
                    dwFlags: KEYBD_EVENT_FLAGS(0),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        count += 1;
        inputs[count] = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk_insert,
                    wScan: 0,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        count += 1;

        if !ctrl_was_pressed {
            inputs[count] = INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_CONTROL,
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };
            count += 1;
        }

        let result = SendInput(&inputs[..count], std::mem::size_of::<INPUT>() as i32);
        if result == 0 {
            log::error!("SendInput 失败");
            return false;
        }

        // If the user is physically holding Ctrl but we didn't press it,
        // the copy might not have worked. But we don't corrupt their state.
        log::info!("已通过 SendInput 发送 Ctrl+C (ctrl_was_pressed: {})", ctrl_was_pressed);
        true
    }
}

#[cfg(not(target_os = "windows"))]
fn send_safe_ctrl_c() -> bool {
    false
}

/// 划词捕获最大重试时长
const CAPTURE_RETRY_MAX_DURATION: Duration = Duration::from_millis(600);
/// 轮询间隔，使用序列号检测时可以更频繁
const CAPTURE_RETRY_INTERVAL: Duration = Duration::from_millis(10);
/// 模拟按键后的初始等待时间
const INITIAL_DELAY: Duration = Duration::from_millis(10);

use crate::core::app_state::AppState as SharedAppState;
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

    // 2. 短暂等待用户发起手动 Ctrl+C，避免后续模拟干扰用户按键
    let deadline = std::time::Instant::now() + Duration::from_millis(200);
    let mut user_copying = false;
    while std::time::Instant::now() < deadline {
        if crate::features::mouse_listener::is_ctrl_pressed_by_os()
            || crate::features::mouse_listener::is_any_ctrl_pressed() {
            user_copying = true;
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }

    // 3. 发送 Ctrl+C（如果用户正在按 Ctrl 则跳过，避免干扰用户按键）
    if user_copying {
        log::info!("检测到用户正在按 Ctrl 键，跳过模拟 Ctrl+C，等待用户手动复制");
    } else {
        send_safe_ctrl_c();
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
