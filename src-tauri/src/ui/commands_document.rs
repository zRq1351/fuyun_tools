use crate::utils::document_database;
use crate::utils::document_text_extract;
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;
use tauri::AppHandle;
use tokio::task;

#[tauri::command]
pub async fn add_doc_root(name: String, root_path: String) -> Result<document_database::DocRoot, String> {
    let path = Path::new(&root_path);
    fs::create_dir_all(path).map_err(|e| format!("创建目录失败: {}", e))?;
    if !path.is_dir() {
        return Err("路径不是一个目录".to_string());
    }
    document_database::add_doc_root(&name, &root_path).await
}

#[tauri::command]
pub async fn get_doc_roots() -> Result<Vec<document_database::DocRoot>, String> {
    document_database::get_doc_roots().await
}

#[tauri::command]
pub async fn remove_doc_root(id: i64) -> Result<(), String> {
    document_database::remove_doc_root(id).await
}

#[tauri::command]
pub async fn add_doc_category(name: String, icon: Option<String>, color: Option<String>) -> Result<document_database::DocCategory, String> {
    let name_trim = name.trim();
    if name_trim.is_empty() {
        return Err("分类名称不能为空".to_string());
    }
    document_database::add_doc_category(
        name_trim,
        &icon.unwrap_or_else(|| "folder".to_string()),
        &color.unwrap_or_else(|| "#409EFF".to_string()),
    ).await
}

#[tauri::command]
pub async fn get_doc_categories() -> Result<Vec<document_database::DocCategory>, String> {
    document_database::get_doc_categories().await
}

#[tauri::command]
pub async fn remove_doc_category(id: i64) -> Result<(), String> {
    document_database::remove_doc_category(id).await
}

#[tauri::command]
pub async fn rename_doc_category(id: i64, name: String) -> Result<(), String> {
    let name_trim = name.trim();
    if name_trim.is_empty() {
        return Err("分类名称不能为空".to_string());
    }
    document_database::rename_doc_category(id, name_trim).await
}

#[tauri::command]
pub async fn reorder_doc_categories(ids: Vec<i64>) -> Result<(), String> {
    document_database::reorder_doc_categories(ids).await
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
        let cats = document_database::get_doc_categories().await?;
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
            errors.push(format!("文件不存在: {}", file_path_str));
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

        let src_path = file_path_str.clone();

        let (resolved_name, managed_path_val, need_move, dest_dir_clone) = if is_repo {
            let dir = target_dir_opt.as_ref().unwrap();
            let name = document_database::resolve_unused_filename(dir, file_name, &file_ext);
            let dest = dir.join(&name);
            (name, dest.to_string_lossy().to_string(), true, Some(dest))
        } else {
            (src.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string(), src_path.clone(), false, None)
        };

        let src_for_extract = src_path.clone();
        let ext_for_extract = file_ext.clone();
        let content_text = task::spawn_blocking(move || {
            document_text_extract::extract_file_content(Path::new(&src_for_extract), &ext_for_extract)
        }).await.unwrap_or_default();

        match document_database::insert_doc_file(
            request.root_id, &resolved_name, &file_ext, file_size, &file_hash,
            request.category_id, &tags, &src_path, &managed_path_val,
            &request.storage_mode, file_modified, &content_text,
        ).await {
            Ok(id) => {
                if need_move {
                    let dest = dest_dir_clone.as_ref().unwrap();
                    let options = fs_extra::file::CopyOptions::new().overwrite(true);
                    if let Err(e) = fs_extra::file::move_file(src, dest, &options) {
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
    pub category_id: Option<Option<i64>>,
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
    let managed_path = document_database::delete_doc_file(request.id).await?;
    if request.delete_file.unwrap_or(false) {
        if let Some(path) = managed_path {
            let p = Path::new(&path);
            if p.exists() {
                let _ = fs::remove_file(p);
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn move_doc(id: i64, new_root_id: i64) -> Result<(), String> {
    document_database::move_doc_file(id, new_root_id).await
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
}

#[tauri::command]
pub async fn scan_folder(path: String, recursive: Option<bool>) -> Result<ScanResult, String> {
    let dir = Path::new(&path);
    if !dir.is_dir() {
        return Err("路径不是一个目录".to_string());
    }

    let mut files = Vec::new();
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

    scan_dir(dir, &mut files, &allowed, recursive)?;

    Ok(ScanResult {
        directory: path,
        files,
    })
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
                });
            }
        } else if path.is_dir() && recursive {
            scan_dir(&path, files, allowed, recursive)?;
        }
    }
    Ok(())
}
