use log;
use rdev::{listen, Button, EventType, Key};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::AppHandle;

use crate::core::app_state::AppState as SharedAppState;
use crate::ui::window_manager::{hide_selection_toolbar_impl, show_selection_toolbar_impl};
use crate::utils::clipboard::ClipboardManager;

#[derive(Debug, Clone, PartialEq)]
enum MouseActionState {
    Idle,
    MouseDown(u64, u64, std::time::Instant),
    MouseUp(u64, u64, std::time::Instant),
}

struct GlobalState {
    mouse_action_state: Arc<Mutex<MouseActionState>>,
    ctrl_left_pressed: AtomicBool,
    ctrl_right_pressed: AtomicBool,
    needs_detection: AtomicBool,
    last_processed_time: Arc<Mutex<std::time::Instant>>,
    last_mouse_pos: Arc<Mutex<(u64, u64)>>, // 存储最后的鼠标位置
}

lazy_static::lazy_static! {
    static ref  GLOBAL_STATE: GlobalState = GlobalState {
        mouse_action_state: Arc::new(Mutex::new(MouseActionState::Idle)),
        ctrl_left_pressed: AtomicBool::new(false),
        ctrl_right_pressed: AtomicBool::new(false),
        needs_detection: AtomicBool::new(false),
        last_processed_time: Arc::new(Mutex::new(std::time::Instant::now())),
        last_mouse_pos: Arc::new(Mutex::new((0, 0))),
    };
}

fn is_any_ctrl_pressed() -> bool {
    GLOBAL_STATE.ctrl_left_pressed.load(Ordering::SeqCst)
        || GLOBAL_STATE.ctrl_right_pressed.load(Ordering::SeqCst)
}

pub fn reset_ctrl_key_state() {
    GLOBAL_STATE
        .ctrl_left_pressed
        .store(false, Ordering::SeqCst);
    GLOBAL_STATE
        .ctrl_right_pressed
        .store(false, Ordering::SeqCst);
    log::info!("已重置Ctrl键状态");
}

/// 跨平台鼠标监听器
pub struct MouseListener;

impl MouseListener {
    pub fn start_mouse_listener(app_handle: AppHandle, state: Arc<Mutex<SharedAppState>>) {
        log::info!("启动跨平台鼠标监听器");

        let detection_thread_app_handle = app_handle.clone();

        thread::spawn(move || loop {
            if GLOBAL_STATE.needs_detection.load(Ordering::SeqCst) {
                GLOBAL_STATE.needs_detection.store(false, Ordering::SeqCst);

                let clipboard_manager = {
                    let state_guard = state.lock().unwrap();
                    state_guard.clipboard_manager.clone()
                };

                if let Some(text) = perform_text_selection_detection(
                    &detection_thread_app_handle,
                    clipboard_manager,
                ) {
                    if !text.trim().is_empty() {
                        if is_valid_selection(&text) {
                            log::info!("检测到有效的选中文本: '{}'", text);
                            let app_handle_clone = detection_thread_app_handle.clone();
                            let text_clone = text.clone();

                            tauri::async_runtime::spawn(async move {
                                log::info!("准备调用 show_selection_toolbar_impl");
                                show_selection_toolbar_impl(app_handle_clone, text_clone);
                                log::info!("已调用 show_selection_toolbar_impl");
                            });
                        }
                    }
                }
            }

            thread::sleep(Duration::from_millis(50));
        });

        thread::spawn(move || {
            log::info!("开始监听鼠标键盘事件");
            if let Err(error) = listen(move |event| match event.event_type {
                EventType::KeyPress(key) => {
                    if key == Key::ControlLeft {
                        GLOBAL_STATE.ctrl_left_pressed.store(true, Ordering::SeqCst);
                        log::info!("检测到左Ctrl键按下");
                    } else if key == Key::ControlRight {
                        GLOBAL_STATE
                            .ctrl_right_pressed
                            .store(true, Ordering::SeqCst);
                        log::info!("检测到右Ctrl键按下");
                    }
                }
                EventType::KeyRelease(key) => {
                    if key == Key::ControlLeft {
                        GLOBAL_STATE
                            .ctrl_left_pressed
                            .store(false, Ordering::SeqCst);
                        log::info!("检测到左Ctrl键释放");
                    } else if key == Key::ControlRight {
                        GLOBAL_STATE
                            .ctrl_right_pressed
                            .store(false, Ordering::SeqCst);
                        log::info!("检测到右Ctrl键释放");
                    }
                }
                EventType::ButtonPress(Button::Left) => {
                    let current_time = std::time::Instant::now();

                    let (last_x, last_y) = {
                        let pos_guard = GLOBAL_STATE.last_mouse_pos.lock().unwrap();
                        *pos_guard
                    };

                    log::info!("检测到鼠标左键按下 at ({}, {})", last_x, last_y);

                    let mut state_guard = GLOBAL_STATE.mouse_action_state.lock().unwrap();
                    *state_guard = MouseActionState::MouseDown(last_x, last_y, current_time);
                }
                EventType::ButtonRelease(Button::Left) => {
                    let current_time = std::time::Instant::now();

                    let (last_x, last_y) = {
                        let pos_guard = GLOBAL_STATE.last_mouse_pos.lock().unwrap();
                        *pos_guard
                    };

                    log::info!("检测到鼠标左键释放 at ({}, {})", last_x, last_y);

                    let mut state_guard = GLOBAL_STATE.mouse_action_state.lock().unwrap();
                    let prev_state = std::mem::replace(&mut *state_guard, MouseActionState::Idle);

                    if let MouseActionState::MouseDown(down_x, down_y, down_time) = prev_state {
                        let up_time = current_time;
                        *state_guard = MouseActionState::MouseUp(last_x, last_y, up_time);

                        let distance = calculate_distance(down_x, down_y, last_x, last_y);
                        let duration = up_time.duration_since(down_time);

                        log::info!(
                            "鼠标移动距离: {:.2}px, 操作持续时间: {:?}ms",
                            distance,
                            duration.as_millis()
                        );

                        if is_valid_drag_operation(distance, duration) {
                            if !is_foreground_window_console() {
                                if !is_any_ctrl_pressed() {
                                    let last_processed = {
                                        GLOBAL_STATE.last_processed_time.lock().unwrap().clone()
                                    };

                                    if up_time.duration_since(last_processed)
                                        > Duration::from_millis(100)
                                    {
                                        GLOBAL_STATE.needs_detection.store(true, Ordering::SeqCst);
                                        log::info!("设置划词检测标志");

                                        *GLOBAL_STATE.last_processed_time.lock().unwrap() = up_time;
                                    } else {
                                        log::info!("操作过于频繁，跳过此次检测");
                                    }
                                } else {
                                    log::info!("Ctrl键被按下，忽略此次点击");
                                }
                            } else {
                                log::info!("当前在命令行/终端环境中，跳过划词检测");
                            }
                        } else {
                            log::info!("不满足划词条件（距离或时间不足），跳过");
                        }
                    }
                }
                EventType::MouseMove { x, y } => {
                    let mouse_x = x as u64;
                    let mouse_y = y as u64;

                    {
                        let mut pos_guard = GLOBAL_STATE.last_mouse_pos.lock().unwrap();
                        *pos_guard = (mouse_x, mouse_y);
                    }
                }
                _ => {
                    hide_selection_toolbar_impl(app_handle.clone());
                }
            }) {
                log::error!("鼠标监听器启动失败: {:?}", error);
            }
        });

        log::info!("跨平台鼠标监听器已启动");
    }
}

fn perform_text_selection_detection(
    app_handle: &AppHandle,
    clipboard_manager: Arc<Mutex<ClipboardManager>>,
) -> Option<String> {
    log::info!("开始执行划词检测");

    if is_foreground_window_console() {
        log::info!("在命令行/终端环境中，跳过划词检测");
        return None;
    }

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

fn calculate_distance(x1: u64, y1: u64, x2: u64, y2: u64) -> f64 {
    let dx = x2 as f64 - x1 as f64;
    let dy = y2 as f64 - y1 as f64;
    (dx * dx + dy * dy).sqrt()
}

fn is_valid_drag_operation(distance: f64, duration: Duration) -> bool {
    const MIN_DRAG_DISTANCE: f64 = 5.0;
    const MAX_OPERATION_TIME: u128 = 5000; // 5秒

    let is_distance_valid = distance >= MIN_DRAG_DISTANCE;
    let is_duration_valid = duration.as_millis() <= MAX_OPERATION_TIME;

    log::info!(
        "拖拽验证 - 距离: {:.2}px (需要 >= {:.1}px), 时间: {:?} (需要 <= {}ms), 结果: {}",
        distance,
        MIN_DRAG_DISTANCE,
        duration,
        MAX_OPERATION_TIME,
        is_distance_valid && is_duration_valid
    );

    is_distance_valid && is_duration_valid
}

fn is_foreground_window_console() -> bool {
    {
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

fn get_selected_text(
    app_handle: &AppHandle,
    clipboard_manager: Arc<Mutex<ClipboardManager>>,
) -> Option<String> {
    log::info!("开始获取选中文本（模拟复制）");

    use crate::features::text_selection::get_selected_text_with_app;
    let result = get_selected_text_with_app(app_handle, clipboard_manager);
    reset_ctrl_key_state();
    result
}

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

fn is_phone_number(text: &str) -> bool {
    let phone_patterns = [
        r"^\+?[\d\s\-\(\)]{10,}$",        // 一般格式
        r"^\d{3}-\d{3}-\d{4}$",           // 123-456-7890 格式
        r"^\d{3}\.\d{3}\.\d{4}$",         // 123.456.7890 格式
        r"^\(\d{3}\)\s*\d{3}-\d{4}$",     // (123) 456-7890 格式
        r"^\+1\s*\d{3}\s*\d{3}\s*\d{4}$", // +1 123 456 7890 格式
    ];

    for pattern in &phone_patterns {
        if let Ok(regex) = regex::Regex::new(pattern) {
            if regex.is_match(text) {
                return true;
            }
        }
    }
    false
}

fn is_email_address(text: &str) -> bool {
    let email_pattern = r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$";

    if let Ok(regex) = regex::Regex::new(email_pattern) {
        regex.is_match(text)
    } else {
        false
    }
}

fn is_url(text: &str) -> bool {
    let url_pattern = r"^https?://[^\s/$.?#].\S*$|^www\.\S+$";

    if let Ok(regex) = regex::Regex::new(url_pattern) {
        regex.is_match(text)
    } else {
        false
    }
}
