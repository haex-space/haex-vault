// src-tauri/src/storage/backend.rs
//!
//! Storage Backend Trait and S3 Implementation
//!

use super::error::StorageError;
use super::types::{S3Config, StorageObjectInfo};
use async_trait::async_trait;
use s3::creds::Credentials;
use s3::region::Region;
use s3::Bucket;

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
}

/// S3-compatible storage backend
pub struct S3Backend {
    bucket: Box<Bucket>,
}

impl S3Backend {
    /// Create a new S3 backend from config
    pub async fn new(config: &S3Config) -> Result<Self, StorageError> {
        // Extract path prefix from endpoint URL if present
        let (clean_endpoint, effective_bucket) = if let Some(endpoint) = &config.endpoint {
            if let Ok(url) = url::Url::parse(endpoint) {
                let path = url.path();
                if path != "/" && !path.is_empty() {
                    let base = format!("{}://{}", url.scheme(), url.host_str().unwrap_or(""));
                    let prefix = path.trim_matches('/');
                    let combined_bucket = format!("{}/{}", prefix, config.bucket);
                    (Some(base), combined_bucket)
                } else {
                    (Some(endpoint.clone()), config.bucket.clone())
                }
            } else {
                (Some(endpoint.clone()), config.bucket.clone())
            }
        } else {
            (None, config.bucket.clone())
        };

        let credentials = Credentials::new(
            Some(&config.access_key_id),
            Some(&config.secret_access_key),
            None,
            None,
            None,
        )
        .map_err(|e| StorageError::ConnectionFailed {
            reason: format!("Failed to create credentials: {}", e),
        })?;

        let region = if let Some(endpoint) = &clean_endpoint {
            Region::Custom {
                region: config.region.clone(),
                endpoint: endpoint.clone(),
            }
        } else {
            config.region.parse().unwrap_or(Region::UsEast1)
        };

        let mut bucket = Bucket::new(&effective_bucket, region, credentials).map_err(|e| {
            StorageError::ConnectionFailed {
                reason: format!("Failed to create bucket: {}", e),
            }
        })?;

        let use_path_style = config.path_style.unwrap_or(false);

        if use_path_style {
            bucket = bucket.with_path_style();
        }

        Ok(Self { bucket })
    }
}

#[async_trait]
impl StorageBackend for S3Backend {
    fn backend_type(&self) -> &'static str {
        "s3"
    }

    async fn test_connection(&self) -> Result<(), StorageError> {
        self.bucket
            .list("".to_string(), Some("/".to_string()))
            .await
            .map_err(|e| StorageError::ConnectionFailed {
                reason: format!("S3 connection test failed: {}", e),
            })?;
        Ok(())
    }

    async fn upload(&self, key: &str, data: &[u8]) -> Result<(), StorageError> {
        self.bucket
            .put_object(key, data)
            .await
            .map_err(|e| StorageError::UploadFailed {
                reason: format!("S3 upload failed: {}", e),
            })?;
        Ok(())
    }

    async fn download(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        let response =
            self.bucket
                .get_object(key)
                .await
                .map_err(|e| StorageError::DownloadFailed {
                    reason: format!("S3 download failed: {}", e),
                })?;
        Ok(response.to_vec())
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.bucket
            .delete_object(key)
            .await
            .map_err(|e| StorageError::DeleteFailed {
                reason: format!("S3 delete failed: {}", e),
            })?;
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        match self.bucket.head_object(key).await {
            Ok(_) => Ok(true),
            Err(e) => {
                let error_str = e.to_string();
                if error_str.contains("404") || error_str.contains("NoSuchKey") {
                    Ok(false)
                } else {
                    Err(StorageError::Internal {
                        reason: format!("S3 head_object failed: {}", e),
                    })
                }
            }
        }
    }

    async fn list(&self, prefix: Option<&str>) -> Result<Vec<StorageObjectInfo>, StorageError> {
        let prefix_str = prefix.unwrap_or("").to_string();

        let results =
            self.bucket
                .list(prefix_str, None)
                .await
                .map_err(|e| StorageError::Internal {
                    reason: format!("S3 list failed: {}", e),
                })?;

        let objects = results
            .into_iter()
            .flat_map(|result| result.contents)
            .map(|obj| StorageObjectInfo {
                key: obj.key,
                size: obj.size,
                last_modified: Some(obj.last_modified),
            })
            .collect();

        Ok(objects)
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
