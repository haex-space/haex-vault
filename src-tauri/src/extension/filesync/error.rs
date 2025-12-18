// src-tauri/src/extension/filesync/error.rs

use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error, Serialize)]
#[serde(tag = "type", content = "details")]
pub enum FileSyncError {
    #[error("Extension not found: {public_key}::{name}")]
    ExtensionNotFound { public_key: String, name: String },

    #[error("Space not found: {id}")]
    SpaceNotFound { id: String },

    #[error("File not found: {id}")]
    FileNotFound { id: String },

    #[error("Backend not found: {id}")]
    BackendNotFound { id: String },

    #[error("Sync rule not found: {id}")]
    SyncRuleNotFound { id: String },

    #[error("Backend connection failed: {reason}")]
    BackendConnectionFailed { reason: String },

    #[error("Upload failed: {reason}")]
    UploadFailed { reason: String },

    #[error("Download failed: {reason}")]
    DownloadFailed { reason: String },

    #[error("Encryption error: {reason}")]
    EncryptionError { reason: String },

    #[error("Decryption error: {reason}")]
    DecryptionError { reason: String },

    #[error("Invalid configuration: {reason}")]
    InvalidConfig { reason: String },

    #[error("Filesystem error: {reason}")]
    FilesystemError { reason: String },

    #[error("Database error: {reason}")]
    DatabaseError { reason: String },

    #[error("Sync conflict: {file_id}")]
    SyncConflict { file_id: String },

    #[error("Permission denied: {reason}")]
    PermissionDenied { reason: String },

    #[error("Operation cancelled")]
    Cancelled,

    #[error("Not initialized")]
    NotInitialized,

    #[error("No backends configured")]
    NoBackendsConfigured,

    #[error("Already syncing")]
    AlreadySyncing,

    #[error("Internal error: {reason}")]
    Internal { reason: String },

    #[error("Not supported: {reason}")]
    NotSupported { reason: String },
}

impl From<std::io::Error> for FileSyncError {
    fn from(e: std::io::Error) -> Self {
        FileSyncError::FilesystemError {
            reason: e.to_string(),
        }
    }
}

impl From<rusqlite::Error> for FileSyncError {
    fn from(e: rusqlite::Error) -> Self {
        FileSyncError::DatabaseError {
            reason: e.to_string(),
        }
    }
}
