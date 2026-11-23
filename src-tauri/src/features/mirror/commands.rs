//! 投屏 Tauri 命令。

use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Mutex, OnceLock};

use tauri::{AppHandle, Manager};

use super::adb;
use super::relay::Hub;
use super::session::{self, SessionHandle};
use super::SCRCPY_VERSION;

struct MirrorState {
    session: Mutex<Option<SessionHandle>>,
    relay_port: AtomicU16,
    hub: Mutex<Option<Hub>>,
}

fn state() -> &'static MirrorState {
    static S: OnceLock<MirrorState> = OnceLock::new();
    S.get_or_init(|| MirrorState {
        session: Mutex::new(None),
        relay_port: AtomicU16::new(0),
        hub: Mutex::new(None),
    })
}

fn resource_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .resource_dir()
        .map_err(|e| format!("获取资源目录失败: {}", e))
}

fn jar_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = resource_dir(app)?;
    // 打包器把 tauri.conf.json 里的 "../third_party/*" 安装到 resource_dir/_up_/third_party/
    // （参照 _up_/docs 的既有布局）；开发态则可能直接是 resource_dir/third_party/。
    let candidates = [
        dir.join("_up_").join("third_party").join("scrcpy"),
        dir.join("third_party").join("scrcpy"),
    ];
    let name = format!("scrcpy-server-v{}", SCRCPY_VERSION);
    for base in &candidates {
        let p = base.join(&name);
        if p.exists() {
            return Ok(p);
        }
    }
    Err(format!(
        "scrcpy-server 未随包分发（已探测: {}）",
        candidates
            .iter()
            .map(|b| b.join(&name).display().to_string())
            .collect::<Vec<_>>()
            .join("; ")
    ))
}

/// 启动（或复用）投屏会话，并打开投屏窗口。
/// 返回给前端的信息供投屏页面连接中继。
#[tauri::command]
pub async fn mirror_start(app: AppHandle) -> Result<serde_json::Value, String> {
    // 已在运行：直接返回现有信息
    if let Some(session) = state().session.lock().unwrap().as_ref() {
        let port = state().relay_port.load(Ordering::SeqCst);
        super::window::show_mirror_window(&app)?;
        return Ok(serde_json::json!({
            "relayPort": port,
            "deviceName": session.device_name,
            "reused": true,
        }));
    }

    let adb_path = adb::resolve_adb(Some(&app))?;
    let jar = jar_path(&app)?;

    let hub = Hub::new();
    // 会话启动是阻塞流程（adb + 握手），放线程池
    let hub_for_session = hub.clone();
    let app_for_session = app.clone();
    let handle = tauri::async_runtime::spawn_blocking(move || {
        session::start(&app_for_session, hub_for_session, adb_path, jar)
    })
    .await
    .map_err(|e| format!("投屏任务失败: {}", e))??;

    // 启动本地中继
    let (control_tx, hub_for_relay) = (handle.control_tx.clone(), handle.hub.clone());
    let relay_port = super::relay::serve(hub_for_relay, control_tx).await?;
    state().relay_port.store(relay_port, Ordering::SeqCst);

    let device_name = handle.device_name.clone();
    *state().hub.lock().unwrap() = Some(hub);
    *state().session.lock().unwrap() = Some(handle);

    super::window::show_mirror_window(&app)?;

    Ok(serde_json::json!({
        "relayPort": relay_port,
        "deviceName": device_name,
        "reused": false,
    }))
}

/// 停止投屏会话。
#[tauri::command]
pub async fn mirror_stop(app: AppHandle) -> Result<(), String> {
    stop_session(&app);
    Ok(())
}

/// 查询投屏状态（供投屏窗口挂载时连接中继）。
#[tauri::command]
pub async fn mirror_status(_app: AppHandle) -> Result<serde_json::Value, String> {
    let session = state().session.lock().unwrap();
    let port = state().relay_port.load(Ordering::SeqCst);
    Ok(serde_json::json!({
        "running": session.is_some(),
        "relayPort": port,
        "deviceName": session.as_ref().map(|s| s.device_name.clone()),
        "version": SCRCPY_VERSION,
    }))
}

/// 停止会话（供窗口销毁/退出时调用）。
pub fn stop_session(app: &AppHandle) {
    if let Some(session) = state().session.lock().unwrap().take() {
        session.stop();
        log::info!("投屏会话已停止");
    }
    let _ = app;
}

/// 会话是否在运行。
pub fn is_running() -> bool {
    state().session.lock().unwrap().is_some()
}

/// 应用退出时的清理。
pub fn shutdown(app: &AppHandle) {
    stop_session(app);
}
