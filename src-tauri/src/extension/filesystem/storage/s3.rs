// src-tauri/src/extension/filesystem/storage/s3.rs
//!
//! S3-compatible Storage Backend
//!
//! Supports AWS S3, Cloudflare R2, MinIO, and other S3-compatible services.
//!

use super::{BlobInfo, BlobMetadata, StorageBackend, TransferProgress};
use crate::extension::filesystem::error::FileSyncError;
use crate::extension::filesystem::types::S3BackendConfig;
use async_trait::async_trait;
use aws_sdk_s3::{
    config::{Credentials, Region},
    primitives::ByteStream,
    Client,
};
use tokio::sync::mpsc;

/// S3-compatible storage backend
pub struct S3Backend {
    client: Client,
    bucket: String,
    backend_type: &'static str,
}

impl S3Backend {
    /// Create a new S3 backend from config
    pub async fn new(config: &S3BackendConfig) -> Result<Self, FileSyncError> {
        let credentials = Credentials::new(
            &config.access_key_id,
            &config.secret_access_key,
            None, // session token
            None, // expiry
            "haex-files",
        );

        let region = Region::new(config.region.clone());

        let mut s3_config_builder = aws_sdk_s3::Config::builder()
            .credentials_provider(credentials)
            .region(region)
            .force_path_style(true); // Required for MinIO and some S3-compatible services

        // Set custom endpoint for R2, MinIO, etc.
        if let Some(endpoint) = &config.endpoint {
            s3_config_builder = s3_config_builder.endpoint_url(endpoint);
        }

        let s3_config = s3_config_builder.build();
        let client = Client::from_conf(s3_config);

        let backend_type = match config.backend_type {
            crate::extension::filesystem::types::StorageBackendType::S3 => "s3",
            crate::extension::filesystem::types::StorageBackendType::R2 => "r2",
            crate::extension::filesystem::types::StorageBackendType::Minio => "minio",
            _ => "s3",
        };

        Ok(Self {
            client,
            bucket: config.bucket.clone(),
            backend_type,
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
        self.client
            .list_objects_v2()
            .bucket(&self.bucket)
            .max_keys(1)
            .send()
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

        let body = ByteStream::from(data.to_vec());

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(remote_id)
            .body(body)
            .send()
            .await
            .map_err(|e| FileSyncError::UploadFailed {
                reason: format!("S3 upload failed: {}", e),
            })?;

        Ok(())
    }

    async fn download(&self, remote_id: &str) -> Result<Vec<u8>, FileSyncError> {
        let response = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(remote_id)
            .send()
            .await
            .map_err(|e| FileSyncError::DownloadFailed {
                reason: format!("S3 download failed: {}", e),
            })?;

        let bytes = response
            .body
            .collect()
            .await
            .map_err(|e| FileSyncError::DownloadFailed {
                reason: format!("Failed to read S3 response body: {}", e),
            })?
            .into_bytes()
            .to_vec();

        Ok(bytes)
    }

    async fn delete(&self, remote_id: &str) -> Result<(), FileSyncError> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(remote_id)
            .send()
            .await
            .map_err(|e| FileSyncError::Internal {
                reason: format!("S3 delete failed: {}", e),
            })?;

        Ok(())
    }

    async fn exists(&self, remote_id: &str) -> Result<bool, FileSyncError> {
        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(remote_id)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => {
                // Check if it's a "not found" error
                let service_error = e.into_service_error();
                if service_error.is_not_found() {
                    Ok(false)
                } else {
                    Err(FileSyncError::Internal {
                        reason: format!("S3 head_object failed: {}", service_error),
                    })
                }
            }
        }
    }

    async fn get_metadata(&self, remote_id: &str) -> Result<BlobMetadata, FileSyncError> {
        let response = self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(remote_id)
            .send()
            .await
            .map_err(|e| FileSyncError::FileNotFound {
                id: format!("{}: {}", remote_id, e),
            })?;

        Ok(BlobMetadata {
            size: response.content_length().unwrap_or(0) as u64,
            last_modified: response.last_modified().map(|dt| dt.to_string()),
            etag: response.e_tag().map(|s| s.to_string()),
        })
    }

    async fn list(&self, prefix: Option<&str>) -> Result<Vec<BlobInfo>, FileSyncError> {
        let mut request = self.client.list_objects_v2().bucket(&self.bucket);

        if let Some(prefix) = prefix {
            request = request.prefix(prefix);
        }

        let response = request.send().await.map_err(|e| FileSyncError::Internal {
            reason: format!("S3 list failed: {}", e),
        })?;

        let objects = response
            .contents()
            .iter()
            .map(|obj| BlobInfo {
                key: obj.key().unwrap_or_default().to_string(),
                size: obj.size().unwrap_or(0) as u64,
                last_modified: obj.last_modified().map(|dt| dt.to_string()),
            })
            .collect();

        Ok(objects)
    }
}
