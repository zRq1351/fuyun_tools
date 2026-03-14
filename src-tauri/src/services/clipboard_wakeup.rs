use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

pub struct ClipboardWakeBackend {
    mode: WakeMode,
}

enum WakeMode {
    #[cfg(target_os = "windows")]
    Event(WindowsClipboardEventBackend),
    Fallback,
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

    pub fn wait(&mut self, timeout: Duration) {
        match &mut self.mode {
            #[cfg(target_os = "windows")]
            WakeMode::Event(backend) => {
                if !backend.wait(timeout) {
                    log::warn!("Windows 事件后端不可用，已降级到自适应轮询");
                    self.mode = WakeMode::Fallback;
                    thread::sleep(timeout);
                }
            }
            WakeMode::Fallback => {
                thread::sleep(timeout);
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
        thread::spawn(move || unsafe {
            use std::mem;
            use std::ptr;
            use winapi::shared::minwindef::{LPARAM, LRESULT, UINT, WPARAM};
            use winapi::shared::windef::HWND;
            use winapi::um::libloaderapi::GetModuleHandleW;
            use winapi::um::winuser::{
                AddClipboardFormatListener, CreateWindowExW, DestroyWindow, DispatchMessageW,
                GetMessageW, RegisterClassW, RemoveClipboardFormatListener, SetWindowLongPtrW,
                TranslateMessage, WNDCLASSW, GWLP_USERDATA, HWND_MESSAGE, MSG,
            };

            unsafe extern "system" fn wndproc(
                hwnd: HWND,
                msg: UINT,
                wparam: WPARAM,
                lparam: LPARAM,
            ) -> LRESULT {
                use winapi::um::winuser::{
                    DefWindowProcW, GetWindowLongPtrW, PostQuitMessage, SetWindowLongPtrW,
                    WM_CLIPBOARDUPDATE, WM_DESTROY, WM_NCDESTROY, GWLP_USERDATA,
                };
                let sender_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA)
                    as *mut std::sync::mpsc::Sender<()>;
                if msg == WM_CLIPBOARDUPDATE {
                    if !sender_ptr.is_null() {
                        let _ = (*sender_ptr).send(());
                    }
                    return 0;
                }
                if msg == WM_NCDESTROY {
                    if !sender_ptr.is_null() {
                        let _ = Box::from_raw(sender_ptr);
                        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
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
                let _ = ready_tx.send(false);
                return;
            }

            let sender_ptr = Box::into_raw(Box::new(event_tx));
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, sender_ptr as isize);

            if AddClipboardFormatListener(hwnd) == 0 {
                let _ = ready_tx.send(false);
                let _ = Box::from_raw(sender_ptr);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                DestroyWindow(hwnd);
                return;
            }

            let _ = ready_tx.send(true);

            let mut msg: MSG = mem::zeroed();
            while GetMessageW(&mut msg as *mut MSG, ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            let _ = RemoveClipboardFormatListener(hwnd);
            DestroyWindow(hwnd);
        });

        match ready_rx.recv_timeout(Duration::from_millis(600)) {
            Ok(true) => Some(Self { rx: event_rx }),
            Ok(false) => None,
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => None,
        }
    }

    fn wait(&mut self, timeout: Duration) -> bool {
        use std::sync::mpsc::RecvTimeoutError;
        match self.rx.recv_timeout(timeout) {
            Ok(_) => true,
            Err(RecvTimeoutError::Timeout) => true,
            Err(RecvTimeoutError::Disconnected) => false,
        }
    }
}
