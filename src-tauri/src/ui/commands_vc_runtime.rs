use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use crate::core::error_codes::AppErrorKind;
#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

#[cfg(debug_assertions)]
static VC_RUNTIME_FORCE_MISSING: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub async fn check_vc_runtime_dependencies() -> Result<serde_json::Value, String> {
    #[cfg(windows)]
    {
        let win_dir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".to_string());
        let system32 = PathBuf::from(win_dir).join("System32");
        let app_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|x| x.to_path_buf()));
        let required = ["vcruntime140.dll", "vcruntime140_1.dll", "msvcp140.dll"];
        let missing: Vec<String> = required
            .iter()
            .filter_map(|name| {
                let in_system32 = system32.join(name).exists();
                let in_app_dir = app_dir
                    .as_ref()
                    .map(|dir| dir.join(name).exists())
                    .unwrap_or(false);
                if in_system32 || in_app_dir {
                    None
                } else {
                    Some((*name).to_string())
                }
            })
            .collect();
        #[cfg(debug_assertions)]
        if VC_RUNTIME_FORCE_MISSING.load(Ordering::Relaxed) {
            return Ok(serde_json::json!({
                "ok": false,
                "missing": required,
                "installUrl": "https://aka.ms/vs/17/release/vc_redist.x64.exe",
                "forcedByDev": true
            }));
        }
        return Ok(serde_json::json!({
            "ok": missing.is_empty(),
            "missing": missing,
            "installUrl": "https://aka.ms/vs/17/release/vc_redist.x64.exe",
            "forcedByDev": false
        }));
    }
    #[cfg(not(windows))]
    {
        Ok(serde_json::json!({
            "ok": true,
            "missing": [],
            "installUrl": ""
        }))
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct VcRuntimeDownloadProgress {
    phase: String,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    progress_percent: Option<u8>,
    message: String,
}

fn normalize_sha256_hex(raw: &str) -> Option<String> {
    let value = raw.trim().to_ascii_lowercase();
    if value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Some(value)
    } else {
        None
    }
}

fn split_download_url_and_sha256(raw: &str) -> Result<(String, Option<String>), String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppErrorKind::VcRuntimeDownloadUrlEmpty.to_frontend_json());
    }
    if let Some((url, fragment)) = trimmed.split_once("#sha256=") {
        let expected = normalize_sha256_hex(fragment)
            .ok_or_else(|| AppErrorKind::VcRuntimeDownloadUrlSha256Invalid.to_frontend_json())?;
        return Ok((url.trim().to_string(), Some(expected)));
    }
    Ok((trimmed.to_string(), None))
}

fn compute_file_sha256(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| format!("读取下载文件失败: {}", e))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("读取下载文件失败: {}", e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for b in digest {
        hex.push_str(format!("{:02x}", b).as_str());
    }
    Ok(hex)
}

fn verify_downloaded_exe_integrity(
    path: &Path,
    expected_sha256: Option<&str>,
) -> Result<(), String> {
    let mut header = [0u8; 2];
    let mut file = fs::File::open(path).map_err(|e| format!("读取下载文件失败: {}", e))?;
    file.read_exact(&mut header)
        .map_err(|e| format!("读取下载文件头失败: {}", e))?;
    if header != [b'M', b'Z'] {
        return Err("下载文件不是有效的 Windows 可执行文件".to_string());
    }
    if let Some(expected) = expected_sha256 {
        let actual = compute_file_sha256(path)?;
        if actual != expected {
            return Err(format!(
                "下载文件 SHA-256 校验失败，expected={}, actual={}",
                expected, actual
            ));
        }
    }
    Ok(())
}

fn validate_vc_runtime_installer_path(installer_path: &str) -> Result<PathBuf, String> {
    let raw = installer_path.trim();
    if raw.is_empty() {
        return Err("安装包路径不能为空".to_string());
    }
    let path = PathBuf::from(raw);
    if !path.exists() || !path.is_file() {
        return Err("安装包文件不存在，请重新下载".to_string());
    }
    let canonical = fs::canonicalize(&path).map_err(|e| format!("解析安装包路径失败: {}", e))?;
    let file_name = canonical
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if file_name != "vc_redist.x64.exe" {
        return Err("安装包文件名不合法，拒绝执行".to_string());
    }
    let allowed_root = fs::canonicalize(std::env::temp_dir().join("fuyun_tools"))
        .map_err(|e| format!("解析安装目录失败: {}", e))?;
    if !canonical.starts_with(&allowed_root) {
        return Err("安装包路径不在受信任目录，拒绝执行".to_string());
    }
    Ok(canonical)
}

#[tauri::command]
pub async fn download_vc_runtime_installer(
    download_url: Option<String>,
    app: AppHandle,
) -> Result<serde_json::Value, String> {
    #[cfg(windows)]
    {
        let default_url = "https://aka.ms/vs/17/release/vc_redist.x64.exe".to_string();
        let raw_url = download_url
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .unwrap_or(default_url);
        let (url, expected_sha256) = split_download_url_and_sha256(&raw_url)?;
        let parsed = reqwest::Url::parse(&url).map_err(|e| format!("下载地址无效: {}", e))?;
        if parsed.scheme() != "https" {
            return Err("下载地址必须使用 HTTPS".to_string());
        }
        let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
        if expected_sha256.is_none() && host != "aka.ms" {
            return Err("未提供 sha256 时，仅允许从 aka.ms 下载 VC Runtime".to_string());
        }
        let target_dir = std::env::temp_dir().join("fuyun_tools");
        fs::create_dir_all(&target_dir).map_err(|e| format!("创建目录失败: {}", e))?;
        let installer_path = target_dir.join("vc_redist.x64.exe");
        let tmp_path = target_dir.join("vc_redist.x64.exe.tmp");
        if tmp_path.exists() {
            let _ = fs::remove_file(&tmp_path);
        }

        let _ = app.emit(
            "vc-runtime-download-progress",
            VcRuntimeDownloadProgress {
                phase: "start".to_string(),
                downloaded_bytes: 0,
                total_bytes: None,
                progress_percent: Some(0),
                message: "开始下载 VC Runtime 安装包".to_string(),
            },
        );

        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("下载请求失败: {}", e))?;
        if !response.status().is_success() {
            return Err(format!(
                "下载 VC Runtime 失败，HTTP 状态: {}",
                response.status()
            ));
        }
        let total_bytes = response.content_length();
        let mut downloaded_bytes: u64 = 0;
        let mut stream = response.bytes_stream();
        let mut file = tokio::fs::File::create(&tmp_path)
            .await
            .map_err(|e| format!("创建临时文件失败: {}", e))?;

        while let Some(chunk_res) = stream.next().await {
            let chunk = chunk_res.map_err(|e| format!("下载数据流失败: {}", e))?;
            use tokio::io::AsyncWriteExt;
            file.write_all(&chunk)
                .await
                .map_err(|e| format!("写入临时文件失败: {}", e))?;
            downloaded_bytes = downloaded_bytes.saturating_add(chunk.len() as u64);
            let progress_percent = total_bytes.and_then(|total| {
                if total == 0 {
                    None
                } else {
                    Some(((downloaded_bytes.saturating_mul(100)) / total).min(100) as u8)
                }
            });
            let _ = app.emit(
                "vc-runtime-download-progress",
                VcRuntimeDownloadProgress {
                    phase: "downloading".to_string(),
                    downloaded_bytes,
                    total_bytes,
                    progress_percent,
                    message: "正在下载 VC Runtime 安装包".to_string(),
                },
            );
        }
        file.flush()
            .await
            .map_err(|e| format!("刷新下载文件失败: {}", e))?;
        let metadata = fs::metadata(&tmp_path).map_err(|e| format!("读取下载文件失败: {}", e))?;
        if metadata.len() == 0 {
            let _ = fs::remove_file(&tmp_path);
            return Err("下载结果为空文件，请重试".to_string());
        }
        verify_downloaded_exe_integrity(&tmp_path, expected_sha256.as_deref()).inspect_err(
            |_| {
                let _ = fs::remove_file(&tmp_path);
            },
        )?;
        fs::rename(&tmp_path, &installer_path)
            .or_else(|_| {
                if installer_path.exists() {
                    let _ = fs::remove_file(&installer_path);
                }
                fs::rename(&tmp_path, &installer_path)
            })
            .map_err(|e| format!("写入安装包失败: {}", e))?;

        let _ = app.emit(
            "vc-runtime-download-progress",
            VcRuntimeDownloadProgress {
                phase: "completed".to_string(),
                downloaded_bytes,
                total_bytes,
                progress_percent: Some(100),
                message: "VC Runtime 安装包下载完成".to_string(),
            },
        );

        return Ok(serde_json::json!({
            "installerPath": installer_path.to_string_lossy().to_string(),
            "downloadUrl": url
        }));
    }
    #[cfg(not(windows))]
    {
        Err("当前平台不支持 VC Runtime 下载".to_string())
    }
}

#[tauri::command]
pub async fn open_vc_runtime_installer(installer_path: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        let path = validate_vc_runtime_installer_path(&installer_path)?;
        std::process::Command::new(&path)
            .spawn()
            .map_err(|e| format!("启动安装程序失败: {}", e))?;
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        Err("当前平台不支持该操作".to_string())
    }
}

#[tauri::command]
pub async fn install_vc_runtime_and_wait(
    installer_path: String,
) -> Result<serde_json::Value, String> {
    #[cfg(windows)]
    {
        let path = validate_vc_runtime_installer_path(&installer_path)?;
        let status = tauri::async_runtime::spawn_blocking(move || {
            std::process::Command::new(&path)
                .arg("/install")
                .arg("/passive")
                .arg("/norestart")
                .status()
        })
            .await
            .map_err(|e| format!("启动安装程序失败: {}", e))?
            .map_err(|e| format!("执行安装程序失败: {}", e))?;
        let exit_code = status.code().unwrap_or(-1);
        let success = matches!(exit_code, 0 | 1638 | 3010);
        let cancelled = exit_code == 1602;
        let reboot_required = exit_code == 3010;
        return Ok(serde_json::json!({
            "success": success,
            "cancelled": cancelled,
            "rebootRequired": reboot_required,
            "exitCode": exit_code
        }));
    }
    #[cfg(not(windows))]
    {
        Err("当前平台不支持该操作".to_string())
    }
}

#[cfg(debug_assertions)]
#[tauri::command]
pub async fn get_vc_runtime_debug_state() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "forceMissing": VC_RUNTIME_FORCE_MISSING.load(Ordering::Relaxed)
    }))
}

#[cfg(debug_assertions)]
#[tauri::command]
pub async fn set_vc_runtime_debug_config(
    force_missing: Option<bool>,
) -> Result<serde_json::Value, String> {
    if let Some(enabled) = force_missing {
        VC_RUNTIME_FORCE_MISSING.store(enabled, Ordering::Relaxed);
    }
    Ok(serde_json::json!({
        "forceMissing": VC_RUNTIME_FORCE_MISSING.load(Ordering::Relaxed)
    }))
}
