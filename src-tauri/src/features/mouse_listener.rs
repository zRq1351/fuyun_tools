use crate::features::screenshot::capture;
use crate::sync::{lock_arc_mutex, Mutex};
use log;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, LazyLock, Mutex as StdMutex, OnceLock};
use std::thread;
use std::time::Duration;
use tauri::AppHandle;

use crate::core::app_state::AppState as SharedAppState;
use crate::ui::window_manager::{
    handle_selection_toolbar_autoclose, hide_selection_toolbar_impl, show_selection_toolbar_impl,
};
use crate::utils::clipboard::ClipboardManager;
#[cfg(target_os = "windows")]
use winapi::shared::minwindef::{LPARAM, LRESULT, WPARAM};
#[cfg(target_os = "windows")]
use winapi::um::libloaderapi::GetModuleHandleW;
#[cfg(target_os = "windows")]
use winapi::um::winuser::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, HC_ACTION, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT,
    WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_USER,
};
#[cfg(target_os = "windows")]
use winapi::um::winuser::{GetAsyncKeyState, VK_LCONTROL, VK_LMENU, VK_RCONTROL, VK_RMENU};
#[cfg(target_os = "windows")]
use winapi::um::winuser::{GetCursorInfo, CURSORINFO};
#[cfg(target_os = "windows")]
use winapi::um::winuser::{LoadCursorW, IDC_IBEAM};

#[derive(Debug, Clone, PartialEq)]
enum MouseActionState {
    Idle,
    MouseDown(i32, i32, std::time::Instant),
    Dragging(i32, i32, std::time::Instant, bool, Vec<(i32, i32)>), // (start_x, start_y, time, has_seen_ibeam, positions)
    MouseUp(i32, i32, std::time::Instant),
}

struct GlobalState {
    mouse_action_state: Arc<Mutex<MouseActionState>>,
    ctrl_left_pressed: AtomicBool,
    ctrl_right_pressed: AtomicBool,
    needs_detection: AtomicBool,
    last_processed_time: Arc<Mutex<std::time::Instant>>,
    last_mouse_pos: Arc<Mutex<(i32, i32)>>,
    detection_anchor_pos: Arc<Mutex<(i32, i32)>>,
    last_toolbar_emit: Arc<Mutex<Option<(String, (i32, i32), std::time::Instant)>>>,
    last_click: Arc<Mutex<Option<(i32, i32, std::time::Instant)>>>,
    detection_notify: Arc<(std::sync::Mutex<bool>, Condvar)>,
}

static GLOBAL_STATE: LazyLock<GlobalState> = LazyLock::new(|| GlobalState {
    mouse_action_state: Arc::new(Mutex::new(MouseActionState::Idle)),
    ctrl_left_pressed: AtomicBool::new(false),
    ctrl_right_pressed: AtomicBool::new(false),
    needs_detection: AtomicBool::new(false),
    last_processed_time: Arc::new(Mutex::new(std::time::Instant::now())),
    last_mouse_pos: Arc::new(Mutex::new((0, 0))),
    detection_anchor_pos: Arc::new(Mutex::new((0, 0))),
    last_toolbar_emit: Arc::new(Mutex::new(None)),
    last_click: Arc::new(Mutex::new(None)),
    detection_notify: Arc::new((std::sync::Mutex::new(false), Condvar::new())),
});

static LISTENER_STARTED: AtomicBool = AtomicBool::new(false);
static LISTENER_ENABLED: AtomicBool = AtomicBool::new(true);
static INPUT_SOURCE_RUNNING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy)]
enum HookEvent {
    CtrlLeftPress,
    CtrlRightPress,
    CtrlLeftRelease,
    CtrlRightRelease,
    CPress,
    LeftButtonPress(i32, i32),
    LeftButtonRelease(i32, i32),
    MouseMove(i32, i32),
}

fn hook_event_sender() -> &'static StdMutex<Option<Sender<HookEvent>>> {
    static HOOK_EVENT_SENDER: OnceLock<StdMutex<Option<Sender<HookEvent>>>> = OnceLock::new();
    HOOK_EVENT_SENDER.get_or_init(|| StdMutex::new(None))
}

#[cfg(target_os = "windows")]
static HOOK_THREAD_ID: AtomicU32 = AtomicU32::new(0);

fn notify_detection_pending() {
    let (lock, cvar) = &*GLOBAL_STATE.detection_notify;
    if let Ok(mut pending) = lock.lock() {
        *pending = true;
        cvar.notify_one();
    }
}

fn handle_hook_event(
    event: HookEvent,
    listener_state: &Arc<Mutex<SharedAppState>>,
    listener_app_handle: &AppHandle,
) {
    if !LISTENER_ENABLED.load(Ordering::SeqCst) {
        return;
    }
    match event {
        HookEvent::CtrlLeftPress => {
            GLOBAL_STATE.ctrl_left_pressed.store(true, Ordering::SeqCst);
            log::debug!("检测到左Ctrl键按下");
        }
        HookEvent::CtrlRightPress => {
            GLOBAL_STATE
                .ctrl_right_pressed
                .store(true, Ordering::SeqCst);
            log::debug!("检测到右Ctrl键按下");
        }
        HookEvent::CtrlLeftRelease => {
            GLOBAL_STATE
                .ctrl_left_pressed
                .store(false, Ordering::SeqCst);
            log::debug!("检测到左Ctrl键释放");
        }
        HookEvent::CtrlRightRelease => {
            GLOBAL_STATE
                .ctrl_right_pressed
                .store(false, Ordering::SeqCst);
            log::debug!("检测到右Ctrl键释放");
        }
        HookEvent::CPress => {
            if is_any_ctrl_pressed() {
                crate::features::text_selection::mark_manual_ctrl_c();
                log::debug!("检测到手动 Ctrl+C");
            }
        }
        HookEvent::LeftButtonPress(last_x, last_y) => {
            let current_time = std::time::Instant::now();
            if let Ok(mut pos_guard) = GLOBAL_STATE.last_mouse_pos.try_lock() {
                *pos_guard = (last_x, last_y);
            }
            handle_selection_toolbar_autoclose(listener_app_handle, Some((last_x, last_y)));
            log::debug!("检测到鼠标左键按下 at ({}, {})", last_x, last_y);
            
            let mut state_guard = lock_arc_mutex(&GLOBAL_STATE.mouse_action_state);
            *state_guard = MouseActionState::MouseDown(last_x, last_y, current_time);
        }
        HookEvent::LeftButtonRelease(last_x, last_y) => {
            let current_time = std::time::Instant::now();
            if let Ok(mut pos_guard) = GLOBAL_STATE.last_mouse_pos.try_lock() {
                *pos_guard = (last_x, last_y);
            }
            log::debug!("检测到鼠标左键释放 at ({}, {})", last_x, last_y);
            let mut state_guard = lock_arc_mutex(&GLOBAL_STATE.mouse_action_state);
            let prev_state = std::mem::replace(&mut *state_guard, MouseActionState::Idle);
            
            // 处理从 MouseDown 或 Dragging 状态转换
            let (down_x, down_y, down_time, has_seen_ibeam, positions) = match prev_state {
                MouseActionState::MouseDown(x, y, t) => {
                    // 从 MouseDown 直接到 MouseUp，检查当前光标
                    let is_ibeam = is_cursor_ibeam();
                    (x, y, t, is_ibeam, vec![(x, y), (last_x, last_y)])
                }
                MouseActionState::Dragging(x, y, t, seen_ibeam, pos) => {
                    // 已经在拖拽过程中，使用记录的标志和轨迹
                    (x, y, t, seen_ibeam, pos)
                }
                _ => return,
            };
            
            let up_time = current_time;
            *state_guard = MouseActionState::MouseUp(last_x, last_y, up_time);
            let distance = calculate_distance(down_x, down_y, last_x, last_y);
            let duration = up_time.duration_since(down_time);
            log::debug!(
                "鼠标移动距离: {:.2}px, 操作持续时间: {:?}ms",
                distance,
                duration.as_millis()
            );
            let is_drag = is_valid_drag_operation(distance, duration);
            let is_double_click = if !is_drag {
                let mut last_click_guard = lock_arc_mutex(&GLOBAL_STATE.last_click);
                let result = if let Some((lx, ly, ltime)) = *last_click_guard {
                    let click_dist = calculate_distance(lx, ly, last_x, last_y);
                    let click_interval = up_time.duration_since(ltime);
                    click_dist < 5.0 && click_interval.as_millis() < 500
                } else {
                    false
                };
                *last_click_guard = Some((last_x, last_y, up_time));
                result
            } else {
                *lock_arc_mutex(&GLOBAL_STATE.last_click) = None;
                false
            };
            
            // 提前获取 modifier_key 用于智能判断
            let modifier_key = {
                let state_guard = lock_arc_mutex(listener_state);
                state_guard.settings.selection_modifier_key.clone()
            };
                            
            // 智能判断是否为划词操作
            let is_likely_text_selection = if modifier_key.is_empty() {
                if is_drag {
                    // 无修饰键的拖拽，需要进一步判断
                    if has_seen_ibeam {
                        // 在开始/滑动中/结束任何一处见过 IBEAM，直接判定为划词
                        log::debug!("拖拽过程中检测到文本输入型光标，直接判定为划词");
                        true
                    } else {
                        // 完全没见过 IBEAM，根据轨迹特征判断
                        let feature_linear = check_linear_movement(&positions);
                        
                        let feature_horizontal = {
                            if let (Some(first), Some(last)) = (positions.first(), positions.last()) {
                                let dx = (last.0 - first.0).abs() as f64;
                                let dy = (last.1 - first.1).abs() as f64;
                                dx > dy * 0.3 && dx > 10.0
                            } else {
                                false
                            }
                        };
                        
                        let feature_speed = {
                            let total_distance = distance;
                            let duration_ms = duration.as_millis() as f64;
                            if duration_ms > 0.0 {
                                let speed = total_distance / duration_ms;
                                speed >= 0.2 && speed <= 10.0
                            } else {
                                false
                            }
                        };
                        
                        // 综合评分：至少满足2个特征才认为是划词
                        let score = [
                            feature_linear,
                            feature_horizontal,
                            feature_speed
                        ].iter().filter(|&&x| x).count();
                        
                        if score >= 2 {
                            log::debug!("拖拽操作通过综合判断 (得分: {}/3)，判定为划词", score);
                            true
                        } else {
                            log::debug!("拖拽操作未通过综合判断 (得分: {}/3)，可能是窗口/滚动操作", score);
                            false
                        }
                    }
                } else if is_double_click {
                    // 无修饰键的双击，必须要求光标是文本输入型
                    let current_is_ibeam = is_cursor_ibeam();
                    if !current_is_ibeam {
                        log::debug!("无修饰键双击时，光标不是文本输入型，判定为非划词操作");
                    }
                    current_is_ibeam
                } else {
                    false
                }
            } else {
                // 有修饰键，信任用户意图
                true
            };
                            
            if is_drag || is_double_click {
                if is_double_click {
                    log::info!("检测到双击/三击操作");
                }

                let is_alt = is_alt_pressed_by_os();
                let is_ctrl = is_ctrl_effectively_pressed();

                let modifier_matched = match modifier_key.as_str() {
                    "Alt" => is_alt,
                    "Ctrl" => is_ctrl,
                    _ => !is_ctrl,
                };

                // 无修饰键模式下，使用智能判断结果
                let ibeam_check_passed = if modifier_key.is_empty() {
                    is_likely_text_selection
                } else {
                    true // 有修饰键时不检查
                };

                if modifier_matched && ibeam_check_passed {
                    if capture::is_screenshot_in_progress() {
                        return;
                    }
                    let app_busy_or_visible = {
                        let state_guard = lock_arc_mutex(listener_state);
                        state_guard.is_visible
                            || state_guard.is_image_visible
                            || state_guard.is_processing_selection
                            || state_guard.is_updating_clipboard
                    };
                    if app_busy_or_visible {
                        log::info!("当前应用窗口可见或正在处理回填，跳过划词检测触发");
                        return;
                    }
                    let last_processed =
                        { *lock_arc_mutex(&GLOBAL_STATE.last_processed_time) };
                    if up_time.duration_since(last_processed) > Duration::from_millis(100) {
                        {
                            let mut pos_guard =
                                lock_arc_mutex(&GLOBAL_STATE.detection_anchor_pos);
                            *pos_guard = (last_x, last_y);
                        }
                        GLOBAL_STATE.needs_detection.store(true, Ordering::SeqCst);
                        notify_detection_pending();
                        log::info!("设置划词检测标志");
                        *lock_arc_mutex(&GLOBAL_STATE.last_processed_time) = up_time;
                    } else {
                        log::info!("操作过于频繁，跳过此次检测");
                    }
                } else {
                    log::info!("辅助键条件不满足或未见文本光标，忽略此次点击");
                }
            } else {
                log::debug!("不满足划词或双击条件，跳过");
            }
        }
        HookEvent::MouseMove(mouse_x, mouse_y) => {
            if let Ok(mut pos_guard) = GLOBAL_STATE.last_mouse_pos.try_lock() {
                *pos_guard = (mouse_x, mouse_y);
            }
            
            // 在拖拽过程中监测光标类型和轨迹
            let mut state_guard = lock_arc_mutex(&GLOBAL_STATE.mouse_action_state);
            match *state_guard {
                MouseActionState::MouseDown(start_x, start_y, start_time) => {
                    // 检查是否开始拖拽（移动距离超过阈值）
                    let distance = calculate_distance(start_x, start_y, mouse_x, mouse_y);
                    if distance >= 5.0 {
                        // 转换为 Dragging 状态
                        let is_ibeam = is_cursor_ibeam();
                        *state_guard = MouseActionState::Dragging(
                            start_x, start_y, start_time, is_ibeam, 
                            vec![(start_x, start_y), (mouse_x, mouse_y)]
                        );
                        if is_ibeam {
                            log::debug!("开始拖拽并检测到文本输入型光标");
                        }
                    }
                }
                MouseActionState::Dragging(start_x, start_y, start_time, has_seen_ibeam, ref positions) => {
                    // 已经在拖拽过程中，持续监测
                    let mut new_positions = positions.clone();
                    new_positions.push((mouse_x, mouse_y));
                    
                    // 限制轨迹长度，只保留最近20个点
                    if new_positions.len() > 20 {
                        new_positions.drain(0..new_positions.len() - 20);
                    }
                    
                    if !has_seen_ibeam {
                        let is_ibeam_now = is_cursor_ibeam();
                        if is_ibeam_now {
                            log::debug!("拖拽过程中检测到文本输入型光标");
                            *state_guard = MouseActionState::Dragging(
                                start_x, start_y, start_time, true, new_positions
                            );
                        } else {
                            *state_guard = MouseActionState::Dragging(
                                start_x, start_y, start_time, false, new_positions
                            );
                        }
                    } else {
                        *state_guard = MouseActionState::Dragging(
                            start_x, start_y, start_time, true, new_positions
                        );
                    }
                }
                _ => {}
            }
        }
    }
}

fn wait_detection_pending(timeout: Duration) {
    let (lock, cvar) = &*GLOBAL_STATE.detection_notify;
    if let Ok(mut pending) = lock.lock() {
        if !*pending {
            let wait_result = cvar.wait_timeout(pending, timeout);
            if let Ok((guard, _)) = wait_result {
                pending = guard;
            } else {
                return;
            }
        }
        *pending = false;
    }
}

fn start_input_listener_source(app_handle: AppHandle, state: Arc<Mutex<SharedAppState>>) {
    #[cfg(target_os = "windows")]
    {
        start_windows_hook_listener(app_handle, state);
        return;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app_handle;
        let _ = state;
    }
}

fn stop_input_listener_source() {
    #[cfg(target_os = "windows")]
    {
        stop_windows_hook_listener();
    }
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn low_level_keyboard_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code == HC_ACTION {
        let keyboard = &*(lparam as *const KBDLLHOOKSTRUCT);
        // 忽略注入的按键事件（如 Enigo 模拟的 Ctrl+C），只响应真实的物理按键
        let is_injected = (keyboard.flags & 0x10) != 0;
        
        if !is_injected {
            let event = match wparam as u32 {
                WM_KEYDOWN | WM_SYSKEYDOWN => {
                    if keyboard.vkCode == VK_LCONTROL as u32 {
                        Some(HookEvent::CtrlLeftPress)
                    } else if keyboard.vkCode == VK_RCONTROL as u32 {
                        Some(HookEvent::CtrlRightPress)
                    } else if keyboard.vkCode == 0x43 { // 'C' key
                        Some(HookEvent::CPress)
                    } else {
                        None
                    }
                }
                WM_KEYUP | WM_SYSKEYUP => {
                    if keyboard.vkCode == VK_LCONTROL as u32 {
                        Some(HookEvent::CtrlLeftRelease)
                    } else if keyboard.vkCode == VK_RCONTROL as u32 {
                        Some(HookEvent::CtrlRightRelease)
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some(event) = event {
                if let Ok(guard) = hook_event_sender().lock() {
                    if let Some(tx) = guard.as_ref() {
                        let _ = tx.send(event);
                        let thread_id = HOOK_THREAD_ID.load(Ordering::SeqCst);
                        if thread_id != 0 {
                            PostThreadMessageW(thread_id, WM_USER, 0, 0);
                        }
                    }
                }
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn low_level_mouse_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code == HC_ACTION {
        let mouse = &*(lparam as *const MSLLHOOKSTRUCT);
        // 忽略注入的鼠标事件
        let is_injected = (mouse.flags & 0x01) != 0;
        
        if !is_injected {
            let x = mouse.pt.x;
            let y = mouse.pt.y;
            let event = match wparam as u32 {
                WM_LBUTTONDOWN => Some(HookEvent::LeftButtonPress(x, y)),
                WM_LBUTTONUP => Some(HookEvent::LeftButtonRelease(x, y)),
                WM_MOUSEMOVE => Some(HookEvent::MouseMove(x, y)),
                _ => None,
            };
            if let Some(event) = event {
                if let Ok(guard) = hook_event_sender().lock() {
                    if let Some(tx) = guard.as_ref() {
                        let _ = tx.send(event);
                        let thread_id = HOOK_THREAD_ID.load(Ordering::SeqCst);
                        if thread_id != 0 {
                            PostThreadMessageW(thread_id, WM_USER, 0, 0);
                        }
                    }
                }
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

#[cfg(target_os = "windows")]
fn start_windows_hook_listener(app_handle: AppHandle, state: Arc<Mutex<SharedAppState>>) {
    if INPUT_SOURCE_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    thread::spawn(move || unsafe {
        let thread_id = winapi::um::processthreadsapi::GetCurrentThreadId();
        HOOK_THREAD_ID.store(thread_id, Ordering::SeqCst);
        let (tx, rx): (Sender<HookEvent>, Receiver<HookEvent>) = mpsc::channel();
        if let Ok(mut guard) = hook_event_sender().lock() {
            *guard = Some(tx);
        }

        let module = GetModuleHandleW(std::ptr::null());
        let keyboard_hook =
            SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_keyboard_proc), module, 0);
        let mouse_hook = SetWindowsHookExW(WH_MOUSE_LL, Some(low_level_mouse_proc), module, 0);

        if keyboard_hook.is_null() || mouse_hook.is_null() {
            if !keyboard_hook.is_null() {
                let _ = UnhookWindowsHookEx(keyboard_hook);
            }
            if !mouse_hook.is_null() {
                let _ = UnhookWindowsHookEx(mouse_hook);
            }
            if let Ok(mut guard) = hook_event_sender().lock() {
                *guard = None;
            }
            INPUT_SOURCE_RUNNING.store(false, Ordering::SeqCst);
            HOOK_THREAD_ID.store(0, Ordering::SeqCst);
            log::error!("安装划词低级键鼠 Hook 失败");
            return;
        }

        log::info!("划词低级键鼠 Hook 已启动");
        let mut msg: MSG = std::mem::zeroed();
        loop {
            let ret = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
            if ret == 0 || ret == -1 {
                break;
            }
            if msg.message == WM_QUIT {
                break;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);

            while let Ok(event) = rx.try_recv() {
                handle_hook_event(event, &state, &app_handle);
            }
        }

        let _ = UnhookWindowsHookEx(keyboard_hook);
        let _ = UnhookWindowsHookEx(mouse_hook);
        if let Ok(mut guard) = hook_event_sender().lock() {
            *guard = None;
        }
        INPUT_SOURCE_RUNNING.store(false, Ordering::SeqCst);
        HOOK_THREAD_ID.store(0, Ordering::SeqCst);
        log::info!("划词低级键鼠 Hook 已停止");
    });
}

#[cfg(target_os = "windows")]
fn stop_windows_hook_listener() {
    let thread_id = HOOK_THREAD_ID.load(Ordering::SeqCst);
    if thread_id != 0 {
        unsafe {
            let _ = PostThreadMessageW(thread_id, WM_QUIT, 0, 0);
        }
    }
}

/// 设置划词监听器启用状态
pub fn set_selection_listener_enabled(
    app_handle: AppHandle,
    state: Arc<Mutex<SharedAppState>>,
    enabled: bool,
) {
    LISTENER_ENABLED.store(enabled, Ordering::SeqCst);
    if enabled {
        MouseListener::start_mouse_listener(app_handle, state);
    } else {
        GLOBAL_STATE.needs_detection.store(false, Ordering::SeqCst);
        stop_input_listener_source();
        hide_selection_toolbar_impl(app_handle);
    }
}

/// 检查是否有Ctrl键被按下
fn is_any_ctrl_pressed() -> bool {
    GLOBAL_STATE.ctrl_left_pressed.load(Ordering::SeqCst)
        || GLOBAL_STATE.ctrl_right_pressed.load(Ordering::SeqCst)
}

#[cfg(target_os = "windows")]
fn is_ctrl_pressed_by_os() -> bool {
    unsafe {
        (GetAsyncKeyState(VK_LCONTROL) as u16 & 0x8000) != 0
            || (GetAsyncKeyState(VK_RCONTROL) as u16 & 0x8000) != 0
    }
}

#[cfg(not(target_os = "windows"))]
fn is_ctrl_pressed_by_os() -> bool {
    false
}

#[cfg(target_os = "windows")]
fn is_alt_pressed_by_os() -> bool {
    unsafe {
        (GetAsyncKeyState(VK_LMENU) as u16 & 0x8000) != 0
            || (GetAsyncKeyState(VK_RMENU) as u16 & 0x8000) != 0
    }
}

#[cfg(not(target_os = "windows"))]
fn is_alt_pressed_by_os() -> bool {
    false
}

/// 检查当前光标是否为文本输入型（IDC_IBEAM）
#[cfg(target_os = "windows")]
fn is_cursor_ibeam() -> bool {
    unsafe {
        let mut cursor_info: CURSORINFO = std::mem::zeroed();
        cursor_info.cbSize = std::mem::size_of::<CURSORINFO>() as u32;
        
        if GetCursorInfo(&mut cursor_info) == 0 {
            log::warn!("GetCursorInfo 调用失败");
            return false;
        }
        
        // 获取系统标准的 IBEAM 光标句柄
        let ibeam_cursor = LoadCursorW(std::ptr::null_mut(), IDC_IBEAM);
        if ibeam_cursor.is_null() {
            log::warn!("LoadCursorW(IDC_IBEAM) 调用失败");
            return false;
        }
        
        // 比较当前光标与 IBEAM 光标
        let is_ibeam = cursor_info.hCursor == ibeam_cursor;
        
        if !is_ibeam {
            log::debug!("当前光标不是文本输入型 (IBEAM)，hCursor={:?}, IBEAM={:?}", 
                       cursor_info.hCursor, ibeam_cursor);
        }
        
        is_ibeam
    }
}

#[cfg(not(target_os = "windows"))]
fn is_cursor_ibeam() -> bool {
    // 非 Windows 平台默认返回 true，不进行检查
    true
}

/// 检查移动轨迹是否呈线性（划词通常是直线）
fn check_linear_movement(positions: &[(i32, i32)]) -> bool {
    if positions.len() < 3 {
        // 点数太少，无法判断
        return true;
    }
    
    // 使用最小二乘法拟合直线，计算 R² 值
    let n = positions.len() as f64;
    let sum_x: f64 = positions.iter().map(|(x, _)| *x as f64).sum();
    let sum_y: f64 = positions.iter().map(|(_, y)| *y as f64).sum();
    let sum_xy: f64 = positions.iter().map(|(x, y)| (*x as f64) * (*y as f64)).sum();
    let sum_x2: f64 = positions.iter().map(|(x, _)| (*x as f64).powi(2)).sum();
    let sum_y2: f64 = positions.iter().map(|(_, y)| (*y as f64).powi(2)).sum();
    
    // 计算相关系数 r
    let numerator = n * sum_xy - sum_x * sum_y;
    let denominator = ((n * sum_x2 - sum_x.powi(2)) * (n * sum_y2 - sum_y.powi(2))).sqrt();
    
    if denominator < 1e-10 {
        return false;
    }
    
    let r = numerator / denominator;
    let r_squared = r * r;
    
    // R² > 0.9 认为是线性运动
    r_squared > 0.9
}

fn clear_ctrl_key_state_silent() {
    GLOBAL_STATE
        .ctrl_left_pressed
        .store(false, Ordering::SeqCst);
    GLOBAL_STATE
        .ctrl_right_pressed
        .store(false, Ordering::SeqCst);
}

fn is_ctrl_effectively_pressed() -> bool {
    let tracked_pressed = is_any_ctrl_pressed();
    let os_pressed = is_ctrl_pressed_by_os();
    if tracked_pressed && !os_pressed {
        clear_ctrl_key_state_silent();
        log::warn!("检测到Ctrl状态滞留，已自动纠正为释放");
        return false;
    }
    tracked_pressed || os_pressed
}

/// 重置Ctrl键状态
pub fn reset_ctrl_key_state() {
    clear_ctrl_key_state_silent();
    if let Err(e) = crate::ui::window_manager::force_release_ctrl_key() {
        log::warn!("重置Ctrl状态时物理释放失败: {}", e);
    }
    log::info!("已重置Ctrl键状态");
}

/// 跨平台鼠标监听器
pub struct MouseListener;

impl MouseListener {
    /// 启动鼠标监听器
    pub fn start_mouse_listener(app_handle: AppHandle, state: Arc<Mutex<SharedAppState>>) {
        if LISTENER_STARTED
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            LISTENER_ENABLED.store(true, Ordering::SeqCst);
            start_input_listener_source(app_handle, state);
            return;
        }

        log::info!("启动划词监听主控线程");

        let detection_thread_app_handle = app_handle.clone();
        let detection_state = state.clone();

        thread::spawn(move || loop {
            if !LISTENER_ENABLED.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(200));
                continue;
            }

            if !GLOBAL_STATE.needs_detection.swap(false, Ordering::SeqCst) {
                wait_detection_pending(Duration::from_millis(50));
                continue;
            }

            {
                if capture::is_screenshot_in_progress() {
                    continue;
                }

                let (selection_enabled, should_skip_detection) = {
                    let state_guard = lock_arc_mutex(&detection_state);
                    (
                        state_guard.settings.selection_enabled,
                        state_guard.is_visible
                            || state_guard.is_image_visible
                            || state_guard.is_processing_selection
                            || state_guard.is_updating_clipboard,
                    )
                };

                if !selection_enabled {
                    continue;
                }

                if should_skip_detection {
                    continue;
                }

                let clipboard_manager = {
                    let state_guard = lock_arc_mutex(&detection_state);
                    state_guard.clipboard_manager.clone()
                };

                if let Some(text) = perform_text_selection_detection(
                    &detection_thread_app_handle,
                    clipboard_manager,
                ) {
                    if !text.trim().is_empty() && is_valid_selection(&text) {
                        log::info!("检测到有效的选中文本: '{}'", text);
                        let app_handle_clone = detection_thread_app_handle.clone();
                        let text_clone = text.clone();
                        let anchor_pos = {
                            let pos_guard = lock_arc_mutex(&GLOBAL_STATE.detection_anchor_pos);
                            *pos_guard
                        };
                        let should_debounce = {
                            let mut last_emit_guard =
                                lock_arc_mutex(&GLOBAL_STATE.last_toolbar_emit);
                            let now = std::time::Instant::now();
                            let should_skip = if let Some((last_text, last_anchor, last_time)) =
                                last_emit_guard.as_ref()
                            {
                                (last_anchor.0 - anchor_pos.0).abs() <= 6
                                    && (last_anchor.1 - anchor_pos.1).abs() <= 6
                                    && *last_text == text
                                    && now.duration_since(*last_time) <= Duration::from_millis(300)
                            } else {
                                false
                            };
                            if !should_skip {
                                *last_emit_guard = Some((text.clone(), anchor_pos, now));
                            }
                            should_skip
                        };
                        if should_debounce {
                            log::info!("命中划词工具栏去抖策略，跳过重复弹窗");
                            continue;
                        }

                        tauri::async_runtime::spawn(async move {
                            log::info!("准备调用 show_selection_toolbar_impl");
                            show_selection_toolbar_impl(
                                app_handle_clone,
                                text_clone,
                                Some(anchor_pos),
                            );
                            log::info!("已调用 show_selection_toolbar_impl");
                        });
                    }
                }
            }
        });
        start_input_listener_source(app_handle, state);

        log::info!("划词监听主控线程已启动");
    }
}

/// 执行划词检测
fn perform_text_selection_detection(
    app_handle: &AppHandle,
    clipboard_manager: Arc<Mutex<ClipboardManager>>,
) -> Option<String> {
    log::info!("开始执行划词检测");

    match get_selected_text(app_handle, clipboard_manager) {
        Some(text) if !text.trim().is_empty() => {
            log::info!("成功获取选中文本: '{}'", text);
            Some(text)
        }
        _ => {
            log::info!("未能获取选中文本或文本为空");
            None
        }
    }
}

/// 计算两点间距离
fn calculate_distance(x1: i32, y1: i32, x2: i32, y2: i32) -> f64 {
    let dx = x2 as f64 - x1 as f64;
    let dy = y2 as f64 - y1 as f64;
    (dx * dx + dy * dy).sqrt()
}

/// 验证是否为有效的拖拽操作
fn is_valid_drag_operation(distance: f64, duration: Duration) -> bool {
    const MIN_DRAG_DISTANCE: f64 = 5.0;
    const MAX_OPERATION_TIME: u128 = 5000;

    let is_distance_valid = distance >= MIN_DRAG_DISTANCE;
    let is_duration_valid = duration.as_millis() <= MAX_OPERATION_TIME;

    log::debug!(
        "拖拽验证 - 距离: {:.2}px (需要 >= {:.1}px), 时间: {:?} (需要 <= {}ms), 结果: {}",
        distance,
        MIN_DRAG_DISTANCE,
        duration,
        MAX_OPERATION_TIME,
        is_distance_valid && is_duration_valid
    );

    is_distance_valid && is_duration_valid
}

/// 检查当前前台窗口是否为命令行窗口
pub fn is_foreground_window_console() -> bool {
    {
        #[cfg(target_os = "windows")]
        unsafe {
            use winapi::um::winuser::{GetClassNameW, GetForegroundWindow, GetWindowTextW};

            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                return false;
            }

            let mut title_buffer = [0u16; 512];
            let title_len =
                GetWindowTextW(hwnd, title_buffer.as_mut_ptr(), title_buffer.len() as i32);
            if title_len == 0 {
                let mut class_buffer = [0u16; 256];
                let class_len =
                    GetClassNameW(hwnd, class_buffer.as_mut_ptr(), class_buffer.len() as i32);
                if class_len == 0 {
                    return false;
                }

                let class = String::from_utf16_lossy(&class_buffer[..class_len as usize]);
                let lower_class = class.to_lowercase();

                let console_classes = [
                    "consolewindowclass",
                    "cascadiacornerwindow",
                    "terminal",
                    "windowsapplicationframehost",
                    "mintty",
                    "sunawtframe",
                    "jbterminal",
                    "windowsterminal",
                    "cmd",
                    "powershell",
                ];

                for class_indicator in console_classes.iter() {
                    if lower_class.contains(class_indicator) {
                        log::warn!("检测到命令行/终端窗口类: {}", lower_class);
                        return true;
                    }
                }

                return false;
            }

            let mut class_buffer = [0u16; 256];
            GetClassNameW(hwnd, class_buffer.as_mut_ptr(), class_buffer.len() as i32);

            let title =
                String::from_utf16_lossy(&title_buffer[..title_len as usize]).to_lowercase();
            let class = String::from_utf16_lossy(&class_buffer)
                .trim_end_matches(char::from(0))
                .to_lowercase();

            let console_indicators = [
                "cmd",
                "command prompt",
                "powershell",
                "terminal",
                "console",
                "bash",
                "shell",
                "git bash",
                "cygwin",
                "wsl",
                "windows terminal",
                "conhost",
                "mintty",
                "idea terminal",
                "jetbrains terminal",
                "terminal - idea",
                "命令提示符",
                "powershell",
                "终端",
            ];

            let console_classes = [
                "consolewindowclass",
                "cascadiacornerwindow",
                "terminal",
                "windowsapplicationframehost",
                "mintty",
                "sunawtframe",
                "jbterminal",
                "windowsterminal",
                "cmd",
                "powershell",
            ];

            for indicator in console_indicators.iter() {
                if title.contains(indicator) || class.contains(indicator) {
                    log::warn!("检测到命令行/终端窗口: {} (class: {})", title, class);
                    return true;
                }
            }

            for class_indicator in console_classes.iter() {
                if class.contains(class_indicator) {
                    log::warn!("检测到命令行/终端窗口类: {} (title: {})", class, title);
                    return true;
                }
            }
        }
    }
    false
}

/// 获取选中文本
fn get_selected_text(
    app_handle: &AppHandle,
    clipboard_manager: Arc<Mutex<ClipboardManager>>,
) -> Option<String> {
    log::info!("开始获取选中文本（模拟复制）");

    use crate::features::text_selection::get_selected_text_with_app;
    get_selected_text_with_app(app_handle, clipboard_manager)
}

/// 验证选中文本是否有效
fn is_valid_selection(text: &str) -> bool {
    let trimmed = text.trim();

    if trimmed.is_empty() {
        log::info!("检测到空文本，跳过");
        return false;
    }

    if is_phone_number(trimmed) {
        log::info!("检测到可能是电话号码的选择: {}", trimmed);
        return false;
    }

    if is_email_address(trimmed) {
        log::info!("检测到可能是邮箱地址的选择: {}", trimmed);
        return false;
    }

    if is_url(trimmed) {
        log::info!("检测到可能是URL的选择: {}", trimmed);
        return false;
    }

    if is_error_text(trimmed) {
        log::info!("检测到错误文本: {}", trimmed);
        return false;
    }

    log::info!("文本通过所有验证，认为是有效的选中文本: {}", trimmed);
    true
}

/// 检查是否为错误文本
fn is_error_text(text: &str) -> bool {
    let error_texts = [
        "chrome legacy windows",
        "chrome legacy",
        "legacy windows",
        "error",
        "null",
        "undefined",
        "",
    ];

    for error_text in error_texts.iter() {
        if text.to_lowercase().trim() == *error_text {
            return true;
        }
    }

    false
}

/// 预编译的电话号码正则表达式
static PHONE_REGEXES: LazyLock<Vec<regex::Regex>> = LazyLock::new(|| {
    let patterns = [
        r"^\+?[\d\s\-\(\)]{10,}$",
        r"^\d{3}-\d{3}-\d{4}$",
        r"^\d{3}\.\d{3}\.\d{4}$",
        r"^\(\d{3}\)\s*\d{3}-\d{4}$",
        r"^\+1\s*\d{3}\s*\d{3}\s*\d{4}$",
    ];
    patterns.iter()
        .filter_map(|p| regex::Regex::new(p).ok())
        .collect()
});

/// 预编译的邮箱正则表达式
static EMAIL_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$").unwrap()
});

/// 预编译的URL正则表达式
static URL_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"^https?://[^\s/$.?#].\S*$|^www\.\S+$").unwrap()
});

/// 检查是否为电话号码
fn is_phone_number(text: &str) -> bool {
    PHONE_REGEXES.iter().any(|regex| regex.is_match(text))
}

/// 检查是否为邮箱地址
fn is_email_address(text: &str) -> bool {
    EMAIL_REGEX.is_match(text)
}

/// 检查是否为URL
fn is_url(text: &str) -> bool {
    URL_REGEX.is_match(text)
}
