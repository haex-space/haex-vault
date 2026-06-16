//! Core types for the file sync engine

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::hashing::ChunkedHash;

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
    /// BLAKE3 of the full file, lowercase hex. None for directories.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub hash: Option<String>,
    /// Chunk size in bytes. None for directories.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub chunk_size: Option<u32>,
    /// BLAKE3 hash of each chunk, lowercase hex, in order. None for directories;
    /// empty Vec is invalid for files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub chunk_hashes: Option<Vec<String>>,
}

impl FileState {
    /// If all three chunk fields are populated, construct the wire-typed
    /// `ChunkedHash` view of this manifest entry. Returns `None` for
    /// directories and for entries from providers (cloud, legacy) that do
    /// not yet announce per-chunk hashes.
    pub fn chunked_hash(&self) -> Option<ChunkedHash> {
        match (
            self.hash.as_deref(),
            self.chunk_size,
            self.chunk_hashes.as_ref(),
        ) {
            // Three guards beyond "all fields populated":
            // - `!self.is_directory` — directories never carry chunk hashes;
            //   defence in depth against a literal FileState that fakes it.
            // - `chunk_size > 0` — defensive against a future code path that
            //   produces a malformed FileState; downstream verifiers reject
            //   `chunk_size == 0` anyway.
            // - `!chunk_hashes.is_empty() || self.size == 0` — a zero-byte
            //   file legitimately has zero chunks (the streaming hasher's
            //   tail-flush never fires), so its manifest entry is
            //   `chunk_hashes: Some(vec![])`. We must accept that case or
            //   the sync flow silently bypasses manifest pinning for
            //   zero-byte files (caller falls back to stat-probe chunks).
            //   A non-empty `size` paired with `Some(vec![])` is still
            //   malformed and gets rejected.
            (Some(file_hash), Some(chunk_size), Some(chunk_hashes))
                if !self.is_directory
                    && chunk_size > 0
                    && (!chunk_hashes.is_empty() || self.size == 0) =>
            {
                Some(ChunkedHash {
                    file_hash: file_hash.to_string(),
                    chunk_size,
                    chunk_hashes: chunk_hashes.clone(),
                })
            }
            _ => None,
        }
    }
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

#[cfg(test)]
mod tests;
