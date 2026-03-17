use std::sync::{Mutex, OnceLock};

fn clipboard_access_mutex() -> &'static Mutex<()> {
    static ACCESS_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();
    ACCESS_MUTEX.get_or_init(|| Mutex::new(()))
}

pub fn with_clipboard_access_lock<T>(f: impl FnOnce() -> T) -> T {
    let _guard = clipboard_access_mutex().lock().unwrap();
    f()
}
