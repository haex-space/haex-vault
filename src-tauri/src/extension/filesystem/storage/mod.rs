// src-tauri/src/extension/filesystem/storage/mod.rs
//!
//! Storage Backend Trait and Implementations
//!

pub mod s3;

use crate::extension::filesystem::error::FileSyncError;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Progress update for uploads/downloads
#[derive(Debug, Clone)]
pub struct TransferProgress {
    pub bytes_transferred: u64,
    pub total_bytes: u64,
}

/// Storage backend trait
///
/// All storage backends must implement this trait.
/// Implementations should be thread-safe (Send + Sync).
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Get the backend type identifier
    fn backend_type(&self) -> &'static str;

    /// Test the connection to the backend
    async fn test_connection(&self) -> Result<(), FileSyncError>;

    /// Upload a blob to the backend
    ///
    /// * `remote_id` - Unique identifier for the blob (e.g., UUID)
    /// * `data` - The data to upload
    /// * `progress` - Optional channel for progress updates
    async fn upload(
        &self,
        remote_id: &str,
        data: &[u8],
        progress: Option<mpsc::Sender<TransferProgress>>,
    ) -> Result<(), FileSyncError>;

    /// Download a blob from the backend
    async fn download(&self, remote_id: &str) -> Result<Vec<u8>, FileSyncError>;

    /// Delete a blob from the backend
    async fn delete(&self, remote_id: &str) -> Result<(), FileSyncError>;

    /// Check if a blob exists
    async fn exists(&self, remote_id: &str) -> Result<bool, FileSyncError>;

    /// Get blob metadata (size, last modified, etc.)
    async fn get_metadata(&self, remote_id: &str) -> Result<BlobMetadata, FileSyncError>;

    /// List blobs with optional prefix
    async fn list(&self, prefix: Option<&str>) -> Result<Vec<BlobInfo>, FileSyncError>;
}

/// Blob metadata
#[derive(Debug, Clone)]
pub struct BlobMetadata {
    pub size: u64,
    pub last_modified: Option<String>,
    pub etag: Option<String>,
}

/// Basic blob info for listing
#[derive(Debug, Clone)]
pub struct BlobInfo {
    pub key: String,
    pub size: u64,
    pub last_modified: Option<String>,
}

/// Backend registry for managing multiple backends
pub struct BackendRegistry {
    backends: std::collections::HashMap<String, Arc<dyn StorageBackend>>,
}

impl BackendRegistry {
    pub fn new() -> Self {
        Self {
            backends: std::collections::HashMap::new(),
        }
    }

    pub fn register(&mut self, id: String, backend: Arc<dyn StorageBackend>) {
        self.backends.insert(id, backend);
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn StorageBackend>> {
        self.backends.get(id).cloned()
    }

    pub fn remove(&mut self, id: &str) -> Option<Arc<dyn StorageBackend>> {
        self.backends.remove(id)
    }

    pub fn list(&self) -> Vec<String> {
        self.backends.keys().cloned().collect()
    }
}

impl Default for BackendRegistry {
    fn default() -> Self {
        Self::new()
    }
}
