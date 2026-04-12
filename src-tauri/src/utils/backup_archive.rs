use crate::utils::backup_model::{
    BackupImageHistoryFile, BackupManifest, BackupSettingsFile, BackupTextHistoryFile,
    PreparedBackupData,
};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use zip::write::FileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const MANIFEST_PATH: &str = "manifest.json";
const SETTINGS_PATH: &str = "settings/settings.json";
const TEXT_HISTORY_PATH: &str = "text_history/history.json";
const IMAGE_HISTORY_PATH: &str = "image_history/image_history.json";
static BACKUP_TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn create_backup_temp_dir() -> Result<PathBuf, String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let pid = std::process::id();
    let base_counter = BACKUP_TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp_root = std::env::temp_dir();
    for attempt in 0..8u64 {
        let nonce = base_counter.saturating_add(attempt);
        let dir = temp_root.join(format!(
            "fuyun_tools_backup_{}_{}_{}",
            timestamp, pid, nonce
        ));
        match fs::create_dir(&dir) {
            Ok(_) => return Ok(dir),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("创建临时备份目录失败: {}", error)),
        }
    }
    Err("创建临时备份目录失败: 目录名冲突".to_string())
}

pub fn write_backup_payload(
    temp_dir: &Path,
    prepared: &PreparedBackupData,
    app_version: &str,
) -> Result<BackupManifest, String> {
    let settings_file = BackupSettingsFile {
        settings: prepared.settings.clone(),
    };
    let text_file = BackupTextHistoryFile {
        snapshot: prepared.text_history.clone(),
    };
    let image_file = BackupImageHistoryFile {
        items: prepared.image_history.items.clone(),
        categories: prepared.image_history.categories.clone(),
        category_list: prepared.image_history.category_list.clone(),
        image_tags: prepared.image_history.image_tags.clone(),
        pinned_items: prepared.image_history.pinned_items.clone(),
    };

    write_json(temp_dir.join(SETTINGS_PATH), &settings_file)?;
    write_json(temp_dir.join(TEXT_HISTORY_PATH), &text_file)?;
    write_json(temp_dir.join(IMAGE_HISTORY_PATH), &image_file)?;

    for blob in &prepared.blobs {
        let target_path = temp_dir.join(&blob.package_path);
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建备份图片目录失败: {}", e))?;
        }
        fs::copy(&blob.source_path, &target_path)
            .map_err(|e| format!("复制图片文件失败 {}: {}", blob.source_path, e))?;
    }

    let created_at = now_ms();
    let manifest = BackupManifest {
        backup_format_version: 1,
        app_name: "fuyun_tools".to_string(),
        app_version: app_version.to_string(),
        created_at,
        platform: "windows".to_string(),
        includes: prepared.includes.clone(),
        stats: prepared.stats.clone(),
        checksums: compute_checksums(temp_dir)?,
    };
    write_json(temp_dir.join(MANIFEST_PATH), &manifest)?;
    Ok(manifest)
}

pub fn zip_backup_dir(source_dir: &Path, target_path: &Path) -> Result<u64, String> {
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建备份输出目录失败: {}", e))?;
    }
    let file = File::create(target_path).map_err(|e| format!("创建备份包失败: {}", e))?;
    let mut zip = ZipWriter::new(file);
    let options: FileOptions<'_, ()> = FileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);

    for entry in collect_files(source_dir)? {
        let rel = entry
            .strip_prefix(source_dir)
            .map_err(|e| format!("构建备份包路径失败: {}", e))?
            .to_string_lossy()
            .replace('\\', "/");
        zip.start_file(rel, options)
            .map_err(|e| format!("写入备份包失败: {}", e))?;
        let bytes = fs::read(&entry).map_err(|e| format!("读取待打包文件失败: {}", e))?;
        zip.write_all(&bytes)
            .map_err(|e| format!("写入备份包内容失败: {}", e))?;
    }

    zip.finish()
        .map_err(|e| format!("完成备份包写入失败: {}", e))?;
    let metadata = fs::metadata(target_path).map_err(|e| format!("读取备份包大小失败: {}", e))?;
    Ok(metadata.len())
}

pub fn read_manifest_from_package(package_path: &Path) -> Result<BackupManifest, String> {
    let file = File::open(package_path).map_err(|e| format!("打开备份包失败: {}", e))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("解析备份包失败: {}", e))?;
    let mut manifest_file = archive
        .by_name(MANIFEST_PATH)
        .map_err(|_| "备份包缺少 manifest.json".to_string())?;
    let mut content = String::new();
    manifest_file
        .read_to_string(&mut content)
        .map_err(|e| format!("读取 manifest 失败: {}", e))?;
    serde_json::from_str::<BackupManifest>(&content).map_err(|e| format!("解析 manifest 失败: {}", e))
}

pub fn extract_package_to_dir(package_path: &Path, target_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(target_dir).map_err(|e| format!("创建解压目录失败: {}", e))?;
    let file = File::open(package_path).map_err(|e| format!("打开备份包失败: {}", e))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("解析备份包失败: {}", e))?;
    for index in 0..archive.len() {
        let mut zipped = archive
            .by_index(index)
            .map_err(|e| format!("读取备份包条目失败: {}", e))?;
        let enclosed = zipped
            .enclosed_name()
            .ok_or_else(|| "备份包中存在非法路径".to_string())?
            .to_path_buf();
        let output_path = target_dir.join(enclosed);
        if zipped.name().ends_with('/') {
            fs::create_dir_all(&output_path).map_err(|e| format!("创建解压目录失败: {}", e))?;
            continue;
        }
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建解压父目录失败: {}", e))?;
        }
        let mut output = File::create(&output_path).map_err(|e| format!("创建解压文件失败: {}", e))?;
        std::io::copy(&mut zipped, &mut output).map_err(|e| format!("写入解压文件失败: {}", e))?;
    }
    Ok(())
}

pub fn validate_manifest_checksums(extracted_dir: &Path, manifest: &BackupManifest) -> Result<(), String> {
    let actual = compute_checksums(extracted_dir)?;
    for (path, expected) in &manifest.checksums {
        match actual.get(path) {
            Some(value) if value == expected => {}
            Some(_) => return Err(format!("校验失败: {}", path)),
            None => return Err(format!("备份包缺少校验文件: {}", path)),
        }
    }
    Ok(())
}

pub fn cleanup_dir(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

pub fn settings_path() -> &'static str {
    SETTINGS_PATH
}

pub fn text_history_path() -> &'static str {
    TEXT_HISTORY_PATH
}

pub fn image_history_path() -> &'static str {
    IMAGE_HISTORY_PATH
}

fn write_json<T: serde::Serialize>(path: PathBuf, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| format!("序列化备份数据失败: {}", e))?;
    fs::write(&path, bytes).map_err(|e| format!("写入备份数据失败 {}: {}", path.display(), e))
}

fn collect_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    collect_files_recursive(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_files_recursive(root: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(root).map_err(|e| format!("读取目录失败 {}: {}", root.display(), e))? {
        let entry = entry.map_err(|e| format!("读取目录项失败: {}", e))?;
        let path = entry.path();
        if path.is_dir() {
            collect_files_recursive(&path, files)?;
        } else {
            files.push(path);
        }
    }
    Ok(())
}

fn compute_checksums(root: &Path) -> Result<std::collections::HashMap<String, String>, String> {
    let mut checksums = std::collections::HashMap::new();
    for entry in collect_files(root)? {
        let rel = entry
            .strip_prefix(root)
            .map_err(|e| format!("构建校验路径失败: {}", e))?
            .to_string_lossy()
            .replace('\\', "/");
        if rel == MANIFEST_PATH {
            continue;
        }
        let bytes = fs::read(&entry).map_err(|e| format!("读取校验文件失败: {}", e))?;
        checksums.insert(rel, format!("sha256:{}", sha256_hex(&bytes)));
    }
    Ok(checksums)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{:02x}", b)).collect::<String>()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
