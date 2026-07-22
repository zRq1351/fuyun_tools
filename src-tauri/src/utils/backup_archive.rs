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

pub async fn write_backup_payload(
    temp_dir: &Path,
    prepared: &PreparedBackupData,
    app_version: &str,
) -> Result<BackupManifest, String> {
    let temp_dir = temp_dir.to_path_buf();
    let prepared = prepared.clone();
    let app_version = app_version.to_string();

    tokio::task::spawn_blocking(move || {
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
            checksums: compute_checksums(&temp_dir)?,
        };
        write_json(temp_dir.join(MANIFEST_PATH), &manifest)?;
        Ok(manifest)
    })
    .await
    .unwrap_or_else(|_| Err("写入备份任务崩溃".to_string()))
}

pub async fn zip_backup_dir(source_dir: &Path, target_path: &Path) -> Result<u64, String> {
    let source_dir = source_dir.to_path_buf();
    let target_path = target_path.to_path_buf();

    tokio::task::spawn_blocking(move || {
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建备份输出目录失败: {}", e))?;
        }
        let file = File::create(&target_path).map_err(|e| format!("创建备份包失败: {}", e))?;
        let mut zip = ZipWriter::new(file);
        let options: FileOptions<'_, ()> = FileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o644);

        for entry in collect_files(&source_dir)? {
            let rel = entry
                .strip_prefix(&source_dir)
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
        let metadata =
            fs::metadata(&target_path).map_err(|e| format!("读取备份包大小失败: {}", e))?;
        Ok(metadata.len())
    })
    .await
    .unwrap_or_else(|_| Err("压缩备份任务崩溃".to_string()))
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
    serde_json::from_str::<BackupManifest>(&content)
        .map_err(|e| format!("解析 manifest 失败: {}", e))
}

/// 单个文件解压后最大 200MB
const MAX_SINGLE_FILE_SIZE: u64 = 200 * 1024 * 1024;
/// 整个备份包解压后最大 2GB
const MAX_TOTAL_EXTRACTED_SIZE: u64 = 2 * 1024 * 1024 * 1024;

pub fn extract_package_to_dir(package_path: &Path, target_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(target_dir).map_err(|e| format!("创建解压目录失败: {}", e))?;
    let file = File::open(package_path).map_err(|e| format!("打开备份包失败: {}", e))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("解析备份包失败: {}", e))?;
    let mut total_extracted: u64 = 0;
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
        let uncompressed_size = zipped.size();
        if uncompressed_size > MAX_SINGLE_FILE_SIZE {
            return Err(format!(
                "备份包中文件过大: {} (最大允许 {}MB)",
                zipped.name(),
                MAX_SINGLE_FILE_SIZE / 1024 / 1024
            ));
        }
        total_extracted += uncompressed_size;
        if total_extracted > MAX_TOTAL_EXTRACTED_SIZE {
            return Err(format!(
                "备份包解压后总大小超过限制 (最大允许 {}GB)",
                MAX_TOTAL_EXTRACTED_SIZE / 1024 / 1024 / 1024
            ));
        }
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建解压父目录失败: {}", e))?;
        }
        let mut output =
            File::create(&output_path).map_err(|e| format!("创建解压文件失败: {}", e))?;
        std::io::copy(&mut zipped, &mut output).map_err(|e| format!("写入解压文件失败: {}", e))?;
    }
    Ok(())
}

pub fn validate_manifest_checksums(
    extracted_dir: &Path,
    manifest: &BackupManifest,
) -> Result<(), String> {
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
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|e| format!("序列化备份数据失败: {}", e))?;
    fs::write(&path, bytes).map_err(|e| format!("写入备份数据失败 {}: {}", path.display(), e))
}

fn collect_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    collect_files_recursive(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_files_recursive(root: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in
        fs::read_dir(root).map_err(|e| format!("读取目录失败 {}: {}", root.display(), e))?
    {
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
    digest
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_empty() {
        let hash = sha256_hex(b"");
        assert_eq!(hash.len(), 64);
        // SHA256 of empty string
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sha256_known_value() {
        let hash = sha256_hex(b"hello");
        assert_eq!(hash.len(), 64);
        // SHA256 of "hello"
        assert_eq!(
            hash,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn sha256_deterministic() {
        let h1 = sha256_hex(b"test data");
        let h2 = sha256_hex(b"test data");
        assert_eq!(h1, h2);
    }

    #[test]
    fn sha256_different_inputs() {
        let h1 = sha256_hex(b"abc");
        let h2 = sha256_hex(b"def");
        assert_ne!(h1, h2);
    }

    #[test]
    fn create_backup_temp_dir_unique() {
        let dir1 = create_backup_temp_dir().unwrap();
        let dir2 = create_backup_temp_dir().unwrap();
        assert_ne!(dir1, dir2);
        assert!(dir1.exists());
        assert!(dir2.exists());
        // Cleanup
        let _ = fs::remove_dir_all(&dir1);
        let _ = fs::remove_dir_all(&dir2);
    }

    #[test]
    fn create_backup_temp_dir_naming() {
        let dir = create_backup_temp_dir().unwrap();
        let name = dir.file_name().unwrap().to_str().unwrap();
        assert!(name.starts_with("fuyun_tools_backup_"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn collect_files_basic() {
        let tmp = create_backup_temp_dir().unwrap();
        fs::write(tmp.join("a.txt"), "hello").unwrap();
        fs::write(tmp.join("b.txt"), "world").unwrap();
        fs::create_dir(tmp.join("sub")).unwrap();
        fs::write(tmp.join("sub").join("c.txt"), "deep").unwrap();

        let files = collect_files(&tmp).unwrap();
        assert_eq!(files.len(), 3);
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn compute_checksums_excludes_manifest() {
        let tmp = create_backup_temp_dir().unwrap();
        fs::write(tmp.join("data.json"), "{}").unwrap();
        fs::write(tmp.join(MANIFEST_PATH), "{}").unwrap();

        let checksums = compute_checksums(&tmp).unwrap();
        assert!(checksums.contains_key("data.json"));
        assert!(!checksums.contains_key(MANIFEST_PATH));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn compute_checksums_sha256_prefix() {
        let tmp = create_backup_temp_dir().unwrap();
        fs::write(tmp.join("test.txt"), "content").unwrap();

        let checksums = compute_checksums(&tmp).unwrap();
        let hash = checksums.get("test.txt").unwrap();
        assert!(hash.starts_with("sha256:"));
        // "sha256:" (7) + 64 hex chars = 71
        assert_eq!(hash.len(), 71);
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn now_ms_positive() {
        let ts = now_ms();
        assert!(ts > 0);
        // Should be after 2020-01-01
        assert!(ts > 1577836800000);
    }

    #[test]
    fn settings_path_constant() {
        assert_eq!(settings_path(), "settings/settings.json");
    }

    #[test]
    fn text_history_path_constant() {
        assert_eq!(text_history_path(), "text_history/history.json");
    }

    #[test]
    fn image_history_path_constant() {
        assert_eq!(image_history_path(), "image_history/image_history.json");
    }

    // ===== cleanup_dir =====

    #[test]
    fn cleanup_removes_directory() {
        let tmp = create_backup_temp_dir().unwrap();
        let sub = tmp.join("to_delete");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("file.txt"), "data").unwrap();
        assert!(sub.exists());
        cleanup_dir(&sub);
        assert!(!sub.exists());
        // parent tmp still exists
        assert!(tmp.exists());
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn cleanup_nonexistent_dir_no_panic() {
        let fake = std::env::temp_dir().join("fuyun_nonexistent_dir_xyz_12345");
        cleanup_dir(&fake); // should not panic
    }

    // ===== collect_files edge cases =====

    #[test]
    fn collect_files_empty_dir() {
        let tmp = create_backup_temp_dir().unwrap();
        let files = collect_files(&tmp).unwrap();
        assert!(files.is_empty());
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn collect_files_nested_dirs() {
        let tmp = create_backup_temp_dir().unwrap();
        fs::create_dir_all(tmp.join("a/b/c")).unwrap();
        fs::write(tmp.join("a/b/c/deep.txt"), "x").unwrap();
        fs::write(tmp.join("root.txt"), "y").unwrap();
        let files = collect_files(&tmp).unwrap();
        assert_eq!(files.len(), 2);
        let _ = fs::remove_dir_all(&tmp);
    }

    // ===== write_json =====

    #[test]
    fn write_json_creates_file() {
        let tmp = create_backup_temp_dir().unwrap();
        let path = tmp.join("test.json");
        let data = serde_json::json!({"key": "value"});
        write_json(path.clone(), &data).unwrap();
        assert!(path.exists());
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("key"));
        assert!(content.contains("value"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn write_json_creates_parent_dirs() {
        let tmp = create_backup_temp_dir().unwrap();
        let path = tmp.join("sub").join("deep").join("test.json");
        let data = serde_json::json!({"a": 1});
        write_json(path, &data).unwrap();
        assert!(tmp.join("sub/deep/test.json").exists());
        let _ = fs::remove_dir_all(&tmp);
    }

    // ===== extract_package_to_dir edge cases =====

    #[test]
    fn extract_nonexistent_file_fails() {
        let fake = std::env::temp_dir().join("fuyun_nonexistent_pkg.zip");
        let target = create_backup_temp_dir().unwrap();
        let result = extract_package_to_dir(&fake, &target);
        assert!(result.is_err());
        let _ = fs::remove_dir_all(&target);
    }

    // ===== validate_manifest_checksums =====

    #[test]
    fn validate_checksums_matches() {
        let tmp = create_backup_temp_dir().unwrap();
        fs::write(tmp.join("data.json"), "test").unwrap();
        let checksums = compute_checksums(&tmp).unwrap();
        let result = validate_manifest_checksums(&tmp, &BackupManifest {
            backup_format_version: 1,
            app_name: "test".into(),
            app_version: "1.0".into(),
            created_at: 0,
            platform: "test".into(),
            includes: Default::default(),
            stats: Default::default(),
            checksums,
        });
        assert!(result.is_ok());
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn validate_checksums_mismatch_fails() {
        let tmp = create_backup_temp_dir().unwrap();
        fs::write(tmp.join("data.json"), "test").unwrap();
        let mut bad_checksums = std::collections::HashMap::new();
        bad_checksums.insert("data.json".to_string(), "sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string());
        let result = validate_manifest_checksums(&tmp, &BackupManifest {
            backup_format_version: 1,
            app_name: "test".into(),
            app_version: "1.0".into(),
            created_at: 0,
            platform: "test".into(),
            includes: Default::default(),
            stats: Default::default(),
            checksums: bad_checksums,
        });
        assert!(result.is_err());
        let _ = fs::remove_dir_all(&tmp);
    }

    // ===================================================================
    // 集成测试：真实文件系统操作
    // ===================================================================

    #[test]
    fn integration_backup_extract_roundtrip() {
        let source_dir = create_backup_temp_dir().unwrap();
        let target_dir = create_backup_temp_dir().unwrap();

        // 创建测试文件结构
        fs::create_dir_all(source_dir.join("settings")).unwrap();
        fs::write(source_dir.join("settings/settings.json"), r#"{"hot_key":"Ctrl+V"}"#).unwrap();
        fs::create_dir_all(source_dir.join("text_history")).unwrap();
        fs::write(
            source_dir.join("text_history/history.json"),
            r#"{"items":["hello","world"],"categories":{},"category_list":[],"pinned_items":[]}"#,
        ).unwrap();
        fs::write(source_dir.join("manifest.json"), r#"{"version":1}"#).unwrap();

        // 打包
        let zip_path = std::env::temp_dir().join("fuyun_test_roundtrip.zip");
        let size = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(zip_backup_dir(&source_dir, &zip_path))
            .unwrap();
        assert!(size > 0);
        assert!(zip_path.exists());

        // 解压到新目录
        extract_package_to_dir(&zip_path, &target_dir).unwrap();

        // 验证文件内容
        let settings = fs::read_to_string(target_dir.join("settings/settings.json")).unwrap();
        assert!(settings.contains("Ctrl+V"));

        let history = fs::read_to_string(target_dir.join("text_history/history.json")).unwrap();
        assert!(history.contains("hello"));

        let manifest = fs::read_to_string(target_dir.join("manifest.json")).unwrap();
        assert!(manifest.contains("version"));

        // 清理
        let _ = fs::remove_file(&zip_path);
        let _ = fs::remove_dir_all(&source_dir);
        let _ = fs::remove_dir_all(&target_dir);
    }

    #[test]
    fn integration_checksum_roundtrip() {
        let dir1 = create_backup_temp_dir().unwrap();
        let dir2 = create_backup_temp_dir().unwrap();

        // 创建相同的文件
        fs::write(dir1.join("data.txt"), "same content").unwrap();
        fs::write(dir2.join("data.txt"), "same content").unwrap();

        let checksums1 = compute_checksums(&dir1).unwrap();
        let checksums2 = compute_checksums(&dir2).unwrap();

        assert_eq!(checksums1, checksums2, "相同文件应有相同校验和");

        // 修改一个文件
        fs::write(dir2.join("data.txt"), "different content").unwrap();
        let checksums3 = compute_checksums(&dir2).unwrap();
        assert_ne!(checksums1, checksums3, "不同文件应有不同校验和");

        let _ = fs::remove_dir_all(&dir1);
        let _ = fs::remove_dir_all(&dir2);
    }

    #[test]
    fn integration_extract_validates_zip() {
        // 使用真实的 BackupManifest 序列化
        let source = create_backup_temp_dir().unwrap();
        fs::write(source.join("test.txt"), "data").unwrap();

        let mut checksums = std::collections::HashMap::new();
        checksums.insert(
            "test.txt".to_string(),
            format!("sha256:{}", sha256_hex(b"data")),
        );
        let manifest = BackupManifest {
            backup_format_version: 1,
            app_name: "test".into(),
            app_version: "1.0".into(),
            created_at: 0,
            platform: "test".into(),
            includes: Default::default(),
            stats: Default::default(),
            checksums,
        };
        let manifest_json = serde_json::to_string(&manifest).unwrap();
        fs::write(source.join(MANIFEST_PATH), &manifest_json).unwrap();

        let zip_path = std::env::temp_dir().join("fuyun_test_validate.zip");
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(zip_backup_dir(&source, &zip_path))
            .unwrap();

        // 读取 manifest
        let manifest = read_manifest_from_package(&zip_path).unwrap();
        assert!(manifest.checksums.len() > 0);

        // 校验 checksum
        let target = create_backup_temp_dir().unwrap();
        extract_package_to_dir(&zip_path, &target).unwrap();
        let result = validate_manifest_checksums(&target, &manifest);
        assert!(result.is_ok(), "校验和应该匹配");

        let _ = fs::remove_file(&zip_path);
        let _ = fs::remove_dir_all(&source);
        let _ = fs::remove_dir_all(&target);
    }

    #[test]
    fn integration_extract_with_nested_dirs() {
        let source = create_backup_temp_dir().unwrap();
        fs::create_dir_all(source.join("a/b/c")).unwrap();
        fs::write(source.join("a/b/c/deep.txt"), "nested content").unwrap();
        fs::write(source.join("root.txt"), "root content").unwrap();

        let zip_path = std::env::temp_dir().join("fuyun_test_nested.zip");
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(zip_backup_dir(&source, &zip_path))
            .unwrap();

        let target = create_backup_temp_dir().unwrap();
        extract_package_to_dir(&zip_path, &target).unwrap();

        assert_eq!(
            fs::read_to_string(target.join("a/b/c/deep.txt")).unwrap(),
            "nested content"
        );
        assert_eq!(
            fs::read_to_string(target.join("root.txt")).unwrap(),
            "root content"
        );

        let _ = fs::remove_file(&zip_path);
        let _ = fs::remove_dir_all(&source);
        let _ = fs::remove_dir_all(&target);
    }

    #[test]
    fn integration_extract_rejects_oversized_file() {
        let source = create_backup_temp_dir().unwrap();
        // 创建一个超过 200MB 的文件（用稀疏文件模拟）
        let big_path = source.join("big.bin");
        let file = fs::File::create(&big_path).unwrap();
        file.set_len(MAX_SINGLE_FILE_SIZE + 1).unwrap();
        drop(file);

        let zip_path = std::env::temp_dir().join("fuyun_test_oversized.zip");
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(zip_backup_dir(&source, &zip_path))
            .unwrap();

        let target = create_backup_temp_dir().unwrap();
        let result = extract_package_to_dir(&zip_path, &target);
        assert!(result.is_err(), "超过大小限制的文件应该被拒绝");

        let _ = fs::remove_file(&zip_path);
        let _ = fs::remove_dir_all(&source);
        let _ = fs::remove_dir_all(&target);
    }
}
