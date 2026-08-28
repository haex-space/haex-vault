//! Round F3b — outermost `SyncProvider` decorator that guards the file-sync
//! stack against a caller (or a buggy inner provider) touching keys outside
//! a configured scope prefix.
//!
//! ## Placement in the F3b provider stack
//!
//! ```text
//!     ScopedProvider(SpaceContentSyncProvider(CloudProvider))
//!         ^                     ^                    ^
//!      LIST guard          encryption            transport
//!  (this module)     (crypto::space_provider)   (cloud_provider)
//! ```
//!
//! The guard sits OUTERMOST on purpose: whatever the encryption decorator
//! hands back to the caller as "the list of relative paths" is what the
//! guard must scope. Reversing the order would guard the pre-encryption,
//! user-facing paths — which is meaningless for a cross-scope leak.
//!
//! ## What is guarded
//!
//! - Per-op key checks (`read_file` / `write_file` / `delete_file` /
//!   `create_directory` / `read_file_to_path` / `write_file_from_path`):
//!   reject if the relative path is absolute (`/…` or `\…`) or contains
//!   a `..` segment. Rejection happens BEFORE the inner call, so a caller
//!   trying to escape the scope never gets to touch the transport.
//! - `manifest()`: forwards to inner then FILTERS OUT any entry whose
//!   relative-path would escape the scope. This is a belt-and-suspenders
//!   check against a buggy or malicious inner. It does NOT surface as
//!   an error — the stripped entries are silently dropped with a
//!   `tracing::warn!` for debuggability. Rationale: a user-visible error
//!   would let a bad inner brick the whole sync loop; strip + warn keeps
//!   the good entries flowing and the incident observable.
//!
//! ## What is NOT done here
//!
//! No S3 SDK calls. ScopedProvider is a pure decorator. In F3b the
//! scoped-credential enforcement (per-space GET-no-LIST) lives inside
//! the underlying `CloudProvider`, which is built with a
//! `StorageBackend` constructed from the `ScopedCred` for the space.
//! ScopedProvider does not prepend `prefix` to keys either — the inner
//! `SpaceContentSyncProvider` (or `CloudProvider` on its own for
//! owner-only) already owns the key-shape decisions.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;

use super::hashing::ChunkedHash;
use super::provider::{ReadFileResult, SyncProvider, SyncProviderError};
use super::types::FileState;

/// Internal error type for the scoped-provider guard. The public trait
/// surfaces this as [`SyncProviderError::PathTraversal`] — `escape` and
/// `traversal` are the same phenomenon (a relative path resolving above
/// or outside its intended root), so we do not invent a second variant
/// on the trait error.
#[derive(Debug, thiserror::Error)]
pub enum ScopedProviderError {
    #[error("relative path {relative_path:?} escapes prefix {prefix:?}")]
    PathEscape {
        relative_path: String,
        prefix: String,
    },
}

impl From<ScopedProviderError> for SyncProviderError {
    fn from(e: ScopedProviderError) -> Self {
        match e {
            ScopedProviderError::PathEscape { relative_path, .. } => {
                SyncProviderError::PathTraversal {
                    path: relative_path,
                }
            }
        }
    }
}

/// Outermost decorator in the F3b provider stack. See module docs for the
/// stacking order and rationale.
pub struct ScopedProvider {
    inner: Arc<dyn SyncProvider>,
    /// Retained for error messages and future use. ScopedProvider does NOT
    /// prepend `prefix` to keys — the inner provider owns key shape.
    /// Normalized in [`Self::new`] to either an empty string or a value
    /// ending in `/`, mirroring the shape `CloudProvider` uses for its
    /// own prefix field.
    prefix: String,
}

impl ScopedProvider {
    pub fn new(inner: Arc<dyn SyncProvider>, prefix: impl Into<String>) -> Self {
        let raw = prefix.into();
        let prefix = if raw.is_empty() || raw.ends_with('/') {
            raw
        } else {
            format!("{raw}/")
        };
        Self { inner, prefix }
    }

    fn check(&self, relative_path: &str) -> Result<(), SyncProviderError> {
        if is_path_escape(relative_path) {
            return Err(ScopedProviderError::PathEscape {
                relative_path: relative_path.to_string(),
                prefix: self.prefix.clone(),
            }
            .into());
        }
        Ok(())
    }
}

/// True if `p` is an absolute path (starts with `/` or `\`) or contains
/// `..` as a path segment (split on either `/` or `\` so a Windows-style
/// escape like `foo\..\bar` is caught the same as `foo/../bar`).
///
/// This is deliberately more restrictive than a check that only looks
/// at the leading segment: `foo/../../bar` normalises to `../bar`, so a
/// pure prefix check on the raw string would let it through.
fn is_path_escape(p: &str) -> bool {
    if p.starts_with('/') || p.starts_with('\\') {
        return true;
    }
    for seg in p.split(['/', '\\']) {
        if seg == ".." {
            return true;
        }
    }
    false
}

#[async_trait]
impl SyncProvider for ScopedProvider {
    fn display_name(&self) -> String {
        format!("scoped({})", self.inner.display_name())
    }

    async fn manifest(&self) -> Result<Vec<FileState>, SyncProviderError> {
        let mut entries = self.inner.manifest().await?;
        let before = entries.len();
        entries.retain(|e| !is_path_escape(&e.relative_path));
        let filtered = before - entries.len();
        if filtered > 0 {
            tracing::warn!(
                filtered,
                prefix = %self.prefix,
                inner = %self.inner.display_name(),
                "inner provider returned cross-scope entries; stripping"
            );
        }
        Ok(entries)
    }

    async fn read_file(&self, relative_path: &str) -> Result<Vec<u8>, SyncProviderError> {
        self.check(relative_path)?;
        self.inner.read_file(relative_path).await
    }

    async fn read_file_with_progress(
        &self,
        relative_path: &str,
        on_progress: Arc<dyn Fn(u64, u64) + Send + Sync>,
    ) -> Result<Vec<u8>, SyncProviderError> {
        self.check(relative_path)?;
        self.inner
            .read_file_with_progress(relative_path, on_progress)
            .await
    }

    async fn read_file_to_path(
        &self,
        relative_path: &str,
        output_path: &Path,
        expected_chunks: Option<ChunkedHash>,
        on_progress: Arc<dyn Fn(u64, u64) + Send + Sync>,
    ) -> Result<ReadFileResult, SyncProviderError> {
        self.check(relative_path)?;
        self.inner
            .read_file_to_path(relative_path, output_path, expected_chunks, on_progress)
            .await
    }

    async fn write_file(&self, relative_path: &str, data: &[u8]) -> Result<(), SyncProviderError> {
        self.check(relative_path)?;
        self.inner.write_file(relative_path, data).await
    }

    async fn write_file_from_path(
        &self,
        relative_path: &str,
        source_path: &Path,
    ) -> Result<(), SyncProviderError> {
        self.check(relative_path)?;
        self.inner
            .write_file_from_path(relative_path, source_path)
            .await
    }

    async fn delete_file(
        &self,
        relative_path: &str,
        to_trash: bool,
    ) -> Result<(), SyncProviderError> {
        self.check(relative_path)?;
        self.inner.delete_file(relative_path, to_trash).await
    }

    async fn create_directory(&self, relative_path: &str) -> Result<(), SyncProviderError> {
        self.check(relative_path)?;
        self.inner.create_directory(relative_path).await
    }

    fn supports_streaming(&self) -> bool {
        self.inner.supports_streaming()
    }

    fn supports_trash(&self) -> bool {
        self.inner.supports_trash()
    }

    fn supports_directories(&self) -> bool {
        self.inner.supports_directories()
    }

    async fn prime_hash_after_write(&self, file: &FileState) {
        self.inner.prime_hash_after_write(file).await
    }

    fn local_target_path(&self, relative_path: &str) -> Option<PathBuf> {
        self.inner.local_target_path(relative_path)
    }
}

#[cfg(test)]
#[path = "scoped_provider_tests.rs"]
mod scoped_provider_tests;
