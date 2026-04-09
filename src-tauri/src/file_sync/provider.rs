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

impl serde::Serialize for SyncProviderError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
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
    crate::filesystem::check_relative_path(path)
        .map_err(|_| SyncProviderError::PathTraversal { path: path.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_traversal_dotdot_rejected() {
        assert!(validate_relative_path("../etc/passwd").is_err());
        assert!(validate_relative_path("foo/../../etc/passwd").is_err());
        assert!(validate_relative_path("..").is_err());
    }

    #[test]
    fn absolute_path_rejected() {
        assert!(validate_relative_path("/etc/passwd").is_err());
        assert!(validate_relative_path("/home/user/file.txt").is_err());
    }

    #[test]
    fn null_byte_rejected() {
        assert!(validate_relative_path("foo\0bar").is_err());
        assert!(validate_relative_path("\0").is_err());
    }

    #[test]
    fn empty_path_rejected() {
        assert!(validate_relative_path("").is_err());
    }

    #[test]
    fn normal_relative_path_accepted() {
        assert!(validate_relative_path("notes.md").is_ok());
        assert!(validate_relative_path("readme.txt").is_ok());
    }

    #[test]
    fn nested_relative_path_accepted() {
        assert!(validate_relative_path("folder/subfolder/file.md").is_ok());
        assert!(validate_relative_path("a/b/c/d.txt").is_ok());
    }

    #[test]
    fn dotdot_in_filename_accepted() {
        assert!(validate_relative_path("data..backup").is_ok());
        assert!(validate_relative_path("foo/data..v2").is_ok());
    }
}
