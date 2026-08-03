use std::env;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn suppress_console_window(command: &mut Command) -> &mut Command {
    #[cfg(target_os = "windows")]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

fn candidate_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(explicit) = env::var("FY_FFMPEG_PATH") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            candidates.push(PathBuf::from(trimmed));
        }
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("ffmpeg.exe"));
            candidates.push(parent.join("resources").join("ffmpeg.exe"));
            candidates.push(parent.join("sidecar").join("ffmpeg.exe"));
            candidates.push(parent.join("bin").join("ffmpeg.exe"));
        }
    }
    if let Ok(cwd) = env::current_dir() {
        candidates.push(cwd.join("ffmpeg.exe"));
        candidates.push(cwd.join("src-tauri").join("bin").join("ffmpeg.exe"));
        if let Some(parent) = cwd.parent() {
            candidates.push(parent.join("src-tauri").join("bin").join("ffmpeg.exe"));
        }
    }
    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        let manifest = PathBuf::from(manifest_dir);
        candidates.push(manifest.join("bin").join("ffmpeg.exe"));
        if let Some(parent) = manifest.parent() {
            candidates.push(parent.join("src-tauri").join("bin").join("ffmpeg.exe"));
        }
    }
    candidates.push(PathBuf::from("ffmpeg.exe"));
    candidates.push(PathBuf::from("ffmpeg"));
    candidates
}

pub fn resolve_ffmpeg_path() -> Result<PathBuf, String> {
    let mut checked = Vec::new();
    for path in candidate_paths() {
        checked.push(path.to_string_lossy().to_string());
        if path == Path::new("ffmpeg") || path == Path::new("ffmpeg.exe") {
            let mut probe = Command::new(&path);
            suppress_console_window(&mut probe);
            if probe.arg("-version").output().is_ok() {
                return Ok(path);
            }
            continue;
        }
        if path.exists() && path.is_file() {
            return Ok(path);
        }
    }
    Err(format!(
        "未找到 ffmpeg 可执行文件。可将 ffmpeg.exe 放到 src-tauri/bin，或设置环境变量 FY_FFMPEG_PATH。已检查路径: {}",
        checked.join(" | ")
    ))
}

pub fn build_output_paths(output_dir: &Path, naming_template: &str) -> (PathBuf, PathBuf, String) {
    let now = chrono::Local::now();
    let timestamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let session_id = format!("rec-{}", timestamp_ms);
    let final_name = naming_template
        .replace("{timestamp}", &timestamp_ms.to_string())
        .replace("{date}", &now.format("%Y%m%d").to_string())
        .replace("{time}", &now.format("%H%M%S").to_string())
        .replace("{type}", "screen");
    let final_name = sanitize_output_filename(&final_name);
    // 净化后为空（如纯分隔符模板）时回退到时间戳，避免生成非法/穿越路径
    let final_name = if final_name.is_empty() {
        timestamp_ms.to_string()
    } else {
        final_name
    };
    let final_name = format!("{}.mp4", final_name);
    let tmp_name = format!("{}.tmp.mp4", session_id);
    (
        output_dir.join(tmp_name),
        output_dir.join(final_name),
        session_id,
    )
}

/// 净化输出文件名：替换路径分隔符与 Windows 非法字符、去除路径穿越片段、规避保留设备名
fn sanitize_output_filename(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.trim().chars() {
        match ch {
            '\\' | '/' | '<' | '>' | ':' | '"' | '|' | '?' | '*' | '\0' => out.push('_'),
            c if (c as u32) < 0x20 => out.push('_'),
            _ => out.push(ch),
        }
    }
    // 去除路径穿越片段
    let cleaned = out.replace("..", "_");
    // Windows 保留设备名（含扩展名前缀，如 CON.txt）
    let stem = cleaned.split('.').next().unwrap_or("").trim();
    let upper = stem.to_ascii_uppercase();
    let reserved = matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL"
            | "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6" | "COM7" | "COM8" | "COM9"
            | "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9"
    ) || upper.starts_with("COM")
        || upper.starts_with("LPT");
    if reserved {
        format!("_{}", cleaned)
    } else {
        cleaned
    }
}
