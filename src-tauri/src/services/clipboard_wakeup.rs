use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

#[cfg(target_os = "windows")]
static CLIPBOARD_WAKE_EVENT_COUNT: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

#[cfg(target_os = "windows")]
fn wake_window_senders() -> &'static std::sync::Mutex<std::collections::HashMap<isize, mpsc::Sender<()>>> {
    static SENDERS: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<isize, mpsc::Sender<()>>>,
    > = std::sync::OnceLock::new();
    SENDERS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

pub struct ClipboardWakeBackend {
    mode: WakeMode,
}

struct WakeHub {
    subscribers: std::sync::Mutex<Vec<mpsc::Sender<WakeSignal>>>,
}

impl WakeHub {
    fn subscribe(&self) -> Receiver<WakeSignal> {
        let (tx, rx) = mpsc::channel::<WakeSignal>();
        if let Ok(mut guard) = self.subscribers.lock() {
            guard.push(tx);
        }
        rx
    }

    fn broadcast_event(&self) {
        if let Ok(mut guard) = self.subscribers.lock() {
            guard.retain(|tx| tx.send(WakeSignal::Event).is_ok());
        }
    }
}

fn wake_hub() -> &'static WakeHub {
    static HUB: std::sync::OnceLock<WakeHub> = std::sync::OnceLock::new();
    HUB.get_or_init(|| WakeHub {
        subscribers: std::sync::Mutex::new(Vec::new()),
    })
}

fn ensure_wake_dispatcher_started() {
    static STARTED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    STARTED.get_or_init(|| {
        thread::spawn(move || {
            let mut wake_backend = ClipboardWakeBackend::new();
            loop {
                let signal = wake_backend.wait_with_signal(Duration::from_secs(24 * 60 * 60));
                if !matches!(signal, WakeSignal::Event) {
                    continue;
                }
                wake_hub().broadcast_event();
            }
        });
    });
}

pub fn subscribe_clipboard_wake_events() -> Receiver<WakeSignal> {
    ensure_wake_dispatcher_started();
    wake_hub().subscribe()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WakeSignal {
    Event,
    Timeout,
    Fallback,
}

enum WakeMode {
    #[cfg(target_os = "windows")]
    Event(WindowsClipboardEventBackend),
    Fallback,
}

impl Default for ClipboardWakeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardWakeBackend {
    pub fn new() -> Self {
        #[cfg(target_os = "windows")]
        {
            if let Some(backend) = WindowsClipboardEventBackend::new() {
                log::info!("剪贴板监听已启用 Windows 事件后端");
                return Self {
                    mode: WakeMode::Event(backend),
                };
            }
            log::warn!("Windows 事件后端初始化失败，自动降级为自适应轮询");
        }
        Self {
            mode: WakeMode::Fallback,
        }
    }

    pub fn wait_with_signal(&mut self, timeout: Duration) -> WakeSignal {
        match &mut self.mode {
            #[cfg(target_os = "windows")]
            WakeMode::Event(backend) => match backend.wait(timeout) {
                WakeSignal::Event => WakeSignal::Event,
                WakeSignal::Timeout => WakeSignal::Timeout,
                WakeSignal::Fallback => {
                    log::warn!("Windows 事件后端不可用，已降级到自适应轮询");
                    self.mode = WakeMode::Fallback;
                    thread::sleep(timeout);
                    WakeSignal::Fallback
                }
            },
            WakeMode::Fallback => {
                thread::sleep(timeout);
                WakeSignal::Fallback
            }
        }
    }
}

#[cfg(target_os = "windows")]
struct WindowsClipboardEventBackend {
    rx: Receiver<()>,
}

#[cfg(target_os = "windows")]
impl WindowsClipboardEventBackend {
    fn new() -> Option<Self> {
        use std::sync::mpsc::RecvTimeoutError;
        let (event_tx, event_rx) = mpsc::channel::<()>();
        let (ready_tx, ready_rx) = mpsc::channel::<bool>();
        thread::spawn(move || {
            let run_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
                use std::mem;
                use std::ptr;
                use winapi::shared::minwindef::{LPARAM, LRESULT, UINT, WPARAM};
                use winapi::shared::windef::HWND;
                use winapi::um::libloaderapi::GetModuleHandleW;
                use winapi::um::winuser::{
                    AddClipboardFormatListener, CreateWindowExW, DestroyWindow, DispatchMessageW,
                    GetMessageW, RegisterClassW, RemoveClipboardFormatListener, TranslateMessage,
                    HWND_MESSAGE, MSG, WNDCLASSW,
                };

                unsafe extern "system" fn wndproc(
                    hwnd: HWND,
                    msg: UINT,
                    wparam: WPARAM,
                    lparam: LPARAM,
                ) -> LRESULT {
                    use winapi::um::winuser::{
                        DefWindowProcW, PostQuitMessage, WM_CLIPBOARDUPDATE, WM_DESTROY,
                        WM_NCDESTROY,
                    };
                    if msg == WM_CLIPBOARDUPDATE {
                        let count = CLIPBOARD_WAKE_EVENT_COUNT
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                            + 1;

                        log::info!("剪贴板消息监听事件计数: {}", count);

                        let key = hwnd as isize;
                        let sender = {
                            if let Ok(guard) = wake_window_senders().lock() {
                                guard.get(&key).cloned()
                            } else {
                                None
                            }
                        };
                        if let Some(sender) = sender {
                            let _ = sender.send(());
                        }
                        return 0;
                    }
                    if msg == WM_NCDESTROY {
                        let key = hwnd as isize;
                        if let Ok(mut guard) = wake_window_senders().lock() {
                            guard.remove(&key);
                        }
                        return DefWindowProcW(hwnd, msg, wparam, lparam);
                    }
                    if msg == WM_DESTROY {
                        PostQuitMessage(0);
                        return 0;
                    }
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }

                let class_name: Vec<u16> = "FuyunClipboardWakeWindow\0".encode_utf16().collect();
                let hinstance = GetModuleHandleW(ptr::null());
                let wnd_class = WNDCLASSW {
                    style: 0,
                    lpfnWndProc: Some(wndproc),
                    cbClsExtra: 0,
                    cbWndExtra: 0,
                    hInstance: hinstance,
                    hIcon: ptr::null_mut(),
                    hCursor: ptr::null_mut(),
                    hbrBackground: ptr::null_mut(),
                    lpszMenuName: ptr::null(),
                    lpszClassName: class_name.as_ptr(),
                };

                let _ = RegisterClassW(&wnd_class);
                let hwnd = CreateWindowExW(
                    0,
                    class_name.as_ptr(),
                    class_name.as_ptr(),
                    0,
                    0,
                    0,
                    0,
                    0,
                    HWND_MESSAGE,
                    ptr::null_mut(),
                    hinstance,
                    ptr::null_mut(),
                );

                if hwnd.is_null() {
                    log::error!("创建剪贴板消息窗口失败");
                    let _ = ready_tx.send(false);
                    return;
                }
                log::info!("剪贴板消息窗口创建成功: hwnd={}", hwnd as isize);

                {
                    if let Ok(mut guard) = wake_window_senders().lock() {
                        guard.insert(hwnd as isize, event_tx);
                    } else {
                        log::error!("剪贴板消息窗口映射注册失败");
                        let _ = ready_tx.send(false);
                        DestroyWindow(hwnd);
                        return;
                    }
                }

                if AddClipboardFormatListener(hwnd) == 0 {
                    log::error!("AddClipboardFormatListener 注册失败");
                    let _ = ready_tx.send(false);
                    {
                        if let Ok(mut guard) = wake_window_senders().lock() {
                            guard.remove(&(hwnd as isize));
                        }
                    }
                    DestroyWindow(hwnd);
                    return;
                }

                let _ = ready_tx.send(true);

                let mut msg: MSG = mem::zeroed();
                loop {
                    let code = GetMessageW(&mut msg as *mut MSG, ptr::null_mut(), 0, 0);
                    if code > 0 {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                        continue;
                    }
                    if code == 0 {
                        break;
                    }
                    log::error!("GetMessageW 返回错误，剪贴板监听线程退出");
                    break;
                }

                {
                    if let Ok(mut guard) = wake_window_senders().lock() {
                        guard.remove(&(hwnd as isize));
                    }
                }
                let _ = RemoveClipboardFormatListener(hwnd);
                DestroyWindow(hwnd);
            }));
            if run_result.is_err() {
                log::error!("剪贴板消息监听线程异常崩溃，自动降级为轮询后端");
                let _ = ready_tx.send(false);
            }
        });

        match ready_rx.recv_timeout(Duration::from_millis(600)) {
            Ok(true) => Some(Self { rx: event_rx }),
            Ok(false) => None,
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => None,
        }
    }

    fn wait(&mut self, timeout: Duration) -> WakeSignal {
        use std::sync::mpsc::RecvTimeoutError;
        match self.rx.recv_timeout(timeout) {
            Ok(_) => WakeSignal::Event,
            Err(RecvTimeoutError::Timeout) => WakeSignal::Timeout,
            Err(RecvTimeoutError::Disconnected) => WakeSignal::Fallback,
        }
    }
}
