//! SyncProvider trait — abstraction for any file storage backend

use async_trait::async_trait;

use super::types::FileState;

/// Error type for sync provider operations
#[derive(Debug, thiserror::Error)]
pub enum SyncProviderError {
    #[error("Path not found: {path}")]
    NotFound { path: String },

    #[error("Access denied: {reason}")]
    AccessDenied { reason: String },

    #[error("Path traversal rejected: {path}")]
    PathTraversal { path: String },

    #[error("Connection failed: {reason}")]
    ConnectionFailed { reason: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Provider error: {reason}")]
    Other { reason: String },
}

/// A backend that can list, read, write, and delete files for sync purposes.
#[async_trait]
pub trait SyncProvider: Send + Sync {
    /// Human-readable name for logging
    fn display_name(&self) -> String;

    /// Get a recursive manifest of all files under the sync root.
    async fn manifest(&self) -> Result<Vec<FileState>, SyncProviderError>;

    /// Read a file's content by relative path.
    async fn read_file(&self, relative_path: &str) -> Result<Vec<u8>, SyncProviderError>;

    /// Write a file by relative path. Creates parent directories as needed.
    async fn write_file(&self, relative_path: &str, data: &[u8]) -> Result<(), SyncProviderError>;

    /// Delete a file by relative path.
    async fn delete_file(
        &self,
        relative_path: &str,
        to_trash: bool,
    ) -> Result<(), SyncProviderError>;

    /// Create a directory by relative path (including parents).
    async fn create_directory(&self, relative_path: &str) -> Result<(), SyncProviderError>;

    /// Whether this provider supports moving files to trash.
    fn supports_trash(&self) -> bool {
        false
    }
}

/// Validate a relative path against path traversal attacks.
/// Call this at the start of every `SyncProvider` method that takes a path.
pub fn validate_relative_path(path: &str) -> Result<(), SyncProviderError> {
    if path.contains("..") {
        return Err(SyncProviderError::PathTraversal {
            path: path.to_string(),
        });
    }
    if std::path::Path::new(path).is_absolute() {
        return Err(SyncProviderError::PathTraversal {
            path: path.to_string(),
        });
    }
    Ok(())
}
