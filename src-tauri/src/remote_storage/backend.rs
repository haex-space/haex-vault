// src-tauri/src/storage/backend.rs
//!
//! Storage Backend Trait and S3 Implementation
//!

use std::path::Path;

use super::error::StorageError;
use super::progress::{ProgressCallback, ProgressReader, ProgressWriter};
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

    async fn upload_from_path(
        &self,
        key: &str,
        source_path: &Path,
        on_progress: Option<ProgressCallback>,
    ) -> Result<u64, StorageError> {
        let total = tokio::fs::metadata(source_path)
            .await
            .map_err(|e| StorageError::UploadFailed {
                reason: format!("stat source: {}", e),
            })?
            .len();

        let file = tokio::fs::File::open(source_path)
            .await
            .map_err(|e| StorageError::UploadFailed {
                reason: format!("open source: {}", e),
            })?;
        let mut reader = ProgressReader::new(file, total, on_progress);

        self.bucket
            .put_object_stream(&mut reader, key)
            .await
            .map_err(|e| StorageError::UploadFailed {
                reason: format!("S3 upload failed: {}", e),
            })?;

        Ok(reader.bytes_read())
    }

    async fn download_to_path(
        &self,
        key: &str,
        output_path: &Path,
        on_progress: Option<ProgressCallback>,
    ) -> Result<u64, StorageError> {
        // Try HEAD for total size so we can show a real %-progress.
        // If HEAD fails (e.g. some S3-compatible backends don't allow it for
        // the credentials), fall back to total = 0 — the callback then reports
        // bytes_done with `total = bytes_done` (monotone, always 100%).
        let total = match self.bucket.head_object(key).await {
            Ok((head, _)) => head.content_length.and_then(|l| u64::try_from(l).ok()).unwrap_or(0),
            Err(_) => 0,
        };

        let file = tokio::fs::File::create(output_path)
            .await
            .map_err(|e| StorageError::DownloadFailed {
                reason: format!("create dest: {}", e),
            })?;
        let mut writer = ProgressWriter::new(file, total, on_progress);

        self.bucket
            .get_object_to_writer(key, &mut writer)
            .await
            .map_err(|e| StorageError::DownloadFailed {
                reason: format!("S3 download failed: {}", e),
            })?;

        // Make sure data hits disk before returning.
        use tokio::io::AsyncWriteExt;
        writer
            .shutdown()
            .await
            .map_err(|e| StorageError::DownloadFailed {
                reason: format!("flush dest: {}", e),
            })?;

        Ok(writer.bytes_written())
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
