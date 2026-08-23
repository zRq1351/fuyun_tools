use crate::services::clipboard_wakeup::subscribe_clipboard_wake_events;
use crate::sync::{lock_arc_mutex, Mutex};
use crate::utils::clipboard::ClipboardManager;
use log;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tauri::AppHandle;

#[cfg(target_os = "windows")]
fn send_copy_combination(vk: u16, with_shift: bool) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
        VIRTUAL_KEY,
    };

    unsafe {
        let ctrl_was_pressed = crate::features::mouse_listener::is_ctrl_pressed_by_os();
        let mut inputs: Vec<INPUT> = Vec::with_capacity(8);

        // 终端场景需要 Shift+Ctrl+C（VK_SHIFT=0x10），其余保持 Ctrl 组合
        if with_shift {
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0x10),
                        wScan: 0,
                        dwFlags: KEYBD_EVENT_FLAGS(0),
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
        }
        if !ctrl_was_pressed {
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0x11),
                        wScan: 0,
                        dwFlags: KEYBD_EVENT_FLAGS(0),
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
        }

        let vkey = VIRTUAL_KEY(vk);
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vkey,
                    wScan: 0,
                    dwFlags: KEYBD_EVENT_FLAGS(0),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        });
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vkey,
                    wScan: 0,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        });

        if !ctrl_was_pressed {
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0x11),
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
        }
        if with_shift {
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0x10),
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
        }

        let result = SendInput(&inputs, std::mem::size_of::<INPUT>() as _);
        if result == 0 {
            log::error!("SendInput 失败");
            return false;
        }

        // If the user is physically holding Ctrl but we didn't press it,
        // the copy might not have worked. But we don't corrupt their state.
        let combo = match (vk, with_shift) {
            (0x43, false) => "Ctrl+C".to_string(),
            (0x43, true) => "Ctrl+Shift+C".to_string(),
            _ => "Ctrl+Insert".to_string(),
        };
        log::debug!(
            "已通过 SendInput 发送 {} (ctrl_was_pressed: {})",
            combo,
            ctrl_was_pressed
        );
        true
    }
}

#[cfg(not(target_os = "windows"))]
fn send_copy_combination(_vk: u16, _with_shift: bool) -> bool {
    false
}

/// 终端类应用名单：其 Ctrl+C 是"中断/中止"语义而不是复制，
/// 模拟复制键必须改用 Ctrl+Shift+C（Windows Terminal / conhost 的复制快捷键）
#[cfg(target_os = "windows")]
const TERMINAL_APP_EXES: &[&str] = &[
    "windowsterminal.exe",
    "windowsterminalpreview.exe",
    "openconsole.exe",
    "conhost.exe",
    "mintty.exe",
    "mintty-2.exe",
    "wezterm-gui.exe",
    "alacritty.exe",
    "kitty.exe",
    "hyper.exe",
    "tabby.exe",
    "fluentterminal.app.exe",
    "conemu64.exe",
    "conemu.exe",
];

/// 判断前台窗口是否属于终端类应用
#[cfg(target_os = "windows")]
fn is_terminal_foreground_window() -> bool {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return false;
    }
    let mut pid: u32 = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    if pid == 0 {
        return false;
    }
    let handle = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
        Ok(h) => h,
        Err(_) => return false,
    };
    let mut buffer = vec![0u16; 1024];
    let mut size = buffer.len() as u32;
    let ok = unsafe {
        QueryFullProcessImageNameW(
            handle,
            windows::Win32::System::Threading::PROCESS_NAME_FORMAT(0),
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        )
    };
    unsafe {
        let _ = CloseHandle(handle);
    }
    if ok.is_err() || size == 0 {
        return false;
    }
    let full_path = String::from_utf16_lossy(&buffer[..size as usize]);
    let exe_name = std::path::Path::new(&full_path)
        .file_name()
        .map(|f| f.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    TERMINAL_APP_EXES.contains(&exe_name.as_str())
}

#[cfg(not(target_os = "windows"))]
fn is_terminal_foreground_window() -> bool {
    false
}

/// 划词捕获最大重试时长
const CAPTURE_RETRY_MAX_DURATION: Duration = Duration::from_millis(600);
/// 轮询间隔，使用序列号检测时可以更频繁
const CAPTURE_RETRY_INTERVAL: Duration = Duration::from_millis(10);

use crate::core::app_state::AppState as SharedAppState;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use crate::utils::utils_helpers::now_unix_ms_u64;
use tauri::image::Image;
use tauri::Manager;
use tauri_plugin_clipboard_manager::ClipboardExt;
#[cfg(target_os = "windows")]
use winapi::um::winuser::GetClipboardSequenceNumber;

static MANUAL_CTRL_C_TIME: AtomicU64 = AtomicU64::new(0);
static ALLOW_CLIPBOARD_LISTENER_DURING_SELECTION: AtomicBool = AtomicBool::new(false);

pub fn mark_manual_ctrl_c() {
    let now = now_unix_ms_u64();
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
    // 完全排除终端窗口：终端划词体验差（Ctrl+C 是中断、右键复制会破坏选区），
    // 直接不进入复制/剪贴板捕获流程，避免打扰用户
    if is_terminal_foreground_window() {
        log::debug!("前台窗口为终端类应用，跳过划词文本捕获");
        return None;
    }
    let state_manager = app_handle.state::<Arc<Mutex<SharedAppState>>>();
    let _processing_guard = SelectionProcessingGuard::acquire(state_manager.inner().clone())?;

    let start_time = now_unix_ms_u64();

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

    // 3. 依次尝试多种复制按键组合，适配不同应用
    //    Ctrl+Insert 是 Windows 传统复制加速键，终端（conhost/Windows Terminal）支持；
    //    在多数 GUI 应用中多为无害空操作。Ctrl+C 覆盖浏览器/Office 等，
    //    但终端中可能是中断信号，故放在 Ctrl+Insert 之后兜底
    const VK_C: u16 = 0x43;
    const VK_INSERT: u16 = 0x2D;
    let mut new_content: Option<String> = None;
    if user_copying {
        log::debug!("检测到用户正在按 Ctrl 键，跳过模拟复制，等待用户手动复制");
        new_content = wait_for_clipboard_update(
            &clipboard_manager,
            app_handle,
            &original_text,
            sequence_before_copy,
        );
    } else {
        // 终端类应用 Ctrl+C 是"中断"语义（会误发中断/中止信号），必须改用 Ctrl+Shift+C；
        // 其余应用保持 Ctrl+Insert → Ctrl+C 的原有组合
        let is_terminal = is_terminal_foreground_window();
        let combos: &[(u16, bool)] = if is_terminal {
            &[(VK_C, true), (VK_INSERT, false)]
        } else {
            &[(VK_INSERT, false), (VK_C, false)]
        };
        for (vk, with_shift) in combos {
            if !send_copy_combination(*vk, *with_shift) {
                continue;
            }
            let combo_name = match (*vk, *with_shift) {
                (VK_C, true) => "Ctrl+Shift+C",
                (VK_C, false) => "Ctrl+C",
                _ => "Ctrl+Insert",
            };
            // 短等待检测剪贴板序列号变化，命中则进入完整等待读取
            let short_deadline = std::time::Instant::now() + Duration::from_millis(150);
            let mut seq_changed = false;
            while std::time::Instant::now() < short_deadline {
                let current_sequence = get_clipboard_sequence_number();
                if current_sequence != 0
                    && sequence_before_copy != 0
                    && current_sequence != sequence_before_copy
                {
                    seq_changed = true;
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
            if !seq_changed {
                log::debug!("{} 未触发剪贴板变化，尝试下一组合", combo_name);
                continue;
            }
            new_content = wait_for_clipboard_update(
                &clipboard_manager,
                app_handle,
                &original_text,
                sequence_before_copy,
            );
            if new_content.is_some() {
                break;
            }
        }
    }

    let sequence_after_copy = get_clipboard_sequence_number();
    let sequence_changed = sequence_after_copy != 0
        && sequence_before_copy != 0
        && sequence_after_copy != sequence_before_copy;

    let manual_c_time = MANUAL_CTRL_C_TIME.load(Ordering::SeqCst);
    let is_manual_copy = manual_c_time >= start_time;

    if new_content.is_none() && !sequence_changed {
        log::debug!("未捕获到新内容且剪贴板序列号未改变，无需恢复，避免覆盖非文本/图片格式");
    } else if is_manual_copy {
        log::debug!("检测到手动 Ctrl+C，跳过剪贴板快照恢复，并主动记录到历史");
        // 如果 new_content 为空，但序列号变了，说明剪贴板确实有变化，再尝试读取一次
        let content_to_record = if new_content.is_none() && sequence_changed {
            log::debug!("手动 Ctrl+C 导致序列号变化但未捕获内容，尝试再次读取");
            get_current_clipboard_content_with_manager(&clipboard_manager, app_handle)
        } else {
            new_content.clone()
        };
        
        if let Some(ref text) = content_to_record {
            if !text.trim().is_empty() {
                let manager = lock_arc_mutex(&clipboard_manager);
                manager.add_to_history(text.clone());
                log::debug!("已记录手动 Ctrl+C 的文本到历史，长度: {}", text.len());
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
            log::debug!("成功捕获选中文本，长度: {}", content.len());
            new_content
        }
        None => {
            log::debug!("未能捕获选中文本");
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
    let mut consecutive_unchanged = 0u32; // 连续未变化计数，用于提前退出
    let wake_rx = subscribe_clipboard_wake_events();

    log::debug!("使用事件优先+轮询兜底检测模式");

    while start_time.elapsed() < CAPTURE_RETRY_MAX_DURATION {
        attempts += 1;
        let _ = wake_rx.recv_timeout(CAPTURE_RETRY_INTERVAL);
        let current_sequence = get_clipboard_sequence_number();
        let sequence_changed = current_sequence != 0
            && sequence_before_copy != 0
            && current_sequence != sequence_before_copy;

        // 如果序列号持续未变化且已超过初始等待期，提前退出
        if !sequence_changed && attempts > 5 {
            consecutive_unchanged += 1;
            // 连续30次未变化（约300ms），且未检测到任何剪贴板活动，提前退出
            if consecutive_unchanged >= 30 && original_content.is_some() {
                log::debug!(
                    "连续{}次序列号未变化，提前退出检测，耗时: {:?}",
                    consecutive_unchanged,
                    start_time.elapsed()
                );
                return None;
            }
        } else if sequence_changed {
            consecutive_unchanged = 0;
        }

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
                    log::debug!(
                        "第{}次尝试成功捕获内容，耗时: {:?}",
                        attempts,
                        start_time.elapsed()
                    );
                    return current_content;
                }
            } else if !current.is_empty() {
                log::debug!(
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

/// 恢复剪贴板时的重试等待（毫秒）：其他进程可能短暂占用剪贴板导致写入失败
const CLIPBOARD_RESTORE_RETRY_DELAYS_MS: &[u64] = &[30, 80];

/// 带重试的剪贴板恢复操作：最多尝试 1 + 重试次数 次，全部失败才返回错误
fn restore_clipboard_with_retry<T, E: std::fmt::Display>(
    desc: &str,
    mut op: impl FnMut() -> Result<T, E>,
) -> Result<T, E> {
    let mut attempt = 0usize;
    loop {
        match op() {
            Ok(value) => return Ok(value),
            Err(e) if attempt < CLIPBOARD_RESTORE_RETRY_DELAYS_MS.len() => {
                log::warn!("{}第{}次失败，将重试: {}", desc, attempt + 1, e);
                thread::sleep(Duration::from_millis(CLIPBOARD_RESTORE_RETRY_DELAYS_MS[attempt]));
                attempt += 1;
            }
            Err(e) => {
                log::error!("{}重试 {} 次后仍失败: {}", desc, attempt, e);
                return Err(e);
            }
        }
    }
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
        log::debug!("检测到剪贴板在捕获后被用户更改，已放弃恢复原始内容以避免覆盖用户操作");
        return;
    }

    match snapshot {
        ClipboardSnapshot::Text(original_content) => {
            let result = restore_clipboard_with_retry("恢复文本剪贴板内容", || {
                let manager = lock_arc_mutex(clipboard_manager);
                manager.set_clipboard_content(app_handle, original_content)
            });
            if let Err(e) = result {
                log::error!("恢复文本剪贴板内容最终失败: {}", e);
            } else {
                log::debug!("已恢复原始文本剪贴板内容");
            }
        }
        ClipboardSnapshot::Image {
            rgba,
            width,
            height,
        } => {
            let image = Image::new_owned(rgba.clone(), *width, *height);
            let result = restore_clipboard_with_retry("恢复图片剪贴板内容", || {
                crate::services::clipboard_access_guard::with_clipboard_access_lock(|| {
                    app_handle.clipboard().write_image(&image)
                })
            });
            if let Err(e) = result {
                log::error!("恢复图片剪贴板内容最终失败: {}", e);
            } else {
                log::debug!("已恢复原始图片剪贴板内容");
            }
        }
        ClipboardSnapshot::Empty => {
            // 空快照：若剪贴板仍是我们捕获的选中文本，清空以恢复空态
            if still_holds_captured_text {
                let result = restore_clipboard_with_retry("清空剪贴板（恢复空态）", || {
                    crate::services::clipboard_access_guard::with_clipboard_access_lock(|| {
                        app_handle.clipboard().write_text("")
                    })
                });
                match result {
                    Ok(()) => log::debug!("已清空剪贴板，恢复空态"),
                    Err(e) => log::warn!("清空剪贴板失败（恢复空态）: {}", e),
                }
                return;
            }
            // 当前剪贴板有内容且不是我们捕获的文本（captured_content为None），
            // 说明用户在捕获期间手动修改了剪贴板，不应覆盖
            log::debug!("检测到空快照但剪贴板已有用户内容，跳过恢复以避免覆盖");
        }
    }
}
