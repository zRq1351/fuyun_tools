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
/// `serial`：优先使用的设备（无线投屏时为 `ip:port`），不传则用唯一已连设备。
/// 返回给前端的信息供投屏页面连接中继。
#[tauri::command]
pub async fn mirror_start(
    app: AppHandle,
    serial: Option<String>,
) -> Result<serde_json::Value, String> {
    // 已在运行：直接返回现有信息
    if let Some(session) = state().session.lock().unwrap().as_ref() {
        let port = state().relay_port.load(Ordering::SeqCst);
        super::window::show_mirror_window(&app)?;
        return Ok(serde_json::json!({
            "relayPort": port,
            "deviceName": session.device_name,
            "serial": session.serial,
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
    let serial_for_session = serial.clone();
    let handle = tauri::async_runtime::spawn_blocking(move || {
        session::start(&app_for_session, hub_for_session, adb_path, jar, serial_for_session)
    })
    .await
    .map_err(|e| format!("投屏任务失败: {}", e))??;

    // 启动本地中继
    let (control_tx, hub_for_relay) = (handle.control_tx.clone(), handle.hub.clone());
    let relay_port = super::relay::serve(hub_for_relay, control_tx).await?;
    state().relay_port.store(relay_port, Ordering::SeqCst);

    let device_name = handle.device_name.clone();
    let session_serial = handle.serial.clone();
    *state().hub.lock().unwrap() = Some(hub);
    *state().session.lock().unwrap() = Some(handle);

    super::window::show_mirror_window(&app)?;
    log::info!(
        "[mirror] 会话已建立: 设备={} 序列={} 中继端口={} 窗口已打开",
        device_name,
        session_serial,
        relay_port
    );

    Ok(serde_json::json!({
        "relayPort": relay_port,
        "deviceName": device_name,
        "serial": session_serial,
        "reused": false,
    }))
}

/// USB 一键切换无线投屏：把当前 USB 连接的手机切到 TCP/IP 调试（5555），
/// 读取手机 WiFi IP 后建立无线连接，返回无线 serial（`ip:5555`）。
/// 成功后可拔掉 USB 线，用 mirror_start(serial) 以无线方式投屏。
#[tauri::command]
pub async fn mirror_enable_wireless(app: AppHandle) -> Result<serde_json::Value, String> {
    let adb_path = adb::resolve_adb(Some(&app))?;
    let adb_for_list = adb_path.clone();
    let serials = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<String>, String> {
        adb::list_device_serials(&adb_for_list)
    })
    .await
    .map_err(|e| e.to_string())??;
    // 过滤出 USB 设备（无线 serial 形如 ip:port）
    let usb: Vec<&String> = serials.iter().filter(|s| !s.contains(':')).collect();
    if usb.is_empty() {
        return Err("未检测到 USB 连接的手机：无线模式首次开启需要用 USB 连接一次".into());
    }
    let serial = usb[0].clone();

    let adb = adb_path.clone();
    let s2 = serial.clone();
    let result = tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        adb::enable_tcpip(&adb, &s2)?;
        // tcpip 重启 adbd，等待几秒后读 IP
        std::thread::sleep(Duration::from_secs(2));
        let ip = adb::get_wlan_ip(&adb, &s2)?;
        let address = format!("{}:5555", ip);
        // connect 可能需要多次尝试（adbd 重启中）
        let mut last_err = String::new();
        for _ in 0..5 {
            let out = adb::connect_wireless(&adb, &address)?;
            if out.contains("connected to") {
                return Ok(address);
            }
            last_err = out;
            std::thread::sleep(Duration::from_secs(2));
        }
        Err(format!("无线连接失败: {}", last_err))
    })
    .await
    .map_err(|e| e.to_string())??;

    log::info!("[mirror] 无线投屏已开启: {}", result);
    Ok(serde_json::json!({ "serial": result }))
}

/// 无线调试配对/连接（Android 11+，无需 USB）。
/// 手机需在「开发者选项 → 无线调试」中打开，并用「使用配对码配对设备」
/// 获取配对 IP:端口与 6 位码。pair_code 提供时先配对，随后 connect 到 debug 端口。
#[tauri::command]
pub async fn mirror_connect_wireless(
    app: AppHandle,
    ip: String,
    connect_port: Option<u16>,
    pair_address: Option<String>,
    pair_code: Option<String>,
) -> Result<serde_json::Value, String> {
    let adb_path = adb::resolve_adb(Some(&app))?;
    let ip = ip.trim().to_string();
    if ip.is_empty() || !ip.contains('.') {
        return Err("请输入手机 WiFi 的 IP 地址".into());
    }
    let connect_port = connect_port.unwrap_or(5555);
    let connect_addr = format!("{}:{}", ip, connect_port);

    let adb = adb_path.clone();
    let pair_addr = pair_address.clone();
    let code = pair_code.clone();
    let caddr = connect_addr.clone();
    let out = tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        if let (Some(pa), Some(pc)) = (pair_addr.as_deref(), code.as_deref()) {
            let r = adb::pair_wireless(&adb, pa, pc)?;
            log::info!("[mirror] 无线配对结果: {}", r);
            if !r.contains("Successfully") && !r.contains("paired") {
                return Err(format!("配对失败: {}", r));
            }
        }
        adb::connect_wireless(&adb, &caddr)
    })
    .await
    .map_err(|e| e.to_string())??;

    if !out.contains("connected to") {
        return Err(format!(
            "连接失败（{}）。请确认手机已打开「开发者选项 → 无线调试」，且电脑与手机在同一 WiFi",
            out.trim()
        ));
    }
    log::info!("[mirror] 无线连接成功: {}", connect_addr);
    Ok(serde_json::json!({ "serial": connect_addr }))
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
        let dirs = [
            "/sdcard/DCIM/Camera",
            "/sdcard/Pictures/Screenshots",
            "/sdcard/Pictures",
        ];
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

/// 列出手机常用目录下的最近文件（供用户在界面上勾选导出）。
#[tauri::command]
pub async fn mirror_list_phone_files(_app: AppHandle) -> Result<serde_json::Value, String> {
    let (adb_path, serial) = {
        let guard = state().session.lock().unwrap();
        match guard.as_ref() {
            Some(s) => (s.adb_path.clone(), s.serial.clone()),
            None => return Err("投屏会话未运行".into()),
        }
    };
    let adb = adb_path.clone();
    let serial2 = serial.clone();
    let res = tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let dirs = [
            "/sdcard/DCIM/Camera",
            "/sdcard/Pictures/Screenshots",
            "/sdcard/Download",
        ];
        let mut files: Vec<serde_json::Value> = Vec::new();
        for dir in dirs {
            let cmd = format!("ls -l -t {} 2>/dev/null | head -30", dir);
            let text = adb::run_adb(
                &adb,
                &["-s", &serial2, "shell", &cmd],
                Duration::from_secs(15),
            )
            .unwrap_or_default();
            for line in text.lines() {
                let t: Vec<&str> = line.split_whitespace().collect();
                // toybox ls -l: 权限 数 链 owner group size date time name...
                if t.len() < 6 || !t[0].starts_with('-') {
                    continue;
                }
                let size = t[4].parse::<u64>().unwrap_or(0);
                let name = t[7.min(t.len())..].join(" ");
                if name.is_empty() {
                    continue;
                }
                files.push(serde_json::json!({
                    "dir": dir,
                    "name": name,
                    "size": size,
                    "path": format!("{}/{}", dir, name),
                }));
            }
        }
        Ok(serde_json::json!({ "files": files }))
    })
    .await
    .map_err(|e| format!("列取手机文件失败: {}", e))??;
    Ok(res)
}

/// 按用户勾选拉取手机文件到本地目录。
#[tauri::command]
pub async fn mirror_pull_files(
    _app: AppHandle,
    paths: Vec<String>,
    save_dir: String,
) -> Result<serde_json::Value, String> {
    if paths.is_empty() {
        return Err("未选择任何文件".into());
    }
    let (adb_path, serial) = {
        let guard = state().session.lock().unwrap();
        match guard.as_ref() {
            Some(s) => (s.adb_path.clone(), s.serial.clone()),
            None => return Err("投屏会话未运行".into()),
        }
    };
    let adb = adb_path.clone();
    let serial2 = serial.clone();
    let save = save_dir.clone();
    let list = paths.clone();
    let res = tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let mut ok: Vec<String> = Vec::new();
        let mut failed = 0usize;
        for p in list {
            let name = p.rsplit('/').next().unwrap_or("file").to_string();
            let local = std::path::Path::new(&save).join(&name);
            let local_str = local.to_string_lossy().to_string();
            match adb::run_adb(
                &adb,
                &["-s", &serial2, "pull", &p, &local_str],
                Duration::from_secs(300),
            ) {
                Ok(_) => ok.push(local_str),
                Err(e) => {
                    failed += 1;
                    log::warn!("[mirror] 拉取失败 {}: {}", p, e);
                }
            }
        }
        log::info!(
            "[mirror] 拉取完成: 成功 {} 失败 {} -> {}",
            ok.len(),
            failed,
            save
        );
        Ok(serde_json::json!({
            "exported": ok,
            "failed": failed,
            "saveDir": save,
        }))
    })
    .await
    .map_err(|e| format!("拉取任务失败: {}", e))??;
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
