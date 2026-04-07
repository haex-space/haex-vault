//! CloudProvider — wraps a StorageBackend (S3/cloud) as a SyncProvider

use async_trait::async_trait;

use crate::remote_storage::backend::StorageBackend;
use crate::remote_storage::error::StorageError;

use super::provider::{validate_relative_path, SyncProvider, SyncProviderError};
use super::types::FileState;

/// Parse an ISO 8601 / RFC 3339 timestamp to a Unix timestamp in seconds.
/// Returns 0 if parsing fails.
fn parse_iso8601_to_unix(s: &str) -> u64 {
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
        .map(|dt| dt.unix_timestamp() as u64)
        .unwrap_or(0)
}

impl From<StorageError> for SyncProviderError {
    fn from(error: StorageError) -> Self {
        match error {
            StorageError::ObjectNotFound { key } => SyncProviderError::NotFound { path: key },
            StorageError::ConnectionFailed { reason } => {
                SyncProviderError::ConnectionFailed { reason }
            }
            other => SyncProviderError::Other {
                reason: other.to_string(),
            },
        }
    }
}

pub struct CloudProvider {
    backend: Box<dyn StorageBackend>,
    /// Prefix within the bucket (e.g. "photos/")
    prefix: String,
}

impl CloudProvider {
    pub fn new(backend: Box<dyn StorageBackend>, prefix: String) -> Self {
        let prefix = if prefix.is_empty() || prefix.ends_with('/') {
            prefix
        } else {
            format!("{}/", prefix)
        };
        Self { backend, prefix }
    }

    fn full_key(&self, relative_path: &str) -> String {
        format!("{}{}", self.prefix, relative_path)
    }
}

#[async_trait]
impl SyncProvider for CloudProvider {
    fn display_name(&self) -> String {
        format!("cloud:{}/{}", self.backend.backend_type(), self.prefix)
    }

    async fn manifest(&self) -> Result<Vec<FileState>, SyncProviderError> {
        let objects = self.backend.list(Some(&self.prefix)).await?;

        let files = objects
            .into_iter()
            .filter_map(|obj| {
                let relative_path = obj.key.strip_prefix(&self.prefix)?.to_string();
                if relative_path.is_empty() {
                    return None;
                }

                let modified_at = obj
                    .last_modified
                    .as_deref()
                    .map(parse_iso8601_to_unix)
                    .unwrap_or(0);

                let is_directory = relative_path.ends_with('/');

                Some(FileState {
                    relative_path,
                    size: obj.size,
                    modified_at,
                    is_directory,
                })
            })
            .collect();

        Ok(files)
    }

    async fn read_file(&self, relative_path: &str) -> Result<Vec<u8>, SyncProviderError> {
        validate_relative_path(relative_path)?;
        let key = self.full_key(relative_path);
        Ok(self.backend.download(&key).await?)
    }

    async fn write_file(&self, relative_path: &str, data: &[u8]) -> Result<(), SyncProviderError> {
        validate_relative_path(relative_path)?;
        let key = self.full_key(relative_path);
        Ok(self.backend.upload(&key, data).await?)
    }

    async fn delete_file(
        &self,
        relative_path: &str,
        _to_trash: bool,
    ) -> Result<(), SyncProviderError> {
        validate_relative_path(relative_path)?;
        let key = self.full_key(relative_path);
        Ok(self.backend.delete(&key).await?)
    }

    async fn create_directory(&self, _relative_path: &str) -> Result<(), SyncProviderError> {
        // S3 doesn't need explicit directory creation — directories are implicit from keys
        Ok(())
    }

    fn supports_trash(&self) -> bool {
        false
    }
}
