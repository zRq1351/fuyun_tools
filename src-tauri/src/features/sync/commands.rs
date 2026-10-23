//! 同步功能的 Tauri 命令层（供设置面板调用）。

use tauri::AppHandle;

use super::{status, stop, unpair, SyncStatus};
use crate::core::app_state::AppState as SharedAppState;
use crate::sync::Mutex;
use std::sync::Arc;

/// 启动同步服务（幂等）
#[tauri::command]
pub fn start_sync(
    app: AppHandle,
    state: tauri::State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<SyncStatus, String> {
    super::start(&app, Arc::clone(&state))
}

/// 停止同步服务
#[tauri::command]
pub fn stop_sync() -> Result<(), String> {
    stop();
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
