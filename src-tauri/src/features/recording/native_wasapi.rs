use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat as CpalSampleFormat, StreamConfig};
use hound::{SampleFormat, WavWriter};
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
use winapi::um::winuser::GetWindowThreadProcessId;

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

#[derive(Debug, Clone)]
pub struct AudioProcessInfo {
    pub pid: u32,
    pub name: String,
}

static AUDIO_RECENT_ACTIVITY: std::sync::OnceLock<Mutex<HashMap<u32, u64>>> =
    std::sync::OnceLock::new();

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
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
    let run = || -> Result<(), String> {
        let _ = initialize_mta();
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
        // 🔧 性能优化：使用 1MB 大缓冲区的 BufWriter 避免高频 I/O 瓶颈
        let file = std::fs::File::create(&output_path)
            .map_err(|e| format!("创建进程音频文件失败(pid={}): {}", process_id, e))?;
        let buf_writer = std::io::BufWriter::with_capacity(1024 * 1024, file);
        let mut writer = hound::WavWriter::new(buf_writer, spec)
            .map_err(|e| format!("初始化 WAV 写入器失败(pid={}): {}", process_id, e))?;
        // ✅ 添加诊断日志：确认文件创建成功
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
        audio_client
            .start_stream()
            .map_err(|e| format!("启动进程 loopback 失败(pid={}): {}", process_id, e))?;
        if let Some(tx) = startup_tx.as_ref() {
            let _ = tx.send((process_id, Ok(())));
        }

        // 🔧 修复 A/V 严重不同步：通过时钟对齐补充静音数据
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
            let enabled = enabled_flag.load(Ordering::SeqCst) && !recording_pause_flag.load(Ordering::SeqCst);
            
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
                    let _ = writer.write_sample(out);
                    actual_total_samples += 1;
                }
            }
            queue.drain(..processed);

            // 🔧 如果目标应用静音导致 WASAPI 不投递音频包，则根据时间流逝填充静音数据
            if !is_paused {
                let expected_total_samples = ((active_time_ns as f64 / 1_000_000_000.0) * 48000.0) as u64 * 2;
                if expected_total_samples > actual_total_samples {
                    let padding_needed = expected_total_samples - actual_total_samples;
                    // 容忍 50ms (4800 samples) 的抖动，避免在正常数据到达前过度填充
                    if padding_needed > 4800 {
                        for _ in 0..padding_needed {
                            let _ = writer.write_sample(0i16);
                        }
                        actual_total_samples += padding_needed;
                    }
                }
            }

            if event.wait_for_event(50).is_err() {
                // ✅ 添加诊断日志：事件等待失败（可能是进程退出或音频设备断开）
                log::warn!("进程音频事件等待失败(pid={})，继续尝试...", process_id);
                std::thread::sleep(Duration::from_millis(10));
            }
            if blockalign == 0 {
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        // ✅ 添加诊断日志：记录循环退出原因
        log::info!(
            "进程音频采集循环结束(pid={}), stop_flag={}",
            process_id,
            stop_flag.load(Ordering::SeqCst)
        );

        // 检查文件是否存在
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

        // ✅ 添加诊断日志：记录最终文件状态
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
        return Err("进程音频录制参数无效".to_string());
    }
    let stop_flag = Arc::new(AtomicBool::new(false));
    let thread_stop = stop_flag.clone();
    let thread_enabled = enabled_flag.clone();
    let thread_pause = recording_pause_flag.clone();
    let process_count = process_ids.len();
    let (startup_tx, startup_rx) = mpsc::channel::<(u32, Result<(), String>)>();
    let mut workers = Vec::new();
    for (pid, path) in process_ids.into_iter().zip(output_paths.into_iter()) {
        let worker_stop = thread_stop.clone();
        let worker_enabled = thread_enabled.clone();
        let worker_pause = thread_pause.clone();
        let worker_startup_tx = startup_tx.clone();
        let worker_path = path.clone(); // ✅ 克隆 path 用于线程退出后检查
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
                // ✅ 添加诊断日志：检查文件状态
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
                // ✅ 添加诊断日志：检查文件状态
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
            // 解析 hwnd
            let hwnd_str = w.hwnd.trim_start_matches("0x");
            if let Ok(hwnd_val) = usize::from_str_radix(hwnd_str, 16) {
                let hwnd = hwnd_val as winapi::shared::windef::HWND;
                let mut pid: u32 = 0;
                unsafe {
                    GetWindowThreadProcessId(hwnd, &mut pid);
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
    let stop_flag = Arc::new(AtomicBool::new(false));
    let thread_stop_flag = stop_flag.clone();
    let thread_output = output_path.clone();
    let thread_device_key = device_desc_key.clone();
    let (tx, rx) = mpsc::channel::<Result<(), String>>();

    let handle = std::thread::spawn(move || {
        let run = || -> Result<(), String> {
            let host = cpal::host_from_id(cpal::HostId::Wasapi)
                .map_err(|e| format!("WASAPI 主机不可用: {}", e))?;
            // 选择设备：优先匹配描述文本，其次默认输出
            let device = if let Some(key) = thread_device_key.as_ref() {
                if let Ok(devs) = host.output_devices() {
                    let mut picked = None;
                    for d in devs {
                        if let Ok(desc) = d.description() {
                            if desc.to_string() == *key {
                                picked = Some(d);
                                break;
                            }
                        }
                    }
                    picked
                } else {
                    None
                }
            } else {
                None
            }
            .or_else(|| host.default_output_device())
            .ok_or_else(|| "未找到输出设备".to_string())?;
            // 选择可用采样配置：优先 supported_input_configs，其次 default_output_config，最后兜底 48kHz/立体声/F32
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
            // 🔧 性能优化：使用 1MB 大缓冲区的 BufWriter 避免高频 I/O 瓶颈
            let file = std::fs::File::create(&thread_output)
                .map_err(|e| format!("创建 wav 文件失败: {}", e))?;
            let buf_writer = std::io::BufWriter::with_capacity(1024 * 1024, file);
            let writer = hound::WavWriter::new(buf_writer, spec)
                .map_err(|e| format!("初始化 WAV 写入器失败: {}", e))?;
            let writer = Arc::new(Mutex::new(Some(writer)));
            let writer_cb = writer.clone();
            let enabled_cb = enabled_flag.clone();
            let pause_cb = recording_pause_flag.clone();

            // ✅ 添加诊断日志：记录音频线程启动时的状态
            log::info!(
                "WASAPI音频线程启动: {:?}, enabled={}, pause={}",
                thread_output.file_name(),
                enabled_flag.load(Ordering::SeqCst),
                recording_pause_flag.load(Ordering::SeqCst)
            );

            let err_fn = |err| eprintln!("WASAPI 捕获错误: {}", err);

            // ✅ 添加诊断日志：记录文件创建状态
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
                CpalSampleFormat::F32 => device
                    .build_input_stream(
                        &config,
                        move |data: &[f32], _| {
                            if let Ok(mut guard) = writer_cb.lock() {
                                if let Some(writer) = guard.as_mut() {
                                    let enabled = enabled_cb.load(Ordering::SeqCst)
                                        && !pause_cb.load(Ordering::SeqCst);
                                    for &v in data {
                                        let s = if enabled {
                                            (v * i16::MAX as f32) as i16
                                        } else {
                                            0
                                        };
                                        let _ = writer.write_sample(s);
                                    }
                                }
                            }
                        },
                        err_fn,
                        Some(Duration::from_millis(100)),
                    )
                    .map_err(|e| format!("创建输入流失败: {}", e))?,
                CpalSampleFormat::I16 => {
                    let writer_cb = writer.clone();
                    let enabled_cb = enabled_flag.clone();
                    let pause_cb = recording_pause_flag.clone();
                    device
                        .build_input_stream(
                            &config,
                            move |data: &[i16], _| {
                                if let Ok(mut guard) = writer_cb.lock() {
                                    if let Some(writer) = guard.as_mut() {
                                        let enabled = enabled_cb.load(Ordering::SeqCst)
                                            && !pause_cb.load(Ordering::SeqCst);
                                        for &v in data {
                                            let _ =
                                                writer.write_sample(if enabled { v } else { 0 });
                                        }
                                    }
                                }
                            },
                            err_fn,
                            Some(Duration::from_millis(100)),
                        )
                        .map_err(|e| format!("创建输入流失败: {}", e))?
                }
                CpalSampleFormat::U16 => {
                    let writer_cb = writer.clone();
                    let enabled_cb = enabled_flag.clone();
                    let pause_cb = recording_pause_flag.clone();
                    device
                        .build_input_stream(
                            &config,
                            move |data: &[u16], _| {
                                if let Ok(mut guard) = writer_cb.lock() {
                                    if let Some(writer) = guard.as_mut() {
                                        let enabled = enabled_cb.load(Ordering::SeqCst)
                                            && !pause_cb.load(Ordering::SeqCst);
                                        for &v in data {
                                            let s: i16 =
                                                if enabled { v.to_sample::<i16>() } else { 0 };
                                            let _ = writer.write_sample(s);
                                        }
                                    }
                                }
                            },
                            err_fn,
                            Some(Duration::from_millis(100)),
                        )
                        .map_err(|e| format!("创建输入流失败: {}", e))?
                }
                CpalSampleFormat::I8 => {
                    let writer_cb = writer.clone();
                    let enabled_cb = enabled_flag.clone();
                    let pause_cb = recording_pause_flag.clone();
                    device
                        .build_input_stream(
                            &config,
                            move |data: &[i8], _| {
                                if let Ok(mut guard) = writer_cb.lock() {
                                    if let Some(writer) = guard.as_mut() {
                                        let enabled = enabled_cb.load(Ordering::SeqCst)
                                            && !pause_cb.load(Ordering::SeqCst);
                                        for &v in data {
                                            let s: i16 =
                                                if enabled { v.to_sample::<i16>() } else { 0 };
                                            let _ = writer.write_sample(s);
                                        }
                                    }
                                }
                            },
                            err_fn,
                            Some(Duration::from_millis(100)),
                        )
                        .map_err(|e| format!("创建输入流失败: {}", e))?
                }
                CpalSampleFormat::U8 => {
                    let writer_cb = writer.clone();
                    let enabled_cb = enabled_flag.clone();
                    let pause_cb = recording_pause_flag.clone();
                    device
                        .build_input_stream(
                            &config,
                            move |data: &[u8], _| {
                                if let Ok(mut guard) = writer_cb.lock() {
                                    if let Some(writer) = guard.as_mut() {
                                        let enabled = enabled_cb.load(Ordering::SeqCst)
                                            && !pause_cb.load(Ordering::SeqCst);
                                        for &v in data {
                                            let s: i16 =
                                                if enabled { v.to_sample::<i16>() } else { 0 };
                                            let _ = writer.write_sample(s);
                                        }
                                    }
                                }
                            },
                            err_fn,
                            Some(Duration::from_millis(100)),
                        )
                        .map_err(|e| format!("创建输入流失败: {}", e))?
                }
                CpalSampleFormat::I32 => {
                    let writer_cb = writer.clone();
                    let enabled_cb = enabled_flag.clone();
                    let pause_cb = recording_pause_flag.clone();
                    device
                        .build_input_stream(
                            &config,
                            move |data: &[i32], _| {
                                if let Ok(mut guard) = writer_cb.lock() {
                                    if let Some(writer) = guard.as_mut() {
                                        let enabled = enabled_cb.load(Ordering::SeqCst)
                                            && !pause_cb.load(Ordering::SeqCst);
                                        for &v in data {
                                            let s: i16 =
                                                if enabled { v.to_sample::<i16>() } else { 0 };
                                            let _ = writer.write_sample(s);
                                        }
                                    }
                                }
                            },
                            err_fn,
                            Some(Duration::from_millis(100)),
                        )
                        .map_err(|e| format!("创建输入流失败: {}", e))?
                }
                CpalSampleFormat::U32 => {
                    let writer_cb = writer.clone();
                    let enabled_cb = enabled_flag.clone();
                    let pause_cb = recording_pause_flag.clone();
                    device
                        .build_input_stream(
                            &config,
                            move |data: &[u32], _| {
                                if let Ok(mut guard) = writer_cb.lock() {
                                    if let Some(writer) = guard.as_mut() {
                                        let enabled = enabled_cb.load(Ordering::SeqCst)
                                            && !pause_cb.load(Ordering::SeqCst);
                                        for &v in data {
                                            let s: i16 =
                                                if enabled { v.to_sample::<i16>() } else { 0 };
                                            let _ = writer.write_sample(s);
                                        }
                                    }
                                }
                            },
                            err_fn,
                            Some(Duration::from_millis(100)),
                        )
                        .map_err(|e| format!("创建输入流失败: {}", e))?
                }
                CpalSampleFormat::F64 => {
                    let writer_cb = writer.clone();
                    let enabled_cb = enabled_flag.clone();
                    let pause_cb = recording_pause_flag.clone();
                    device
                        .build_input_stream(
                            &config,
                            move |data: &[f64], _| {
                                if let Ok(mut guard) = writer_cb.lock() {
                                    if let Some(writer) = guard.as_mut() {
                                        let enabled = enabled_cb.load(Ordering::SeqCst)
                                            && !pause_cb.load(Ordering::SeqCst);
                                        for &v in data {
                                            let s: i16 =
                                                if enabled { v.to_sample::<i16>() } else { 0 };
                                            let _ = writer.write_sample(s);
                                        }
                                    }
                                }
                            },
                            err_fn,
                            Some(Duration::from_millis(100)),
                        )
                        .map_err(|e| format!("创建输入流失败: {}", e))?
                }
                _ => return Err("不支持的采样格式".to_string()),
            };
            stream
                .play()
                .map_err(|e| format!("启动输入流失败: {}", e))?;
            let _ = tx.send(Ok(()));

            // 等待停止信号
            while !thread_stop_flag.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(100));
            }

            // 关键修复：收到停止信号后，继续录制更长时间以确保音频完全覆盖视频最后一段
            // 测试表明FFmpeg收到"q"命令后处理最后一帧的时间可能超过2秒
            log::info!("收到停止信号，继续录制2000ms以确保音频完全覆盖视频最后一段...");
            std::thread::sleep(Duration::from_millis(2000));

            // 停止音频流，让CPAL完成最后的回调
            let _ = stream.pause();

            // 等待一小段时间，确保音频回调处理完最后的缓冲区数据
            // 这是必要的，因为CPAL可能在回调中还有待处理的数据
            std::thread::sleep(Duration::from_millis(300));

            // 销毁流，释放资源
            drop(stream);

            // 确保所有音频数据都已写入文件
            if let Ok(mut guard) = writer.lock() {
                if let Some(w) = guard.take() {
                    let _ = w.finalize();
                }
            }

            // 记录文件最终状态
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

    let init = rx
        .recv_timeout(Duration::from_secs(2))
        .map_err(|_| "启动 WASAPI 捕获超时".to_string())??;
    let _ = init;

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
    let stop_flag = Arc::new(AtomicBool::new(false));
    let thread_stop_flag = stop_flag.clone();
    let thread_output = output_path.clone();
    let thread_device_key = device_desc_key.clone();
    let (tx, rx) = mpsc::channel::<Result<(), String>>();

    let handle = std::thread::spawn(move || {
        let run = || -> Result<(), String> {
            let host = cpal::host_from_id(cpal::HostId::Wasapi)
                .map_err(|e| format!("WASAPI 主机不可用: {}", e))?;
            let device = if let Some(key) = thread_device_key.as_ref() {
                if let Ok(devs) = host.input_devices() {
                    let mut picked = None;
                    for d in devs {
                        if let Ok(desc) = d.description() {
                            if desc.to_string() == *key {
                                picked = Some(d);
                                break;
                            }
                        }
                    }
                    picked
                } else {
                    None
                }
            } else {
                None
            }
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
            // 🔧 性能优化：使用 1MB 大缓冲区的 BufWriter 避免高频 I/O 瓶颈
            let file = std::fs::File::create(&thread_output)
                .map_err(|e| format!("创建麦克风 wav 文件失败: {}", e))?;
            let buf_writer = std::io::BufWriter::with_capacity(1024 * 1024, file);
            let writer = hound::WavWriter::new(buf_writer, spec)
                .map_err(|e| format!("初始化麦克风 WAV 写入器失败: {}", e))?;
            let writer = Arc::new(Mutex::new(Some(writer)));
            let writer_cb = writer.clone();
            let enabled_cb = enabled_flag.clone();
            let pause_cb = recording_pause_flag.clone();

            // ✅ 添加诊断日志：记录麦克风线程启动时的状态
            log::info!(
                "WASAPI麦克风线程启动: {:?}, enabled={}, pause={}",
                thread_output.file_name(),
                enabled_flag.load(Ordering::SeqCst),
                recording_pause_flag.load(Ordering::SeqCst)
            );

            let err_fn = |err| eprintln!("WASAPI 麦克风捕获错误: {}", err);

            // ✅ 添加诊断日志：记录文件创建状态
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
                CpalSampleFormat::F32 => device
                    .build_input_stream(
                        &config,
                        move |data: &[f32], _| {
                            if let Ok(mut guard) = writer_cb.lock() {
                                if let Some(writer) = guard.as_mut() {
                                    let enabled = enabled_cb.load(Ordering::SeqCst)
                                        && !pause_cb.load(Ordering::SeqCst);
                                    for &v in data {
                                        let s = if enabled {
                                            (v * i16::MAX as f32) as i16
                                        } else {
                                            0
                                        };
                                        let _ = writer.write_sample(s);
                                    }
                                }
                            }
                        },
                        err_fn,
                        Some(Duration::from_millis(100)),
                    )
                    .map_err(|e| format!("创建麦克风输入流失败: {}", e))?,
                CpalSampleFormat::I16 => {
                    let writer_cb = writer.clone();
                    let enabled_cb = enabled_flag.clone();
                    let pause_cb = recording_pause_flag.clone();
                    device
                        .build_input_stream(
                            &config,
                            move |data: &[i16], _| {
                                if let Ok(mut guard) = writer_cb.lock() {
                                    if let Some(writer) = guard.as_mut() {
                                        let enabled = enabled_cb.load(Ordering::SeqCst)
                                            && !pause_cb.load(Ordering::SeqCst);
                                        for &v in data {
                                            let _ =
                                                writer.write_sample(if enabled { v } else { 0 });
                                        }
                                    }
                                }
                            },
                            err_fn,
                            Some(Duration::from_millis(100)),
                        )
                        .map_err(|e| format!("创建麦克风输入流失败: {}", e))?
                }
                CpalSampleFormat::U16 => {
                    let writer_cb = writer.clone();
                    let enabled_cb = enabled_flag.clone();
                    let pause_cb = recording_pause_flag.clone();
                    device
                        .build_input_stream(
                            &config,
                            move |data: &[u16], _| {
                                if let Ok(mut guard) = writer_cb.lock() {
                                    if let Some(writer) = guard.as_mut() {
                                        let enabled = enabled_cb.load(Ordering::SeqCst)
                                            && !pause_cb.load(Ordering::SeqCst);
                                        for &v in data {
                                            let s: i16 =
                                                if enabled { v.to_sample::<i16>() } else { 0 };
                                            let _ = writer.write_sample(s);
                                        }
                                    }
                                }
                            },
                            err_fn,
                            Some(Duration::from_millis(100)),
                        )
                        .map_err(|e| format!("创建麦克风输入流失败: {}", e))?
                }
                CpalSampleFormat::I8 => {
                    let writer_cb = writer.clone();
                    let enabled_cb = enabled_flag.clone();
                    let pause_cb = recording_pause_flag.clone();
                    device
                        .build_input_stream(
                            &config,
                            move |data: &[i8], _| {
                                if let Ok(mut guard) = writer_cb.lock() {
                                    if let Some(writer) = guard.as_mut() {
                                        let enabled = enabled_cb.load(Ordering::SeqCst)
                                            && !pause_cb.load(Ordering::SeqCst);
                                        for &v in data {
                                            let s: i16 =
                                                if enabled { v.to_sample::<i16>() } else { 0 };
                                            let _ = writer.write_sample(s);
                                        }
                                    }
                                }
                            },
                            err_fn,
                            Some(Duration::from_millis(100)),
                        )
                        .map_err(|e| format!("创建麦克风输入流失败: {}", e))?
                }
                CpalSampleFormat::U8 => {
                    let writer_cb = writer.clone();
                    let enabled_cb = enabled_flag.clone();
                    let pause_cb = recording_pause_flag.clone();
                    device
                        .build_input_stream(
                            &config,
                            move |data: &[u8], _| {
                                if let Ok(mut guard) = writer_cb.lock() {
                                    if let Some(writer) = guard.as_mut() {
                                        let enabled = enabled_cb.load(Ordering::SeqCst)
                                            && !pause_cb.load(Ordering::SeqCst);
                                        for &v in data {
                                            let s: i16 =
                                                if enabled { v.to_sample::<i16>() } else { 0 };
                                            let _ = writer.write_sample(s);
                                        }
                                    }
                                }
                            },
                            err_fn,
                            Some(Duration::from_millis(100)),
                        )
                        .map_err(|e| format!("创建麦克风输入流失败: {}", e))?
                }
                CpalSampleFormat::I32 => {
                    let writer_cb = writer.clone();
                    let enabled_cb = enabled_flag.clone();
                    let pause_cb = recording_pause_flag.clone();
                    device
                        .build_input_stream(
                            &config,
                            move |data: &[i32], _| {
                                if let Ok(mut guard) = writer_cb.lock() {
                                    if let Some(writer) = guard.as_mut() {
                                        let enabled = enabled_cb.load(Ordering::SeqCst)
                                            && !pause_cb.load(Ordering::SeqCst);
                                        for &v in data {
                                            let s: i16 =
                                                if enabled { v.to_sample::<i16>() } else { 0 };
                                            let _ = writer.write_sample(s);
                                        }
                                    }
                                }
                            },
                            err_fn,
                            Some(Duration::from_millis(100)),
                        )
                        .map_err(|e| format!("创建麦克风输入流失败: {}", e))?
                }
                CpalSampleFormat::U32 => {
                    let writer_cb = writer.clone();
                    let enabled_cb = enabled_flag.clone();
                    let pause_cb = recording_pause_flag.clone();
                    device
                        .build_input_stream(
                            &config,
                            move |data: &[u32], _| {
                                if let Ok(mut guard) = writer_cb.lock() {
                                    if let Some(writer) = guard.as_mut() {
                                        let enabled = enabled_cb.load(Ordering::SeqCst)
                                            && !pause_cb.load(Ordering::SeqCst);
                                        for &v in data {
                                            let s: i16 =
                                                if enabled { v.to_sample::<i16>() } else { 0 };
                                            let _ = writer.write_sample(s);
                                        }
                                    }
                                }
                            },
                            err_fn,
                            Some(Duration::from_millis(100)),
                        )
                        .map_err(|e| format!("创建麦克风输入流失败: {}", e))?
                }
                CpalSampleFormat::F64 => {
                    let writer_cb = writer.clone();
                    let enabled_cb = enabled_flag.clone();
                    let pause_cb = recording_pause_flag.clone();
                    device
                        .build_input_stream(
                            &config,
                            move |data: &[f64], _| {
                                if let Ok(mut guard) = writer_cb.lock() {
                                    if let Some(writer) = guard.as_mut() {
                                        let enabled = enabled_cb.load(Ordering::SeqCst)
                                            && !pause_cb.load(Ordering::SeqCst);
                                        for &v in data {
                                            let s: i16 =
                                                if enabled { v.to_sample::<i16>() } else { 0 };
                                            let _ = writer.write_sample(s);
                                        }
                                    }
                                }
                            },
                            err_fn,
                            Some(Duration::from_millis(100)),
                        )
                        .map_err(|e| format!("创建麦克风输入流失败: {}", e))?
                }
                _ => return Err("不支持的采样格式".to_string()),
            };
            stream
                .play()
                .map_err(|e| format!("启动麦克风输入流失败: {}", e))?;
            let _ = tx.send(Ok(()));

            // 等待停止信号
            while !thread_stop_flag.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(100));
            }

            // 🔧 优化：麦克风停止时不额外延迟，立即关闭流
            // 原因：enabled_flag=false时已写入静音，继续录制只会增加文件大小
            log::info!("收到麦克风停止信号，立即停止音频流...");

            // 停止音频流，让CPAL完成最后的回调
            let _ = stream.pause();

            // 短暂等待确保音频回调处理完最后的缓冲区数据
            std::thread::sleep(Duration::from_millis(100));

            // 销毁流，释放资源
            drop(stream);

            // 确保所有音频数据都已写入文件
            if let Ok(mut guard) = writer.lock() {
                if let Some(w) = guard.take() {
                    let _ = w.finalize();
                }
            }

            // 记录文件最终状态
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

    let init = rx
        .recv_timeout(Duration::from_secs(2))
        .map_err(|_| "启动 WASAPI 麦克风捕获超时".to_string())??;
    let _ = init;

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
) -> Result<WasapiFfmpegHandle, String> {
    let stop_flag = Arc::new(AtomicBool::new(false));
    let thread_stop_flag = stop_flag.clone();
    let thread_output = output_path.clone();
    let thread_device_key = device_desc_key.clone();
    let ffmpeg_child = Arc::new(Mutex::new(None::<ChildGuard>));
    let thread_ffmpeg = ffmpeg_child.clone();
    let (tx, rx) = mpsc::channel::<Result<(), String>>();

    let handle = std::thread::spawn(move || {
        let run = || -> Result<(), String> {
            // 🔧 解析 FFmpeg 路径
            let ffmpeg_path = crate::features::recording::ffmpeg_runner::resolve_ffmpeg_path()
                .map_err(|e| format!("解析 FFmpeg 路径失败: {}", e))?;

            // 1. 启动 FFmpeg 子进程，从 stdin 读取 PCM F32LE，输出 AAC
            let mut ffmpeg_cmd = Command::new(&ffmpeg_path);
            #[cfg(target_os = "windows")]
            {
                use std::os::windows::process::CommandExt;
                const CREATE_NO_WINDOW: u32 = 0x0800_0000;
                ffmpeg_cmd.creation_flags(CREATE_NO_WINDOW);
            }

            ffmpeg_cmd
                .args(&[
                    "-f",
                    "f32le", // 输入格式：32位浮点小端
                    "-ar",
                    "48000", // 采样率：48kHz
                    "-ac",
                    "2", // 声道数：立体声
                    "-i",
                    "-", // 从 stdin 读取
                    "-c:a",
                    "aac", // 编码器：AAC
                    "-b:a",
                    "128k", // 比特率：128kbps
                    "-profile:a",
                    "aac_low", // 快速配置
                    "-y",      // 覆盖输出文件
                    thread_output.to_str().ok_or("无效的输出路径")?,
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            let mut child = ffmpeg_cmd
                .spawn()
                .map_err(|e| format!("启动 FFmpeg 失败: {}", e))?;

            let stdin = child.stdin.take().ok_or("无法获取 FFmpeg stdin")?;

            {
                if let Ok(mut guard) = thread_ffmpeg.lock() {
                    *guard = Some(ChildGuard(child));
                }
            }

            log::info!(
                "🔧 FFmpeg AAC 编码管道已启动: {:?}",
                thread_output.file_name()
            );

            // 2. WASAPI 捕获音频并写入 FFmpeg stdin
            let host = cpal::host_from_id(cpal::HostId::Wasapi)
                .map_err(|e| format!("WASAPI 主机不可用: {}", e))?;

            let device = if let Some(key) = thread_device_key.as_ref() {
                if let Ok(devs) = host.output_devices() {
                    let mut picked = None;
                    for d in devs {
                        if let Ok(desc) = d.description() {
                            if desc.to_string() == *key {
                                picked = Some(d);
                                break;
                            }
                        }
                    }
                    picked
                } else {
                    None
                }
            } else {
                None
            }
            .or_else(|| host.default_output_device())
            .ok_or_else(|| "未找到输出设备".to_string())?;

            let config: StreamConfig = StreamConfig {
                channels: 2,
                sample_rate: 48_000,
                buffer_size: cpal::BufferSize::Default,
            };

            // 3. 将音频数据写入 FFmpeg stdin（无锁缓冲池模式）
            let (tx_audio, rx_audio) = std::sync::mpsc::sync_channel::<Vec<u8>>(100);
            let (tx_pool, rx_pool) = std::sync::mpsc::sync_channel::<Vec<u8>>(100);
            for _ in 0..50 {
                let _ = tx_pool.try_send(Vec::with_capacity(4096));
            }
            
            let mut writer_opt = Some(std::io::BufWriter::new(stdin));
            let writer_thread = std::thread::spawn(move || {
                while let Ok(mut data) = rx_audio.recv() {
                    if data.is_empty() {
                        break;
                    }
                    if let Some(writer) = writer_opt.as_mut() {
                        let _ = writer.write_all(&data);
                        let _ = writer.flush();
                    }
                    data.clear();
                    let _ = tx_pool.try_send(data);
                }
                if let Some(writer) = writer_opt.take() {
                    drop(writer); // Close stdin triggers FFmpeg EOF
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
                        let enabled = enabled_cb.load(Ordering::SeqCst)
                            && !pause_cb.load(Ordering::SeqCst);
                        let mut buffer = rx_pool.try_recv().unwrap_or_else(|_| Vec::with_capacity(data.len() * 4));
                        buffer.clear();
                        if enabled {
                            for &sample in data {
                                buffer.extend_from_slice(&sample.to_le_bytes());
                            }
                        } else {
                            buffer.resize(data.len() * 4, 0);
                        }
                        let _ = tx_cb.try_send(buffer);
                    },
                    err_fn,
                    Some(Duration::from_millis(100)),
                )
                .map_err(|e| format!("创建输入流失败: {}", e))?;

            stream
                .play()
                .map_err(|e| format!("启动输入流失败: {}", e))?;
            let _ = tx.send(Ok(()));

            // 等待停止信号
            while !thread_stop_flag.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(100));
            }

            // 🔧 优化：系统音频停止时不固定sleep，而是等待FFmpeg编码完成
            let stop_start = std::time::Instant::now();
            log::info!("收到系统音频停止信号，关闭FFmpeg输入流...");

            drop(stream);
            log::info!("WASAPI 流已停止");

            // 关闭 stdin 触发 FFmpeg 完成编码
            let _ = tx_audio.send(Vec::new()); // send EOF to writer thread
            let _ = writer_thread.join(); // Wait for writer thread to finish closing stdin

            // 🔧 主动等待 FFmpeg 进程退出（带超时）
            log::info!("等待 FFmpeg AAC 编码完成...");
            if let Ok(mut guard) = thread_ffmpeg.lock() {
                if let Some(ref mut child) = *guard {
                    // 等待FFmpeg退出，最多15秒
                    for _ in 0..150 {
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
                                std::thread::sleep(Duration::from_millis(100));
                            }
                            Err(e) => {
                                log::warn!("检查 FFmpeg 状态失败: {}", e);
                                break;
                            }
                        }
                    }
                    // 如果超时，强制杀死进程
                    if child.0.try_wait().ok().flatten().is_none() {
                        log::warn!("FFmpeg 编码超时，强制终止");
                        let _ = child.0.kill();
                    }
                }
            }
            Ok(())
        };
        if let Err(e) = run() {
            let _ = tx.send(Err(e));
        }
    });

    let init = rx
        .recv_timeout(Duration::from_secs(2))
        .map_err(|_| "启动 WASAPI+FFmpeg 捕获超时".to_string())??;
    let _ = init;

    Ok(WasapiFfmpegHandle {
        stop_flag,
        join: Some(handle),
        output_path,
        ffmpeg_child,
    })
}
