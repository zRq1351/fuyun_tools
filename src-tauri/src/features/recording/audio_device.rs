use crate::features::recording::types::AudioInputDevice;
use std::path::Path;

pub fn list_microphones(_ffmpeg_path: &Path) -> Result<Vec<AudioInputDevice>, String> {
    #[cfg(target_os = "windows")]
    {
        use cpal::traits::{DeviceTrait, HostTrait};
        let host = cpal::host_from_id(cpal::HostId::Wasapi)
            .map_err(|e| format!("WASAPI 主机不可用: {}", e))?;
        let default_desc = host
            .default_input_device()
            .and_then(|d| d.description().ok().map(|x| x.to_string()));
        let mut out = Vec::new();
        let devices = host
            .input_devices()
            .map_err(|e| format!("枚举输入设备失败: {}", e))?;
        for d in devices {
            let desc = d
                .description()
                .map(|x| x.to_string())
                .unwrap_or_else(|_| "Unknown Input".to_string());
            out.push(AudioInputDevice {
                id: desc.clone(),
                name: desc.clone(),
                is_default: default_desc.as_deref() == Some(desc.as_str()),
            });
        }
        Ok(out)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = _ffmpeg_path;
        Ok(Vec::new())
    }
}

pub fn list_system_audio_sources(ffmpeg_path: &Path) -> Result<Vec<AudioInputDevice>, String> {
    let _ = ffmpeg_path;
    #[cfg(target_os = "windows")]
    {
        use cpal::traits::{DeviceTrait, HostTrait};
        let host = cpal::host_from_id(cpal::HostId::Wasapi)
            .map_err(|e| format!("WASAPI 主机不可用: {}", e))?;
        let default_desc = host
            .default_output_device()
            .and_then(|d| d.description().ok().map(|x| x.to_string()));
        let mut out = Vec::new();
        let devices = host
            .output_devices()
            .map_err(|e| format!("枚举输出设备失败: {}", e))?;
        for d in devices {
            let desc = d
                .description()
                .map(|x| x.to_string())
                .unwrap_or_else(|_| "Unknown Output".to_string());
            out.push(AudioInputDevice {
                id: desc.clone(),
                name: format!("WASAPI {}", desc),
                is_default: default_desc.as_deref() == Some(desc.as_str()),
            });
        }
        if out.is_empty() {
            return Err("未检测到可用输出设备".to_string());
        }
        return Ok(out);
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(Vec::new())
    }
}

pub fn probe_system_audio_source(ffmpeg_path: &Path, source: &str) -> bool {
    let _ = ffmpeg_path;
    let _ = source;
    #[cfg(target_os = "windows")]
    {
        list_system_audio_sources(Path::new("")).map(|v| !v.is_empty()).unwrap_or(false)
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/* 旧 ffmpeg 系统音频探测路径已移除，改为 Rust 原生 WASAPI 捕获 */
