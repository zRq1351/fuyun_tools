use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat as CpalSampleFormat, StreamConfig};
use hound::{SampleFormat, WavWriter};
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

pub struct WasapiCaptureHandle {
    pub stop_flag: Arc<AtomicBool>,
    pub join: Option<std::thread::JoinHandle<()>>,
    pub output_path: PathBuf,
}

impl WasapiCaptureHandle {
    pub fn stop(self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        if let Some(join) = self.join {
            let _ = join.join();
        }
    }
}

pub fn start_system_loopback_wav_with_device(
    device_desc_key: Option<String>,
    output_path: PathBuf,
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
            let writer = WavWriter::create(&thread_output, spec)
                .map_err(|e| format!("创建 wav 文件失败: {}", e))?;
            let writer = Arc::new(Mutex::new(Some(writer)));
            let writer_cb = writer.clone();
            let err_fn = |err| eprintln!("WASAPI 捕获错误: {}", err);
            let stream = match sample_format {
                CpalSampleFormat::F32 => device
                    .build_input_stream(
                        &config,
                        move |data: &[f32], _| {
                            if let Ok(mut guard) = writer_cb.lock() {
                                if let Some(writer) = guard.as_mut() {
                                    for &v in data {
                                        let s = (v * i16::MAX as f32) as i16;
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
                    device
                        .build_input_stream(
                            &config,
                            move |data: &[i16], _| {
                                if let Ok(mut guard) = writer_cb.lock() {
                                    if let Some(writer) = guard.as_mut() {
                                        for &v in data {
                                            let _ = writer.write_sample(v);
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
                    device
                        .build_input_stream(
                            &config,
                            move |data: &[u16], _| {
                                if let Ok(mut guard) = writer_cb.lock() {
                                    if let Some(writer) = guard.as_mut() {
                                        for &v in data {
                                            let s: i16 = v.to_sample::<i16>();
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
            stream.play().map_err(|e| format!("启动输入流失败: {}", e))?;
            let _ = tx.send(Ok(()));
            while !thread_stop_flag.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(100));
            }
            drop(stream);
            if let Ok(mut guard) = writer.lock() {
                if let Some(w) = guard.take() {
                    let _ = w.finalize();
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
        .map_err(|_| "启动 WASAPI 捕获超时".to_string())??;
    let _ = init;

    Ok(WasapiCaptureHandle {
        stop_flag,
        join: Some(handle),
        output_path,
    })
}

// 兼容旧签名
pub fn start_system_loopback_wav(output_path: PathBuf) -> Result<WasapiCaptureHandle, String> {
    start_system_loopback_wav_with_device(None, output_path)
}

pub fn start_microphone_wav_with_device(
    device_desc_key: Option<String>,
    output_path: PathBuf,
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
            let writer = WavWriter::create(&thread_output, spec)
                .map_err(|e| format!("创建麦克风 wav 文件失败: {}", e))?;
            let writer = Arc::new(Mutex::new(Some(writer)));
            let writer_cb = writer.clone();
            let err_fn = |err| eprintln!("WASAPI 麦克风捕获错误: {}", err);
            let stream = match sample_format {
                CpalSampleFormat::F32 => device
                    .build_input_stream(
                        &config,
                        move |data: &[f32], _| {
                            if let Ok(mut guard) = writer_cb.lock() {
                                if let Some(writer) = guard.as_mut() {
                                    for &v in data {
                                        let s = (v * i16::MAX as f32) as i16;
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
                    device
                        .build_input_stream(
                            &config,
                            move |data: &[i16], _| {
                                if let Ok(mut guard) = writer_cb.lock() {
                                    if let Some(writer) = guard.as_mut() {
                                        for &v in data {
                                            let _ = writer.write_sample(v);
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
                    device
                        .build_input_stream(
                            &config,
                            move |data: &[u16], _| {
                                if let Ok(mut guard) = writer_cb.lock() {
                                    if let Some(writer) = guard.as_mut() {
                                        for &v in data {
                                            let s: i16 = v.to_sample::<i16>();
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
            stream.play().map_err(|e| format!("启动麦克风输入流失败: {}", e))?;
            let _ = tx.send(Ok(()));
            while !thread_stop_flag.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(100));
            }
            drop(stream);
            if let Ok(mut guard) = writer.lock() {
                if let Some(w) = guard.take() {
                    let _ = w.finalize();
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
        .map_err(|_| "启动 WASAPI 麦克风捕获超时".to_string())??;
    let _ = init;

    Ok(WasapiCaptureHandle {
        stop_flag,
        join: Some(handle),
        output_path,
    })
}
