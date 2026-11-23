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
/// 关键帧间隔（秒）：viewer 中途加入最多等这么久
const I_FRAME_INTERVAL_SECS: u32 = 2;
const DEVICE_NAME_LEN: usize = 64;

pub struct SessionHandle {
    pub hub: Hub,
    pub device_name: String,
    stop_flag: Arc<AtomicBool>,
    child: Mutex<Option<Child>>,
    adb_path: PathBuf,
    serial: String,
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
        self.hub.send_text(
            serde_json::json!({"type": "stopped", "reason": "stopped"}).to_string(),
        );
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
pub fn start(
    _app: &tauri::AppHandle,
    hub: Hub,
    adb_path: std::path::PathBuf,
    jar_path: std::path::PathBuf,
) -> Result<SessionHandle, String> {
    let serials = adb::list_device_serials(&adb_path)?;
    let serial = match serials.len() {
        0 => return Err("未检测到已连接的安卓设备（请用 USB 连接并授权调试）".into()),
        1 => serials[0].clone(),
        _ => return Err(format!("检测到多台设备（{} 台），暂不支持选择", serials.len())),
    };

    let scid: u32 = rand_u31();
    let socket_name = format!("scrcpy_{:08x}", scid);
    let (listener, local_port) = adb::bind_local_listener()?;

    adb::push_server(&adb_path, &serial, &jar_path)?;
    adb::enable_reverse(&adb_path, &serial, &socket_name, local_port)?;

    let options = format!(
        "scid={:08x} log_level=info video=true audio=false control=true \
         max_size={} video_bit_rate={} max_fps={} \
         video_codec_options=i-frame-interval={}s \
         send_dummy_byte=false send_device_meta=true send_frame_meta=true \
         send_stream_meta=true tunnel_forward=false",
        scid, MAX_SIZE, BIT_RATE, MAX_FPS, I_FRAME_INTERVAL_SECS
    );
    let child = adb::start_server(&adb_path, &serial, &options, None)?;

    // 依次 accept：video → control（audio=false 时只有这两个）
    let mut video = accept_with_timeout(&listener, Duration::from_secs(15))
        .ok_or("等待设备视频连接超时（server 未响应，可查看日志）")?;
    let mut control = accept_with_timeout(&listener, Duration::from_secs(10))
        .ok_or("等待设备控制连接超时")?;
    video.set_nodelay(true).ok();
    control.set_nodelay(true).ok();
    // 读（设备回传）与写（控制指令）分离，避免两个线程争抢同一句柄
    let mut control_read = control.try_clone().map_err(|e| format!("克隆控制连接失败: {}", e))?;

    // 设备元数据：64 字节 UTF-8 设备名（仅在第一个 socket 上发送）
    let device_name = read_device_meta(&mut video)?;

    let (control_tx, control_rx) = mpsc::channel::<Vec<u8>>();
    let stop_flag = Arc::new(AtomicBool::new(false));

    hub.send_text(
        serde_json::json!({"type": "device", "name": device_name}).to_string(),
    );

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

    // 设备回传消息消费线程（v1 仅按格式丢弃，保证字节流不错位）
    {
        let stop_flag = stop_flag.clone();
        std::thread::Builder::new()
            .name("mirror-device-msg-reader".into())
            .spawn(move || {
                run_device_msg_reader(&mut control_read, &stop_flag);
            })
            .map_err(|e| e.to_string())?;
    }

    // 控制写线程
    {
        let stop_flag = stop_flag.clone();
        std::thread::Builder::new()
            .name("mirror-control-writer".into())
            .spawn(move || {
                while !stop_flag.load(Ordering::SeqCst) {
                    match control_rx.recv_timeout(Duration::from_millis(500)) {
                        Ok(bytes) => {
                            if control.write_all(&bytes).is_err() {
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
        child: Mutex::new(Some(child)),
        adb_path,
        serial,
        socket_name,
        control_tx,
    })
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
    read_exact_loop(video, &mut buf).map_err(|e| format!("读取设备元数据失败: {}", e))?;
    let end = buf.iter().position(|b| *b == 0).unwrap_or(DEVICE_NAME_LEN);
    Ok(String::from_utf8_lossy(&buf[..end]).to_string())
}

fn run_video_reader(video: &mut TcpStream, hub: &Hub, stop_flag: &AtomicBool) {
    let mut header = [0u8; 12];
    // 1. codec id（u32）
    if read_exact_loop(video, &mut header[0..4]).is_err() {
        hub.send_text(r#"{"type":"stopped","reason":"stream-open"}"#.into());
        return;
    }
    let codec = &header[0..4];
    hub.send_text(
        serde_json::json!({
            "type": "codec",
            "id": String::from_utf8_lossy(codec),
        })
        .to_string(),
    );

    // 2. 帧循环
    loop {
        if stop_flag.load(Ordering::SeqCst) {
            return;
        }
        if read_exact_loop(video, &mut header).is_err() {
            break;
        }
        match protocol::parse_video_header(&header) {
            VideoHeader::Session { width, height } => {
                hub.send_text(
                    serde_json::json!({"type": "session", "width": width, "height": height})
                        .to_string(),
                );
            }
            VideoHeader::Media { config, key_frame, size, .. } => {
                if size == 0 || size > 16 * 1024 * 1024 {
                    break; // 协议错位或异常帧
                }
                let mut payload = vec![0u8; size as usize];
                if read_exact_loop(video, &mut payload).is_err() {
                    break;
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
/// UHID_OUTPUT(2)=u16 id + u16 len + data。v1 仅消费丢弃。
fn run_device_msg_reader(control: &mut TcpStream, stop_flag: &AtomicBool) {
    let mut u32_buf = [0u8; 4];
    let mut u16_buf = [0u8; 2];
    let mut u8_buf = [0u8; 1];
    let mut i64_buf = [0u8; 8];
    while !stop_flag.load(Ordering::SeqCst) {
        if read_exact_loop(control, &mut u8_buf).is_err() {
            return;
        }
        match u8_buf[0] {
            0 => {
                if read_exact_loop(control, &mut u32_buf).is_err() {
                    return;
                }
                let len = u32::from_be_bytes(u32_buf) as usize;
                let mut sink = vec![0u8; len];
                if read_exact_loop(control, &mut sink).is_err() {
                    return;
                }
                // v1：设备剪贴板同步暂不处理
            }
            1 => {
                if read_exact_loop(control, &mut i64_buf).is_err() {
                    return;
                }
            }
            2 => {
                if read_exact_loop(control, &mut u16_buf).is_err() {
                    return;
                }
                if read_exact_loop(control, &mut u16_buf).is_err() {
                    return;
                }
                let len = u16::from_be_bytes(u16_buf) as usize;
                let mut sink = vec![0u8; len];
                if read_exact_loop(control, &mut sink).is_err() {
                    return;
                }
            }
            _ => return, // 未知类型，流错位风险，直接退出
        }
    }
}
