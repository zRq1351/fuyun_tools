//! 同步功能的 Tauri 命令层（供设置面板调用）。

use tauri::AppHandle;

use super::{status, stop, unpair, SyncStatus};
use crate::core::app_state::AppState as SharedAppState;
use crate::sync::Mutex;
use std::sync::Arc;

/// 启动同步服务（幂等）
///
/// 必须是 async 命令：`start()` 内部的 `tokio::net::TcpListener::from_std`
/// 需要 Tokio reactor 上下文。同步命令执行在主线程上，调用它会 panic
/// （"there is no reactor running"）并被全局 panic 钩子记录后直接退出进程。
/// Tauri 的 async 命令运行在 async_runtime（tokio）上，天然满足该约束。
#[tauri::command]
pub async fn start_sync(
    app: AppHandle,
    state: tauri::State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<SyncStatus, String> {
    super::start(&app, Arc::clone(&state))
}

/// 停止同步服务
#[tauri::command]
pub fn stop_sync(app: AppHandle) -> Result<(), String> {
    stop(&app);
    Ok(())
}

/// 获取同步状态
#[tauri::command]
pub fn get_sync_status() -> SyncStatus {
    status()
}

/// 重新生成配对码（旧码立即作废）
#[tauri::command]
pub fn regenerate_sync_pairing_code() -> Result<String, String> {
    Ok(super::regenerate_pairing_code())
}

/// 吊销已配对设备（删除令牌，手机需重新配对）
#[tauri::command]
pub fn unpair_sync_device(app: AppHandle) -> Result<(), String> {
    unpair(&app)
}

/// 便捷：当前是否已运行（供其他模块查询，无命令注册）
pub fn is_running() -> bool {
    super::status().running
}
