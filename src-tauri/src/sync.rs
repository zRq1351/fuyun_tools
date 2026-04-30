pub use parking_lot::MutexGuard;
use std::convert::Infallible;
use std::fmt;
use std::sync::Arc;

pub struct Mutex<T>(parking_lot::Mutex<T>);

pub struct TryLockError;

impl fmt::Display for TryLockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "mutex is already locked")
    }
}

impl fmt::Debug for TryLockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TryLockError")
    }
}

impl<T> Mutex<T> {
    pub fn new(value: T) -> Self {
        Self(parking_lot::Mutex::new(value))
    }

    pub fn lock(&self) -> Result<MutexGuard<'_, T>, Infallible> {
        Ok(self.0.lock())
    }

    pub fn try_lock(&self) -> Result<MutexGuard<'_, T>, TryLockError> {
        self.0.try_lock().ok_or(TryLockError)
    }
}

/// 锁定 Arc<Mutex<T>> 并返回 MutexGuard
/// 这是一个便利函数，用于简化对 Arc<Mutex<T>> 的锁定操作
pub fn lock_arc_mutex<T>(mutex: &Arc<Mutex<T>>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|never| match never {})
}
