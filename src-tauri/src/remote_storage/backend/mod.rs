// src-tauri/src/storage/backend.rs
//!
//! Storage Backend Trait and S3 Implementation
//!

use std::path::Path;

use super::error::StorageError;
use super::progress::ProgressCallback;
use super::types::{S3Config, StorageListDirResponse, StorageObjectInfo};
use async_trait::async_trait;

pub mod s3;

pub(crate) use s3::build_s3_bucket;
pub use s3::S3Backend;

/// Progress update for uploads/downloads
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct TransferProgress {
    pub bytes_transferred: u64,
    pub total_bytes: u64,
}

/// Storage backend trait
///
/// All storage backends must implement this trait.
/// Implementations should be thread-safe (Send + Sync).
#[allow(dead_code)]
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Get the backend type identifier
    fn backend_type(&self) -> &'static str;

    /// Test the connection to the backend
    async fn test_connection(&self) -> Result<(), StorageError>;

    /// Make sure the backing container (e.g. S3 bucket) exists, creating it
    /// if missing. Backends without a container concept can leave the default
    /// implementation untouched.
    async fn ensure_container(&self) -> Result<(), StorageError> {
        Ok(())
    }

    /// Upload data to the backend
    async fn upload(&self, key: &str, data: &[u8]) -> Result<(), StorageError>;

    /// Download data from the backend
    async fn download(&self, key: &str) -> Result<Vec<u8>, StorageError>;

    /// Delete an object from the backend
    async fn delete(&self, key: &str) -> Result<(), StorageError>;

    /// Check if an object exists
    async fn exists(&self, key: &str) -> Result<bool, StorageError>;

    /// List objects with optional prefix
    async fn list(&self, prefix: Option<&str>) -> Result<Vec<StorageObjectInfo>, StorageError>;

    /// Directory-style listing of a single hierarchy level under the prefix.
    /// Returns sub-prefixes (folders) and objects whose keys do not contain
    /// any further `/` after the prefix.
    ///
    /// Default impl falls back to a flat `list` and reconstructs the
    /// hierarchy client-side, which is fine for small backends but should
    /// be overridden by anything supporting native delimiter-based listing
    /// (S3) to avoid enumerating an entire bucket per folder open.
    async fn list_dir(&self, prefix: Option<&str>) -> Result<StorageListDirResponse, StorageError> {
        let objects = self.list(prefix).await?;
        let prefix_str = prefix.unwrap_or("");
        let mut folders: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut files: Vec<StorageObjectInfo> = Vec::new();
        for obj in objects {
            let rest = obj.key.strip_prefix(prefix_str).unwrap_or(&obj.key);
            if let Some(idx) = rest.find('/') {
                let folder = format!("{}{}", prefix_str, &rest[..=idx]);
                folders.insert(folder);
            } else {
                files.push(obj);
            }
        }
        Ok(StorageListDirResponse {
            folders: folders.into_iter().collect(),
            objects: files,
        })
    }

    /// Upload a local file to the backend, streaming if supported.
    ///
    /// Default impl reads the whole file into memory and calls `upload`. Override
    /// in backends that can stream (e.g. S3 multipart) to avoid full-file
    /// buffering for large files.
    async fn upload_from_path(
        &self,
        key: &str,
        source_path: &Path,
        on_progress: Option<ProgressCallback>,
    ) -> Result<u64, StorageError> {
        let data = tokio::fs::read(source_path)
            .await
            .map_err(|e| StorageError::UploadFailed {
                reason: format!("read source: {}", e),
            })?;
        let n = data.len() as u64;
        self.upload(key, &data).await?;
        if let Some(cb) = on_progress {
            cb(n, n);
        }
        Ok(n)
    }

    /// Cancellation-aware variant of `upload_from_path`. Backends that can
    /// observe `cancel_token` between chunks **must** also clean up server-side
    /// state (e.g. abort the multipart upload) when the token fires so we
    /// don't leak in-flight uploads (rust-s3 charges until lifecycle policies
    /// kick in).
    ///
    /// Default impl ignores `cancel_token` and falls back to `upload_from_path`,
    /// which is correct for backends whose `upload` is atomic from the server's
    /// point of view (no orphaned state on caller drop). S3 overrides this
    /// with a manual multipart loop that calls `abort_upload` on cancel.
    async fn upload_from_path_cancellable(
        &self,
        key: &str,
        source_path: &Path,
        on_progress: Option<ProgressCallback>,
        _cancel_token: Option<tokio_util::sync::CancellationToken>,
    ) -> Result<u64, StorageError> {
        self.upload_from_path(key, source_path, on_progress).await
    }

    /// Download an object from the backend into a local file, streaming if
    /// supported.
    ///
    /// Default impl downloads into memory and writes to disk. Override in
    /// backends that can stream (e.g. S3 chunked GET) to avoid full-file
    /// buffering for large files.
    async fn download_to_path(
        &self,
        key: &str,
        output_path: &Path,
        on_progress: Option<ProgressCallback>,
    ) -> Result<u64, StorageError> {
        let data = self.download(key).await?;
        let n = data.len() as u64;
        tokio::fs::write(output_path, &data)
            .await
            .map_err(|e| StorageError::DownloadFailed {
                reason: format!("write dest: {}", e),
            })?;
        if let Some(cb) = on_progress {
            cb(n, n);
        }
        Ok(n)
    }

    /// Resumable streaming download.
    ///
    /// If `output_path` already exists, the implementation should treat its
    /// current size as the resume offset and continue from there (append
    /// mode, Range-GET from that byte). The progress callback should report
    /// `(total_done_so_far, total_size)` so the UI shows monotonic progress
    /// across resume events. Returns the total number of bytes the file
    /// holds after the call.
    ///
    /// Default impl rejects with a clear error so callers can detect that
    /// the active backend doesn't yet support resumable downloads (today:
    /// non-S3 backends).
    async fn download_to_path_resumable(
        &self,
        _key: &str,
        _output_path: &Path,
        _on_progress: Option<ProgressCallback>,
    ) -> Result<u64, StorageError> {
        Err(StorageError::Internal {
            reason: format!(
                "Resumable downloads not supported by {} backend",
                self.backend_type()
            ),
        })
    }
}

/// Create a storage backend from type and config
pub async fn create_backend(
    backend_type: &str,
    config: &serde_json::Value,
) -> Result<Box<dyn StorageBackend>, StorageError> {
    match backend_type {
        "s3" => {
            let s3_config: S3Config = serde_json::from_value(config.clone()).map_err(|e| {
                StorageError::InvalidConfig {
                    reason: format!("Invalid S3 config: {}", e),
                }
            })?;
            let backend = S3Backend::new(&s3_config).await?;
            Ok(Box::new(backend))
        }
        _ => Err(StorageError::InvalidConfig {
            reason: format!("Unknown backend type: {}", backend_type),
        }),
    }
}
