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
        if path == PathBuf::from("ffmpeg") || path == PathBuf::from("ffmpeg.exe") {
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
    let final_name = format!("{}.mp4", final_name.trim());
    let tmp_name = format!("{}.tmp.mp4", session_id);
    (
        output_dir.join(tmp_name),
        output_dir.join(final_name),
        session_id,
    )
}
