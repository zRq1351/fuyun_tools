//! 投屏会话：与 scrcpy-server 的完整生命周期。
//!
//! 流程（scrcpy 4.0+ 反向隧道协议，参考 doc/develop.md 与 DesktopConnection.java）：
//! 1. 本机随机端口监听 + `adb reverse localabstract:scrcpy_<scid>` 建立隧道
//! 2. `adb shell app_process ...` 启动 server（audio=false → 两个 socket：video、control）
//! 3. 依次 accept：video socket（先读 64 字节设备名）→ control socket
//! 4. video 线程解析帧头，H264 负载经 Hub 广播给投屏窗口
//! 5. control 写线程把 viewer 发来的控制指令写给设备；读线程消费设备回传消息（剪贴板等）

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use tauri::Manager;

use super::adb;
use super::protocol::{self, VideoHeader};
use super::relay::Hub;

/// 与随包 scrcpy-server 版本严格一致（server 校验，无兼容窗口）
pub const SCRCPY_VERSION: &str = "5.0.1";
/// server 参数里限制视频长边的像素
const MAX_SIZE: u32 = 1600;
/// 视频码率
const BIT_RATE: u32 = 8_000_000;
/// 帧率上限
const MAX_FPS: u32 = 30;
const DEVICE_NAME_LEN: usize = 64;

pub struct SessionHandle {
    pub hub: Hub,
    pub device_name: String,
    stop_flag: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
    pub adb_path: PathBuf,
    pub serial: String,
    socket_name: String,
    pub control_tx: mpsc::Sender<Vec<u8>>,
}

impl SessionHandle {
    /// 停止会话：杀 server 进程、撤隧道、广播结束事件。
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        if let Ok(mut guard) = self.child.lock() {
            if let Some(child) = guard.as_mut() {
                let _ = child.kill();
                let _ = child.wait();
            }
            *guard = None;
        }
        adb::disable_reverse(&self.adb_path, &self.serial, &self.socket_name);
        self.hub
            .send_text(serde_json::json!({"type": "stopped", "reason": "stopped"}).to_string());
    }

    /// 发送控制指令（协议字节）到设备。
    pub fn send_control(&self, bytes: Vec<u8>) {
        let _ = self.control_tx.send(bytes);
    }
}

pub struct StartOutcome {
    pub handle: SessionHandle,
}

/// 启动一次完整会话。阻塞调用（含 adb 命令与 socket 握手），应在线程池中执行。
/// `preferred_serial`：优先使用的设备序列号（无线投屏时为 `ip:port`）。
pub fn start(
    app: &tauri::AppHandle,
    hub: Hub,
    adb_path: std::path::PathBuf,
    jar_path: std::path::PathBuf,
    preferred_serial: Option<String>,
) -> Result<SessionHandle, String> {
    let serials = adb::list_device_serials(&adb_path)?;
    log::info!("[mirror] 发现设备: {:?}", serials);
    if serials.is_empty() {
        log::error!("[mirror] 未检测到已连接的安卓设备");
        return Err("未检测到已连接的安卓设备（请用 USB 连接并授权调试，或先建立无线连接）".into());
    }
    let serial = match preferred_serial {
        Some(ref want) if serials.iter().any(|s| s == want) => want.clone(),
        Some(ref want) => {
            return Err(format!(
                "指定的设备 {} 未连接（当前已连接: {}）",
                want,
                serials.join(", ")
            ));
        }
        None => match serials.len() {
            1 => serials[0].clone(),
            _ => {
                log::error!("[mirror] 检测到 {} 台设备，暂不支持自动选择", serials.len());
                return Err(format!(
                    "检测到多台设备（{} 台），请在连接时指定",
                    serials.len()
                ));
            }
        },
    };

    let scid: u32 = rand_u31();
    let socket_name = format!("scrcpy_{:08x}", scid);
    let (listener, local_port) = adb::bind_local_listener()?;
    log::info!(
        "[mirror] 本地监听 127.0.0.1:{} socket={}",
        local_port,
        socket_name
    );

    adb::push_server(&adb_path, &serial, &jar_path)?;
    adb::enable_reverse(&adb_path, &serial, &socket_name, local_port)?;

    // server 的 stdout/stderr 落盘：握手失败时有现场可查
    let server_log = app.path().app_log_dir().ok().map(|dir| {
        let _ = std::fs::create_dir_all(&dir);
        dir.join("scrcpy_server.log")
    });

    // 注意：codec option 值按整数解析（不要带 "s" 后缀）。
    // i-frame-interval=2：viewer 晚接入错过首个关键帧时最多 2 秒即可等到下一个。
    let options = format!(
        "scid={:08x} log_level=info video=true audio=false control=true \
         max_size={} video_bit_rate={} max_fps={} \
         video_codec_options=i-frame-interval=2 \
         send_dummy_byte=false send_device_meta=true send_frame_meta=true \
         send_stream_meta=true tunnel_forward=false",
        scid, MAX_SIZE, BIT_RATE, MAX_FPS
    );
    let child = adb::start_server(&adb_path, &serial, &options, server_log.as_ref())?;
    let child = Arc::new(Mutex::new(Some(child)));

    // 失败清理所需的独立副本（build 闭包会把原值移入 SessionHandle）
    let cleanup_adb = adb_path.clone();
    let cleanup_serial = serial.clone();
    let cleanup_socket = socket_name.clone();
    let cleanup_child = child.clone();

    let build = || -> Result<SessionHandle, String> {
        // 依次 accept：video → control（audio=false 时只有这两个）
        let mut video =
            accept_with_timeout(&listener, Duration::from_secs(15)).ok_or_else(|| {
                log::error!("[mirror] 等待设备视频连接超时（15s）");
                "等待设备视频连接超时（server 未响应，可查看日志）".to_string()
            })?;
        let mut control =
            accept_with_timeout(&listener, Duration::from_secs(10)).ok_or_else(|| {
                log::error!("[mirror] 等待设备控制连接超时（10s）");
                "等待设备控制连接超时".to_string()
            })?;
        video.set_nodelay(true).ok();
        control.set_nodelay(true).ok();
        // 读（设备回传）与写（控制指令）分离，避免两个线程争抢同一句柄
        let mut control_read = control
            .try_clone()
            .map_err(|e| format!("克隆控制连接失败: {}", e))?;

        // 设备元数据：64 字节 UTF-8 设备名（仅在第一个 socket 上发送）
        let device_name = read_device_meta(&mut video)?;

        let (control_tx, control_rx) = mpsc::channel::<Vec<u8>>();
        let stop_flag = Arc::new(AtomicBool::new(false));

        hub.send_text(serde_json::json!({"type": "device", "name": device_name}).to_string());

        // video 读线程：解析帧头 → 广播 H264 负载
        {
            let hub = hub.clone();
            let stop_flag = stop_flag.clone();
            std::thread::Builder::new()
                .name("mirror-video-reader".into())
                .spawn(move || {
                    run_video_reader(&mut video, &hub, &stop_flag);
                })
                .map_err(|e| e.to_string())?;
        }

        // 设备回传消息消费线程（转发剪贴板回显，其余按格式丢弃保证字节流不错位）
        {
            let hub = hub.clone();
            let stop_flag = stop_flag.clone();
            std::thread::Builder::new()
                .name("mirror-device-msg-reader".into())
                .spawn(move || {
                    run_device_msg_reader(&mut control_read, &stop_flag, &hub);
                })
                .map_err(|e| e.to_string())?;
        }

        // 控制写线程：写失败必须广播（否则画面还在、操作全失效，用户无从感知）
        {
            let stop_flag = stop_flag.clone();
            let hub = hub.clone();
            std::thread::Builder::new()
                .name("mirror-control-writer".into())
                .spawn(move || {
                    while !stop_flag.load(Ordering::SeqCst) {
                        match control_rx.recv_timeout(Duration::from_millis(500)) {
                            Ok(bytes) => {
                                if control.write_all(&bytes).is_err() {
                                    log::error!(
                                        "[mirror] 控制链路写入失败，操作通道已断（画面流可能仍存活）"
                                    );
                                    if !stop_flag.load(Ordering::SeqCst) {
                                        hub.send_text(
                                            r#"{"type":"stopped","reason":"control-writer"}"#
                                                .into(),
                                        );
                                    }
                                    break;
                                }
                            }
                            Err(mpsc::RecvTimeoutError::Timeout) => continue,
                            Err(_) => break,
                        }
                    }
                })
                .map_err(|e| e.to_string())?;
        }

        Ok(SessionHandle {
            hub,
            device_name,
            stop_flag,
            child,
            adb_path,
            serial,
            socket_name,
            control_tx,
        })
    };

    match build() {
        Ok(handle) => {
            log::info!(
                "[mirror] 会话握手完成: 设备={} 本地端口={}",
                handle.device_name,
                local_port
            );
            Ok(handle)
        }
        Err(e) => {
            log::error!(
                "[mirror] 会话建立失败: {}（server 输出见 scrcpy_server.log）",
                e
            );
            // 失败清理：撤隧道 + 杀 server 进程
            adb::disable_reverse(&cleanup_adb, &cleanup_serial, &cleanup_socket);
            if let Ok(mut guard) = cleanup_child.lock() {
                if let Some(c) = guard.as_mut() {
                    let _ = c.kill();
                    let _ = c.wait();
                }
                *guard = None;
            }
            Err(e)
        }
    }
}

fn rand_u31() -> u32 {
    // 简单取熵：时间 + 地址，满足“同设备多实例不撞名”即可
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let pid = std::process::id();
    (t ^ (pid.wrapping_mul(2654435761))) & 0x7FFF_FFFF_u32
}

fn accept_with_timeout(listener: &std::net::TcpListener, timeout: Duration) -> Option<TcpStream> {
    if listener.set_nonblocking(true).is_err() {
        return None;
    }
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = listener.set_nonblocking(false);
                // Windows 上 accept 出的 socket 会继承监听 socket 的非阻塞状态，
                // 必须显式恢复阻塞，否则后续 read 会立刻返回 WSAEWOULDBLOCK
                let _ = stream.set_nonblocking(false);
                return Some(stream);
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if std::time::Instant::now() > deadline {
                    let _ = listener.set_nonblocking(false);
                    return None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => {
                let _ = listener.set_nonblocking(false);
                return None;
            }
        }
    }
}

fn read_exact_loop(stream: &mut TcpStream, buf: &mut [u8]) -> std::io::Result<()> {
    let mut filled = 0;
    while filled < buf.len() {
        let n = stream.read(&mut buf[filled..])?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "stream closed",
            ));
        }
        filled += n;
    }
    Ok(())
}

fn read_device_meta(video: &mut TcpStream) -> Result<String, String> {
    let mut buf = [0u8; DEVICE_NAME_LEN];
    if let Err(e) = read_exact_loop(video, &mut buf) {
        log::error!("[mirror] 读取设备元数据失败: {}", e);
        return Err(format!("读取设备元数据失败: {}", e));
    }
    let end = buf.iter().position(|b| *b == 0).unwrap_or(DEVICE_NAME_LEN);
    Ok(String::from_utf8_lossy(&buf[..end]).to_string())
}

fn run_video_reader(video: &mut TcpStream, hub: &Hub, stop_flag: &AtomicBool) {
    let mut header = [0u8; 12];
    // 1. codec id（u32）
    if let Err(e) = read_exact_loop(video, &mut header[0..4]) {
        log::warn!("[mirror] 读 codec id 失败: {}", e);
        hub.send_text(r#"{"type":"stopped","reason":"stream-open"}"#.into());
        return;
    }
    let codec = &header[0..4];
    log::warn!("[mirror] codec id: {:02x?}", codec);
    hub.send_text(
        serde_json::json!({
            "type": "codec",
            "id": String::from_utf8_lossy(codec),
        })
        .to_string(),
    );

    // 2. 帧循环
    let mut frame_count = 0u64;
    loop {
        if stop_flag.load(Ordering::SeqCst) {
            return;
        }
        if let Err(e) = read_exact_loop(video, &mut header) {
            log::warn!(
                "[mirror] video reader 退出（帧头读取失败，已读 {} 帧）: {}",
                frame_count,
                e
            );
            break;
        }
        match protocol::parse_video_header(&header) {
            VideoHeader::Session { width, height } => {
                log::warn!("[mirror] session meta: {}x{}", width, height);
                hub.send_text(
                    serde_json::json!({"type": "session", "width": width, "height": height})
                        .to_string(),
                );
            }
            VideoHeader::Media {
                config,
                key_frame,
                size,
                ..
            } => {
                if size == 0 || size > 16 * 1024 * 1024 {
                    log::warn!(
                        "[mirror] video reader 退出：异常帧大小 {}（第 {} 帧, key={} config={}）",
                        size,
                        frame_count,
                        key_frame,
                        config
                    );
                    break; // 协议错位或异常帧
                }
                let mut payload = vec![0u8; size as usize];
                if let Err(e) = read_exact_loop(video, &mut payload) {
                    log::warn!(
                        "[mirror] video reader 退出（帧负载读取失败，已读 {} 帧）: {}",
                        frame_count,
                        e
                    );
                    break;
                }
                frame_count += 1;
                if frame_count <= 3 || frame_count.is_multiple_of(300) {
                    log::warn!(
                        "[mirror] 帧 #{}: size={} key={} config={}",
                        frame_count,
                        size,
                        key_frame,
                        config
                    );
                }
                // 信封：byte0 bit0=config bit1=keyframe
                let mut flags = 0u8;
                if config {
                    flags |= 1;
                }
                if key_frame {
                    flags |= 2;
                }
                let mut envelope = Vec::with_capacity(payload.len() + 1);
                envelope.push(flags);
                envelope.extend_from_slice(&payload);
                hub.send_binary(envelope);
            }
        }
    }
    if !stop_flag.load(Ordering::SeqCst) {
        hub.send_text(r#"{"type":"stopped","reason":"stream-eof"}"#.into());
    }
}

/// 设备 → client 的消息：type u8；CLIPBOARD(0)=len u32+data；ACK_CLIPBOARD(1)=i64；
/// UHID_OUTPUT(2)=u16 id + u16 len + data。
/// CLIPBOARD 转发为中继文本消息（autosync 下设备剪贴板变化会自动回传）。
fn run_device_msg_reader(control: &mut TcpStream, stop_flag: &AtomicBool, hub: &Hub) {
    let mut u32_buf = [0u8; 4];
    let mut u16_buf = [0u8; 2];
    let mut u8_buf = [0u8; 1];
    let mut i64_buf = [0u8; 8];
    let mut msg_count = 0u64;
    let mut abnormal = false;
    while !stop_flag.load(Ordering::SeqCst) {
        if let Err(e) = read_exact_loop(control, &mut u8_buf) {
            log::warn!(
                "[mirror] control reader 退出（读消息类型失败，已收 {} 条）: {}",
                msg_count,
                e
            );
            abnormal = true;
            break;
        }
        msg_count += 1;
        match u8_buf[0] {
            0 => {
                if read_exact_loop(control, &mut u32_buf).is_err() {
                    log::warn!("[mirror] control reader: CLIPBOARD 长度读取失败");
                    abnormal = true;
                    break;
                }
                let len = u32::from_be_bytes(u32_buf) as usize;
                let mut sink = vec![0u8; len];
                if read_exact_loop(control, &mut sink).is_err() {
                    log::warn!(
                        "[mirror] control reader: CLIPBOARD 数据读取失败 (len={})",
                        len
                    );
                    abnormal = true;
                    break;
                }
                let text = String::from_utf8_lossy(&sink).to_string();
                log::warn!("[mirror] 收到设备剪贴板 ({} 字节): {}", len, text);
                hub.send_text(serde_json::json!({ "type": "clipboard", "text": text }).to_string());
            }
            1 => {
                if read_exact_loop(control, &mut i64_buf).is_err() {
                    log::warn!("[mirror] control reader: ACK 读取失败");
                    abnormal = true;
                    break;
                }
            }
            2 => {
                if read_exact_loop(control, &mut u16_buf).is_err() {
                    log::warn!("[mirror] control reader: UHID id 读取失败");
                    abnormal = true;
                    break;
                }
                if read_exact_loop(control, &mut u16_buf).is_err() {
                    log::warn!("[mirror] control reader: UHID len 读取失败");
                    abnormal = true;
                    break;
                }
                let len = u16::from_be_bytes(u16_buf) as usize;
                let mut sink = vec![0u8; len];
                if read_exact_loop(control, &mut sink).is_err() {
                    log::warn!("[mirror] control reader: UHID 数据读取失败 (len={})", len);
                    abnormal = true;
                    break;
                }
            }
            other => {
                log::warn!(
                    "[mirror] control reader: 未知消息类型 {}，流错位退出",
                    other
                );
                abnormal = true;
                break;
            }
        }
    }
    // 非主动停止时退出 = 控制链路异常断开，广播给 viewer 触发自动恢复
    if abnormal && !stop_flag.load(Ordering::SeqCst) {
        hub.send_text(r#"{"type":"stopped","reason":"control-reader"}"#.into());
    }
}
