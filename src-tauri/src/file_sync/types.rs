//! Core types for the file sync engine

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Metadata for a single file or directory.
/// Both local scans and remote manifests produce `Vec<FileState>`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileState {
    pub relative_path: String,
    pub size: u64,
    /// Unix timestamp in seconds
    pub modified_at: u64,
    pub is_directory: bool,
}

/// Sync direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SyncDirection {
    OneWay,
    TwoWay,
}

/// How to handle deletions during sync
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DeleteMode {
    Trash,
    Permanent,
    Ignore,
}

/// Actions computed by the diff engine
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncActions {
    pub to_download: Vec<FileState>,
    pub to_upload: Vec<FileState>,
    /// Relative paths of files to delete
    pub to_delete: Vec<String>,
    pub to_create_directories: Vec<String>,
    pub conflicts: Vec<SyncConflict>,
}

/// A conflict detected during two-way sync
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncConflict {
    pub relative_path: String,
    pub source_state: FileState,
    pub target_state: FileState,
}

/// Result after executing a sync
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncResult {
    pub files_downloaded: u32,
    pub files_deleted: u32,
    pub directories_created: u32,
    pub bytes_transferred: u64,
    pub conflicts_resolved: u32,
    pub errors: Vec<String>,
}

/// Progress update during sync execution
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncProgress {
    pub current_file: String,
    pub files_done: u32,
    pub files_total: u32,
    pub bytes_done: u64,
    pub bytes_total: u64,
    /// Files currently being transferred in parallel
    pub active_files: Vec<String>,
    /// Current transfer rate in bytes/second
    pub bytes_per_second: u64,
}
