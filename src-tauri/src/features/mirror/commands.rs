//! 投屏 Tauri 命令。

use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

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

    let adb_path = adb::resolve_adb(Some(&app)).map_err(|e| {
        log::error!("[mirror] {}", e);
        e
    })?;
    let jar = jar_path(&app).map_err(|e| {
        log::error!("[mirror] {}", e);
        e
    })?;

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
    log::info!(
        "[mirror] 会话已建立: 设备={} 中继端口={} 窗口已打开",
        device_name,
        relay_port
    );

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

/// 拖拽文件到投屏窗口：adb push 到手机公共目录并触发媒体扫描。
#[tauri::command]
pub async fn mirror_push_file(_app: AppHandle, path: String) -> Result<serde_json::Value, String> {
    let (adb_path, serial) = {
        let guard = state().session.lock().unwrap();
        match guard.as_ref() {
            Some(s) => (s.adb_path.clone(), s.serial.clone()),
            None => return Err("投屏会话未运行".into()),
        }
    };
    let src = std::path::PathBuf::from(&path);
    if !src.exists() {
        return Err(format!("文件不存在: {}", path));
    }
    let name = src
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".into());
    let ext = src
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let is_image = matches!(
        ext.as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "heic"
    );
    let dest_dir = if is_image {
        "/sdcard/Pictures/FuyunMirror"
    } else {
        "/sdcard/Download/FuyunMirror"
    };

    // push 可能耗时（大文件），放线程池
    let adb = adb_path.clone();
    let serial2 = serial.clone();
    let path2 = path.clone();
    let name2 = name.clone();
    let dest_dir2 = dest_dir.to_string();
    let dest = tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        adb::run_adb(
            &adb,
            &["-s", &serial2, "shell", "mkdir", "-p", &dest_dir2],
            Duration::from_secs(10),
        )?;
        let dest = format!("{}/{}", dest_dir2, name2);
        adb::run_adb(
            &adb,
            &["-s", &serial2, "push", &path2, &dest],
            Duration::from_secs(600),
        )?;
        Ok(dest)
    })
    .await
    .map_err(|e| format!("推送任务失败: {}", e))??;

    // 媒体扫描（尽力而为：失败不影响文件已落地）
    let adb3 = adb_path;
    let serial3 = serial;
    let dest2 = dest.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || {
        let _ = adb::run_adb(
            &adb3,
            &[
                "-s",
                &serial3,
                "shell",
                "am",
                "broadcast",
                "-a",
                "android.intent.action.MEDIA_SCANNER_SCAN_FILE",
                "-d",
                &format!("file://{}", dest2),
            ],
            Duration::from_secs(15),
        );
    })
    .await;

    log::info!("[mirror] 文件已推送: {} -> {}", path, dest);
    Ok(serde_json::json!({
        "dest": dest,
        "name": name,
        "isImage": is_image,
    }))
}

/// 从手机导出最近的文件/图片到本地目录（DCIM 相机、截图、Pictures 逐目录按时间倒序）。
#[tauri::command]
pub async fn mirror_export_recent(
    _app: AppHandle,
    save_dir: String,
    count: Option<u32>,
) -> Result<serde_json::Value, String> {
    let (adb_path, serial) = {
        let guard = state().session.lock().unwrap();
        match guard.as_ref() {
            Some(s) => (s.adb_path.clone(), s.serial.clone()),
            None => return Err("投屏会话未运行".into()),
        }
    };
    let count = count.unwrap_or(10).clamp(1, 50) as usize;

    let adb = adb_path.clone();
    let serial2 = serial.clone();
    let save = save_dir.clone();
    let res = tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let mut exported: Vec<String> = Vec::new();
        let dirs = ["/sdcard/DCIM/Camera", "/sdcard/Pictures/Screenshots", "/sdcard/Pictures"];
        for dir in dirs {
            let out = match adb::run_adb(
                &adb,
                &["-s", &serial2, "shell", "ls", "-t", dir],
                Duration::from_secs(15),
            ) {
                Ok(o) => o,
                Err(_) => continue, // 目录可能不存在
            };
            for line in out.lines().take(count) {
                let name = line.trim().to_string();
                if name.is_empty() || name.contains('/') || !name.contains('.') {
                    continue; // 跳过子目录与非文件行
                }
                if exported.iter().any(|e| e.ends_with(&name)) {
                    continue; // 同名文件已在其他目录导出过
                }
                let remote = format!("{}/{}", dir, name);
                let local = std::path::Path::new(&save).join(&name);
                let local_str = local.to_string_lossy().to_string();
                if let Err(e) = adb::run_adb(
                    &adb,
                    &["-s", &serial2, "pull", &remote, &local_str],
                    Duration::from_secs(300),
                ) {
                    log::warn!("[mirror] 导出失败 {}: {}", remote, e);
                    continue;
                }
                exported.push(local_str);
                if exported.len() >= count {
                    break;
                }
            }
            if exported.len() >= count {
                break;
            }
        }
        log::info!("[mirror] 从手机导出 {} 个文件到 {}", exported.len(), save);
        Ok(serde_json::json!({ "exported": exported, "saveDir": save }))
    })
    .await
    .map_err(|e| format!("导出任务失败: {}", e))??;
    Ok(res)
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
