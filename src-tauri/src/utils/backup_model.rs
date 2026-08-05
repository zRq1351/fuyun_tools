use crate::utils::database::ClipboardHistoryData;
use crate::utils::settings_model::AppSettingsData;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackupIncludes {
    pub settings: bool,
    pub text_history: bool,
    pub image_history: bool,
    pub image_blobs: bool,
    pub api_keys: bool,
    pub recordings: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackupStats {
    pub text_item_count: usize,
    pub image_item_count: usize,
    pub image_blob_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackupManifest {
    pub backup_format_version: u32,
    pub app_name: String,
    pub app_version: String,
    pub created_at: i64,
    pub platform: String,
    pub includes: BackupIncludes,
    pub stats: BackupStats,
    pub checksums: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackupExportPreviewData {
    pub includes: BackupIncludes,
    pub stats: BackupStats,
    pub estimated_bytes: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackupExportPreviewResponse {
    pub success: bool,
    pub message: String,
    pub data: BackupExportPreviewData,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackupExportRequest {
    pub target_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackupExportResultData {
    pub file_path: String,
    pub file_size_bytes: u64,
    pub created_at: i64,
    pub stats: BackupStats,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackupExportResultResponse {
    pub success: bool,
    pub message: String,
    pub data: BackupExportResultData,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackupPackagePreviewRequest {
    pub package_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackupRestoreOptions {
    pub can_restore_settings: bool,
    pub can_restore_text_history: bool,
    pub can_restore_image_history: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackupPackagePreviewData {
    pub manifest: BackupManifest,
    pub includes: BackupIncludes,
    pub stats: BackupStats,
    pub warnings: Vec<String>,
    pub restore_options: BackupRestoreOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackupPackagePreviewResponse {
    pub success: bool,
    pub message: String,
    pub data: BackupPackagePreviewData,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackupRestoreRequest {
    pub package_path: String,
    pub mode: String,
    pub restore_settings: bool,
    pub restore_text_history: bool,
    pub restore_image_history: bool,
    #[serde(default = "default_create_rollback_point")]
    pub create_rollback_point: bool,
    /// 恢复策略: "merge"(合并) 或 "overwrite"(覆盖),默认为 "merge"
    #[serde(default = "default_restore_strategy")]
    pub restore_strategy: String,
}

fn default_create_rollback_point() -> bool {
    true
}

fn default_restore_strategy() -> String {
    "merge".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackupRestoreModules {
    pub settings: bool,
    pub text_history: bool,
    pub image_history: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackupRestoreResultData {
    pub mode: String,
    pub restored: BackupRestoreModules,
    pub rollback_point_created: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackupRestoreResultResponse {
    pub success: bool,
    pub message: String,
    pub data: BackupRestoreResultData,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackupHistoryItem {
    pub file_name: String,
    pub file_path: String,
    pub file_size_bytes: u64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackupSettingsData {
    pub enabled: bool,
    pub frequency: String,
    pub target_dir: String,
    pub max_backup_count: usize,
    pub last_run_at: i64,
    pub last_run_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SaveBackupSettingsRequest {
    pub enabled: bool,
    pub frequency: String,
    pub target_dir: String,
    pub max_backup_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DeleteBackupHistoryItemRequest {
    pub file_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackupSettingsFile {
    pub settings: AppSettingsData,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackupTextHistoryFile {
    pub snapshot: ClipboardHistoryData,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackupImageHistoryItem {
    pub id: String,
    pub width: u32,
    pub height: u32,
    pub blob_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackupImageHistoryFile {
    pub items: Vec<BackupImageHistoryItem>,
    pub categories: HashMap<String, String>,
    pub category_list: Vec<String>,
    pub image_tags: HashMap<String, Vec<String>>,
    pub pinned_items: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct BackupBlobFile {
    pub item_id: String,
    pub source_path: String,
    pub package_path: String,
    pub file_size: u64,
}

#[derive(Debug, Clone, Default)]
pub struct PreparedBackupData {
    pub settings: AppSettingsData,
    pub text_history: ClipboardHistoryData,
    pub image_history: BackupImageHistoryFile,
    pub blobs: Vec<BackupBlobFile>,
    pub includes: BackupIncludes,
    pub stats: BackupStats,
    pub estimated_bytes: u64,
    pub warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backup_includes_default_all_false() {
        let inc = BackupIncludes::default();
        assert!(!inc.settings && !inc.text_history && !inc.image_history);
        assert!(!inc.image_blobs && !inc.api_keys && !inc.recordings);
    }

    #[test]
    fn test_backup_includes_camel_case_serde() {
        let inc = BackupIncludes {
            settings: true,
            text_history: false,
            image_history: true,
            image_blobs: false,
            api_keys: false,
            recordings: true,
        };
        let v: serde_json::Value = serde_json::to_value(&inc).unwrap();
        assert_eq!(v["settings"], true);
        assert_eq!(v["textHistory"], false);
        assert_eq!(v["imageHistory"], true);
        assert_eq!(v["recordings"], true);
    }

    #[test]
    fn test_backup_manifest_serde_roundtrip() {
        let manifest = BackupManifest {
            backup_format_version: 1,
            app_name: "fuyun".to_string(),
            app_version: "0.8.31".to_string(),
            created_at: 12345,
            platform: "windows".to_string(),
            includes: BackupIncludes::default(),
            stats: BackupStats::default(),
            checksums: HashMap::from([("file".to_string(), "abc".to_string())]),
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let back: BackupManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.app_name, "fuyun");
        assert_eq!(back.backup_format_version, 1);
        assert_eq!(back.checksums["file"], "abc");
    }

    #[test]
    fn test_backup_restore_request_defaults() {
        // 未提供可选字段时：createRollbackPoint=true, restoreStrategy=merge
        let req: BackupRestoreRequest = serde_json::from_str(
            r#"{"packagePath":"p","mode":"full","restoreSettings":true,"restoreTextHistory":true,"restoreImageHistory":true}"#,
        )
            .unwrap();
        assert_eq!(req.package_path, "p");
        assert_eq!(req.mode, "full");
        assert!(req.restore_settings);
        assert!(req.create_rollback_point);
        assert_eq!(req.restore_strategy, "merge");
    }

    #[test]
    fn test_backup_restore_request_explicit_values() {
        let req: BackupRestoreRequest = serde_json::from_str(
            r#"{"packagePath":"p","mode":"full","restoreSettings":false,"restoreTextHistory":false,"restoreImageHistory":false,"createRollbackPoint":false,"restoreStrategy":"overwrite"}"#,
        )
            .unwrap();
        assert!(!req.create_rollback_point);
        assert_eq!(req.restore_strategy, "overwrite");
        assert!(!req.restore_settings);
    }

    #[test]
    fn test_backup_image_history_file_serde() {
        let f = BackupImageHistoryFile {
            items: vec![BackupImageHistoryItem {
                id: "i1".to_string(),
                width: 100,
                height: 50,
                blob_path: "blobs/i1.png".to_string(),
            }],
            categories: HashMap::from([("i1".to_string(), "工作".to_string())]),
            category_list: vec!["工作".to_string()],
            image_tags: HashMap::new(),
            pinned_items: vec![],
        };
        let v: serde_json::Value = serde_json::to_value(&f).unwrap();
        assert_eq!(v["items"][0]["id"], "i1");
        assert_eq!(v["items"][0]["blobPath"], "blobs/i1.png");
        assert_eq!(v["categoryList"][0], "工作");
        assert_eq!(v["categories"]["i1"], "工作");
    }

    #[test]
    fn test_backup_defaults_all_empty() {
        let preview = BackupExportPreviewResponse::default();
        assert!(!preview.success);
        assert!(preview.data.warnings.is_empty());

        let result = BackupExportResultResponse::default();
        assert_eq!(result.data.file_size_bytes, 0);
    }
}
