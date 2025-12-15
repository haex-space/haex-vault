// src-tauri/src/extension/filesystem/types.rs

use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ============================================================================
// Spaces
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileSpace {
    pub id: String,
    pub name: String,
    pub is_personal: bool,
    pub file_count: u64,
    pub total_size: u64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CreateSpaceRequest {
    pub name: String,
}

// ============================================================================
// Files
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileInfo {
    pub id: String,
    pub space_id: String,
    pub name: String,
    pub path: String,
    pub mime_type: Option<String>,
    pub size: u64,
    pub content_hash: String,
    pub is_directory: bool,
    pub sync_state: FileSyncState,
    pub backends: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum FileSyncState {
    Synced,
    Syncing,
    LocalOnly,
    RemoteOnly,
    Conflict,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ListFilesRequest {
    pub space_id: String,
    pub path: Option<String>,
    pub recursive: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UploadFileRequest {
    pub space_id: String,
    pub local_path: String,
    pub remote_path: Option<String>,
    pub backend_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DownloadFileRequest {
    pub file_id: String,
    pub local_path: String,
}

// ============================================================================
// Storage Backends
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StorageBackendInfo {
    pub id: String,
    #[serde(rename = "type")]
    pub backend_type: StorageBackendType,
    pub name: String,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum StorageBackendType {
    S3,
    R2,
    Minio,
    #[serde(rename = "gdrive")]
    GDrive,
    Dropbox,
}

impl std::fmt::Display for StorageBackendType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageBackendType::S3 => write!(f, "s3"),
            StorageBackendType::R2 => write!(f, "r2"),
            StorageBackendType::Minio => write!(f, "minio"),
            StorageBackendType::GDrive => write!(f, "gdrive"),
            StorageBackendType::Dropbox => write!(f, "dropbox"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct S3BackendConfig {
    #[serde(rename = "type")]
    pub backend_type: StorageBackendType,
    pub endpoint: Option<String>,
    pub region: String,
    pub bucket: String,
    pub access_key_id: String,
    pub secret_access_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AddBackendRequest {
    #[serde(rename = "type")]
    pub backend_type: StorageBackendType,
    pub name: String,
    pub config: S3BackendConfig,
}

// ============================================================================
// Sync Rules
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncRule {
    pub id: String,
    pub space_id: String,
    pub local_path: String,
    pub backend_ids: Vec<String>,
    pub direction: SyncDirection,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum SyncDirection {
    Up,
    Down,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AddSyncRuleRequest {
    pub space_id: String,
    pub local_path: String,
    pub backend_ids: Vec<String>,
    pub direction: Option<SyncDirection>,
}

// ============================================================================
// Sync Status
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncStatus {
    pub is_syncing: bool,
    pub pending_uploads: u32,
    pub pending_downloads: u32,
    pub last_sync: Option<String>,
    pub errors: Vec<SyncError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncError {
    pub file_id: String,
    pub file_name: String,
    pub error: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncProgress {
    pub file_id: String,
    pub file_name: String,
    pub bytes_transferred: u64,
    pub total_bytes: u64,
    pub direction: SyncProgressDirection,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum SyncProgressDirection {
    Upload,
    Download,
}

// ============================================================================
// Conflict Resolution
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ConflictResolution {
    Local,
    Remote,
    KeepBoth,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ResolveConflictRequest {
    pub file_id: String,
    pub resolution: ConflictResolution,
}
