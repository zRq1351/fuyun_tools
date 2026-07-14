use crate::services::clipboard_wakeup::subscribe_clipboard_wake_events;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

/// 剪贴板监听器的通用轮询框架
/// 封装了启动/停止生命周期、唤醒事件订阅和自适应轮询间隔
pub struct ClipboardPoller {
    running: &'static AtomicBool,
    stop_tx: &'static OnceLock<std::sync::Mutex<Option<Sender<()>>>>,
}

impl ClipboardPoller {
    /// 创建一个新的轮询器实例
    /// `running` 和 `stop_tx` 必须是静态生命周期的引用
    pub const fn new(
        running: &'static AtomicBool,
        stop_tx: &'static OnceLock<std::sync::Mutex<Option<Sender<()>>>>,
    ) -> Self {
        Self { running, stop_tx }
    }

    /// 启动监听线程。如果已在运行则直接返回。
    /// `on_event` 回调在每次检测到剪贴板唤醒事件时调用，返回 true 表示需要处理
    /// `on_poll` 回调在每次轮询时调用（无论是否有事件），返回 false 表示应跳过本次轮询
    pub fn start<F, G>(&self, mut on_event: F, mut on_poll: G)
    where
        F: FnMut() + Send + 'static,
        G: FnMut() -> bool + Send + 'static,
    {
        if self
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        if let Ok(mut guard) = self.stop_tx.get_or_init(|| std::sync::Mutex::new(None)).lock() {
            *guard = Some(stop_tx);
        }

        let running = self.running;

        thread::spawn(move || {
            let wake_rx = subscribe_clipboard_wake_events();
            let mut missed_event = false;

            loop {
                if stop_rx.try_recv().is_ok() {
                    break;
                }

                let timeout_ms = if missed_event { 50 } else { 250 };
                let has_event = match wake_rx.recv_timeout(Duration::from_millis(timeout_ms)) {
                    Ok(_) => true,
                    Err(mpsc::RecvTimeoutError::Timeout) => false,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                };

                if has_event {
                    missed_event = true;
                }

                if !on_poll() {
                    missed_event = false;
                    continue;
                }

                if missed_event {
                    missed_event = false;
                    on_event();
                }
            }

            running.store(false, Ordering::SeqCst);
        });
    }

    /// 停止监听线程
    pub fn stop(&self) {
        if let Ok(mut guard) = self.stop_tx.get_or_init(|| std::sync::Mutex::new(None)).lock() {
            if let Some(tx) = guard.take() {
                let _ = tx.send(());
            }
        }
    }

    /// 根据 enabled 参数启动或停止监听器
    pub fn set_enabled(&self, enabled: bool, start_fn: impl FnOnce(), stop_fn: impl FnOnce()) {
        if enabled {
            start_fn();
        } else {
            stop_fn();
        }
    }
}
