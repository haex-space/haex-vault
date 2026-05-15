// src-tauri/src/storage/backend.rs
//!
//! Storage Backend Trait and S3 Implementation
//!

use std::path::Path;

use super::error::StorageError;
use super::progress::{ProgressCallback, ProgressReader, ProgressWriter};
use super::types::{S3Config, StorageListDirResponse, StorageObjectInfo};
use async_trait::async_trait;
use s3::bucket::Bucket;
use s3::bucket_ops::{BucketConfiguration, CannedBucketAcl};
use s3::creds::Credentials;
use s3::region::Region;

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

/// Result of building an S3 `Bucket` from `S3Config`.
///
/// `effective_bucket` is the bucket name that actually targets the same
/// object as `bucket`. When the configured endpoint includes a path prefix,
/// the prefix is folded into the bucket name (`"prefix/bucket"`) so existence
/// probes and `Bucket::create` operate on the identical name.
pub(crate) struct S3BucketSetup {
    pub bucket: Box<Bucket>,
    pub effective_bucket: String,
}

/// Rebuild an endpoint URL keeping scheme + authority but dropping any path.
///
/// Preserves the explicit port, and brackets IPv6 hosts the way URLs require
/// (`[::1]`). Returns `None` if the input lacks a host (which would make
/// the endpoint unusable for S3 anyway).
fn endpoint_authority(url: &url::Url) -> Option<String> {
    let host = url.host_str()?;
    let bracketed = if host.contains(':') && !host.starts_with('[') {
        format!("[{}]", host)
    } else {
        host.to_string()
    };
    let mut base = format!("{}://{}", url.scheme(), bracketed);
    if let Some(port) = url.port() {
        base.push(':');
        base.push_str(&port.to_string());
    }
    Some(base)
}

/// Construct an S3 `Bucket` from `S3Config`.
///
/// Shared between `S3Backend` (general CRUD) and the streaming layer (range
/// reads via `haex-stream://`). Keep both in sync by funneling all bucket
/// construction through this helper.
pub(crate) fn build_s3_bucket(config: &S3Config) -> Result<S3BucketSetup, StorageError> {
    let (clean_endpoint, effective_bucket) = if let Some(endpoint) = &config.endpoint {
        if let Ok(url) = url::Url::parse(endpoint) {
            let path = url.path();
            if path != "/" && !path.is_empty() {
                let base = endpoint_authority(&url).unwrap_or_else(|| endpoint.clone());
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

    let mut bucket =
        Bucket::new(&effective_bucket, region, credentials).map_err(|e| {
            StorageError::ConnectionFailed {
                reason: format!("Failed to create bucket: {}", e),
            }
        })?;

    if config.path_style.unwrap_or(false) {
        bucket = bucket.with_path_style();
    }

    Ok(S3BucketSetup {
        bucket,
        effective_bucket,
    })
}

/// S3-compatible storage backend
pub struct S3Backend {
    bucket: Box<Bucket>,
    /// Original config kept for re-creating the bucket on demand (auto-create).
    config: S3Config,
    /// Bucket name that actually targets the same object as `self.bucket`.
    /// When the configured endpoint includes a path prefix, the prefix is
    /// folded into the bucket name (`"prefix/bucket"`) so existence probes
    /// and `Bucket::create` operate on the identical name — using the raw
    /// `config.bucket` here would create a different bucket than the probe
    /// just listed.
    effective_bucket: String,
}

impl S3Backend {
    /// Create a new S3 backend from config
    pub async fn new(config: &S3Config) -> Result<Self, StorageError> {
        let setup = build_s3_bucket(config)?;
        Ok(Self {
            bucket: setup.bucket,
            config: config.clone(),
            effective_bucket: setup.effective_bucket,
        })
    }

    /// Build a fresh `Credentials` value from the stored config.
    fn build_credentials(&self) -> Result<Credentials, StorageError> {
        Credentials::new(
            Some(&self.config.access_key_id),
            Some(&self.config.secret_access_key),
            None,
            None,
            None,
        )
        .map_err(|e| StorageError::ConnectionFailed {
            reason: format!("Failed to create credentials: {}", e),
        })
    }

    /// Build the `Region` value matching the stored config.
    fn build_region(&self) -> Region {
        if let Some(endpoint) = &self.config.endpoint {
            // Strip any path prefix from the endpoint, same as in `new`
            let base = url::Url::parse(endpoint)
                .ok()
                .and_then(|url| {
                    let path = url.path();
                    if path == "/" || path.is_empty() {
                        Some(endpoint.clone())
                    } else {
                        endpoint_authority(&url)
                    }
                })
                .unwrap_or_else(|| endpoint.clone());
            Region::Custom {
                region: self.config.region.clone(),
                endpoint: base,
            }
        } else {
            self.config.region.parse().unwrap_or(Region::UsEast1)
        }
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

    async fn ensure_container(&self) -> Result<(), StorageError> {
        // Cheap existence probe — if the bucket lists, we're done. Any error
        // is inspected to distinguish "missing" from other failures (auth,
        // network, etc.) so we only attempt creation when we're sure the
        // bucket is absent.
        match self
            .bucket
            .list("".to_string(), Some("/".to_string()))
            .await
        {
            Ok(_) => return Ok(()),
            Err(e) => {
                let msg = e.to_string();
                let lower = msg.to_lowercase();
                let missing = lower.contains("nosuchbucket")
                    || (lower.contains("404") && lower.contains("bucket"));
                if !missing {
                    return Err(StorageError::ConnectionFailed {
                        reason: format!("Bucket check failed: {}", e),
                    });
                }
            }
        }

        let credentials = self.build_credentials()?;
        let region = self.build_region();

        // The LocationConstraint payload is the trickiest part of bucket
        // creation across S3 implementations:
        //
        // - AWS us-east-1: must NOT include the payload (the API default).
        // - AWS other regions: must include it with the matching region name.
        // - S3-compatible services (MinIO, Rabata, R2, B2, …): mostly reject
        //   AWS region names entirely — each has its own naming.
        //
        // rust-s3 0.37 makes this awkward: `Bucket::create` unconditionally
        // calls `config.set_region(region.clone())` which overwrites our
        // `location_constraint=None`. Because we use `Region::Custom { region,
        // endpoint }` for custom endpoints, the resulting payload serializes
        // as `<LocationConstraint>{Custom.region}</LocationConstraint>` —
        // which the target service then rejects.
        //
        // The crate provides an explicit env-var escape hatch
        // (`RUST_S3_SKIP_LOCATION_CONSTRAINT`) which skips the `set_region`
        // override. Use it for any custom-endpoint backend so the payload
        // stays empty. Process-global side effect: once set in this process
        // all subsequent `Bucket::create` calls skip the payload — fine while
        // this app only targets S3-compatible services, but would need
        // scoped handling if AWS direct support is added later.
        let bucket_config = if self.config.endpoint.is_some() {
            std::env::set_var("RUST_S3_SKIP_LOCATION_CONSTRAINT", "true");
            BucketConfiguration::private()
        } else if self.config.region.eq_ignore_ascii_case("us-east-1") {
            BucketConfiguration::private()
        } else {
            BucketConfiguration::new(
                Some(CannedBucketAcl::Private),
                false,
                None,
                None,
                None,
                None,
                None,
                Some(region.clone()),
            )
        };

        let response = Bucket::create(
            &self.effective_bucket,
            region,
            credentials,
            bucket_config,
        )
        .await
        .map_err(|e| StorageError::ConnectionFailed {
            reason: format!("Bucket auto-create failed: {}", e),
        })?;

        // S3 returns 200/conflict for "already owned by you" — both fine.
        if !response.success() {
            let code = response.response_code;
            // 409 = BucketAlreadyOwnedByYou / BucketAlreadyExists → tolerate.
            if code != 409 {
                return Err(StorageError::ConnectionFailed {
                    reason: format!(
                        "Bucket auto-create returned HTTP {}: {}",
                        code, response.response_text
                    ),
                });
            }
        }

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

    async fn list_dir(&self, prefix: Option<&str>) -> Result<StorageListDirResponse, StorageError> {
        let prefix_str = prefix.unwrap_or("").to_string();

        let results = self
            .bucket
            .list(prefix_str.clone(), Some("/".to_string()))
            .await
            .map_err(|e| StorageError::Internal {
                reason: format!("S3 list failed: {}", e),
            })?;

        let mut folders: Vec<String> = Vec::new();
        let mut objects: Vec<StorageObjectInfo> = Vec::new();

        for result in results {
            for cp in result.common_prefixes.into_iter().flatten() {
                folders.push(cp.prefix);
            }
            for obj in result.contents {
                // S3 returns the prefix itself as a zero-size object when a
                // "directory marker" exists — skip it so it doesn't show up
                // as a duplicate empty file next to the folder entry.
                if obj.key == prefix_str {
                    continue;
                }
                objects.push(StorageObjectInfo {
                    key: obj.key,
                    size: obj.size,
                    last_modified: Some(obj.last_modified),
                });
            }
        }

        Ok(StorageListDirResponse { folders, objects })
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

    async fn download_to_path_resumable(
        &self,
        key: &str,
        output_path: &Path,
        on_progress: Option<ProgressCallback>,
    ) -> Result<u64, StorageError> {
        // Resolve the full object size up front so we can (a) tell the
        // caller they're already complete and skip the network entirely,
        // and (b) feed a meaningful "total" into the progress callback
        // even after a resume.
        let total = match self.bucket.head_object(key).await {
            Ok((head, _)) => head
                .content_length
                .and_then(|l| u64::try_from(l).ok())
                .unwrap_or(0),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("404") || msg.contains("NoSuchKey") {
                    return Err(StorageError::ObjectNotFound { key: key.to_string() });
                }
                return Err(StorageError::DownloadFailed {
                    reason: format!("head_object: {}", e),
                });
            }
        };

        // Existing file = resume offset. Missing file is fine (we'll create
        // it below in append mode); a length 0 file is identical to no
        // file from our perspective.
        let existing_len = tokio::fs::metadata(output_path)
            .await
            .ok()
            .map(|m| m.len())
            .unwrap_or(0);

        // Empty remote object: skip the range GET entirely. A `bytes=0-`
        // request fails on S3 for 0-byte objects; just produce/truncate the
        // local file to length 0 and report success.
        if total == 0 {
            tokio::fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(output_path)
                .await
                .map_err(|e| StorageError::DownloadFailed {
                    reason: format!("open dest: {}", e),
                })?;
            if let Some(cb) = on_progress {
                cb(0, 0);
            }
            return Ok(0);
        }

        // Local file larger than the remote object: truncate down so the
        // resulting file actually matches the remote length. Treating it
        // as "already complete" would leave stale trailing bytes on disk.
        let start_offset = if existing_len > total {
            let f = tokio::fs::OpenOptions::new()
                .write(true)
                .open(output_path)
                .await
                .map_err(|e| StorageError::DownloadFailed {
                    reason: format!("open dest for truncate: {}", e),
                })?;
            f.set_len(total)
                .await
                .map_err(|e| StorageError::DownloadFailed {
                    reason: format!("truncate dest: {}", e),
                })?;
            total
        } else {
            existing_len
        };

        if start_offset >= total {
            if let Some(cb) = on_progress {
                cb(start_offset, start_offset);
            }
            return Ok(start_offset);
        }

        // Wrap the user-supplied callback so progress samples report the
        // *combined* (already-on-disk + freshly-downloaded) byte count.
        // Without this the UI would jump from "50%" back to "0%" each time
        // a resume runs.
        let cb_for_writer: Option<ProgressCallback> = on_progress.map(|cb| {
            let cb = cb.clone();
            std::sync::Arc::new(move |fresh_done: u64, _fresh_total: u64| {
                let absolute = start_offset + fresh_done;
                let absolute_total = total.max(absolute);
                cb(absolute, absolute_total);
            }) as ProgressCallback
        });

        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(output_path)
            .await
            .map_err(|e| StorageError::DownloadFailed {
                reason: format!("open dest: {}", e),
            })?;

        let remaining = total - start_offset;
        let mut writer = ProgressWriter::new(file, remaining, cb_for_writer);

        // Range-GET from the resume offset to end-of-object. rust-s3 streams
        // the body chunk-by-chunk into the writer, so memory stays flat
        // regardless of object size.
        self.bucket
            .get_object_range_to_writer(key, start_offset, None, &mut writer)
            .await
            .map_err(|e| StorageError::DownloadFailed {
                reason: format!("S3 range get failed: {}", e),
            })?;

        use tokio::io::AsyncWriteExt;
        writer
            .shutdown()
            .await
            .map_err(|e| StorageError::DownloadFailed {
                reason: format!("flush dest: {}", e),
            })?;

        Ok(start_offset + writer.bytes_written())
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
