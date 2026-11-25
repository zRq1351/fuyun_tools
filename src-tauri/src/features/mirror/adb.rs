//! adb 命令封装：定位可执行文件、推 server、建反向隧道、启动 server 进程。
//!
//! adb 与 scrcpy-server 随包分发（Apache-2.0，见 docs/scrcpy_LICENSE），
//! 同时兼容本机已安装的 Android SDK platform-tools。

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use std::time::Duration;
use tauri::Manager;

/// Windows 下不弹出控制台黑框。
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(windows)]
fn hide_console(cmd: &mut Command) {
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console(_cmd: &mut Command) {}

/// 依序探测 adb：随包资源 → PATH → 常见 SDK 位置。
pub fn resolve_adb(app: Option<&tauri::AppHandle>) -> Result<PathBuf, String> {
    if let Some(app) = app {
        if let Ok(dir) = app.path().resource_dir() {
            // 打包器把 "../third_party/*" 安装到 resource_dir/_up_/third_party/；
            // 开发态可能是 resource_dir/third_party/。
            let candidates = [
                dir.join("_up_")
                    .join("third_party")
                    .join("scrcpy")
                    .join("adb.exe"),
                dir.join("third_party").join("scrcpy").join("adb.exe"),
                dir.join("adb.exe"),
            ];
            for cand in candidates {
                if cand.exists() {
                    return Ok(cand);
                }
            }
        }
    }

    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let cand = dir.join("adb.exe");
            if cand.exists() {
                return Ok(cand);
            }
        }
    }

    let known = [
        r"D:\Android\Sdk\platform-tools\adb.exe",
        r"C:\Android\Sdk\platform-tools\adb.exe",
        r"C:\Program Files\Android\android-sdk\platform-tools\adb.exe",
    ];
    for cand in known {
        let p = PathBuf::from(cand);
        if p.exists() {
            return Ok(p);
        }
    }

    Err("未找到 adb.exe（应随应用分发）".to_string())
}

pub fn run_adb(adb: &std::path::Path, args: &[&str], timeout: Duration) -> Result<String, String> {
    let adb = adb.to_path_buf();
    let args_msg = format!("adb {}", args.join(" "));
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut cmd = Command::new(&adb);
        cmd.args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        hide_console(&mut cmd);
        let output = cmd.output();
        let _ = tx.send(output);
    });
    let output = rx
        .recv_timeout(timeout)
        .map_err(|_| {
            log::error!("[mirror] {} 超时", args_msg);
            format!("{} 超时", args_msg)
        })?
        .map_err(|e| {
            log::error!("[mirror] 启动 adb 失败: {}", e);
            format!("启动 adb 失败: {}", e)
        })?;

    if !output.status.success() {
        let msg = format!(
            "{} 失败({}): {}",
            args_msg,
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
        log::error!("[mirror] {}", msg);
        return Err(msg);
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// 列出处于 device 状态的序列号。
pub fn list_device_serials(adb: &std::path::Path) -> Result<Vec<String>, String> {
    let out = run_adb(adb, &["devices"], Duration::from_secs(5))?;
    let serials = out
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let serial = parts.next()?;
            let state = parts.next()?;
            (state == "device").then(|| serial.to_string())
        })
        .collect();
    Ok(serials)
}

/// 推送 scrcpy-server 到设备。
pub fn push_server(
    adb: &std::path::Path,
    serial: &str,
    jar: &std::path::Path,
) -> Result<(), String> {
    let jar_str = jar.to_string_lossy().to_string();
    run_adb(
        adb,
        &[
            "-s",
            serial,
            "push",
            &jar_str,
            "/data/local/tmp/scrcpy-server.jar",
        ],
        Duration::from_secs(15),
    )
    .map(|_| ())
}

/// 建立反向隧道：设备连接 localabstract:`socket_name` 时转发到本机 `tcp_port`。
pub fn enable_reverse(
    adb: &std::path::Path,
    serial: &str,
    socket_name: &str,
    tcp_port: u16,
) -> Result<(), String> {
    let name = format!("localabstract:{}", socket_name);
    let port = format!("tcp:{}", tcp_port);
    run_adb(
        adb,
        &["-s", serial, "reverse", &name, &port],
        Duration::from_secs(5),
    )
    .map(|_| ())
}

/// 移除反向隧道（静默失败，隧道可能已随 adb 掉线消失）。
pub fn disable_reverse(adb: &std::path::Path, serial: &str, socket_name: &str) {
    let name = format!("localabstract:{}", socket_name);
    let _ = run_adb(
        adb,
        &["-s", serial, "reverse", "--remove", &name],
        Duration::from_secs(5),
    );
}

/// 启动 scrcpy server 进程（长驻，返回 Child 以便停止时 kill）。
pub fn start_server(
    adb: &std::path::Path,
    serial: &str,
    options: &str,
    log_path: Option<&PathBuf>,
) -> Result<Child, String> {
    let shell_cmd = format!(
        "CLASSPATH=/data/local/tmp/scrcpy-server.jar app_process / com.genymobile.scrcpy.Server {} {}",
        crate::features::mirror::SCRCPY_VERSION,
        options
    );
    let (stdout, stderr) = match log_path {
        Some(path) => (
            Stdio::from(std::fs::File::create(path).map_err(|e| e.to_string())?),
            Stdio::null(),
        ),
        None => (Stdio::null(), Stdio::null()),
    };
    let mut cmd = Command::new(adb);
    cmd.args(["-s", serial, "shell", &shell_cmd])
        .stdout(stdout)
        .stderr(stderr);
    hide_console(&mut cmd);
    cmd.spawn().map_err(|e| {
        log::error!("[mirror] 启动 scrcpy server 失败: {}", e);
        format!("启动 scrcpy server 失败: {}", e)
    })
}

/// 在随机端口上监听本机回环，等待 scrcpy server 反向连入。
pub fn bind_local_listener() -> Result<(TcpListener, u16), String> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    Ok((listener, port))
}

/// 切换设备到 TCP/IP 调试模式（默认端口 5555）。要求 USB 已连接。
pub fn enable_tcpip(adb: &std::path::Path, serial: &str) -> Result<(), String> {
    run_adb(adb, &["-s", serial, "tcpip", "5555"], Duration::from_secs(10)).map(|_| ())
}

/// 读取设备 wlan0 的 IPv4 地址。
pub fn get_wlan_ip(adb: &std::path::Path, serial: &str) -> Result<String, String> {
    let out = run_adb(
        adb,
        &["-s", serial, "shell", "ip", "-f", "inet", "-o", "addr", "show", "wlan0"],
        Duration::from_secs(10),
    )?;
    // 行如: wlan0  inet 192.168.0.104/24 brd ... scope global ...
    for line in out.lines() {
        if let Some(idx) = line.find("inet ") {
            let rest = &line[idx + 5..];
            if let Some(end) = rest.find('/') {
                let ip = rest[..end].trim();
                if ip.split('.').count() == 4 && !ip.starts_with("127.") {
                    return Ok(ip.to_string());
                }
            }
        }
    }
    Err("未能获取手机 WiFi IP（wlan0 未连接？）".into())
}

/// adb connect 到无线设备（ip:port）。返回原始输出（含成功/失败信息）。
pub fn connect_wireless(adb: &std::path::Path, address: &str) -> Result<String, String> {
    run_adb(adb, &["connect", address], Duration::from_secs(15))
}

/// adb pair（Android 11+ 无线调试配对）。address 为手机显示的配对地址端口。
pub fn pair_wireless(adb: &std::path::Path, address: &str, code: &str) -> Result<String, String> {
    run_adb(adb, &["pair", address, code], Duration::from_secs(15))
}
