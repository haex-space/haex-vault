//! LocalProvider — SyncProvider implementation for the local filesystem
//!
//! Wraps a base directory path and provides sync operations against it.
//! All paths are relative to the base directory.
//! Desktop supports moving files to OS trash via the `trash` crate.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;

use super::provider::{validate_relative_path, SyncProvider, SyncProviderError};
use super::types::FileState;

#[derive(Debug)]
pub struct LocalProvider {
    base_path: PathBuf,
}

impl LocalProvider {
    pub fn new(base_path: PathBuf) -> Result<Self, SyncProviderError> {
        if !base_path.exists() {
            return Err(SyncProviderError::NotFound {
                path: base_path.to_string_lossy().to_string(),
            });
        }
        if !base_path.is_dir() {
            return Err(SyncProviderError::Other {
                reason: format!("Not a directory: {}", base_path.display()),
            });
        }
        Ok(Self { base_path })
    }

    /// Resolve a relative path to an absolute path within the base directory.
    /// Validates against path traversal and verifies the result stays within base_path.
    fn resolve_path(&self, relative_path: &str) -> Result<PathBuf, SyncProviderError> {
        validate_relative_path(relative_path)?;
        let full = self.base_path.join(relative_path);

        // Canonicalize what exists and verify it's within base_path.
        // For non-existent paths (write_file), verify the parent is within base_path.
        let check_path = if full.exists() {
            full.canonicalize().map_err(SyncProviderError::Io)?
        } else if let Some(parent) = full.parent() {
            if parent.exists() {
                let canonical_parent = parent.canonicalize().map_err(SyncProviderError::Io)?;
                canonical_parent.join(full.file_name().unwrap_or_default())
            } else {
                full.clone()
            }
        } else {
            full.clone()
        };

        let canonical_base = self.base_path.canonicalize().map_err(SyncProviderError::Io)?;
        if !check_path.starts_with(&canonical_base) {
            return Err(SyncProviderError::PathTraversal {
                path: relative_path.to_string(),
            });
        }
        Ok(full)
    }

    fn static_supports_trash() -> bool {
        #[cfg(not(target_os = "android"))]
        {
            true
        }
        #[cfg(target_os = "android")]
        {
            false
        }
    }
}

/// Recursively scan a directory and collect FileState entries.
fn scan_directory(dir: &Path, base: &Path) -> Result<Vec<FileState>, SyncProviderError> {
    let mut entries = Vec::new();
    let read_dir = std::fs::read_dir(dir).map_err(SyncProviderError::Io)?;

    for entry in read_dir {
        let entry = entry.map_err(SyncProviderError::Io)?;
        let metadata = entry.metadata().map_err(SyncProviderError::Io)?;

        // Normalize path separators to forward slash
        let relative = entry
            .path()
            .strip_prefix(base)
            .unwrap_or(&entry.path())
            .to_string_lossy()
            .to_string()
            .replace('\\', "/");

        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        entries.push(FileState {
            relative_path: relative,
            size: if metadata.is_dir() { 0 } else { metadata.len() },
            modified_at,
            is_directory: metadata.is_dir(),
        });

        if metadata.is_dir() {
            entries.extend(scan_directory(&entry.path(), base)?);
        }
    }

    Ok(entries)
}

#[async_trait]
impl SyncProvider for LocalProvider {
    fn display_name(&self) -> String {
        format!("local:{}", self.base_path.display())
    }

    async fn manifest(&self) -> Result<Vec<FileState>, SyncProviderError> {
        let base = self.base_path.clone();
        tokio::task::spawn_blocking(move || scan_directory(&base, &base))
            .await
            .map_err(|e| SyncProviderError::Other {
                reason: e.to_string(),
            })?
    }

    async fn read_file(&self, relative_path: &str) -> Result<Vec<u8>, SyncProviderError> {
        let full = self.resolve_path(relative_path)?;
        if !full.exists() {
            return Err(SyncProviderError::NotFound {
                path: relative_path.to_string(),
            });
        }
        std::fs::read(&full).map_err(SyncProviderError::Io)
    }

    async fn write_file(&self, relative_path: &str, data: &[u8]) -> Result<(), SyncProviderError> {
        let full = self.resolve_path(relative_path)?;
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).map_err(SyncProviderError::Io)?;
        }
        std::fs::write(&full, data).map_err(SyncProviderError::Io)
    }

    async fn delete_file(
        &self,
        relative_path: &str,
        to_trash: bool,
    ) -> Result<(), SyncProviderError> {
        let full = self.resolve_path(relative_path)?;
        if !full.exists() {
            return Ok(());
        }
        if to_trash && Self::static_supports_trash() {
            #[cfg(not(target_os = "android"))]
            {
                trash::delete(&full).map_err(|e| SyncProviderError::Other {
                    reason: format!("Trash failed: {e}"),
                })?;
            }
            #[cfg(target_os = "android")]
            {
                std::fs::remove_file(&full).map_err(SyncProviderError::Io)?;
            }
        } else if full.is_dir() {
            std::fs::remove_dir_all(&full).map_err(SyncProviderError::Io)?;
        } else {
            std::fs::remove_file(&full).map_err(SyncProviderError::Io)?;
        }
        Ok(())
    }

    async fn create_directory(&self, relative_path: &str) -> Result<(), SyncProviderError> {
        let full = self.resolve_path(relative_path)?;
        std::fs::create_dir_all(&full).map_err(SyncProviderError::Io)
    }

    async fn read_file_to_path(
        &self,
        relative_path: &str,
        output_path: &std::path::Path,
        on_progress: Arc<dyn Fn(u64, u64) + Send + Sync>,
    ) -> Result<u64, SyncProviderError> {
        let src = self.resolve_path(relative_path)?;
        if !src.exists() {
            return Err(SyncProviderError::NotFound { path: relative_path.to_string() });
        }
        let size = tokio::fs::metadata(&src).await.map_err(SyncProviderError::Io)?.len();
        tokio::fs::copy(&src, output_path).await.map_err(SyncProviderError::Io)?;
        on_progress(size, size);
        Ok(size)
    }

    async fn write_file_from_path(
        &self,
        relative_path: &str,
        source_path: &std::path::Path,
    ) -> Result<(), SyncProviderError> {
        let dst = self.resolve_path(relative_path)?;
        if let Some(parent) = dst.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(SyncProviderError::Io)?;
        }
        tokio::fs::copy(source_path, &dst).await.map_err(SyncProviderError::Io)?;
        Ok(())
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn supports_trash(&self) -> bool {
        Self::static_supports_trash()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_provider() -> (tempfile::TempDir, LocalProvider) {
        let tmp = tempfile::TempDir::new().unwrap();
        let provider = LocalProvider::new(tmp.path().to_path_buf()).unwrap();
        (tmp, provider)
    }

    #[tokio::test]
    async fn manifest_includes_all_files_recursively() {
        let (tmp, provider) = make_provider();

        std::fs::write(tmp.path().join("a.txt"), b"hello").unwrap();
        std::fs::create_dir_all(tmp.path().join("sub")).unwrap();
        std::fs::write(tmp.path().join("sub/b.txt"), b"world").unwrap();

        let manifest = provider.manifest().await.unwrap();

        let paths: Vec<&str> = manifest.iter().map(|f| f.relative_path.as_str()).collect();
        assert!(paths.contains(&"a.txt"));
        assert!(paths.contains(&"sub"));
        assert!(paths.contains(&"sub/b.txt"));

        let dir_entry = manifest.iter().find(|f| f.relative_path == "sub").unwrap();
        assert!(dir_entry.is_directory);
        assert_eq!(dir_entry.size, 0);

        let file_entry = manifest.iter().find(|f| f.relative_path == "a.txt").unwrap();
        assert!(!file_entry.is_directory);
        assert_eq!(file_entry.size, 5);
    }

    #[tokio::test]
    async fn path_traversal_is_rejected() {
        let (_tmp, provider) = make_provider();

        let result = provider.read_file("../etc/passwd").await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SyncProviderError::PathTraversal { .. }
        ));
    }

    #[tokio::test]
    async fn write_creates_parent_directories() {
        let (tmp, provider) = make_provider();

        provider
            .write_file("deep/nested/dir/file.txt", b"content")
            .await
            .unwrap();

        let on_disk = std::fs::read(tmp.path().join("deep/nested/dir/file.txt")).unwrap();
        assert_eq!(on_disk, b"content");
    }

    #[tokio::test]
    async fn read_returns_correct_content() {
        let (tmp, provider) = make_provider();

        let content = b"test content 123";
        std::fs::write(tmp.path().join("read_me.txt"), content).unwrap();

        let read = provider.read_file("read_me.txt").await.unwrap();
        assert_eq!(read, content);
    }

    #[tokio::test]
    async fn read_nonexistent_returns_not_found() {
        let (_tmp, provider) = make_provider();

        let result = provider.read_file("does_not_exist.txt").await;
        assert!(matches!(
            result.unwrap_err(),
            SyncProviderError::NotFound { .. }
        ));
    }

    #[tokio::test]
    async fn delete_removes_file() {
        let (tmp, provider) = make_provider();

        std::fs::write(tmp.path().join("gone.txt"), b"bye").unwrap();
        assert!(tmp.path().join("gone.txt").exists());

        provider.delete_file("gone.txt", false).await.unwrap();
        assert!(!tmp.path().join("gone.txt").exists());
    }

    #[tokio::test]
    async fn delete_nonexistent_is_ok() {
        let (_tmp, provider) = make_provider();
        provider.delete_file("nope.txt", false).await.unwrap();
    }

    #[tokio::test]
    async fn delete_directory() {
        let (tmp, provider) = make_provider();

        std::fs::create_dir_all(tmp.path().join("dir/child")).unwrap();
        std::fs::write(tmp.path().join("dir/child/f.txt"), b"x").unwrap();

        provider.delete_file("dir", false).await.unwrap();
        assert!(!tmp.path().join("dir").exists());
    }

    #[tokio::test]
    async fn create_directory_creates_nested() {
        let (tmp, provider) = make_provider();

        provider.create_directory("a/b/c").await.unwrap();
        assert!(tmp.path().join("a/b/c").is_dir());
    }

    #[test]
    fn new_fails_for_nonexistent_path() {
        let result = LocalProvider::new(PathBuf::from("/tmp/absolutely_does_not_exist_xyz"));
        assert!(matches!(
            result.unwrap_err(),
            SyncProviderError::NotFound { .. }
        ));
    }

    #[test]
    fn new_fails_for_file_path() {
        let tmp = tempfile::TempDir::new().unwrap();
        let file = tmp.path().join("not_a_dir");
        std::fs::write(&file, b"").unwrap();

        let result = LocalProvider::new(file);
        assert!(matches!(
            result.unwrap_err(),
            SyncProviderError::Other { .. }
        ));
    }

    #[test]
    fn display_name_includes_path() {
        let tmp = tempfile::TempDir::new().unwrap();
        let provider = LocalProvider::new(tmp.path().to_path_buf()).unwrap();
        let name = provider.display_name();
        assert!(name.starts_with("local:"));
        assert!(name.contains(&tmp.path().display().to_string()));
    }

    #[test]
    fn supports_trash_on_desktop() {
        assert!(LocalProvider::static_supports_trash());
    }
}
