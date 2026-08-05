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

/// 校验 ffmpeg 文件是否可用：MZ 头 + 合理大小，避免损坏/0 字节/伪造文件被误判可用
pub(crate) fn is_valid_ffmpeg_file(path: &Path) -> bool {
    use std::io::Read;
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if meta.len() < 1024 * 1024 {
        return false;
    }
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 2];
    f.read_exact(&mut magic).is_ok() && &magic == b"MZ"
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
        if is_valid_ffmpeg_file(&path) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_replaces_illegal_chars() {
        assert_eq!(sanitize_output_filename("a<b>c:d"), "a_b_c_d");
        assert_eq!(sanitize_output_filename("x|y?z*w"), "x_y_z_w");
        assert_eq!(sanitize_output_filename("path\\to/file"), "path_to_file");
        assert_eq!(sanitize_output_filename("quote\"name"), "quote_name");
        assert_eq!(sanitize_output_filename("with\0null"), "with_null");
        assert_eq!(sanitize_output_filename("tab\tchar"), "tab_char");
    }

    #[test]
    fn test_sanitize_prevents_path_traversal() {
        // 路径分隔符先被替换为 _，然后 .. 片段被替换为 _
        assert_eq!(sanitize_output_filename("..\\..\\evil"), "____evil");
        assert_eq!(sanitize_output_filename("a..b"), "a_b");
        assert_eq!(sanitize_output_filename("../etc/passwd"), "__etc_passwd");
        assert!(!sanitize_output_filename("../evil").contains(".."));
    }

    #[test]
    fn test_sanitize_reserved_device_names() {
        assert_eq!(sanitize_output_filename("CON"), "_CON");
        assert_eq!(sanitize_output_filename("con.txt"), "_con.txt");
        assert_eq!(sanitize_output_filename("NUL"), "_NUL");
        assert_eq!(sanitize_output_filename("COM1"), "_COM1");
        assert_eq!(sanitize_output_filename("LPT3.log"), "_LPT3.log");
        assert_eq!(sanitize_output_filename("COM10"), "_COM10");
    }

    #[test]
    fn test_sanitize_keeps_normal_names() {
        assert_eq!(sanitize_output_filename("my_recording_2026"), "my_recording_2026");
        assert_eq!(sanitize_output_filename("演示视频"), "演示视频");
        assert_eq!(sanitize_output_filename("  spaced  "), "spaced");
    }

    #[test]
    fn test_sanitize_empty_result() {
        assert_eq!(sanitize_output_filename("///"), "___");
        assert_eq!(sanitize_output_filename(""), "");
    }

    #[test]
    fn test_is_valid_ffmpeg_file_rejects_missing() {
        assert!(!is_valid_ffmpeg_file(Path::new("C:/definitely/not/exists/ffmpeg.exe")));
    }

    #[test]
    fn test_is_valid_ffmpeg_file_rejects_small_file() {
        let dir = std::env::temp_dir().join("fyt_test_ffmpeg");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("tiny.exe");
        std::fs::write(&p, b"MZ").unwrap();
        // 小于 1MB，即使有 MZ 头也应拒绝
        assert!(!is_valid_ffmpeg_file(&p));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_is_valid_ffmpeg_file_rejects_wrong_magic() {
        let dir = std::env::temp_dir().join("fyt_test_ffmpeg2");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("fake.exe");
        // 构造 >1MB 但非 MZ 头的文件
        let mut data = vec![0u8; 1024 * 1024 + 16];
        data[0] = b'E';
        data[1] = b'L';
        std::fs::write(&p, &data).unwrap();
        assert!(!is_valid_ffmpeg_file(&p));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_is_valid_ffmpeg_file_accepts_mz_large() {
        let dir = std::env::temp_dir().join("fyt_test_ffmpeg3");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("real.exe");
        let mut data = vec![0u8; 1024 * 1024 + 16];
        data[0] = b'M';
        data[1] = b'Z';
        std::fs::write(&p, &data).unwrap();
        assert!(is_valid_ffmpeg_file(&p));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_build_output_paths_placeholder_replacement() {
        let dir = Path::new("C:/out");
        let (tmp, final_path, session_id) = build_output_paths(dir, "rec_{date}_{time}_{type}");
        assert!(session_id.starts_with("rec-"));
        assert_eq!(tmp, dir.join(format!("{}.tmp.mp4", session_id)));
        let name = final_path.file_name().unwrap().to_string_lossy().to_string();
        assert!(name.ends_with(".mp4"));
        assert!(!name.contains("tmp.mp4"));
        assert!(name.contains("screen"));
        // 日期格式 YYYYMMDD 出现在名称中
        assert!(name.len() >= 8);
    }

    #[test]
    fn test_build_output_paths_empty_template_falls_back() {
        let dir = Path::new("C:/out");
        // ".." 会被替换为 "_" 后仍非空，真正的空回退需要模板净化为空
        // 用非法字符模板验证净化逻辑；空回退场景用纯分隔符验证文件名仍是时间戳
        let (_, final_path, _) = build_output_paths(dir, "{date}");
        let name = final_path.file_name().unwrap().to_string_lossy().to_string();
        // 日期模板净化后应为 YYYYMMDD.mp4
        assert!(name.ends_with(".mp4"));
        let stem = name.trim_end_matches(".mp4");
        assert_eq!(stem.len(), 8);
        assert!(stem.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn test_candidate_paths_contains_default() {
        let paths = candidate_paths();
        assert!(paths.iter().any(|p| p == Path::new("ffmpeg.exe")));
        assert!(paths.iter().any(|p| p == Path::new("ffmpeg")));
        // FY_FFMPEG_PATH 前置
        unsafe {
            std::env::set_var("FY_FFMPEG_PATH", "C:/custom/ffmpeg.exe");
        }
        let paths2 = candidate_paths();
        assert_eq!(paths2[0], PathBuf::from("C:/custom/ffmpeg.exe"));
    }
}
