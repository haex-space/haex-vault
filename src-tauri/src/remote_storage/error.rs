// src-tauri/src/storage/error.rs
//!
//! Storage Error Types
//!

use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error, Serialize)]
#[serde(tag = "type", content = "details")]
pub enum StorageError {
    #[error("Backend not found: {id}")]
    BackendNotFound { id: String },

    #[error("Backend connection failed: {reason}")]
    ConnectionFailed { reason: String },

    #[error("Upload failed: {reason}")]
    UploadFailed { reason: String },

    #[error("Download failed: {reason}")]
    DownloadFailed { reason: String },

    #[error("Delete failed: {reason}")]
    DeleteFailed { reason: String },

    #[error("Object not found: {key}")]
    ObjectNotFound { key: String },

    #[error("Invalid configuration: {reason}")]
    InvalidConfig { reason: String },

    #[error("Database error: {reason}")]
    DatabaseError { reason: String },

    #[error("Internal error: {reason}")]
    Internal { reason: String },
}

impl From<rusqlite::Error> for StorageError {
    fn from(e: rusqlite::Error) -> Self {
        StorageError::DatabaseError {
            reason: e.to_string(),
        }
    }
}
