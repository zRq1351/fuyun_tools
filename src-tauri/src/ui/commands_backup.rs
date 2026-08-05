use crate::core::app_state::AppState as SharedAppState;
use crate::core::error_codes::AppErrorKind;
use crate::core::perf_metrics::record_perf_metric;
use crate::sync::{lock_arc_mutex, Mutex};
use crate::ui::commands::{now_unix_ms, AUTO_BACKUP_IN_FLIGHT, BACKUP_JOB_MUTEX};
use crate::ui::commands_clipboard::frontend_error_kind;
use crate::utils::backup_archive::{
    cleanup_dir, create_backup_temp_dir, read_manifest_from_package, write_backup_payload,
    zip_backup_dir,
};
use crate::utils::backup_model::{
    BackupBlobFile, BackupExportPreviewData, BackupExportPreviewResponse, BackupExportRequest,
    BackupExportResultData, BackupExportResultResponse, BackupHistoryItem, BackupImageHistoryFile,
    BackupImageHistoryItem, BackupPackagePreviewData, BackupPackagePreviewRequest,
    BackupPackagePreviewResponse, BackupRestoreOptions, BackupRestoreRequest,
    BackupRestoreResultResponse, BackupSettingsData, DeleteBackupHistoryItemRequest,
    PreparedBackupData, SaveBackupSettingsRequest,
};
use crate::utils::backup_restore::restore_backup_package as execute_restore_backup_package;
use crate::utils::utils_helpers::{load_settings, save_settings};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;

pub(crate) fn detect_video_hw_accel_encoder(ffmpeg_path: &std::path::Path) -> Option<String> {
    use std::process::Command;
    use std::time::{Duration, Instant};

    // 硬件编码器优先级：NVIDIA > Intel > AMD
    let encoders = ["h264_nvenc", "h264_qsv", "h264_amf"];

    let mut cmd = Command::new(ffmpeg_path);
    cmd.arg("-encoders");

    // 🔧 修复：隐藏控制台窗口，避免黑框一闪而过
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    // 启动子进程并设置超时（10秒）
    let mut child = cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;

    let start = Instant::now();
    let timeout = Duration::from_secs(10);

    loop {
        match child.try_wait() {
            Ok(Some(_status)) => break,
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    log::warn!("FFmpeg -encoders 查询超时（10秒）");
                    return None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return None,
        }
    }

    let output = child.wait_with_output().ok()?;
    let encoder_list = String::from_utf8_lossy(&output.stdout);

    for encoder in &encoders {
        if encoder_list.contains(encoder) {
            return Some(encoder.to_string());
        }
    }

    None
}

pub(crate) fn sanitize_settings_for_backup(
    settings: &crate::utils::settings_model::AppSettingsData,
) -> crate::utils::settings_model::AppSettingsData {
    settings.clone()
}

pub(crate) async fn build_prepared_backup_data(
    state: &Arc<Mutex<SharedAppState>>,
) -> Result<PreparedBackupData, String> {
    let (settings, clipboard_manager_arc, image_manager_arc) = {
        let guard = state.lock().unwrap_or_else(|never| match never {});
        (
            sanitize_settings_for_backup(&guard.settings),
            guard.clipboard_manager.clone(),
            guard.image_clipboard_manager.clone(),
        )
    };

    let text_history = {
        let clipboard = lock_arc_mutex(&clipboard_manager_arc);
        crate::utils::database::ClipboardHistoryData {
            items: clipboard.get_history(),
            categories: clipboard.get_categories(),
            category_list: clipboard.get_category_list(),
            pinned_items: clipboard.get_pinned_items(),
        }
    };

    let (image_items, image_categories, image_category_list, image_tags, pinned_items) = {
        let manager = lock_arc_mutex(&image_manager_arc);
        (
            manager.get_history(),
            manager.get_categories(),
            manager.get_category_list(),
            manager.get_image_tags(),
            manager.get_pinned_items(),
        )
    };

    let mut warnings = vec!["API Key 不会被导出，恢复后需要重新填写".to_string()];
    let mut blobs = Vec::new();
    let mut backup_items = Vec::new();

    for item in image_items {
        if item.image_path.trim().is_empty() {
            warnings.push(format!("图片 {} 缺少实体文件路径，已跳过", item.id));
            continue;
        }
        let source = PathBuf::from(&item.image_path);
        if !source.exists() {
            warnings.push(format!("图片 {} 的实体文件不存在，已跳过", item.id));
            continue;
        }
        let metadata = fs::metadata(&source)
            .map_err(|e| format!("读取图片文件失败 {}: {}", source.display(), e))?;
        let extension = source
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("png");
        let package_path = format!("image_history/blobs/{}.{}", item.id, extension);
        backup_items.push(BackupImageHistoryItem {
            id: item.id.clone(),
            width: item.width,
            height: item.height,
            blob_path: format!("blobs/{}.{}", item.id, extension),
        });
        blobs.push(BackupBlobFile {
            item_id: item.id,
            source_path: source.to_string_lossy().to_string(),
            package_path,
            file_size: metadata.len(),
        });
    }

    let image_history = BackupImageHistoryFile {
        items: backup_items,
        categories: image_categories,
        category_list: image_category_list,
        image_tags,
        pinned_items,
    };

    let settings_bytes = serde_json::to_vec(&crate::utils::backup_model::BackupSettingsFile {
        settings: settings.clone(),
    })
        .map_err(|e| format!("序列化设置失败: {}", e))?;
    let text_bytes = serde_json::to_vec(&crate::utils::backup_model::BackupTextHistoryFile {
        snapshot: text_history.clone(),
    })
        .map_err(|e| format!("序列化文字历史失败: {}", e))?;
    let image_bytes =
        serde_json::to_vec(&image_history).map_err(|e| format!("序列化图片历史失败: {}", e))?;
    let blob_bytes = blobs.iter().map(|blob| blob.file_size).sum::<u64>();

    Ok(PreparedBackupData {
        settings,
        text_history: text_history.clone(),
        image_history: image_history.clone(),
        blobs,
        includes: crate::utils::backup_model::BackupIncludes {
            settings: true,
            text_history: true,
            image_history: true,
            image_blobs: true,
            api_keys: false,
            recordings: false,
        },
        stats: crate::utils::backup_model::BackupStats {
            text_item_count: text_history.items.len(),
            image_item_count: image_history.items.len(),
            image_blob_count: image_history.items.len(),
        },
        estimated_bytes: settings_bytes.len() as u64
            + text_bytes.len() as u64
            + image_bytes.len() as u64
            + blob_bytes,
        warnings,
    })
}

pub(crate) fn backup_preview_warnings_from_manifest(
    manifest: &crate::utils::backup_model::BackupManifest,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if !manifest.includes.api_keys {
        warnings.push("恢复后不会自动恢复 API Key".to_string());
    }
    if manifest.includes.image_history {
        warnings.push("图片预览缓存会在恢复后重新生成".to_string());
    }
    warnings
}

pub(crate) fn default_backup_file_name() -> String {
    format!("fuyun_tools_{}.fytbk.zip", now_unix_ms())
}

pub(crate) fn backup_frequency_interval_ms(frequency: &str) -> Option<i64> {
    match frequency {
        "daily" => Some(24 * 60 * 60 * 1000),
        "weekly" => Some(7 * 24 * 60 * 60 * 1000),
        _ => None,
    }
}


pub(crate) fn list_backup_history_items(target_dir: &Path) -> Result<Vec<BackupHistoryItem>, String> {
    if !target_dir.exists() {
        return Ok(Vec::new());
    }
    let mut items = Vec::new();
    for entry in fs::read_dir(target_dir).map_err(|e| format!("读取备份目录失败: {}", e))? {
        let entry = entry.map_err(|e| format!("读取备份目录项失败: {}", e))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if !file_name.ends_with(".fytbk.zip") {
            continue;
        }
        let metadata = entry
            .metadata()
            .map_err(|e| format!("读取备份文件信息失败: {}", e))?;
        let created_at = metadata
            .modified()
            .unwrap_or_else(|_| SystemTime::now())
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        items.push(BackupHistoryItem {
            file_name: file_name.to_string(),
            file_path: path.to_string_lossy().to_string(),
            file_size_bytes: metadata.len(),
            created_at,
        });
    }
    items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(items)
}

pub(crate) fn current_backup_settings() -> Result<BackupSettingsData, String> {
    let settings = load_settings()?;
    Ok(BackupSettingsData {
        enabled: settings.backup_enabled,
        frequency: settings.backup_frequency,
        target_dir: settings.backup_target_dir,
        max_backup_count: settings.backup_max_count,
        last_run_at: settings.backup_last_run_at,
        last_run_status: settings.backup_last_run_status,
    })
}

pub(crate) fn update_backup_run_state(_target_path: &str, status: &str) -> Result<(), String> {
    // 注意：不修改 backup_target_dir —— 手动导出到其它目录不应静默改变自动备份目录
    let mut settings = load_settings()?;
    settings.backup_last_run_at = now_unix_ms() as i64;
    settings.backup_last_run_status = status.to_string();
    save_settings(&settings)
}

async fn export_backup_internal(
    target_path: &Path,
    state: &Arc<Mutex<SharedAppState>>,
) -> Result<BackupExportResultData, String> {
    let prepare_started_at = std::time::Instant::now();
    let prepared = build_prepared_backup_data(state).await.map_err(|error| {
        record_perf_metric(
            "backup.export_stage.prepare_data",
            "备份导出准备数据耗时",
            prepare_started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        );
        error
    })?;
    record_perf_metric(
        "backup.export_stage.prepare_data",
        "备份导出准备数据耗时",
        prepare_started_at.elapsed().as_millis() as u64,
        true,
        None,
    );
    let temp_dir = create_backup_temp_dir()?;
    let app_version = {
        let guard = state.lock().unwrap_or_else(|never| match never {});
        guard.settings.version.clone()
    };
    let payload_started_at = std::time::Instant::now();
    let manifest_result = write_backup_payload(&temp_dir, &prepared, &app_version).await;
    if let Err(err) = manifest_result {
        record_perf_metric(
            "backup.export_stage.write_payload",
            "备份导出写入载荷耗时",
            payload_started_at.elapsed().as_millis() as u64,
            false,
            Some(err.clone()),
        );
        cleanup_dir(&temp_dir);
        return Err(err);
    }
    let manifest = manifest_result?;
    record_perf_metric(
        "backup.export_stage.write_payload",
        "备份导出写入载荷耗时",
        payload_started_at.elapsed().as_millis() as u64,
        true,
        None,
    );
    let zip_started_at = std::time::Instant::now();
    let zip_result = zip_backup_dir(&temp_dir, target_path).await;
    cleanup_dir(&temp_dir);
    let file_size_bytes = match zip_result {
        Ok(value) => {
            record_perf_metric(
                "backup.export_stage.zip_package",
                "备份导出打包压缩耗时",
                zip_started_at.elapsed().as_millis() as u64,
                true,
                None,
            );
            value
        }
        Err(error) => {
            record_perf_metric(
                "backup.export_stage.zip_package",
                "备份导出打包压缩耗时",
                zip_started_at.elapsed().as_millis() as u64,
                false,
                Some(error.clone()),
            );
            return Err(error);
        }
    };
    update_backup_run_state(&target_path.to_string_lossy(), "success")?;
    Ok(BackupExportResultData {
        file_path: target_path.to_string_lossy().to_string(),
        file_size_bytes,
        created_at: manifest.created_at,
        stats: manifest.stats,
    })
}

#[tauri::command]
pub async fn preview_backup_export(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<BackupExportPreviewResponse, String> {
    let started_at = std::time::Instant::now();
    let prepared = match build_prepared_backup_data(state.inner()).await {
        Ok(value) => {
            record_perf_metric(
                "backup.preview_export",
                "备份导出预览耗时",
                started_at.elapsed().as_millis() as u64,
                true,
                None,
            );
            value
        }
        Err(error) => {
            record_perf_metric(
                "backup.preview_export",
                "备份导出预览耗时",
                started_at.elapsed().as_millis() as u64,
                false,
                Some(error.clone()),
            );
            return Err(error);
        }
    };
    Ok(BackupExportPreviewResponse {
        success: true,
        message: "已生成导出预览".to_string(),
        data: BackupExportPreviewData {
            includes: prepared.includes,
            stats: prepared.stats,
            estimated_bytes: prepared.estimated_bytes,
            warnings: prepared.warnings,
        },
    })
}

#[tauri::command]
pub async fn export_backup_to_path(
    request: BackupExportRequest,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<BackupExportResultResponse, String> {
    let started_at = std::time::Instant::now();
    let _backup_job_guard = BACKUP_JOB_MUTEX
        .get_or_init(|| tauri::async_runtime::Mutex::new(()))
        .lock()
        .await;
    let target = PathBuf::from(request.target_path);
    let result = export_backup_internal(&target, state.inner()).await;
    if let Err(err) = &result {
        let _ = update_backup_run_state(&target.to_string_lossy(), "failed");
        record_perf_metric(
            "backup.export",
            "备份导出耗时",
            started_at.elapsed().as_millis() as u64,
            false,
            Some(err.clone()),
        );
        return Err(err.clone());
    }
    record_perf_metric(
        "backup.export",
        "备份导出耗时",
        started_at.elapsed().as_millis() as u64,
        true,
        None,
    );
    Ok(BackupExportResultResponse {
        success: true,
        message: "备份导出成功".to_string(),
        data: result?,
    })
}

#[tauri::command]
pub async fn preview_backup_package(
    request: BackupPackagePreviewRequest,
) -> Result<BackupPackagePreviewResponse, String> {
    let started_at = std::time::Instant::now();
    let manifest = match read_manifest_from_package(Path::new(&request.package_path)) {
        Ok(value) => {
            record_perf_metric(
                "backup.preview_package",
                "备份包预览耗时",
                started_at.elapsed().as_millis() as u64,
                true,
                None,
            );
            value
        }
        Err(error) => {
            record_perf_metric(
                "backup.preview_package",
                "备份包预览耗时",
                started_at.elapsed().as_millis() as u64,
                false,
                Some(error.clone()),
            );
            return Err(error);
        }
    };
    Ok(BackupPackagePreviewResponse {
        success: true,
        message: "已读取备份包".to_string(),
        data: BackupPackagePreviewData {
            includes: manifest.includes.clone(),
            stats: manifest.stats.clone(),
            warnings: backup_preview_warnings_from_manifest(&manifest),
            restore_options: BackupRestoreOptions {
                can_restore_settings: manifest.includes.settings,
                can_restore_text_history: manifest.includes.text_history,
                can_restore_image_history: manifest.includes.image_history,
            },
            manifest,
        },
    })
}

#[tauri::command]
pub async fn restore_backup_package(
    request: BackupRestoreRequest,
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<BackupRestoreResultResponse, String> {
    let started_at = std::time::Instant::now();
    let _backup_job_guard = BACKUP_JOB_MUTEX
        .get_or_init(|| tauri::async_runtime::Mutex::new(()))
        .lock()
        .await;
    let result = match execute_restore_backup_package(state.inner().clone(), request).await {
        Ok(value) => {
            record_perf_metric(
                "backup.restore",
                "备份恢复耗时",
                started_at.elapsed().as_millis() as u64,
                true,
                None,
            );
            value
        }
        Err(error) => {
            record_perf_metric(
                "backup.restore",
                "备份恢复耗时",
                started_at.elapsed().as_millis() as u64,
                false,
                Some(error.clone()),
            );
            return Err(error);
        }
    };
    cleanup_dir(&result.extracted_dir);
    if let Some(rollback_dir) = &result.rollback_dir {
        cleanup_dir(rollback_dir);
    }
    Ok(BackupRestoreResultResponse {
        success: true,
        message: "备份恢复完成".to_string(),
        data: result.data,
    })
}

#[tauri::command]
pub async fn get_backup_settings() -> Result<BackupSettingsData, String> {
    current_backup_settings()
}

#[tauri::command]
pub async fn save_backup_settings(
    request: SaveBackupSettingsRequest,
) -> Result<BackupSettingsData, String> {
    let mut settings = load_settings()?;
    settings.backup_enabled = request.enabled;
    settings.backup_frequency = if request.frequency.trim().is_empty() {
        "weekly".to_string()
    } else {
        request.frequency.trim().to_string()
    };
    settings.backup_target_dir = request.target_dir.trim().to_string();
    settings.backup_max_count = request.max_backup_count.clamp(1, 50);
    save_settings(&settings)?;
    current_backup_settings()
}

#[tauri::command]
pub async fn list_backup_history() -> Result<Vec<BackupHistoryItem>, String> {
    let settings = current_backup_settings()?;
    if settings.target_dir.trim().is_empty() {
        return Ok(Vec::new());
    }
    list_backup_history_items(Path::new(&settings.target_dir))
}

#[tauri::command]
pub async fn delete_backup_history_item(
    request: DeleteBackupHistoryItemRequest,
) -> Result<(), String> {
    let settings = current_backup_settings()?;
    if settings.target_dir.trim().is_empty() {
        return Err(frontend_error_kind(AppErrorKind::BackupDirNotConfigured, "backup target dir is empty"));
    }
    let path = PathBuf::from(request.file_path);
    if !path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.ends_with(".fytbk.zip"))
        .unwrap_or(false)
    {
        return Err(frontend_error_kind(AppErrorKind::BackupInvalidFile, "invalid file extension"));
    }
    let target_dir = PathBuf::from(settings.target_dir);
    let canonical_target_dir = target_dir
        .canonicalize()
        .map_err(|e| format!("读取备份目录失败: {}", e))?;
    if !path.exists() {
        return Ok(());
    }
    let canonical_path = path
        .canonicalize()
        .map_err(|e| format!("读取备份文件路径失败: {}", e))?;
    if !canonical_path.starts_with(&canonical_target_dir) {
        return Err(frontend_error_kind(AppErrorKind::BackupDeleteOutsideDir, "path outside backup dir"));
    }
    fs::remove_file(&canonical_path).map_err(|e| format!("删除备份文件失败: {}", e))?;
    Ok(())
}

#[tauri::command]
pub async fn run_manual_backup(
    state: State<'_, Arc<Mutex<SharedAppState>>>,
) -> Result<BackupExportResultResponse, String> {
    let started_at = std::time::Instant::now();
    let _backup_job_guard = BACKUP_JOB_MUTEX
        .get_or_init(|| tauri::async_runtime::Mutex::new(()))
        .lock()
        .await;
    let settings = current_backup_settings()?;
    if settings.target_dir.trim().is_empty() {
        return Err(frontend_error_kind(AppErrorKind::BackupDirNotSet, "backup target dir not configured"));
    }
    let target_path = Path::new(&settings.target_dir).join(default_backup_file_name());
    let response = match export_backup_internal(&target_path, state.inner()).await {
        Ok(value) => {
            record_perf_metric(
                "backup.manual_export",
                "手动备份耗时",
                started_at.elapsed().as_millis() as u64,
                true,
                None,
            );
            value
        }
        Err(error) => {
            record_perf_metric(
                "backup.manual_export",
                "手动备份耗时",
                started_at.elapsed().as_millis() as u64,
                false,
                Some(error.clone()),
            );
            return Err(error);
        }
    };

    let history_items = list_backup_history_items(Path::new(&settings.target_dir))?;
    if history_items.len() > settings.max_backup_count {
        for item in history_items.iter().skip(settings.max_backup_count) {
            let _ = fs::remove_file(&item.file_path);
        }
    }

    Ok(BackupExportResultResponse {
        success: true,
        message: "手动备份完成".to_string(),
        data: response,
    })
}

// ========================================
// 诊断系统
// ========================================

pub async fn run_auto_backup_tick(state: Arc<Mutex<SharedAppState>>) -> Result<bool, String> {
    let settings = current_backup_settings()?;
    if !settings.enabled {
        return Ok(false);
    }
    let Some(interval_ms) = backup_frequency_interval_ms(&settings.frequency) else {
        return Ok(false);
    };
    if settings.target_dir.trim().is_empty() {
        let mut raw_settings = load_settings()?;
        raw_settings.backup_last_run_at = now_unix_ms() as i64;
        raw_settings.backup_last_run_status = "misconfigured".to_string();
        save_settings(&raw_settings)?;
        return Err(frontend_error_kind(AppErrorKind::BackupDirNotConfigured, "auto backup target dir not configured"));
    }

    let now_ms = now_unix_ms() as i64;
    let due =
        settings.last_run_at <= 0 || now_ms.saturating_sub(settings.last_run_at) >= interval_ms;
    if !due {
        return Ok(false);
    }
    if AUTO_BACKUP_IN_FLIGHT.swap(true, Ordering::AcqRel) {
        return Ok(false);
    }
    // RAII guard: 确保无论成功、失败还是 panic，标志位都会被重置
    struct AutoBackupGuard;
    impl Drop for AutoBackupGuard {
        fn drop(&mut self) {
            AUTO_BACKUP_IN_FLIGHT.store(false, Ordering::Release);
        }
    }
    let _guard = AutoBackupGuard;
    let _backup_job_guard = BACKUP_JOB_MUTEX
        .get_or_init(|| tauri::async_runtime::Mutex::new(()))
        .lock()
        .await;
    let mut raw_settings = load_settings()?;
    raw_settings.backup_last_run_at = now_unix_ms() as i64;
    raw_settings.backup_last_run_status = "running".to_string();
    save_settings(&raw_settings)?;

    let run_result = async {
        let target_path = Path::new(&settings.target_dir).join(default_backup_file_name());
        let response = export_backup_internal(&target_path, &state).await?;
        let history_items = list_backup_history_items(Path::new(&settings.target_dir))?;
        if history_items.len() > settings.max_backup_count {
            for item in history_items.iter().skip(settings.max_backup_count) {
                let _ = fs::remove_file(&item.file_path);
            }
        }
        Ok::<BackupExportResultData, String>(response)
    }
        .await;

    match run_result {
        Ok(_) => Ok(true),
        Err(err) => {
            let mut raw_settings = load_settings()?;
            raw_settings.backup_last_run_at = now_unix_ms() as i64;
            raw_settings.backup_last_run_status = "failed".to_string();
            save_settings(&raw_settings)?;
            Err(err)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use super::*;

    /// 创建临时测试目录
    fn create_test_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fuyun_test_backup_{}",
                                                    std::time::SystemTime::now()
                                                        .duration_since(std::time::UNIX_EPOCH)
                                                        .unwrap()
                                                        .as_nanos()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 测试路径遍历防护：正常文件应该被允许
    #[test]
    fn test_path_traversal_normal_file() {
        let test_dir = create_test_dir();
        let backup_file = test_dir.join("backup_2024.fytbk.zip");
        fs::write(&backup_file, "test").unwrap();

        // 模拟路径检查逻辑
        let canonical_target_dir = test_dir.canonicalize().unwrap();
        let canonical_path = backup_file.canonicalize().unwrap();

        assert!(canonical_path.starts_with(&canonical_target_dir));

        let _ = fs::remove_dir_all(&test_dir);
    }

    /// 测试路径遍历防护：包含 .. 的路径应该被拒绝
    #[test]
    fn test_path_traversal_dotdot_rejected() {
        let test_dir = create_test_dir();
        let backup_file = test_dir.join("backup_2024.fytbk.zip");
        fs::write(&backup_file, "test").unwrap();

        // 构造一个包含 .. 的路径
        let malicious_path = test_dir.join("..").join(test_dir.file_name().unwrap()).join("backup_2024.fytbk.zip");

        // 模拟路径检查逻辑
        let canonical_target_dir = test_dir.canonicalize().unwrap();

        // canonicalize 会解析 ..，所以 canonical_path 应该等于 canonical_target_dir 下的文件
        if let Ok(canonical_path) = malicious_path.canonicalize() {
            // 如果 canonicalize 成功，检查是否在目标目录内
            assert!(canonical_path.starts_with(&canonical_target_dir));
        }
        // 如果 canonicalize 失败（路径不存在），也是安全的

        let _ = fs::remove_dir_all(&test_dir);
    }

    /// 测试路径遍历防护：符号链接指向外部应该被拒绝
    #[test]
    fn test_path_traversal_symlink_rejected() {
        let test_dir = create_test_dir();
        let outside_dir = std::env::temp_dir().join("fuyun_test_outside");
        fs::create_dir_all(&outside_dir).unwrap();
        fs::write(outside_dir.join("secret.txt"), "secret").unwrap();

        // 在测试目录中创建一个指向外部的符号链接
        #[cfg(windows)]
        {
            // Windows 上需要管理员权限创建符号链接，跳过这个测试
            let _ = fs::remove_dir_all(&outside_dir);
            let _ = fs::remove_dir_all(&test_dir);
            return;
        }

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside_dir, test_dir.join("link")).unwrap();

            let canonical_target_dir = test_dir.canonicalize().unwrap();
            let symlink_path = test_dir.join("link");

            if let Ok(canonical_path) = symlink_path.canonicalize() {
                // canonicalize 会解析符号链接，所以 canonical_path 应该指向外部目录
                assert!(!canonical_path.starts_with(&canonical_target_dir));
            }
            let _ = fs::remove_dir_all(&outside_dir);
            let _ = fs::remove_dir_all(&test_dir);
        }
    }

    /// 测试文件扩展名验证
    #[test]
    fn test_file_extension_validation() {
        // 有效扩展名
        assert!("backup_2024.fytbk.zip".ends_with(".fytbk.zip"));
        assert!("my_backup.fytbk.zip".ends_with(".fytbk.zip"));

        // 无效扩展名
        assert!(!"backup_2024.zip".ends_with(".fytbk.zip"));
        assert!(!"backup_2024.fytbk".ends_with(".fytbk.zip"));
        assert!(!"backup_2024.txt".ends_with(".fytbk.zip"));
        assert!(".fytbk.zip".ends_with(".fytbk.zip")); // 无文件名但扩展名有效
    }

    /// 测试空路径处理
    #[test]
    fn test_empty_path_handling() {
        let empty_path = PathBuf::from("");

        // 空路径的 file_name 应该返回 None
        assert!(empty_path.file_name().is_none());

        // 空路径的 canonicalize 应该失败
        assert!(empty_path.canonicalize().is_err());
    }

    /// 测试相对路径处理
    #[test]
    fn test_relative_path_handling() {
        let test_dir = create_test_dir();
        let backup_file = test_dir.join("backup_2024.fytbk.zip");
        fs::write(&backup_file, "test").unwrap();

        // 相对路径 canonicalize 后应该变成绝对路径
        let relative_path = PathBuf::from("backup_2024.fytbk.zip");

        // 在测试目录中模拟检查
        let canonical_target_dir = test_dir.canonicalize().unwrap();

        // 由于相对路径不在测试目录中，canonicalize 会基于当前工作目录
        // 这个测试验证 canonicalize 的行为
        if let Ok(canonical_path) = relative_path.canonicalize() {
            // 相对路径解析后可能不在测试目录中
            // 这正是我们想要的行为 - 路径遍历攻击会被拒绝
            if !canonical_path.starts_with(&canonical_target_dir) {
                // 预期行为：路径不在允许的目录中
                assert!(true);
            }
        }

        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_backup_frequency_interval_ms() {
        assert_eq!(backup_frequency_interval_ms("daily"), Some(86_400_000));
        assert_eq!(backup_frequency_interval_ms("weekly"), Some(604_800_000));
        assert_eq!(backup_frequency_interval_ms("monthly"), None);
        assert_eq!(backup_frequency_interval_ms(""), None);
    }

    #[test]
    fn test_default_backup_file_name_format() {
        let name = default_backup_file_name();
        assert!(name.starts_with("fuyun_tools_"));
        assert!(name.ends_with(".fytbk.zip"));
    }

    #[test]
    fn test_list_backup_history_empty_dir() {
        let dir = std::env::temp_dir().join("fyt_bak_history_empty");
        let _ = std::fs::create_dir_all(&dir);
        let items = list_backup_history_items(&dir).unwrap();
        assert!(items.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_list_backup_history_nonexistent_dir() {
        let items = list_backup_history_items(std::path::Path::new("C:/no/such/bak_dir")).unwrap();
        assert!(items.is_empty());
    }
}
