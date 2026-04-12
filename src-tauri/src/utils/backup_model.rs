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
}

fn default_create_rollback_point() -> bool {
    true
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
