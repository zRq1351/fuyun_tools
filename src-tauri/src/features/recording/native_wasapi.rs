use crate::core::error_codes::AppErrorKind;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat as CpalSampleFormat, StreamConfig};
use hound::SampleFormat;
use std::collections::HashMap;
use std::collections::HashSet;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
use sysinfo::{ProcessRefreshKind, RefreshKind, System};
use wasapi::{
    initialize_mta, AudioClient, DeviceEnumerator, Direction, SampleType, SessionState, StreamMode,
    WaveFormat,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

pub struct WasapiCaptureHandle {
    pub stop_flag: Arc<AtomicBool>,
    pub joins: Vec<std::thread::JoinHandle<()>>,
    pub output_path: PathBuf,
}

// 🔧 方案A：FFmpeg 实时 AAC 编码句柄
pub struct WasapiFfmpegHandle {
    pub stop_flag: Arc<AtomicBool>,
    pub join: Option<std::thread::JoinHandle<()>>,
    pub output_path: PathBuf,
    pub ffmpeg_child: Arc<Mutex<Option<ChildGuard>>>,
}

pub struct ChildGuard(pub Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Ok(None) = self.0.try_wait() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

/// 按设备 ID 选择设备。ID 格式为 "{描述}_{枚举索引}"（audio_device.rs 生成），
/// 同时兼容纯描述形式的旧 ID。匹配失败返回 None，由调用方回退默认设备。
fn pick_device_by_key(
    devices: impl IntoIterator<Item = cpal::Device>,
    key: &str,
) -> Option<cpal::Device> {
    let mut exact: Option<cpal::Device> = None;
    for (i, d) in devices.into_iter().enumerate() {
        if let Ok(desc) = d.description() {
            let desc = desc.to_string();
            if format!("{}_{}", desc, i) == key {
                return Some(d);
            }
            if desc == key && exact.is_none() {
                exact = Some(d);
            }
        }
    }
    exact
}

#[derive(Debug, Clone)]
pub struct AudioProcessInfo {
    pub pid: u32,
    pub name: String,
}

static AUDIO_RECENT_ACTIVITY: std::sync::OnceLock<Mutex<HashMap<u32, u64>>> =
    std::sync::OnceLock::new();
static COM_INIT: std::sync::Once = std::sync::Once::new();

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_else(|e| {
            log::warn!("SystemTime before UNIX_EPOCH: {}", e);
            0
        })
}

/// 全局音频写入错误计数器（每个录音会话重置）
static AUDIO_WRITE_ERR_COUNT: AtomicBool = AtomicBool::new(false);

/// 通用化音频写入回调：将任意 cpal 采样格式转为 i16 写入 WAV，受 enabled/pause 标志控制。
/// 标签（label）用于错误日志区分系统音频/麦克风。
macro_rules! audio_write_loop {
    ($writer_cb:expr, $enabled_cb:expr, $pause_cb:expr, $label:expr, $sample_type:ty) => {{
        let writer = $writer_cb.clone();
        let enabled = $enabled_cb.clone();
        let pause = $pause_cb.clone();
        move |data: &[$sample_type], _| {
            if let Ok(mut guard) = writer.lock() {
                if let Some(w) = guard.as_mut() {
                    let active = enabled.load(Ordering::SeqCst)
                        && !pause.load(Ordering::SeqCst);
                    for &v in data {
                        let s: i16 = if active { v.to_sample::<i16>() } else { 0 };
                        write_sample_or_log!(w, s, $label);
                    }
                }
            }
        }
    }};
}

/// 写入音频采样，失败时记录日志（避免日志洪水，仅记录前几次错误）
macro_rules! write_sample_or_log {
    ($writer:expr, $sample:expr, $context:expr) => {
        if let Err(e) = $writer.write_sample($sample) {
            if !AUDIO_WRITE_ERR_COUNT.swap(true, Ordering::Relaxed) {
                log::error!("音频写入失败({}): {}", $context, e);
            }
        }
    };
}

impl WasapiCaptureHandle {
    pub fn stop(self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        for join in self.joins {
            let _ = join.join();
        }
    }
}

fn capture_process_loopback_to_wav(
    process_id: u32,
    output_path: PathBuf,
    stop_flag: Arc<AtomicBool>,
    enabled_flag: Arc<AtomicBool>,
    recording_pause_flag: Arc<AtomicBool>,
    startup_tx: Option<mpsc::Sender<(u32, Result<(), String>)>>,
) -> Result<(), String> {
    AUDIO_WRITE_ERR_COUNT.store(false, Ordering::Relaxed);
    let run = || -> Result<(), String> {
        COM_INIT.call_once(|| {
            let _ = initialize_mta();
        });
        let desired_format = WaveFormat::new(32, 32, &SampleType::Float, 48000, 2, None);
        let mut audio_client = AudioClient::new_application_loopback_client(process_id, true)
            .map_err(|e| format!("创建进程 loopback 客户端失败(pid={}): {}", process_id, e))?;
        let mode = StreamMode::EventsShared {
            autoconvert: true,
            buffer_duration_hns: 0,
        };
        audio_client
            .initialize_client(&desired_format, &Direction::Capture, &mode)
            .map_err(|e| format!("初始化进程 loopback 失败(pid={}): {}", process_id, e))?;
        let event = audio_client
            .set_get_eventhandle()
            .map_err(|e| format!("创建进程 loopback 事件失败(pid={}): {}", process_id, e))?;
        let capture_client = audio_client
            .get_audiocaptureclient()
            .map_err(|e| format!("获取进程捕获客户端失败(pid={}): {}", process_id, e))?;
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };

        let file = std::fs::File::create(&output_path)
            .map_err(|e| format!("创建进程音频文件失败(pid={}): {}", process_id, e))?;
        let buf_writer = std::io::BufWriter::with_capacity(1024 * 1024, file);
        let mut writer = hound::WavWriter::new(buf_writer, spec)
            .map_err(|e| format!("初始化 WAV 写入器失败(pid={}): {}", process_id, e))?;

        if let Ok(meta) = std::fs::metadata(&output_path) {
            log::info!(
                "进程音频文件创建成功(pid={}): {:?}, 大小: {} bytes",
                process_id,
                output_path.file_name(),
                meta.len()
            );
        } else {
            log::warn!(
                "进程音频文件创建后检查失败(pid={}): {:?}",
                process_id,
                output_path.file_name()
            );
        }
        let mut queue = std::collections::VecDeque::<u8>::new();
        let blockalign = desired_format.get_blockalign() as usize;
        if blockalign == 0 {
            return Err(format!("无效的 blockalign: 0 (pid={})", process_id));
        }
        audio_client
            .start_stream()
            .map_err(|e| format!("启动进程 loopback 失败(pid={}): {}", process_id, e))?;
        if let Some(tx) = startup_tx.as_ref() {
            let _ = tx.send((process_id, Ok(())));
        }

        let mut active_time_ns: u64 = 0;
        let mut last_loop_time = std::time::Instant::now();
        let mut actual_total_samples: u64 = 0;

        while !stop_flag.load(Ordering::SeqCst) {
            let now = std::time::Instant::now();
            let dt_ns = now.duration_since(last_loop_time).as_nanos() as u64;
            last_loop_time = now;

            let is_paused = recording_pause_flag.load(Ordering::SeqCst);
            if !is_paused {
                active_time_ns += dt_ns;
            }

            let new_frames = capture_client
                .get_next_packet_size()
                .map_err(|e| {
                    log::warn!("进程音频包大小读取失败(pid={}): {}", process_id, e);
                    format!("读取进程音频包大小失败(pid={}): {}", process_id, e)
                })?
                .unwrap_or(0);
            if new_frames > 0 {
                capture_client
                    .read_from_device_to_deque(&mut queue)
                    .map_err(|e| {
                        log::warn!("进程音频数据读取失败(pid={}): {}", process_id, e);
                        format!("读取进程音频数据失败(pid={}): {}", process_id, e)
                    })?;
            }
            let enabled =
                enabled_flag.load(Ordering::SeqCst) && !recording_pause_flag.load(Ordering::SeqCst);

            let slices = queue.as_slices();
            let mut processed = 0;

            for slice in &[slices.0, slices.1] {
                let chunks = slice.chunks_exact(4);
                processed += chunks.len() * 4;
                for chunk in chunks {
                    let sample = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                    let out = if enabled {
                        (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
                    } else {
                        0
                    };
                    write_sample_or_log!(writer, out, "进程音频");
                    actual_total_samples += 1;
                }
            }
            queue.drain(..processed);

            if !is_paused {
                let expected_total_samples =
                    ((active_time_ns as f64 / 1_000_000_000.0) * 48000.0) as u64 * 2;
                if expected_total_samples > actual_total_samples {
                    let padding_needed = expected_total_samples - actual_total_samples;

                    if padding_needed > 4800 {
                        for _ in 0..padding_needed {
                            write_sample_or_log!(writer, 0i16, "进程音频静音填充");
                        }
                        actual_total_samples += padding_needed;
                    }
                }
            }

            if event.wait_for_event(50).is_err() {
                log::warn!("进程音频事件等待失败(pid={})，继续尝试...", process_id);
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        log::info!(
            "进程音频采集循环结束(pid={}), stop_flag={}",
            process_id,
            stop_flag.load(Ordering::SeqCst)
        );

        if let Ok(meta) = std::fs::metadata(&output_path) {
            log::info!(
                "进程音频文件在循环退出后存在(pid={}): {:?}, 大小: {} bytes",
                process_id,
                output_path.file_name(),
                meta.len()
            );
        } else {
            log::warn!(
                "进程音频文件在循环退出后不存在(pid={}): {:?}",
                process_id,
                output_path.file_name()
            );
        }

        let _ = audio_client.stop_stream();
        writer.finalize().map_err(|e| {
            log::error!("进程音频文件完成失败(pid={}): {}", process_id, e);
            format!("完成进程音频文件失败(pid={}): {}", process_id, e)
        })?;

        match std::fs::metadata(&output_path) {
            Ok(meta) => log::info!(
                "进程音频文件最终状态(pid={}): {:?}, 大小: {} bytes",
                process_id,
                output_path.file_name(),
                meta.len()
            ),
            Err(e) => log::warn!(
                "进程音频文件最终状态检查失败(pid={}): {:?}, {}",
                process_id,
                output_path.file_name(),
                e
            ),
        }

        Ok(())
    };
    let result = run();
    if let Err(err) = &result {
        if let Some(tx) = startup_tx {
            let _ = tx.send((process_id, Err(err.clone())));
        }
    }
    result
}

pub fn start_process_loopback_wavs(
    process_ids: Vec<u32>,
    output_paths: Vec<PathBuf>,
    enabled_flag: Arc<AtomicBool>,
    recording_pause_flag: Arc<AtomicBool>,
) -> Result<WasapiCaptureHandle, String> {
    if process_ids.is_empty() || process_ids.len() != output_paths.len() {
        return Err(AppErrorKind::InternalError.to_frontend_json());
    }
    let stop_flag = Arc::new(AtomicBool::new(false));
    let thread_stop = stop_flag.clone();
    let thread_enabled = enabled_flag.clone();
    let thread_pause = recording_pause_flag.clone();
    let process_count = process_ids.len();
    let (startup_tx, startup_rx) = mpsc::channel::<(u32, Result<(), String>)>();
    let mut workers = Vec::new();
    for (pid, path) in process_ids.into_iter().zip(output_paths) {
        let worker_stop = thread_stop.clone();
        let worker_enabled = thread_enabled.clone();
        let worker_pause = thread_pause.clone();
        let worker_startup_tx = startup_tx.clone();
        let worker_path = path.clone();
        workers.push(std::thread::spawn(move || {
            if let Err(e) = capture_process_loopback_to_wav(
                pid,
                path,
                worker_stop,
                worker_enabled,
                worker_pause,
                Some(worker_startup_tx),
            ) {
                log::error!("进程音频采集线程异常退出(pid={}): {}", pid, e);

                match std::fs::metadata(&worker_path) {
                    Ok(meta) => log::warn!(
                        "进程音频线程退出后文件仍存在: {:?}, 大小: {} bytes",
                        worker_path.file_name(),
                        meta.len()
                    ),
                    Err(e) => log::warn!(
                        "进程音频线程退出后文件不存在: {:?}, {}",
                        worker_path.file_name(),
                        e
                    ),
                }
            } else {
                log::info!("进程音频采集线程正常退出(pid={})", pid);

                match std::fs::metadata(&worker_path) {
                    Ok(meta) => log::info!(
                        "进程音频线程正常退出后文件存在: {:?}, 大小: {} bytes",
                        worker_path.file_name(),
                        meta.len()
                    ),
                    Err(e) => log::warn!(
                        "进程音频线程正常退出后文件不存在: {:?}, {}",
                        worker_path.file_name(),
                        e
                    ),
                }
            }
        }));
    }
    drop(startup_tx);
    let mut startup_errors = Vec::new();
    for _ in 0..process_count {
        match startup_rx.recv_timeout(Duration::from_secs(3)) {
            Ok((pid, Ok(()))) => {
                log::info!("进程音频采集启动成功(pid={})", pid);
            }
            Ok((pid, Err(e))) => {
                startup_errors.push(format!("pid={}: {}", pid, e));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                startup_errors.push("进程音频采集启动超时".to_string());
                break;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                startup_errors.push("进程音频采集启动通道断开".to_string());
                break;
            }
        }
    }
    if !startup_errors.is_empty() {
        stop_flag.store(true, Ordering::SeqCst);
        for worker in workers {
            let _ = worker.join();
        }
        return Err(format!(
            "进程音频采集启动失败: {}",
            startup_errors.join(" | ")
        ));
    }
    Ok(WasapiCaptureHandle {
        stop_flag,
        joins: workers,
        output_path: PathBuf::from(""),
    })
}

pub fn list_audio_processes() -> Vec<AudioProcessInfo> {
    let refresh = RefreshKind::nothing().with_processes(ProcessRefreshKind::everything());
    let sys = System::new_with_specifics(refresh);

    let window_titles = visible_window_process_titles();
    let visible_pids = window_titles.keys().copied().collect::<HashSet<u32>>();

    let active_now = active_audio_process_ids();
    let now = now_ms();
    let recent_map = AUDIO_RECENT_ACTIVITY.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut map) = recent_map.lock() {
        for pid in &active_now {
            map.insert(*pid, now);
        }
        map.retain(|_, ts| now.saturating_sub(*ts) <= 5 * 60 * 1000);
    }
    let mut list = sys
        .processes()
        .iter()
        .map(|(pid, process)| AudioProcessInfo {
            pid: pid.as_u32(),
            name: {
                let process_name = process.name().to_string_lossy().to_string();
                let title = window_titles
                    .get(&pid.as_u32())
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                if !title.is_empty() {
                    format!("{} - {}", title, process_name)
                } else {
                    process_name
                }
            },
        })
        .filter(|p| {
            p.pid > 0
                && !p.name.trim().is_empty()
                && (visible_pids.is_empty() || visible_pids.contains(&p.pid))
        })
        .collect::<Vec<_>>();
    let activity_snapshot = AUDIO_RECENT_ACTIVITY
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()
        .map(|m| m.clone())
        .unwrap_or_default();
    list.sort_by(|a, b| {
        let a_active = active_now.contains(&a.pid);
        let b_active = active_now.contains(&b.pid);
        if a_active != b_active {
            return b_active.cmp(&a_active);
        }
        let a_recent = activity_snapshot.get(&a.pid).copied().unwrap_or(0);
        let b_recent = activity_snapshot.get(&b.pid).copied().unwrap_or(0);
        if a_recent != b_recent {
            return b_recent.cmp(&a_recent);
        }
        a.name.to_lowercase().cmp(&b.name.to_lowercase())
    });
    list.dedup_by(|a, b| a.pid == b.pid);
    list
}

fn active_audio_process_ids() -> HashSet<u32> {
    let mut set = HashSet::new();
    let _ = initialize_mta();
    let Ok(enumerator) = DeviceEnumerator::new() else {
        return set;
    };
    let Ok(device) = enumerator.get_default_device(&Direction::Render) else {
        return set;
    };
    let Ok(manager) = device.get_iaudiosessionmanager() else {
        return set;
    };
    let Ok(session_enum) = manager.get_audiosessionenumerator() else {
        return set;
    };
    let Ok(count) = session_enum.get_count() else {
        return set;
    };
    for i in 0..count {
        let Ok(control) = session_enum.get_session(i) else {
            continue;
        };
        let Ok(state) = control.get_state() else {
            continue;
        };
        if state == SessionState::Active {
            if let Ok(pid) = control.get_process_id() {
                if pid > 0 {
                    set.insert(pid);
                }
            }
        }
    }
    set
}

#[cfg(target_os = "windows")]
fn visible_window_process_titles() -> HashMap<u32, String> {
    let mut map = HashMap::new();
    if let Ok(windows) = crate::features::screenshot::window_detect::get_window_list() {
        for w in windows {
            let hwnd_str = w.hwnd.trim_start_matches("0x");
            if let Ok(hwnd_val) = usize::from_str_radix(hwnd_str, 16) {
                let hwnd = windows::Win32::Foundation::HWND(hwnd_val as *mut core::ffi::c_void);
                let mut pid: u32 = 0;
                unsafe {
                    GetWindowThreadProcessId(hwnd, Some(&mut pid));
                }
                if pid > 0 && !w.title.is_empty() {
                    map.entry(pid).or_insert(w.title);
                }
            }
        }
    }
    map
}

#[cfg(not(target_os = "windows"))]
fn visible_window_process_titles() -> HashMap<u32, String> {
    HashMap::new()
}

pub fn start_system_loopback_wav_with_device(
    device_desc_key: Option<String>,
    output_path: PathBuf,
    enabled_flag: Arc<AtomicBool>,
    recording_pause_flag: Arc<AtomicBool>,
) -> Result<WasapiCaptureHandle, String> {
    AUDIO_WRITE_ERR_COUNT.store(false, Ordering::Relaxed);
    let stop_flag = Arc::new(AtomicBool::new(false));
    let thread_stop_flag = stop_flag.clone();
    let thread_output = output_path.clone();
    let thread_device_key = device_desc_key.clone();
    let (tx, rx) = mpsc::channel::<Result<(), String>>();

    let handle = std::thread::spawn(move || {
        let run = || -> Result<(), String> {
            let host = cpal::host_from_id(cpal::HostId::Wasapi)
                .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(e.to_string()))?;

            let device = thread_device_key
                .as_ref()
                .and_then(|key| {
                    host.output_devices()
                        .ok()
                        .and_then(|devs| pick_device_by_key(devs, key))
                })
                .or_else(|| host.default_output_device())
                .ok_or_else(|| "未找到输出设备".to_string())?;

            let mut sample_format = CpalSampleFormat::F32;
            let mut config: StreamConfig = StreamConfig {
                channels: 2,
                sample_rate: 48_000,
                buffer_size: cpal::BufferSize::Default,
            };
            if let Ok(mut supported) = device.supported_input_configs() {
                if let Some(s) = supported.next() {
                    let s = s.with_max_sample_rate();
                    sample_format = s.sample_format();
                    config = s.config();
                } else if let Ok(def) = device.default_output_config() {
                    sample_format = def.sample_format();
                    let defc = def.config();
                    config = StreamConfig {
                        channels: defc.channels,
                        sample_rate: defc.sample_rate,
                        buffer_size: cpal::BufferSize::Default,
                    };
                }
            } else if let Ok(def) = device.default_output_config() {
                sample_format = def.sample_format();
                let defc = def.config();
                config = StreamConfig {
                    channels: defc.channels,
                    sample_rate: defc.sample_rate,
                    buffer_size: cpal::BufferSize::Default,
                };
            }
            let spec = hound::WavSpec {
                channels: config.channels,
                sample_rate: config.sample_rate,
                bits_per_sample: 16,
                sample_format: SampleFormat::Int,
            };

            let file = std::fs::File::create(&thread_output)
                .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(e.to_string()))?;
            let buf_writer = std::io::BufWriter::with_capacity(1024 * 1024, file);
            let writer = hound::WavWriter::new(buf_writer, spec)
                .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(e.to_string()))?;
            let writer = Arc::new(Mutex::new(Some(writer)));

            log::info!(
                "WASAPI音频线程启动: {:?}, enabled={}, pause={}",
                thread_output.file_name(),
                enabled_flag.load(Ordering::SeqCst),
                recording_pause_flag.load(Ordering::SeqCst)
            );

            let err_fn = |err| eprintln!("WASAPI 捕获错误: {}", err);

            match std::fs::metadata(&thread_output) {
                Ok(meta) => log::info!(
                    "WASAPI音频文件创建成功: {:?}, 大小: {} bytes",
                    thread_output.file_name(),
                    meta.len()
                ),
                Err(e) => log::warn!(
                    "WASAPI音频文件创建后检查失败: {:?}, {}",
                    thread_output.file_name(),
                    e
                ),
            }
            let stream = match sample_format {
                CpalSampleFormat::F32 => device.build_input_stream(&config,
                    audio_write_loop!(writer, enabled_flag, recording_pause_flag, "系统F32", f32),
                    err_fn, Some(Duration::from_millis(10))).map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?,
                CpalSampleFormat::I16 => device.build_input_stream(&config,
                    audio_write_loop!(writer, enabled_flag, recording_pause_flag, "系统I16", i16),
                    err_fn, Some(Duration::from_millis(10))).map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?,
                CpalSampleFormat::U16 => device.build_input_stream(&config,
                    audio_write_loop!(writer, enabled_flag, recording_pause_flag, "系统U16", u16),
                    err_fn, Some(Duration::from_millis(10))).map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?,
                CpalSampleFormat::I8 => device.build_input_stream(&config,
                    audio_write_loop!(writer, enabled_flag, recording_pause_flag, "系统I8", i8),
                    err_fn, Some(Duration::from_millis(10))).map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?,
                CpalSampleFormat::U8 => device.build_input_stream(&config,
                    audio_write_loop!(writer, enabled_flag, recording_pause_flag, "系统U8", u8),
                    err_fn, Some(Duration::from_millis(10))).map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?,
                CpalSampleFormat::I32 => device.build_input_stream(&config,
                    audio_write_loop!(writer, enabled_flag, recording_pause_flag, "系统I32", i32),
                    err_fn, Some(Duration::from_millis(10))).map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?,
                CpalSampleFormat::U32 => device.build_input_stream(&config,
                    audio_write_loop!(writer, enabled_flag, recording_pause_flag, "系统U32", u32),
                    err_fn, Some(Duration::from_millis(10))).map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?,
                CpalSampleFormat::F64 => device.build_input_stream(&config,
                    audio_write_loop!(writer, enabled_flag, recording_pause_flag, "系统F64", f64),
                    err_fn, Some(Duration::from_millis(10))).map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?,
                _ => return Err(AppErrorKind::InternalError.to_frontend_json()),
            };
            stream
                .play()
                .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(e.to_string()))?;
            let _ = tx.send(Ok(()));

            while !thread_stop_flag.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(10));
            }

            log::info!("收到停止信号，填充2s静音数据以确保音频完全覆盖视频最后一段...");
            // 按实际采样率/声道数填充 2s，避免 44.1k/96k 设备填充时长偏差
            let tail_samples = (config.sample_rate as usize) * (config.channels as usize) * 2;
            if let Ok(mut guard) = writer.lock() {
                if let Some(w) = guard.as_mut() {
                    for _ in 0..tail_samples {
                        let _ = w.write_sample(0i16);
                    }
                }
            }

            let _ = stream.pause();

            drop(stream);

            if let Ok(mut guard) = writer.lock() {
                if let Some(w) = guard.take() {
                    let _ = w.finalize();
                }
            }

            match std::fs::metadata(&thread_output) {
                Ok(meta) => log::info!(
                    "✅ WASAPI音频文件最终大小: {:?}, {} bytes",
                    thread_output.file_name(),
                    meta.len()
                ),
                Err(e) => log::warn!(
                    "❌ WASAPI音频文件最终状态检查失败: {:?}, {}",
                    thread_output.file_name(),
                    e
                ),
            }

            Ok(())
        };
        if let Err(e) = run() {
            let _ = tx.send(Err(e));
        }
    });

    rx.recv_timeout(Duration::from_secs(2))
        .map_err(|_| "启动 WASAPI 捕获超时".to_string())??;

    Ok(WasapiCaptureHandle {
        stop_flag,
        joins: vec![handle],
        output_path,
    })
}

// 兼容旧签名
pub fn start_system_loopback_wav(output_path: PathBuf) -> Result<WasapiCaptureHandle, String> {
    start_system_loopback_wav_with_device(
        None,
        output_path,
        Arc::new(AtomicBool::new(true)),
        Arc::new(AtomicBool::new(false)),
    )
}

pub fn start_microphone_wav_with_device(
    device_desc_key: Option<String>,
    output_path: PathBuf,
    enabled_flag: Arc<AtomicBool>,
    recording_pause_flag: Arc<AtomicBool>,
) -> Result<WasapiCaptureHandle, String> {
    AUDIO_WRITE_ERR_COUNT.store(false, Ordering::Relaxed);
    let stop_flag = Arc::new(AtomicBool::new(false));
    let thread_stop_flag = stop_flag.clone();
    let thread_output = output_path.clone();
    let thread_device_key = device_desc_key.clone();
    let (tx, rx) = mpsc::channel::<Result<(), String>>();

    let handle = std::thread::spawn(move || {
        let run = || -> Result<(), String> {
            let host = cpal::host_from_id(cpal::HostId::Wasapi)
                .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(e.to_string()))?;
            let device = thread_device_key
                .as_ref()
                .and_then(|key| {
                    host.input_devices()
                        .ok()
                        .and_then(|devs| pick_device_by_key(devs, key))
                })
                .or_else(|| host.default_input_device())
                .ok_or_else(|| "未找到输入设备".to_string())?;

            let mut sample_format = CpalSampleFormat::F32;
            let mut config: StreamConfig = StreamConfig {
                channels: 1,
                sample_rate: 48_000,
                buffer_size: cpal::BufferSize::Default,
            };
            if let Ok(mut supported) = device.supported_input_configs() {
                if let Some(s) = supported.next() {
                    let s = s.with_max_sample_rate();
                    sample_format = s.sample_format();
                    config = s.config();
                } else if let Ok(def) = device.default_input_config() {
                    sample_format = def.sample_format();
                    let defc = def.config();
                    config = StreamConfig {
                        channels: defc.channels,
                        sample_rate: defc.sample_rate,
                        buffer_size: cpal::BufferSize::Default,
                    };
                }
            } else if let Ok(def) = device.default_input_config() {
                sample_format = def.sample_format();
                let defc = def.config();
                config = StreamConfig {
                    channels: defc.channels,
                    sample_rate: defc.sample_rate,
                    buffer_size: cpal::BufferSize::Default,
                };
            }

            let spec = hound::WavSpec {
                channels: config.channels,
                sample_rate: config.sample_rate,
                bits_per_sample: 16,
                sample_format: SampleFormat::Int,
            };

            let file = std::fs::File::create(&thread_output)
                .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(e.to_string()))?;
            let buf_writer = std::io::BufWriter::with_capacity(1024 * 1024, file);
            let writer = hound::WavWriter::new(buf_writer, spec)
                .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(e.to_string()))?;
            let writer = Arc::new(Mutex::new(Some(writer)));

            log::info!(
                "WASAPI麦克风线程启动: {:?}, enabled={}, pause={}",
                thread_output.file_name(),
                enabled_flag.load(Ordering::SeqCst),
                recording_pause_flag.load(Ordering::SeqCst)
            );

            let err_fn = |err| eprintln!("WASAPI 麦克风捕获错误: {}", err);

            match std::fs::metadata(&thread_output) {
                Ok(meta) => log::info!(
                    "WASAPI麦克风文件创建成功: {:?}, 大小: {} bytes",
                    thread_output.file_name(),
                    meta.len()
                ),
                Err(e) => log::warn!(
                    "WASAPI麦克风文件创建后检查失败: {:?}, {}",
                    thread_output.file_name(),
                    e
                ),
            }
            let stream = match sample_format {
                CpalSampleFormat::F32 => device.build_input_stream(&config,
                    audio_write_loop!(writer, enabled_flag, recording_pause_flag, "麦克风F32", f32),
                    err_fn, Some(Duration::from_millis(10))).map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?,
                CpalSampleFormat::I16 => device.build_input_stream(&config,
                    audio_write_loop!(writer, enabled_flag, recording_pause_flag, "麦克风I16", i16),
                    err_fn, Some(Duration::from_millis(10))).map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?,
                CpalSampleFormat::U16 => device.build_input_stream(&config,
                    audio_write_loop!(writer, enabled_flag, recording_pause_flag, "麦克风U16", u16),
                    err_fn, Some(Duration::from_millis(10))).map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?,
                CpalSampleFormat::I8 => device.build_input_stream(&config,
                    audio_write_loop!(writer, enabled_flag, recording_pause_flag, "麦克风I8", i8),
                    err_fn, Some(Duration::from_millis(10))).map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?,
                CpalSampleFormat::U8 => device.build_input_stream(&config,
                    audio_write_loop!(writer, enabled_flag, recording_pause_flag, "麦克风U8", u8),
                    err_fn, Some(Duration::from_millis(10))).map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?,
                CpalSampleFormat::I32 => device.build_input_stream(&config,
                    audio_write_loop!(writer, enabled_flag, recording_pause_flag, "麦克风I32", i32),
                    err_fn, Some(Duration::from_millis(10))).map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?,
                CpalSampleFormat::U32 => device.build_input_stream(&config,
                    audio_write_loop!(writer, enabled_flag, recording_pause_flag, "麦克风U32", u32),
                    err_fn, Some(Duration::from_millis(10))).map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?,
                CpalSampleFormat::F64 => device.build_input_stream(&config,
                    audio_write_loop!(writer, enabled_flag, recording_pause_flag, "麦克风F64", f64),
                    err_fn, Some(Duration::from_millis(10))).map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(format!("{}", e)))?,
                _ => return Err(AppErrorKind::InternalError.to_frontend_json()),
            };
            stream
                .play()
                .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(e.to_string()))?;
            let _ = tx.send(Ok(()));

            while !thread_stop_flag.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(10));
            }

            log::info!("收到麦克风停止信号，填充1s静音数据以确保音频完全覆盖...");
            // 按实际采样率/声道数填充 1s
            let tail_samples = (config.sample_rate as usize) * (config.channels as usize);
            if let Ok(mut guard) = writer.lock() {
                if let Some(w) = guard.as_mut() {
                    for _ in 0..tail_samples {
                        let _ = w.write_sample(0i16);
                    }
                }
            }

            let _ = stream.pause();

            std::thread::sleep(Duration::from_millis(10));

            drop(stream);

            if let Ok(mut guard) = writer.lock() {
                if let Some(w) = guard.take() {
                    let _ = w.finalize();
                }
            }

            match std::fs::metadata(&thread_output) {
                Ok(meta) => log::info!(
                    "✅ WASAPI麦克风文件最终大小: {:?}, {} bytes",
                    thread_output.file_name(),
                    meta.len()
                ),
                Err(e) => log::warn!(
                    "❌ WASAPI麦克风文件最终状态检查失败: {:?}, {}",
                    thread_output.file_name(),
                    e
                ),
            }

            Ok(())
        };
        if let Err(e) = run() {
            let _ = tx.send(Err(e));
        }
    });

    rx.recv_timeout(Duration::from_secs(2))
        .map_err(|_| "启动 WASAPI 麦克风捕获超时".to_string())??;

    Ok(WasapiCaptureHandle {
        stop_flag,
        joins: vec![handle],
        output_path,
    })
}

// 🔧 方案A：使用 FFmpeg 管道实时编码 AAC（而非 WAV）
pub fn start_system_loopback_aac_with_device(
    device_desc_key: Option<String>,
    output_path: PathBuf,
    enabled_flag: Arc<AtomicBool>,
    recording_pause_flag: Arc<AtomicBool>,
    audio_bitrate_kbps: Option<u32>,
) -> Result<WasapiFfmpegHandle, String> {
    AUDIO_WRITE_ERR_COUNT.store(false, Ordering::Relaxed);
    let stop_flag = Arc::new(AtomicBool::new(false));
    let thread_stop_flag = stop_flag.clone();
    let thread_output = output_path.clone();
    let thread_device_key = device_desc_key.clone();
    let ffmpeg_child = Arc::new(Mutex::new(None::<ChildGuard>));
    let thread_ffmpeg = ffmpeg_child.clone();
    let (tx, rx) = mpsc::channel::<Result<(), String>>();

    let handle = std::thread::spawn(move || {
        let run = || -> Result<(), String> {
            let ffmpeg_path = crate::features::recording::ffmpeg_runner::resolve_ffmpeg_path()
                .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(e.to_string()))?;

            let mut ffmpeg_cmd = Command::new(&ffmpeg_path);
            #[cfg(target_os = "windows")]
            {
                use std::os::windows::process::CommandExt;
                const CREATE_NO_WINDOW: u32 = 0x0800_0000;
                ffmpeg_cmd.creation_flags(CREATE_NO_WINDOW);
            }

            let effective_bitrate = audio_bitrate_kbps.unwrap_or(128).clamp(32, 512);
            ffmpeg_cmd
                .args([
                    "-f",
                    "f32le",
                    "-ar",
                    "48000",
                    "-ac",
                    "2",
                    "-i",
                    "-",
                    "-c:a",
                    "aac",
                    "-b:a",
                    &format!("{}k", effective_bitrate),
                    "-profile:a",
                    "aac_low",
                    "-y",
                    thread_output.to_str().ok_or("无效的输出路径")?,
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            let mut child = ffmpeg_cmd
                .spawn()
                .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(e.to_string()))?;

            let stdin = child.stdin.take().ok_or("无法获取 FFmpeg stdin")?;
            // H1 修复：消费 FFmpeg stderr 防止管道满导致挂起
            if let Some(stderr) = child.stderr.take() {
                std::thread::spawn(move || {
                    use std::io::BufRead;
                    let reader = std::io::BufReader::new(stderr);
                    for line in reader.lines() {
                        match line {
                            Ok(l) if !l.trim().is_empty() => {
                                log::debug!("[ffmpeg-aac-stderr] {}", l);
                            }
                            Err(_) => break,
                            _ => {}
                        }
                    }
                });
            }

            {
                if let Ok(mut guard) = thread_ffmpeg.lock() {
                    *guard = Some(ChildGuard(child));
                }
            }

            log::info!(
                "🔧 FFmpeg AAC 编码管道已启动: {:?}",
                thread_output.file_name()
            );

            let host = cpal::host_from_id(cpal::HostId::Wasapi)
                .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(e.to_string()))?;

            let device = thread_device_key
                .as_ref()
                .and_then(|key| {
                    host.output_devices()
                        .ok()
                        .and_then(|devs| pick_device_by_key(devs, key))
                })
                .or_else(|| host.default_output_device())
                .ok_or_else(|| "未找到输出设备".to_string())?;

            let config: StreamConfig = StreamConfig {
                channels: 2,
                sample_rate: 48_000,
                buffer_size: cpal::BufferSize::Default,
            };

            // H2 修复：使用有界通道防止音频缓冲区无限累积导致 OOM
            let (tx_audio, rx_audio) = std::sync::mpsc::sync_channel::<Vec<u8>>(200);
            let (tx_pool, rx_pool) = std::sync::mpsc::sync_channel::<Vec<u8>>(200);
            for _ in 0..50 {
                let _ = tx_pool.send(Vec::with_capacity(4096));
            }

            let mut writer_opt = Some(std::io::BufWriter::new(stdin));
            let writer_thread = std::thread::spawn(move || {
                while let Ok(mut data) = rx_audio.recv() {
                    if data.is_empty() {
                        break;
                    }
                    if let Some(writer) = writer_opt.as_mut() {
                        if let Err(e) = writer.write_all(&data) {
                            log::error!("FFmpeg stdin 写入失败，停止音频采集: {}", e);
                            let _ = tx_pool.send(data);
                            break;
                        }
                        if let Err(e) = writer.flush() {
                            log::error!("FFmpeg stdin flush 失败，停止音频采集: {}", e);
                            let _ = tx_pool.send(data);
                            break;
                        }
                    }
                    data.clear();
                    let _ = tx_pool.send(data);
                }
                if let Some(writer) = writer_opt.take() {
                    drop(writer);
                }
            });

            let tx_cb = tx_audio.clone();
            let enabled_cb = enabled_flag.clone();
            let pause_cb = recording_pause_flag.clone();

            log::info!(
                "WASAPI+FFmpeg线程启动: {:?}, enabled={}, pause={}",
                thread_output.file_name(),
                enabled_flag.load(Ordering::SeqCst),
                recording_pause_flag.load(Ordering::SeqCst)
            );

            let err_fn = |err| eprintln!("WASAPI 捕获错误: {}", err);

            let stream = device
                .build_input_stream(
                    &config,
                    move |data: &[f32], _| {
                        let enabled =
                            enabled_cb.load(Ordering::SeqCst) && !pause_cb.load(Ordering::SeqCst);
                        let mut buffer = rx_pool
                            .try_recv()
                            .unwrap_or_else(|_| Vec::with_capacity(data.len() * 4));
                        buffer.clear();
                        if enabled {
                            for &sample in data {
                                buffer.extend_from_slice(&sample.to_le_bytes());
                            }
                        } else {
                            buffer.resize(data.len() * 4, 0);
                        }
                        let _ = tx_cb.send(buffer);
                    },
                    err_fn,
                    Some(Duration::from_millis(10)),
                )
                .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(e.to_string()))?;

            stream
                .play()
                .map_err(|e| AppErrorKind::InternalError.to_frontend_json_with_details(e.to_string()))?;
            let _ = tx.send(Ok(()));

            while !thread_stop_flag.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(10));
            }

            let stop_start = std::time::Instant::now();
            log::info!("收到系统音频停止信号，关闭FFmpeg输入流...");

            drop(stream);
            log::info!("WASAPI 流已停止");

            let _ = tx_audio.send(Vec::new());
            let _ = writer_thread.join();

            log::info!("等待 FFmpeg AAC 编码完成...");
            if let Ok(mut guard) = thread_ffmpeg.lock() {
                if let Some(ref mut child) = *guard {
                    for _ in 0..1000 {
                        match child.0.try_wait() {
                            Ok(Some(status)) => {
                                log::info!(
                                    "✅ FFmpeg AAC 编码完成: {:?}, exit_status={}, 耗时={}ms",
                                    thread_output.file_name(),
                                    status,
                                    stop_start.elapsed().as_millis()
                                );
                                break;
                            }
                            Ok(None) => {
                                std::thread::sleep(Duration::from_millis(10));
                            }
                            Err(e) => {
                                log::warn!("检查 FFmpeg 状态失败: {}", e);
                                break;
                            }
                        }
                    }

                    if child.0.try_wait().ok().flatten().is_none() {
                        log::warn!("FFmpeg 编码超时，强制终止");
                        let _ = child.0.kill();
                    }
                }
            }

            // 🔧 验证输出文件：检查 AAC 文件是否有效
            match std::fs::metadata(&thread_output) {
                Ok(meta) => {
                    let size = meta.len();
                    if size < 1024 {
                        log::warn!(
                            "⚠️ FFmpeg AAC 输出文件过小: {:?}, {} bytes — 可能编码数据不足",
                            thread_output.file_name(),
                            size
                        );
                    } else {
                        log::info!(
                            "✅ FFmpeg AAC 输出文件: {:?}, {} bytes",
                            thread_output.file_name(),
                            size
                        );
                    }
                }
                Err(e) => {
                    log::warn!(
                        "❌ FFmpeg AAC 输出文件不存在: {:?}, {}",
                        thread_output.file_name(),
                        e
                    );
                }
            }

            Ok(())
        };
        if let Err(e) = run() {
            let _ = tx.send(Err(e));
        }
    });

    rx.recv_timeout(Duration::from_secs(2))
        .map_err(|_| "启动 WASAPI+FFmpeg 捕获超时".to_string())??;

    Ok(WasapiFfmpegHandle {
        stop_flag,
        join: Some(handle),
        output_path,
        ffmpeg_child,
    })
}
