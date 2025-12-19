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
        eprintln!("[S3Backend::new] Creating backend for bucket: {}", config.bucket);
        eprintln!("[S3Backend::new] Endpoint: {:?}", config.endpoint);
        eprintln!("[S3Backend::new] Region: {}", config.region);
        eprintln!("[S3Backend::new] Backend type: {:?}", backend_type);

        // Extract path prefix from endpoint URL if present
        // This handles S3-compatible services that use path-based endpoints (e.g., Supabase /storage/v1/s3)
        // For path-style URLs, we prepend the path prefix to the bucket name so rust-s3 constructs
        // the URL correctly: https://host/{prefix}/{bucket}/{key}
        let (clean_endpoint, effective_bucket) = if let Some(endpoint) = &config.endpoint {
            if let Ok(url) = url::Url::parse(endpoint) {
                let path = url.path();
                if path != "/" && !path.is_empty() {
                    // Extract the path and reconstruct the base URL
                    let base = format!("{}://{}", url.scheme(), url.host_str().unwrap_or(""));
                    let prefix = path.trim_matches('/');
                    // Combine path prefix with bucket name: "storage/v1/s3" + "/" + "my-bucket"
                    let combined_bucket = format!("{}/{}", prefix, config.bucket);
                    eprintln!("[S3Backend::new] Extracted path prefix: {} from endpoint", prefix);
                    eprintln!("[S3Backend::new] Clean endpoint: {}", base);
                    eprintln!("[S3Backend::new] Effective bucket name: {}", combined_bucket);
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
            None, // security token
            None, // session token
            None, // profile
        )
        .map_err(|e| FileSyncError::BackendConnectionFailed {
            reason: format!("Failed to create credentials: {}", e),
        })?;

        // Determine region - for custom endpoints, use a custom region
        let region = if let Some(endpoint) = &clean_endpoint {
            eprintln!("[S3Backend::new] Using custom endpoint: {}", endpoint);
            Region::Custom {
                region: config.region.clone(),
                endpoint: endpoint.clone(),
            }
        } else if config.endpoint.is_some() {
            // Fallback if clean_endpoint extraction failed
            let ep = config.endpoint.as_ref().unwrap();
            eprintln!("[S3Backend::new] Using original endpoint: {}", ep);
            Region::Custom {
                region: config.region.clone(),
                endpoint: ep.clone(),
            }
        } else {
            eprintln!("[S3Backend::new] Using standard AWS region: {}", config.region);
            config.region.parse().unwrap_or(Region::UsEast1)
        };

        let mut bucket = Bucket::new(&effective_bucket, region, credentials)
            .map_err(|e| FileSyncError::BackendConnectionFailed {
                reason: format!("Failed to create bucket: {}", e),
            })?;

        // Determine if path-style should be used:
        // 1. MinIO always uses path-style
        // 2. Supabase S3-compatible storage uses path-style
        // 3. Other services with /s3/ or /storage/ in the path need path-style
        // 4. Explicitly configured via use_path_style option
        let endpoint_needs_path_style = config.endpoint.as_ref().map(|e| {
            let needs = e.contains("supabase.co")
                || e.contains("supabase.in")
                || e.contains("/s3/")
                || e.contains("/storage/");
            eprintln!("[S3Backend::new] Endpoint '{}' needs path-style: {}", e, needs);
            needs
        }).unwrap_or(false);

        let use_path_style = backend_type == StorageBackendType::Minio
            || config.use_path_style.unwrap_or(false)
            || endpoint_needs_path_style;

        eprintln!("[S3Backend::new] use_path_style decision: {} (minio={}, config={:?}, endpoint={})",
            use_path_style,
            backend_type == StorageBackendType::Minio,
            config.use_path_style,
            endpoint_needs_path_style
        );

        if use_path_style {
            eprintln!("[S3Backend::new] Enabling path-style URLs");
            bucket = bucket.with_path_style();
        } else {
            eprintln!("[S3Backend::new] Using virtual-hosted-style URLs");
        }

        let backend_type_str = match backend_type {
            StorageBackendType::S3 => "s3",
            StorageBackendType::R2 => "r2",
            StorageBackendType::Minio => "minio",
            _ => "s3",
        };

        eprintln!("[S3Backend::new] Backend created successfully, path_style={}, bucket={}",
            bucket.is_path_style(), bucket.name());

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
        eprintln!("[S3Backend::test_connection] Testing connection to bucket: {}", self.bucket.name());

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
        eprintln!("[S3Backend::upload] Starting upload");
        eprintln!("[S3Backend::upload] Remote ID (key): {}", remote_id);
        eprintln!("[S3Backend::upload] Data size: {} bytes", data.len());
        eprintln!("[S3Backend::upload] Bucket: {}", self.bucket.name());
        eprintln!("[S3Backend::upload] Path style: {}", self.bucket.is_path_style());
        eprintln!("[S3Backend::upload] Host: {}", self.bucket.host());
        eprintln!("[S3Backend::upload] Region: {:?}", self.bucket.region());

        // TODO: Implement multipart upload with progress for large files
        let result = self.bucket
            .put_object(remote_id, data)
            .await;

        match &result {
            Ok(response) => {
                eprintln!("[S3Backend::upload] SUCCESS - Status: {}", response.status_code());
            }
            Err(e) => {
                eprintln!("[S3Backend::upload] FAILED - Error: {}", e);
                eprintln!("[S3Backend::upload] Error details: {:?}", e);
            }
        }

        result.map_err(|e| FileSyncError::UploadFailed {
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
