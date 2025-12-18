// src-tauri/src/extension/filesync/types.rs

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
    pub endpoint: Option<String>,
    pub region: String,
    pub bucket: String,
    pub access_key_id: String,
    pub secret_access_key: String,
}

/// Backend configuration for S3-compatible storage
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export)]
pub enum BackendConfig {
    #[serde(rename = "s3")]
    S3(S3BackendConfig),
    #[serde(rename = "r2")]
    R2(S3BackendConfig),
    #[serde(rename = "minio")]
    Minio(S3BackendConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AddBackendRequest {
    pub name: String,
    pub config: BackendConfig,
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

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateSyncRuleRequest {
    pub rule_id: String,
    pub backend_ids: Option<Vec<String>>,
    pub direction: Option<SyncDirection>,
    pub enabled: Option<bool>,
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
// Local File Scanning (unencrypted, for display only)
// ============================================================================

/// Request to scan local files in a sync rule folder
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ScanLocalRequest {
    /// Sync rule ID to get the local path from
    pub rule_id: String,
    /// Optional subdirectory path relative to sync rule root
    pub subpath: Option<String>,
}

/// Local file information (not encrypted, for UI display)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LocalFileInfo {
    /// Unique identifier (local path hash)
    pub id: String,
    /// File name
    pub name: String,
    /// Full local path
    pub path: String,
    /// Relative path from sync root
    pub relative_path: String,
    /// MIME type (if detected)
    pub mime_type: Option<String>,
    /// File size in bytes
    pub size: u64,
    /// Whether this is a directory
    pub is_directory: bool,
    /// Last modified time (ISO 8601)
    pub modified_at: Option<String>,
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
