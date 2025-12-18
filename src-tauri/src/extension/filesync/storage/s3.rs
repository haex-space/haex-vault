// src-tauri/src/extension/filesync/storage/s3.rs
//!
//! S3-compatible Storage Backend
//!
//! Supports AWS S3, Cloudflare R2, MinIO, and other S3-compatible services.
//! Uses rust-s3 with rustls for cross-platform support (including Android/iOS).
//!

use super::{BlobInfo, BlobMetadata, StorageBackend, TransferProgress};
use crate::extension::filesync::error::FileSyncError;
use crate::extension::filesync::types::{S3BackendConfig, StorageBackendType};
use async_trait::async_trait;
use s3::creds::Credentials;
use s3::region::Region;
use s3::Bucket;
use tokio::sync::mpsc;

/// S3-compatible storage backend
pub struct S3Backend {
    bucket: Box<Bucket>,
    backend_type: &'static str,
}

impl S3Backend {
    /// Create a new S3 backend from config
    ///
    /// The `backend_type` parameter determines the type string returned by `backend_type()`.
    pub async fn new(
        config: &S3BackendConfig,
        backend_type: StorageBackendType,
    ) -> Result<Self, FileSyncError> {
        let credentials = Credentials::new(
            Some(&config.access_key_id),
            Some(&config.secret_access_key),
            None, // security token
            None, // session token
            None, // profile
        )
        .map_err(|e| FileSyncError::BackendConnectionFailed {
            reason: format!("Failed to create credentials: {}", e),
        })?;

        // Determine region - for custom endpoints, use a custom region
        let region = if let Some(endpoint) = &config.endpoint {
            Region::Custom {
                region: config.region.clone(),
                endpoint: endpoint.clone(),
            }
        } else {
            config.region.parse().unwrap_or(Region::UsEast1)
        };

        let bucket = Bucket::new(&config.bucket, region, credentials)
            .map_err(|e| FileSyncError::BackendConnectionFailed {
                reason: format!("Failed to create bucket: {}", e),
            })?
            .with_path_style(); // Required for MinIO and some S3-compatible services

        let backend_type_str = match backend_type {
            StorageBackendType::S3 => "s3",
            StorageBackendType::R2 => "r2",
            StorageBackendType::Minio => "minio",
            _ => "s3",
        };

        Ok(Self {
            bucket,
            backend_type: backend_type_str,
        })
    }
}

#[async_trait]
impl StorageBackend for S3Backend {
    fn backend_type(&self) -> &'static str {
        self.backend_type
    }

    async fn test_connection(&self) -> Result<(), FileSyncError> {
        // Try to list objects (with max 1) to verify credentials and bucket access
        self.bucket
            .list("".to_string(), Some("/".to_string()))
            .await
            .map_err(|e| FileSyncError::BackendConnectionFailed {
                reason: format!("S3 connection test failed: {}", e),
            })?;

        Ok(())
    }

    async fn upload(
        &self,
        remote_id: &str,
        data: &[u8],
        _progress: Option<mpsc::Sender<TransferProgress>>,
    ) -> Result<(), FileSyncError> {
        // TODO: Implement multipart upload with progress for large files
        self.bucket
            .put_object(remote_id, data)
            .await
            .map_err(|e| FileSyncError::UploadFailed {
                reason: format!("S3 upload failed: {}", e),
            })?;

        Ok(())
    }

    async fn download(&self, remote_id: &str) -> Result<Vec<u8>, FileSyncError> {
        let response = self
            .bucket
            .get_object(remote_id)
            .await
            .map_err(|e| FileSyncError::DownloadFailed {
                reason: format!("S3 download failed: {}", e),
            })?;

        Ok(response.to_vec())
    }

    async fn delete(&self, remote_id: &str) -> Result<(), FileSyncError> {
        self.bucket
            .delete_object(remote_id)
            .await
            .map_err(|e| FileSyncError::Internal {
                reason: format!("S3 delete failed: {}", e),
            })?;

        Ok(())
    }

    async fn exists(&self, remote_id: &str) -> Result<bool, FileSyncError> {
        match self.bucket.head_object(remote_id).await {
            Ok(_) => Ok(true),
            Err(e) => {
                // Check if it's a "not found" error (404)
                let error_str = e.to_string();
                if error_str.contains("404") || error_str.contains("NoSuchKey") {
                    Ok(false)
                } else {
                    Err(FileSyncError::Internal {
                        reason: format!("S3 head_object failed: {}", e),
                    })
                }
            }
        }
    }

    async fn get_metadata(&self, remote_id: &str) -> Result<BlobMetadata, FileSyncError> {
        let (head, _status_code) = self
            .bucket
            .head_object(remote_id)
            .await
            .map_err(|e| FileSyncError::FileNotFound {
                id: format!("{}: {}", remote_id, e),
            })?;

        Ok(BlobMetadata {
            size: head.content_length.unwrap_or(0) as u64,
            last_modified: head.last_modified,
            etag: head.e_tag,
        })
    }

    async fn list(&self, prefix: Option<&str>) -> Result<Vec<BlobInfo>, FileSyncError> {
        let prefix_str = prefix.unwrap_or("").to_string();

        let results = self
            .bucket
            .list(prefix_str, None)
            .await
            .map_err(|e| FileSyncError::Internal {
                reason: format!("S3 list failed: {}", e),
            })?;

        let objects = results
            .into_iter()
            .flat_map(|result| result.contents)
            .map(|obj| BlobInfo {
                key: obj.key,
                size: obj.size,
                last_modified: Some(obj.last_modified),
            })
            .collect();

        Ok(objects)
    }
}
