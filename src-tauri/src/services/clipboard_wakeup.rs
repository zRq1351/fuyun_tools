use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::Duration;

#[cfg(target_os = "windows")]
static CLIPBOARD_WAKE_EVENT_COUNT: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

#[cfg(target_os = "windows")]
fn wake_window_senders(
) -> &'static std::sync::Mutex<std::collections::HashMap<isize, mpsc::Sender<()>>> {
    static SENDERS: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<isize, mpsc::Sender<()>>>,
    > = std::sync::OnceLock::new();
    SENDERS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

pub struct ClipboardWakeBackend {
    mode: WakeMode,
}

struct WakeHub {
    subscribers: std::sync::Mutex<Vec<WakeSubscriber>>,
}

impl WakeHub {
    fn subscribe(&self) -> ClipboardWakeSubscription {
        let (tx, rx) = mpsc::sync_channel::<WakeSignal>(1);
        let owner = std::sync::Arc::new(());
        let owner_weak = std::sync::Arc::downgrade(&owner);
        if let Ok(mut guard) = self.subscribers.lock() {
            guard.retain(|entry| entry.owner.upgrade().is_some());
            guard.push(WakeSubscriber {
                tx,
                owner: owner_weak,
            });
        }
        ClipboardWakeSubscription { rx, owner }
    }

    fn broadcast_event(&self) {
        let mut guard = if let Ok(guard) = self.subscribers.lock() {
            guard
        } else {
            return;
        };
        guard.retain(|entry| {
            if entry.owner.upgrade().is_none() {
                return false;
            }
            let _ = entry.tx.try_send(WakeSignal::Event);
            true
        });
    }
}

struct WakeSubscriber {
    tx: SyncSender<WakeSignal>,
    owner: std::sync::Weak<()>,
}

pub struct ClipboardWakeSubscription {
    rx: Receiver<WakeSignal>,
    owner: std::sync::Arc<()>,
}

impl ClipboardWakeSubscription {
    pub fn recv(&self) -> Result<WakeSignal, mpsc::RecvError> {
        let _ = &self.owner;
        self.rx.recv()
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<WakeSignal, mpsc::RecvTimeoutError> {
        let _ = &self.owner;
        self.rx.recv_timeout(timeout)
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

pub fn subscribe_clipboard_wake_events() -> ClipboardWakeSubscription {
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
        use std::sync::{
            atomic::{AtomicBool, AtomicIsize, Ordering},
            Arc,
        };
        use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow,
            DispatchMessageW, GetMessageW, PostMessageW, PostQuitMessage, RegisterClassW,
            TranslateMessage, MSG, WM_CLIPBOARDUPDATE,
            WM_CLOSE, WM_DESTROY, WM_NCDESTROY, WNDCLASSW,
        };
        use windows::Win32::UI::WindowsAndMessaging::HWND_MESSAGE;

        let (event_tx, event_rx) = mpsc::channel::<()>();
        let (ready_tx, ready_rx) = mpsc::channel::<bool>();
        let cancelled = Arc::new(AtomicBool::new(false));
        let hwnd_holder = Arc::new(AtomicIsize::new(0));
        let cancelled_for_thread = cancelled.clone();
        let hwnd_holder_for_thread = hwnd_holder.clone();
        thread::spawn(move || {
            let run_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
                use std::mem;
                use std::ptr;

                // Bug修复 (B10): RAII 包装器，确保 panic 时也能清理窗口资源
                struct WindowGuard {
                    hwnd: HWND,
                    listener_added: bool,
                }
                impl Drop for WindowGuard {
                    fn drop(&mut self) {
                        unsafe {
                            if self.listener_added {
                                let _ = winapi::um::winuser::RemoveClipboardFormatListener(self.hwnd.0 as *mut winapi::shared::windef::HWND__);
                            }
                            if !self.hwnd.0.is_null() {
                                let _ = DestroyWindow(self.hwnd);
                            }
                        }
                    }
                }

                unsafe extern "system" fn wndproc(
                    hwnd: HWND,
                    msg: u32,
                    wparam: WPARAM,
                    lparam: LPARAM,
                ) -> LRESULT {
                    if msg == WM_CLIPBOARDUPDATE {
                        let count = CLIPBOARD_WAKE_EVENT_COUNT
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                            + 1;

                        log::debug!("剪贴板消息监听事件计数: {}", count);

                        let key = hwnd.0 as isize;
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
                        return LRESULT(0);
                    }
                    if msg == WM_NCDESTROY {
                        let key = hwnd.0 as isize;
                        if let Ok(mut guard) = wake_window_senders().lock() {
                            guard.remove(&key);
                        }
                        return DefWindowProcW(hwnd, msg, wparam, lparam);
                    }
                    if msg == WM_DESTROY {
                        PostQuitMessage(0);
                        return LRESULT(0);
                    }
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }

                let class_name: Vec<u16> = "FuyunClipboardWakeWindow\0".encode_utf16().collect();
                let hinstance = GetModuleHandleW(None).unwrap_or_default();
                let wnd_class = WNDCLASSW {
                    style: Default::default(),
                    lpfnWndProc: Some(wndproc),
                    cbClsExtra: 0,
                    cbWndExtra: 0,
                    hInstance: hinstance.into(),
                    hIcon: Default::default(),
                    hCursor: Default::default(),
                    hbrBackground: Default::default(),
                    lpszMenuName: windows::core::PCWSTR(ptr::null()),
                    lpszClassName: windows::core::PCWSTR(class_name.as_ptr()),
                };

                let _ = RegisterClassW(&wnd_class);
                let hwnd = CreateWindowExW(
                    Default::default(),
                    windows::core::PCWSTR(class_name.as_ptr()),
                    windows::core::PCWSTR(class_name.as_ptr()),
                    Default::default(),
                    0,
                    0,
                    0,
                    0,
                    Some(HWND_MESSAGE),
                    None,
                    Some(hinstance.into()),
                    None,
                );

                if hwnd.is_err() {
                    log::error!("创建剪贴板消息窗口失败");
                    let _ = ready_tx.send(false);
                    return;
                }
                let hwnd = hwnd.unwrap();
                // Bug修复 (B10): 使用 RAII 保护窗口资源
                let mut window_guard = WindowGuard { hwnd, listener_added: false };
                hwnd_holder_for_thread.store(hwnd.0 as isize, Ordering::Release);
                log::info!("剪贴板消息窗口创建成功: hwnd={}", hwnd.0 as isize);
                if cancelled_for_thread.load(Ordering::Acquire) {
                    let _ = ready_tx.send(false);
                    // window_guard Drop 会自动清理
                    return;
                }

                {
                    if let Ok(mut guard) = wake_window_senders().lock() {
                        guard.insert(hwnd.0 as isize, event_tx);
                    } else {
                        log::error!("剪贴板消息窗口映射注册失败");
                        let _ = ready_tx.send(false);
                        // window_guard Drop 会自动清理
                        return;
                    }
                }

                if winapi::um::winuser::AddClipboardFormatListener(hwnd.0 as *mut winapi::shared::windef::HWND__) == 0 {
                    log::error!("AddClipboardFormatListener 注册失败");
                    let _ = ready_tx.send(false);
                    {
                        if let Ok(mut guard) = wake_window_senders().lock() {
                            guard.remove(&(hwnd.0 as isize));
                        }
                    }
                    // window_guard Drop 会自动清理
                    return;
                }
                window_guard.listener_added = true;

                let _ = ready_tx.send(true);

                let mut msg: MSG = mem::zeroed();
                loop {
                    if cancelled_for_thread.load(Ordering::Acquire) {
                        break;
                    }
                    let code = GetMessageW(&mut msg, None, 0, 0);
                    if code.0 > 0 {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                        continue;
                    }
                    if code.0 == 0 {
                        break;
                    }
                    log::error!("GetMessageW 返回错误，剪贴板监听线程退出");
                    break;
                }

                {
                    if let Ok(mut guard) = wake_window_senders().lock() {
                        guard.remove(&(hwnd.0 as isize));
                    }
                }
                hwnd_holder_for_thread.store(0, Ordering::Release);
                // window_guard Drop 会自动调用 RemoveClipboardFormatListener 和 DestroyWindow
                drop(window_guard);
            }));
            if run_result.is_err() {
                log::error!("剪贴板消息监听线程异常崩溃，自动降级为轮询后端");
                let _ = ready_tx.send(false);
            }
        });

        match ready_rx.recv_timeout(Duration::from_millis(600)) {
            Ok(true) => Some(Self { rx: event_rx }),
            Ok(false) => None,
            Err(RecvTimeoutError::Timeout) => {
                cancelled.store(true, Ordering::Release);
                let hwnd = hwnd_holder.load(Ordering::Acquire);
                if hwnd != 0 {
                    unsafe {
                        let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
                    }
                }
                None
            }
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
