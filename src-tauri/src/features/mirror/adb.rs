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

/// 结束 adb 守护进程（应用退出 / 安装升级前调用）。
/// adb.exe 会以守护进程形式常驻，锁住安装目录下 `_up_/third_party/scrcpy/*.dll`，
/// 导致下次覆盖安装报 "Error opening file for writing"（2026-11-28 实测踩坑）。
pub fn kill_server(app: Option<&tauri::AppHandle>) {
    let Ok(adb) = resolve_adb(app) else {
        return;
    };
    let mut cmd = Command::new(&adb);
    cmd.arg("kill-server");
    hide_console(&mut cmd);
    match cmd.output() {
        Ok(_) => log::info!("已停止 adb 守护进程"),
        Err(e) => log::warn!("停止 adb 守护进程失败: {}", e),
    }
}

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

/// 读取设备硬件序列号（ro.serialno）。同一台手机无线连接返回的值与 USB 序列号一致，
/// 可用于识别「USB + 无线双连接」实为同一台设备。
pub fn device_serialno(adb: &std::path::Path, serial: &str) -> Result<String, String> {
    let out = run_adb(
        adb,
        &["-s", serial, "shell", "getprop", "ro.serialno"],
        Duration::from_secs(5),
    )?;
    let s = out.trim().to_string();
    if s.is_empty() {
        Err("读取 ro.serialno 为空".into())
    } else {
        Ok(s)
    }
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
    run_adb(
        adb,
        &["-s", serial, "tcpip", "5555"],
        Duration::from_secs(10),
    )
    .map(|_| ())
}

/// 等待指定 serial 回到 device 状态。
/// `adb tcpip` 会重启 adbd，设备会在 adb 列表中短暂离线或消失（数秒），必须等它回来。
pub fn wait_for_device(
    adb: &std::path::Path,
    serial: &str,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Ok(list) = list_device_serials(adb) {
            if list.iter().any(|s| s == serial) {
                return Ok(());
            }
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!("设备 {} 未重新上线（等待超时）", serial));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// 读取设备 wlan0 的 IPv4 地址。
pub fn get_wlan_ip(adb: &std::path::Path, serial: &str) -> Result<String, String> {
    let out = run_adb(
        adb,
        &[
            "-s", serial, "shell", "ip", "-f", "inet", "-o", "addr", "show", "wlan0",
        ],
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

/// 解析 dumpsys display 输出中的显示器 id 集合。
/// 兼容 `mDisplayId=N` 与 `Display N:` 两种行格式。
pub fn parse_display_ids(out: &str) -> std::collections::HashSet<u32> {
    let mut ids = std::collections::HashSet::new();
    for line in out.lines() {
        for marker in ["mDisplayId=", "Display "] {
            if let Some(idx) = line.find(marker) {
                let rest = &line[idx + marker.len()..];
                let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(v) = num.parse::<u32>() {
                    ids.insert(v);
                }
            }
        }
    }
    ids
}

/// 列出设备当前的显示器 id（虚拟显示器定位用）。
pub fn list_display_ids(
    adb: &std::path::Path,
    serial: &str,
) -> Result<std::collections::HashSet<u32>, String> {
    let out = run_adb(
        adb,
        &["-s", serial, "shell", "dumpsys", "display"],
        Duration::from_secs(10),
    )?;
    Ok(parse_display_ids(&out))
}

/// 解析应用的可启动 Activity（`cmd package resolve-activity --brief` 输出的最后一个 pkg/activity 行）。
pub fn resolve_launch_activity(
    adb: &std::path::Path,
    serial: &str,
    package: &str,
) -> Result<String, String> {
    let out = run_adb(
        adb,
        &[
            "-s",
            serial,
            "shell",
            "cmd",
            "package",
            "resolve-activity",
            "--brief",
            "-c",
            "android.intent.category.LAUNCHER",
            package,
        ],
        Duration::from_secs(10),
    )?;
    out.lines()
        .rev()
        .map(str::trim)
        .find(|l| l.contains('/') && l.contains('.'))
        .map(str::to_string)
        .ok_or_else(|| {
            format!(
                "未找到 {} 的可启动 Activity（包名无效或应用无启动入口）",
                package
            )
        })
}

/// 在指定显示器上启动 Activity（应用投屏核心：虚拟显示器 + am start --display）。
pub fn am_start_on_display(
    adb: &std::path::Path,
    serial: &str,
    display_id: u32,
    component: &str,
) -> Result<String, String> {
    run_adb(
        adb,
        &[
            "-s",
            serial,
            "shell",
            "am",
            "start",
            "--display",
            &display_id.to_string(),
            "-n",
            component,
        ],
        Duration::from_secs(15),
    )
}

/// 列出第三方应用包名（`pm list packages -3`，去掉 `package:` 前缀并排序）。
pub fn list_third_party_packages(
    adb: &std::path::Path,
    serial: &str,
) -> Result<Vec<String>, String> {
    let out = run_adb(
        adb,
        &["-s", serial, "shell", "pm", "list", "packages", "-3"],
        Duration::from_secs(15),
    )?;
    let mut pkgs: Vec<String> = out
        .lines()
        .map(str::trim)
        .filter_map(|l| l.strip_prefix("package:"))
        .map(str::to_string)
        .collect();
    pkgs.sort();
    Ok(pkgs)
}

/// 从 dumpsys activity activities 输出解析前台应用包名。
/// 取 ResumedActivity 行中 `包名/Activity` 形式的 token（如 com.tencent.mm/.ui.LauncherUI）。
pub fn parse_foreground_package(out: &str) -> Option<String> {
    for line in out.lines() {
        if !line.contains("ResumedActivity") {
            continue;
        }
        for token in line.split_whitespace() {
            if let Some(pos) = token.find('/') {
                let pkg = &token[..pos];
                let first = pkg.chars().next();
                if pkg.contains('.') && first.is_some_and(|c| c.is_ascii_alphabetic()) {
                    return Some(pkg.to_string());
                }
            }
        }
    }
    None
}

/// 读取设备当前前台应用包名。
pub fn foreground_package(adb: &std::path::Path, serial: &str) -> Result<String, String> {
    let out = run_adb(
        adb,
        &["-s", serial, "shell", "dumpsys", "activity", "activities"],
        Duration::from_secs(10),
    )?;
    parse_foreground_package(&out).ok_or_else(|| "未能识别手机当前前台应用".to_string())
}

/// 读取主屏分辨率与密度（应用投屏虚拟显示器尺寸用）。Override 值优先于 Physical。
pub fn wm_display_metrics(adb: &std::path::Path, serial: &str) -> Result<(u32, u32, u32), String> {
    let size_out = run_adb(
        adb,
        &["-s", serial, "shell", "wm", "size"],
        Duration::from_secs(10),
    )?;
    let mut wh: Option<(u32, u32)> = None;
    for line in size_out.lines() {
        let t = line.trim();
        let rest = if let Some(r) = t.strip_prefix("Override size: ") {
            r
        } else if let Some(r) = t.strip_prefix("Physical size: ") {
            r
        } else {
            continue;
        };
        let mut it = rest.trim().split('x');
        if let (Some(pw), Some(ph)) = (it.next(), it.next()) {
            if let (Ok(w), Ok(h)) = (pw.trim().parse::<u32>(), ph.trim().parse::<u32>()) {
                // 行序 Physical → Override，后写覆盖实现 Override 优先
                wh = Some((w, h));
            }
        }
    }
    let density_out = run_adb(
        adb,
        &["-s", serial, "shell", "wm", "density"],
        Duration::from_secs(10),
    )?;
    let mut dpi: Option<u32> = None;
    for line in density_out.lines() {
        let t = line.trim();
        let rest = if let Some(r) = t.strip_prefix("Override density: ") {
            r
        } else if let Some(r) = t.strip_prefix("Physical density: ") {
            r
        } else {
            continue;
        };
        if let Ok(v) = rest.trim().parse::<u32>() {
            dpi = Some(v);
        }
    }
    match (wh, dpi) {
        (Some((w, h)), Some(dpi)) => Ok((w, h, dpi)),
        _ => Err("wm size/density 输出无法解析".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_display_ids;

    #[test]
    fn parse_dumpsys_display_ids() {
        let out = "Displays: size=2\n  Display 0:\n    mDisplayId=0\n  Display 2:\n    mDisplayId=2\nDisplay Devices: size=1\n";
        let ids = parse_display_ids(out);
        assert!(ids.contains(&0) && ids.contains(&2));
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn parse_empty_output() {
        assert!(parse_display_ids("").is_empty());
    }
}

#[cfg(test)]
mod fg_tests {
    use super::parse_foreground_package;

    #[test]
    fn parse_resumed_activity_line() {
        let out = "  some noise\n  ResumedActivity: ActivityRecord{1234abcd u0 com.tencent.mm/.ui.LauncherUI t123}\n";
        assert_eq!(
            parse_foreground_package(out),
            Some("com.tencent.mm".to_string())
        );
    }

    #[test]
    fn parse_returns_none_when_absent() {
        assert!(parse_foreground_package("no resumed info here").is_none());
    }
}
