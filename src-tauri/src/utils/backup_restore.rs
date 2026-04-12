use crate::core::app_state::AppState;
use crate::core::perf_metrics::record_perf_metric;
use crate::utils::backup_archive::{
    cleanup_dir, create_backup_temp_dir, extract_package_to_dir, image_history_path, read_manifest_from_package,
    settings_path, text_history_path, validate_manifest_checksums,
};
use crate::utils::backup_model::{
    BackupImageHistoryFile, BackupImageHistoryItem, BackupRestoreModules, BackupRestoreRequest,
    BackupRestoreResultData, BackupTextHistoryFile,
};
use crate::utils::clipboard::ClipboardManager;
use crate::utils::image_clipboard::{ImageClipboardManager, ImageHistoryItem};
use crate::utils::{image_store, system_utils};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub struct RestoreExecutionResult {
    pub data: BackupRestoreResultData,
    pub extracted_dir: PathBuf,
    pub rollback_dir: Option<PathBuf>,
}

fn record_backup_restore_stage_metric(
    stage: &str,
    label: &str,
    duration_ms: u64,
    success: bool,
    error: Option<String>,
) {
    let key = format!("backup.restore_stage.{}", stage);
    record_perf_metric(&key, label, duration_ms, success, error);
}

pub async fn restore_backup_package(
    state: std::sync::Arc<crate::sync::Mutex<AppState>>,
    request: BackupRestoreRequest,
) -> Result<RestoreExecutionResult, String> {
    let package_path = PathBuf::from(&request.package_path);
    let extracted_dir = create_backup_temp_dir()?;
    let extract_started_at = Instant::now();
    extract_package_to_dir(&package_path, &extracted_dir).map_err(|error| {
        record_backup_restore_stage_metric(
            "extract_package",
            "备份恢复解压耗时",
            extract_started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        );
        error
    })?;
    record_backup_restore_stage_metric(
        "extract_package",
        "备份恢复解压耗时",
        extract_started_at.elapsed().as_millis() as u64,
        true,
        None,
    );

    let manifest_started_at = Instant::now();
    let manifest = read_manifest_from_package(&package_path).map_err(|error| {
        record_backup_restore_stage_metric(
            "read_manifest",
            "备份恢复读取清单耗时",
            manifest_started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        );
        error
    })?;
    record_backup_restore_stage_metric(
        "read_manifest",
        "备份恢复读取清单耗时",
        manifest_started_at.elapsed().as_millis() as u64,
        true,
        None,
    );
    let validate_started_at = Instant::now();
    validate_manifest_checksums(&extracted_dir, &manifest).map_err(|error| {
        record_backup_restore_stage_metric(
            "validate_checksums",
            "备份恢复校验耗时",
            validate_started_at.elapsed().as_millis() as u64,
            false,
            Some(error.clone()),
        );
        error
    })?;
    record_backup_restore_stage_metric(
        "validate_checksums",
        "备份恢复校验耗时",
        validate_started_at.elapsed().as_millis() as u64,
        true,
        None,
    );

    let rollback_dir = if request.create_rollback_point {
        let rollback_started_at = Instant::now();
        let rollback = create_rollback_point(&state).await.map_err(|error| {
            record_backup_restore_stage_metric(
                "create_rollback",
                "备份恢复创建回滚点耗时",
                rollback_started_at.elapsed().as_millis() as u64,
                false,
                Some(error.clone()),
            );
            error
        })?;
        record_backup_restore_stage_metric(
            "create_rollback",
            "备份恢复创建回滚点耗时",
            rollback_started_at.elapsed().as_millis() as u64,
            true,
            None,
        );
        Some(rollback)
    } else {
        None
    };

    let restore_modules = resolve_restore_modules(&request, &manifest);

    let result = async {
        if restore_modules.settings {
            let started_at = Instant::now();
            restore_settings(&state, &extracted_dir).await.map_err(|error| {
                record_backup_restore_stage_metric(
                    "restore_settings",
                    "备份恢复设置耗时",
                    started_at.elapsed().as_millis() as u64,
                    false,
                    Some(error.clone()),
                );
                error
            })?;
            record_backup_restore_stage_metric(
                "restore_settings",
                "备份恢复设置耗时",
                started_at.elapsed().as_millis() as u64,
                true,
                None,
            );
        }
        if restore_modules.text_history {
            let started_at = Instant::now();
            let strategy = request.restore_strategy.clone();
            restore_text_history(&state, &extracted_dir, &strategy).await.map_err(|error| {
                record_backup_restore_stage_metric(
                    "restore_text_history",
                    "备份恢复文本历史耗时",
                    started_at.elapsed().as_millis() as u64,
                    false,
                    Some(error.clone()),
                );
                error
            })?;
            record_backup_restore_stage_metric(
                "restore_text_history",
                "备份恢复文本历史耗时",
                started_at.elapsed().as_millis() as u64,
                true,
                None,
            );
        }
        if restore_modules.image_history {
            let started_at = Instant::now();
            let strategy = request.restore_strategy.clone();
            restore_image_history(&state, &extracted_dir, &strategy).await.map_err(|error| {
                record_backup_restore_stage_metric(
                    "restore_image_history",
                    "备份恢复图片历史耗时",
                    started_at.elapsed().as_millis() as u64,
                    false,
                    Some(error.clone()),
                );
                error
            })?;
            record_backup_restore_stage_metric(
                "restore_image_history",
                "备份恢复图片历史耗时",
                started_at.elapsed().as_millis() as u64,
                true,
                None,
            );
        }
        let rebuild_started_at = Instant::now();
        rebuild_runtime_managers(&state).await.map_err(|error| {
            record_backup_restore_stage_metric(
                "rebuild_runtime",
                "备份恢复重建运行时耗时",
                rebuild_started_at.elapsed().as_millis() as u64,
                false,
                Some(error.clone()),
            );
            error
        })?;
        record_backup_restore_stage_metric(
            "rebuild_runtime",
            "备份恢复重建运行时耗时",
            rebuild_started_at.elapsed().as_millis() as u64,
            true,
            None,
        );
        Ok::<(), String>(())
    }
    .await;

    if let Err(err) = result {
        if let Some(rollback) = &rollback_dir {
            apply_rollback(&state, rollback).await?;
            rebuild_runtime_managers(&state).await?;
            cleanup_dir(&extracted_dir);
            return Err(format!("{}，已自动回滚", err));
        }
        cleanup_dir(&extracted_dir);
        return Err(err);
    }

    let response = BackupRestoreResultData {
        mode: request.mode.clone(),
        restored: restore_modules,
        rollback_point_created: rollback_dir.is_some(),
        warnings: vec![
            "API Key 不会自动恢复，请在 AI 设置中重新填写".to_string(),
            "图片预览缓存会在后续按需重新生成".to_string(),
        ],
    };

    Ok(RestoreExecutionResult {
        data: response,
        extracted_dir,
        rollback_dir,
    })
}

async fn create_rollback_point(state: &std::sync::Arc<crate::sync::Mutex<AppState>>) -> Result<PathBuf, String> {
    let rollback_dir = create_backup_temp_dir()?;
    
    // 先读取 settings,释放 state 锁
    let settings = {
        let guard = state.lock().unwrap_or_else(|never| match never {});
        guard.settings.clone()
    };
    
    // 再读取 text_snapshot,避免嵌套锁定
    let text_snapshot = {
        let guard = state.lock().unwrap_or_else(|never| match never {});
        let clipboard_manager_arc = guard.clipboard_manager.clone();
        drop(guard); // 释放 state 锁
        
        let clipboard = clipboard_manager_arc
            .lock()
            .map_err(|_| "读取文字历史失败".to_string())?;
        crate::utils::database::ClipboardHistoryData {
            items: clipboard.get_history(),
            categories: clipboard.get_categories(),
            category_list: clipboard.get_category_list(),
            pinned_items: clipboard.get_pinned_items(),
        }
    };
    
    // 使用异步版本,避免在 tokio 运行时中嵌套调用 block_on
    let image_snapshot = image_store::load_all_data_async().await?;

    let settings_wrapper = crate::utils::backup_model::BackupSettingsFile { settings };
    let text_wrapper = BackupTextHistoryFile {
        snapshot: text_snapshot,
    };
    let image_wrapper = map_image_data_to_backup(&image_snapshot.items, &image_snapshot.categories, &image_snapshot.category_list, &image_snapshot.image_tags, &image_snapshot.pinned_items)?;

    fs::create_dir_all(rollback_dir.join("settings")).map_err(|e| format!("创建回滚目录失败: {}", e))?;
    fs::create_dir_all(rollback_dir.join("text_history")).map_err(|e| format!("创建回滚目录失败: {}", e))?;
    fs::create_dir_all(rollback_dir.join("image_history")).map_err(|e| format!("创建回滚目录失败: {}", e))?;
    fs::write(
        rollback_dir.join(settings_path()),
        serde_json::to_vec_pretty(&settings_wrapper).map_err(|e| format!("序列化回滚设置失败: {}", e))?,
    )
    .map_err(|e| format!("写入回滚设置失败: {}", e))?;
    fs::write(
        rollback_dir.join(text_history_path()),
        serde_json::to_vec_pretty(&text_wrapper).map_err(|e| format!("序列化回滚文本失败: {}", e))?,
    )
    .map_err(|e| format!("写入回滚文本失败: {}", e))?;
    fs::write(
        rollback_dir.join(image_history_path()),
        serde_json::to_vec_pretty(&image_wrapper).map_err(|e| format!("序列化回滚图片失败: {}", e))?,
    )
    .map_err(|e| format!("写入回滚图片失败: {}", e))?;

    let blob_target_dir = rollback_dir.join("image_history").join("blobs");
    fs::create_dir_all(&blob_target_dir).map_err(|e| format!("创建回滚图片目录失败: {}", e))?;
    for item in &image_snapshot.items {
        if item.image_path.is_empty() {
            continue;
        }
        let source = PathBuf::from(&item.image_path);
        if !source.exists() {
            continue;
        }
        let extension = source.extension().and_then(|ext| ext.to_str()).unwrap_or("png");
        let file_name = format!("{}.{}", item.id, extension);
        fs::copy(&source, blob_target_dir.join(file_name))
            .map_err(|e| format!("复制回滚图片失败 {}: {}", source.display(), e))?;
    }

    Ok(rollback_dir)
}

async fn apply_rollback(state: &std::sync::Arc<crate::sync::Mutex<AppState>>, rollback_dir: &Path) -> Result<(), String> {
    restore_settings(state, rollback_dir).await?;
    // 回滚时使用覆盖模式,确保完全恢复到备份前状态
    restore_text_history(state, rollback_dir, "overwrite").await?;
    restore_image_history(state, rollback_dir, "overwrite").await?;
    Ok(())
}

async fn restore_settings(state: &std::sync::Arc<crate::sync::Mutex<AppState>>, extracted_dir: &Path) -> Result<(), String> {
    let bytes = fs::read(extracted_dir.join(settings_path())).map_err(|e| format!("读取备份设置失败: {}", e))?;
    let wrapper = serde_json::from_slice::<crate::utils::backup_model::BackupSettingsFile>(&bytes)
        .map_err(|e| format!("解析备份设置失败: {}", e))?;
    system_utils::save_settings(&wrapper.settings)?;
    let mut guard = state.lock().unwrap_or_else(|never| match never {});
    guard.settings = wrapper.settings;
    Ok(())
}

async fn restore_text_history(state: &std::sync::Arc<crate::sync::Mutex<AppState>>, extracted_dir: &Path, strategy: &str) -> Result<(), String> {
    let bytes = fs::read(extracted_dir.join(text_history_path())).map_err(|e| format!("读取文字历史备份失败: {}", e))?;
    let wrapper =
        serde_json::from_slice::<BackupTextHistoryFile>(&bytes).map_err(|e| format!("解析文字历史备份失败: {}", e))?;
    
    // 根据策略选择合并或覆盖模式
    if strategy.eq_ignore_ascii_case("overwrite") {
        // 覆盖模式:完全替换现有数据
        crate::utils::database::save_history_data_snapshot_async(&wrapper.snapshot).await?;
        log::info!("文本历史恢复: 使用覆盖模式");
    } else {
        // 合并模式:保留现有记录,只添加备份中不存在的新记录
        crate::utils::database::merge_history_data_async(&wrapper.snapshot).await?;
        log::info!("文本历史恢复: 使用合并模式");
    }
    
    let mut guard = state.lock().unwrap_or_else(|never| match never {});
    let _ = &mut guard;
    Ok(())
}

async fn restore_image_history(state: &std::sync::Arc<crate::sync::Mutex<AppState>>, extracted_dir: &Path, strategy: &str) -> Result<(), String> {
    let bytes = fs::read(extracted_dir.join(image_history_path())).map_err(|e| format!("读取图片历史备份失败: {}", e))?;
    let wrapper = serde_json::from_slice::<BackupImageHistoryFile>(&bytes)
        .map_err(|e| format!("解析图片历史备份失败: {}", e))?;

    let blob_root = app_blob_dir()?;
    
    if strategy.eq_ignore_ascii_case("overwrite") {
        // 覆盖模式:清空现有数据
        image_store::clear_all_history_async().await?;
        if blob_root.exists() {
            fs::remove_dir_all(&blob_root).map_err(|e| format!("清理旧图片目录失败: {}", e))?;
        }
        log::info!("图片历史恢复: 使用覆盖模式");
    } else {
        // 合并模式:保留现有记录
        log::info!("图片历史恢复: 使用合并模式");
    }
    
    fs::create_dir_all(&blob_root).map_err(|e| format!("创建图片目录失败: {}", e))?;

    for (position, item) in wrapper.items.iter().enumerate() {
        let source = extracted_dir.join("image_history").join(&item.blob_path);
        let extension = source.extension().and_then(|ext| ext.to_str()).unwrap_or("png");
        let target = blob_root.join(format!("{}.{}", item.id, extension));
        
        // 如果是合并模式且文件已存在,跳过
        if !strategy.eq_ignore_ascii_case("overwrite") && target.exists() {
            log::info!("图片文件已存在,跳过: {}", item.id);
            continue;
        }
        
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建图片父目录失败: {}", e))?;
        }
        fs::copy(&source, &target)
            .map_err(|e| format!("恢复图片文件失败 {}: {}", source.display(), e))?;
        let history_item = ImageHistoryItem {
            id: item.id.clone(),
            width: item.width,
            height: item.height,
            image_path: target.to_string_lossy().to_string(),
            rgba_bytes: Vec::new(),
            signature: item.id.clone(),
            lazy_load: true,
            cached_signature: None,
        };
        
        if strategy.eq_ignore_ascii_case("overwrite") {
            // 覆盖模式:直接插入
            image_store::upsert_item_async(&history_item, position).await?;
        } else {
            // 合并模式:如果已存在则跳过
            if let Err(e) = image_store::upsert_item_async(&history_item, position).await {
                log::warn!("插入图片记录失败(可能已存在): {}, 错误: {}", item.id, e);
            }
        }
    }

    if strategy.eq_ignore_ascii_case("overwrite") {
        // 覆盖模式:完全替换分类、标签和置顶
        for (item_id, category) in &wrapper.categories {
            image_store::upsert_category_async(item_id, category).await?;
        }
        image_store::sync_category_list_order_async(&wrapper.category_list).await?;
        for (item_id, tags) in &wrapper.image_tags {
            image_store::sync_tags_for_item_async(item_id, tags).await?;
        }
        image_store::sync_pinned_order_async(&wrapper.pinned_items).await?;
    } else {
        // 合并模式:只添加新的
        for (item_id, category) in &wrapper.categories {
            image_store::upsert_category_async(item_id, category).await?;
        }
        // 注意:不覆盖分类列表顺序,只添加新的分类
        for category in &wrapper.category_list {
            if let Err(_) = image_store::add_category_if_not_exists_async(category).await {
                // 如果分类已存在,忽略错误
            }
        }
        for (item_id, tags) in &wrapper.image_tags {
            image_store::sync_tags_for_item_async(item_id, tags).await?;
        }
        // 合并且置顶项:将备份中的置顶项添加到当前置顶列表末尾
        image_store::merge_pinned_items_async(&wrapper.pinned_items).await?;
    }

    let preview_item_ids = wrapper.items.iter().map(|item| item.id.clone()).collect::<Vec<_>>();
    if !preview_item_ids.is_empty() {
        image_store::delete_async_previews_bulk_async(&preview_item_ids).await?;
    }

    let _ = state;
    Ok(())
}

async fn rebuild_runtime_managers(state: &std::sync::Arc<crate::sync::Mutex<AppState>>) -> Result<(), String> {
    // 先读取配置,然后释放 state 锁
    let (text_max_items, grouped_items_protected, image_max_items, image_disk_limit_mb) = {
        let guard = state.lock().unwrap_or_else(|never| match never {});
        (
            guard.settings.text_max_items,
            guard.settings.grouped_items_protected_from_limit,
            guard.settings.image_max_items,
            guard.settings.image_disk_limit_mb,
        )
    };
    
    // 在阻塞任务中创建新的管理器实例,避免在 tokio 运行时中嵌套调用 block_on
    let new_clipboard_manager = tauri::async_runtime::spawn_blocking(move || {
        std::sync::Arc::new(crate::sync::Mutex::new(ClipboardManager::new(
            text_max_items,
            grouped_items_protected,
        )))
    })
    .await
    .map_err(|e| format!("创建剪贴板管理器失败: {}", e))?;
    
    let new_image_clipboard_manager = tauri::async_runtime::spawn_blocking(move || {
        std::sync::Arc::new(crate::sync::Mutex::new(ImageClipboardManager::new(
            image_max_items,
            image_disk_limit_mb,
            grouped_items_protected,
        )))
    })
    .await
    .map_err(|e| format!("创建图片剪贴板管理器失败: {}", e))?;
    
    // 重新获取 state 锁并替换管理器
    let mut guard = state.lock().unwrap_or_else(|never| match never {});
    guard.clipboard_manager = new_clipboard_manager;
    guard.image_clipboard_manager = new_image_clipboard_manager;
    Ok(())
}

fn resolve_restore_modules(
    request: &BackupRestoreRequest,
    manifest: &crate::utils::backup_model::BackupManifest,
) -> BackupRestoreModules {
    let is_full = request.mode.eq_ignore_ascii_case("full");
    BackupRestoreModules {
        settings: manifest.includes.settings && (is_full || request.restore_settings),
        text_history: manifest.includes.text_history && (is_full || request.restore_text_history),
        image_history: manifest.includes.image_history && (is_full || request.restore_image_history),
    }
}

fn map_image_data_to_backup(
    items: &[ImageHistoryItem],
    categories: &std::collections::HashMap<String, String>,
    category_list: &[String],
    image_tags: &std::collections::HashMap<String, Vec<String>>,
    pinned_items: &[String],
) -> Result<BackupImageHistoryFile, String> {
    let mapped_items = items
        .iter()
        .map(|item| {
            let source = PathBuf::from(&item.image_path);
            let extension = source.extension().and_then(|ext| ext.to_str()).unwrap_or("png");
            Ok(BackupImageHistoryItem {
                id: item.id.clone(),
                width: item.width,
                height: item.height,
                blob_path: format!("blobs/{}.{}", item.id, extension),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(BackupImageHistoryFile {
        items: mapped_items,
        categories: categories.clone(),
        category_list: category_list.to_vec(),
        image_tags: image_tags.clone(),
        pinned_items: pinned_items.to_vec(),
    })
}

fn app_blob_dir() -> Result<PathBuf, String> {
    let mut path = std::env::current_exe().map_err(|e| format!("读取程序目录失败: {}", e))?;
    path.pop();
    path.push("image_history_blobs");
    Ok(path)
}
