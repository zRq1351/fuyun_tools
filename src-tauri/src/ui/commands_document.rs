use crate::core::error_codes::AppErrorKind;
use crate::utils::document_database;
use crate::utils::document_text_extract;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use tauri::AppHandle;
use tokio::task;

#[tauri::command]
pub async fn add_doc_root(name: String, root_path: String) -> Result<document_database::DocRoot, String> {
    let path = Path::new(&root_path);
    // Validate name and path to prevent path traversal and empty values
    if name.trim().is_empty() {
        return Err(AppErrorKind::InternalError.to_frontend_json_with_details(
            "名称不能为空".to_string(),
        ));
    }
    let canonical = path.canonicalize().map_err(|e| format!("路径无效: {}", e))?;
    fs::create_dir_all(&canonical).map_err(|e| format!("创建目录失败: {}", e))?;
    if !canonical.is_dir() {
        return Err(AppErrorKind::DocumentPathNotDir.to_frontend_json());
    }
    document_database::add_doc_root(&name, &canonical.to_string_lossy()).await
}

#[tauri::command]
pub async fn get_doc_roots() -> Result<Vec<document_database::DocRoot>, String> {
    document_database::get_doc_roots().await
}

#[tauri::command]
pub async fn remove_doc_root(id: i64) -> Result<(), String> {
    document_database::remove_doc_root(id).await
}

fn validate_category_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err(AppErrorKind::DocumentCategoryNameEmpty.to_frontend_json());
    }
    for ch in name.chars() {
        match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => {
                return Err(AppErrorKind::DocumentCategoryNameInvalidChar.to_frontend_json_with_details(format!("{}", ch)));
            }
            _ => {}
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn add_doc_category(name: String, icon: Option<String>, color: Option<String>, root_id: i64) -> Result<document_database::DocCategory, String> {
    let name_trim = name.trim();
    validate_category_name(name_trim)?;
    let root = document_database::get_doc_root_by_id(root_id).await?.ok_or("根目录不存在".to_string())?;
    let result = document_database::add_doc_category(
        name_trim,
        &icon.unwrap_or_else(|| "folder".to_string()),
        &color.unwrap_or_else(|| "#409EFF".to_string()),
        root_id,
    ).await?;
    fs::create_dir_all(Path::new(&root.root_path).join(name_trim))
        .map_err(|e| format!("创建分类目录失败: {}", e))?;
    Ok(result)
}

#[tauri::command]
pub async fn get_doc_categories(root_id: Option<i64>) -> Result<Vec<document_database::DocCategory>, String> {
    document_database::get_doc_categories(root_id).await
}

#[tauri::command]
pub async fn remove_doc_category(id: i64) -> Result<(), String> {
    document_database::remove_doc_category(id).await
}

#[tauri::command]
pub async fn rename_doc_category(id: i64, name: String) -> Result<(), String> {
    let name_trim = name.trim();
    validate_category_name(name_trim)?;
    let cat = document_database::get_doc_categories(None).await?.into_iter().find(|c| c.id == id).ok_or("分类不存在".to_string())?;
    let root = document_database::get_doc_root_by_id(cat.root_id).await?.ok_or("根目录不存在".to_string())?;
    let old_dir = Path::new(&root.root_path).join(&cat.name);
    let new_dir = Path::new(&root.root_path).join(name_trim);
    if old_dir.exists() && old_dir != new_dir {
        document_database::safe_move_file(&old_dir, &new_dir).map_err(|e| format!("重命名目录失败: {}", e))?;
    }
    let _old_name = document_database::rename_doc_category(id, name_trim).await?;
    let old_prefix = old_dir.to_string_lossy().to_string();
    let new_prefix = new_dir.to_string_lossy().to_string();
    document_database::update_managed_path_prefix(&old_prefix, &new_prefix, root.id, id).await?;
    Ok(())
}

#[tauri::command]
pub async fn reorder_doc_categories(ids: Vec<i64>) -> Result<(), String> {
    document_database::reorder_doc_categories(ids).await
}

#[tauri::command]
pub async fn reorder_doc_roots(ids: Vec<i64>) -> Result<(), String> {
    document_database::reorder_doc_roots(ids).await
}

#[tauri::command]
pub async fn reorder_doc_files(ids: Vec<i64>) -> Result<(), String> {
    document_database::reorder_doc_files(ids).await
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportFilesRequest {
    pub paths: Vec<String>,
    pub root_id: i64,
    pub category_id: Option<i64>,
    pub tags: Option<String>,
    #[serde(default = "default_storage_mode")]
    pub storage_mode: String,
    #[serde(default)]
    pub source_dir: String,
}

fn default_storage_mode() -> String {
    "index".to_string()
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub success: Vec<i64>,
    pub errors: Vec<String>,
}

#[tauri::command]
pub async fn import_files(request: ImportFilesRequest) -> Result<ImportResult, String> {
    let roots = document_database::get_doc_roots().await?;
    let root = roots
        .iter()
        .find(|r| r.id == request.root_id)
        .ok_or("指定的根目录不存在".to_string())?;

    let category_name = if let Some(cid) = request.category_id {
        let cats = document_database::get_doc_categories(None).await?;
        cats.iter().find(|c| c.id == cid).map(|c| c.name.clone()).unwrap_or_else(|| "未分类".to_string())
    } else {
        "未分类".to_string()
    };

    let tags = request.tags.unwrap_or_else(|| "[]".to_string());
    let is_repo = request.storage_mode == "repo";
    let mut success = Vec::new();
    let mut errors = Vec::new();

    let repo_dir = Path::new(&root.root_path).join(&category_name);
    let target_dir: String = if is_repo {
        fs::create_dir_all(&repo_dir).map_err(|e| format!("创建目标目录失败: {}", e))?;
        repo_dir.to_string_lossy().to_string()
    } else {
        "未搬迁".to_string()
    };
    let target_dir_opt = if is_repo { Some(repo_dir) } else { None };

    let mut success_ids = Vec::new();
    for file_path_str in &request.paths {
        let src = Path::new(file_path_str);
        if !src.exists() {
            errors.push(AppErrorKind::DocumentFileNotFound.to_frontend_json_with_details(format!("{}", file_path_str)));
            continue;
        }
        if !src.is_file() {
            errors.push(format!("不是文件: {}", file_path_str));
            continue;
        }

        let file_name = src
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
        let file_ext = src
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        let file_size = src.metadata().map(|m| m.len() as i64).unwrap_or(0);
        let file_modified = src
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        let file_hash = document_database::compute_file_hash(src).unwrap_or_default();

        if !file_hash.is_empty() {
            if let Ok(true) = document_database::doc_exists_by_hash(&file_hash, request.root_id, request.category_id).await {
                errors.push(format!("文件已存在（重复）: {}", file_path_str));
                continue;
            }
        }

        let src_path = file_path_str.clone();

        let (resolved_name, managed_path_val, need_move, dest_dir_clone) = if is_repo {
            let dir = match target_dir_opt.as_ref() {
                Some(d) => d,
                None => {
                    log::error!("target_dir 未设置但 is_repo 为 true，跳过该文件");
                    errors.push(format!("内部错误：目标目录未设置 {}", file_name));
                    continue;
                }
            };
            let name = document_database::resolve_unused_filename(dir, file_name, &file_ext);
            let dest = dir.join(&name);
            (name, dest.to_string_lossy().to_string(), true, Some(dest))
        } else {
            (src.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string(), src_path.clone(), false, None)
        };

        let src_for_extract = src_path.clone();
        let ext_for_extract = file_ext.clone();
        let content_text = match task::spawn_blocking(move || {
            document_text_extract::extract_file_content(Path::new(&src_for_extract), &ext_for_extract)
        }).await {
            Ok(text) => text,
            Err(e) => {
                let msg = if e.is_panic() {
                    format!("文件内容提取线程 panic: {}", file_name)
                } else {
                    format!("文件内容提取被取消: {}", file_name)
                };
                log::error!("{}", msg);
                errors.push(msg);
                continue;
            }
        };

        match document_database::insert_doc_file(
            request.root_id, &resolved_name, &file_ext, file_size, &file_hash,
            request.category_id, &tags, &src_path, &managed_path_val,
            &request.storage_mode, file_modified, &content_text,
        ).await {
            Ok(id) => {
                if need_move {
                    let dest = match dest_dir_clone.as_ref() {
                        Some(d) => d,
                        None => {
                            log::error!("dest_dir 未设置但 need_move 为 true，跳过文件移动: {}", file_name);
                            errors.push(format!("内部错误：目标目录未设置 {}", file_name));
                            continue;
                        }
                    };
                    if let Err(e) = document_database::safe_move_file(src, dest) {
                        document_database::delete_doc_file(id).await.ok();
                        errors.push(format!("移动文件失败 {}: {}", file_name, e));
                        continue;
                    }
                }
                success.push(id);
                success_ids.push((id, src_path, managed_path_val));
            }
            Err(e) => {
                errors.push(format!("保存记录失败 {}: {}", file_name, e));
            }
        }
    }

    if !success_ids.is_empty() {
        let source_dir = if !request.source_dir.is_empty() {
            request.source_dir.clone()
        } else {
            let first_path = &request.paths[0];
            std::path::Path::new(first_path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "手动选择".to_string())
        };
        if let Ok(import_id) = document_database::create_import_history(
            request.root_id,
            request.category_id,
            &request.storage_mode,
            &source_dir,
            &target_dir,
            success_ids.len() as i64,
        ).await {
            for (doc_id, src, managed) in &success_ids {
                document_database::link_import_item(import_id, *doc_id, src, managed).await.ok();
            }
        }
    }

    Ok(ImportResult { success, errors })
}

#[tauri::command]
pub async fn get_import_history(limit: Option<i64>) -> Result<Vec<document_database::ImportHistory>, String> {
    document_database::get_import_history(limit.unwrap_or(20)).await
}

#[tauri::command]
pub async fn undo_import(import_id: i64) -> Result<Vec<String>, String> {
    document_database::undo_import(import_id).await
}

#[tauri::command]
pub async fn undo_import_item(import_id: i64, doc_file_id: i64) -> Result<(), String> {
    document_database::undo_import_item(import_id, doc_file_id).await
}

#[tauri::command]
pub async fn get_import_files(import_id: i64) -> Result<Vec<document_database::ImportFileItem>, String> {
    document_database::get_import_files(import_id).await
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocPageRequest {
    #[serde(default)]
    pub offset: i64,
    #[serde(default = "default_page_limit")]
    pub limit: i64,
    pub category_id: Option<i64>,
    pub root_id: Option<i64>,
    pub keyword: Option<String>,
    pub file_ext: Option<String>,
}

fn default_page_limit() -> i64 {
    50
}

#[tauri::command]
pub async fn get_doc_page(request: DocPageRequest) -> Result<document_database::DocPageData, String> {
    document_database::get_doc_page(
        request.offset,
        request.limit,
        request.category_id,
        request.root_id,
        request.keyword,
        request.file_ext,
    ).await
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDocMetaRequest {
    pub id: i64,
    pub title: Option<String>,
    pub category_id: Option<i64>,
    pub tags: Option<String>,
    pub notes: Option<String>,
}

#[tauri::command]
pub async fn update_doc_meta(request: UpdateDocMetaRequest) -> Result<(), String> {
    document_database::update_doc_file_meta(
        request.id,
        request.title.as_deref(),
        request.category_id,
        request.tags.as_deref(),
        request.notes.as_deref(),
    ).await
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteDocRequest {
    pub id: i64,
    pub delete_file: Option<bool>,
}

#[tauri::command]
pub async fn delete_doc(request: DeleteDocRequest) -> Result<(), String> {
    let doc = document_database::get_doc_file_by_id(request.id)
        .await?
        .ok_or("文档不存在".to_string())?;

    if request.delete_file.unwrap_or(false) {
        let p = Path::new(&doc.managed_path);
        if p.exists() {
            fs::remove_file(p).map_err(|e| format!("删除文件失败: {}", e))?;
        }
    } else if doc.storage_mode == "repo" && !doc.source_path.is_empty() {
        let managed = Path::new(&doc.managed_path);
        let source = Path::new(&doc.source_path);
        if managed.exists() && managed != source {
            if let Some(parent) = source.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    log::error!("创建源文件目录失败 {}: {}", parent.display(), e);
                }
            }
            document_database::safe_move_file(managed, source)
                .map_err(|e| format!("回搬文件失败: {}", e))?;
        }
    }

    document_database::delete_doc_record(request.id).await?;

    Ok(())
}

#[tauri::command]
pub async fn move_doc(id: i64, new_root_id: i64) -> Result<(), String> {
    document_database::move_doc_file(id, new_root_id).await
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AtomicMoveRequest {
    pub id: i64,
    pub new_root_id: Option<i64>,
    pub new_category_id: Option<i64>,
}

#[tauri::command]
pub async fn atomic_move_doc(request: AtomicMoveRequest) -> Result<(), String> {
    document_database::atomic_move_doc(
        request.id,
        request.new_root_id,
        request.new_category_id,
    ).await
}

#[tauri::command]
pub async fn get_doc_stats(root_id: Option<i64>) -> Result<document_database::DocStats, String> {
    document_database::get_doc_stats(root_id).await
}

#[tauri::command]
pub async fn open_doc(_app_handle: AppHandle, id: i64) -> Result<(), String> {
    let doc = document_database::get_doc_file_by_id(id)
        .await?
        .ok_or("文档不存在".to_string())?;

    let path = Path::new(&doc.managed_path);
    if !path.exists() {
        let _ = document_database::mark_doc_missing(id).await;
        return Err(AppErrorKind::DocumentFileNotFound.to_frontend_json_with_details(format!("{}", doc.managed_path)));
    }

    document_database::increment_visit_count(id).await.ok();

    let _ = tauri_plugin_opener::open_path(&doc.managed_path, None::<&str>);

    Ok(())
}

#[tauri::command]
pub async fn open_doc_folder(_app_handle: AppHandle, id: i64) -> Result<(), String> {
    let doc = document_database::get_doc_file_by_id(id)
        .await?
        .ok_or("文档不存在".to_string())?;

    let parent = Path::new(&doc.managed_path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or(doc.managed_path.clone());

    if !Path::new(&parent).exists() {
        return Err(format!("目录不存在: {}", parent));
    }

    let _ = tauri_plugin_opener::open_path(&parent, None::<&str>);

    Ok(())
}

#[tauri::command]
pub async fn get_doc_detail(id: i64) -> Result<document_database::DocFile, String> {
    document_database::get_doc_file_by_id(id)
        .await?
        .ok_or("文档不存在".to_string())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub files: Vec<ScannedFile>,
    pub directory: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannedFile {
    pub path: String,
    pub name: String,
    pub ext: String,
    pub size: i64,
    pub modified: i64,
    pub category_id: Option<i64>,
    pub category_name: Option<String>,
}

#[tauri::command]
pub async fn scan_folder(path: String, recursive: Option<bool>) -> Result<ScanResult, String> {
    let dir = PathBuf::from(&path);
    if !dir.is_dir() {
        return Err(AppErrorKind::DocumentPathNotDir.to_frontend_json());
    }

    let recursive = recursive.unwrap_or(true);

    let text_exts = [
        "txt", "md", "csv", "log", "json", "xml", "yaml", "yml", "toml", "ini", "cfg", "conf",
        "pdf", "docx", "doc", "xlsx", "xls", "pptx", "ppt",
        "py", "js", "ts", "jsx", "tsx", "java", "go", "rs", "c", "cpp", "h", "hpp", "cs",
        "php", "rb", "swift", "kt", "scala", "sql", "sh", "bat", "ps1", "lua",
        "html", "htm", "css", "scss", "less", "vue", "svelte", "r", "zig",
        "png", "jpg", "jpeg", "gif", "bmp", "webp", "svg",
    ];

    let allowed: std::collections::HashSet<&str> = text_exts.iter().copied().collect();

    task::spawn_blocking(move || -> Result<ScanResult, String> {
        let mut files = Vec::new();
        scan_dir(&dir, &mut files, &allowed, recursive)?;
        Ok(ScanResult {
            directory: path,
            files,
        })
    })
    .await
    .map_err(|e| format!("扫描任务失败: {}", e))?
}

fn scan_dir(
    dir: &Path,
    files: &mut Vec<ScannedFile>,
    allowed: &std::collections::HashSet<&str>,
    recursive: bool,
) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("读取目录失败: {}", e))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("读取目录项失败: {}", e))?;
        let path = entry.path();
        if path.is_file() {
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            if allowed.contains(ext.as_str()) {
                let size = path.metadata().map(|m| m.len() as i64).unwrap_or(0);
                let modified = path
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                let name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                files.push(ScannedFile {
                    path: path.to_string_lossy().to_string(),
                    name,
                    ext,
                    size,
                    modified,
                    category_id: None,
                    category_name: None,
                });
            }
        } else if path.is_dir() && recursive {
            scan_dir(&path, files, allowed, recursive)?;
        }
    }
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrphanFilesResult {
    pub files: Vec<ScannedFile>,
    pub root_id: i64,
    pub root_name: String,
}

#[tauri::command]
pub async fn detect_orphan_files(root_id: Option<i64>) -> Result<Vec<OrphanFilesResult>, String> {
    let roots = if let Some(rid) = root_id {
        vec![document_database::get_doc_root_by_id(rid)
            .await?
            .ok_or("根目录不存在".to_string())?]
    } else {
        document_database::get_doc_roots().await?
    };

    let categories = document_database::get_doc_categories(None).await?;

    let text_exts = [
        "txt", "md", "csv", "log", "json", "xml", "yaml", "yml", "toml", "ini", "cfg", "conf",
        "pdf", "docx", "doc", "xlsx", "xls", "pptx", "ppt",
        "py", "js", "ts", "jsx", "tsx", "java", "go", "rs", "c", "cpp", "h", "hpp", "cs",
        "php", "rb", "swift", "kt", "scala", "sql", "sh", "bat", "ps1", "lua",
        "html", "htm", "css", "scss", "less", "vue", "svelte", "r", "zig",
        "png", "jpg", "jpeg", "gif", "bmp", "webp", "svg",
    ];
    let allowed: std::collections::HashSet<&str> = text_exts.iter().copied().collect();

    let mut results = Vec::new();

    for root in roots {
        let existing = document_database::get_managed_paths_for_root(root.id).await?;
        let root_path_for_closure = root.root_path.clone();
        let allowed_clone = allowed.clone();
        let root_name_clone = root.name.clone();

        let orphans_future = task::spawn_blocking(move || -> Result<Vec<ScannedFile>, String> {
            let mut all_files = Vec::new();
            let root_path = Path::new(&root_path_for_closure);
            if root_path.is_dir() {
                scan_dir(root_path, &mut all_files, &allowed_clone, true)?;
            }
            let orphans: Vec<ScannedFile> = all_files
                .into_iter()
                .filter(|f| !existing.contains(&f.path))
                .collect();
            Ok(orphans)
        });

        let orphans = orphans_future
            .await
            .map_err(|e| format!("扫描任务失败: {}", e))??;

        let mut tagged = orphans;

        for f in &mut tagged {
            if let Ok(rel) = Path::new(&f.path).strip_prefix(&root.root_path) {
                if let Some(first) = rel.components().next() {
                    let dir_name = first.as_os_str().to_string_lossy().to_string();
                    if let Some(cat) = categories.iter().find(|c| c.name == dir_name) {
                        f.category_id = Some(cat.id);
                        f.category_name = Some(cat.name.clone());
                    }
                }
            }
        }

        if !tagged.is_empty() {
            results.push(OrphanFilesResult {
                files: tagged,
                root_id: root.id,
                root_name: root_name_clone,
            });
        }
    }

    Ok(results)
}

#[tauri::command]
pub async fn show_document_manager(app: AppHandle) -> Result<(), String> {
    crate::ui::window_manager::show_standard_window_by_label(&app, "document_manager")
}

#[tauri::command]
pub async fn show_doc_manager_widget(app: AppHandle) -> Result<(), String> {
    crate::ui::window_manager::show_doc_manager_widget_window(&app)
}

#[tauri::command]
pub async fn hide_doc_manager_widget(app: AppHandle) -> Result<(), String> {
    crate::ui::window_manager::hide_doc_manager_widget_window(&app)
}
