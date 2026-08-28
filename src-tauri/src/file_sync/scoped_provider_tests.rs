//! Tests for the F3b `ScopedProvider` decorator.
//!
//! Coverage strategy: build a `StubProvider` test-double that implements
//! `SyncProvider` with a call log and a seedable manifest, wrap it in a
//! `ScopedProvider`, then assert on
//!
//! - which inner methods are (and are not) invoked, and
//! - the shape of the error the outer surface returns.
//!
//! The stub itself performs no I/O, so the path-escape assertions can be
//! definite about "inner must not be invoked" — anything the stub records
//! provably came from the decorator's call.

use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::file_sync::hashing::ChunkedHash;
use crate::file_sync::provider::{SyncProvider, SyncProviderError};
use crate::file_sync::scoped_provider::ScopedProvider;
use crate::file_sync::types::FileState;

/// Minimal in-process `SyncProvider` double. Every method records the
/// `(method_name, arg)` it was called with; the manifest and read-file
/// responses are seedable so a test can drive scenarios like "inner
/// returned a cross-scope entry".
#[derive(Default)]
struct StubProvider {
    manifest_entries: Mutex<Vec<FileState>>,
    calls: Mutex<Vec<(String, String)>>,
}

impl StubProvider {
    fn new() -> Self {
        Self::default()
    }

    fn with_manifest(entries: Vec<FileState>) -> Self {
        Self {
            manifest_entries: Mutex::new(entries),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<(String, String)> {
        self.calls.lock().unwrap().clone()
    }

    fn record(&self, method: &str, arg: &str) {
        self.calls
            .lock()
            .unwrap()
            .push((method.to_string(), arg.to_string()));
    }
}

#[async_trait]
impl SyncProvider for StubProvider {
    fn display_name(&self) -> String {
        "stub".into()
    }

    async fn manifest(&self) -> Result<Vec<FileState>, SyncProviderError> {
        self.record("manifest", "");
        Ok(self.manifest_entries.lock().unwrap().clone())
    }

    async fn read_file(&self, relative_path: &str) -> Result<Vec<u8>, SyncProviderError> {
        self.record("read_file", relative_path);
        Ok(Vec::new())
    }

    async fn write_file(&self, relative_path: &str, _data: &[u8]) -> Result<(), SyncProviderError> {
        self.record("write_file", relative_path);
        Ok(())
    }

    async fn delete_file(
        &self,
        relative_path: &str,
        _to_trash: bool,
    ) -> Result<(), SyncProviderError> {
        self.record("delete_file", relative_path);
        Ok(())
    }

    async fn create_directory(&self, relative_path: &str) -> Result<(), SyncProviderError> {
        self.record("create_directory", relative_path);
        Ok(())
    }
}

/// Helper to construct a `FileState` with only `relative_path` set — the
/// scoped guard only inspects that field.
fn file_state(rel: &str) -> FileState {
    FileState {
        relative_path: rel.to_string(),
        size: 0,
        modified_at: 0,
        is_directory: false,
        hash: None,
        chunk_size: None,
        chunk_hashes: None,
    }
}

#[tokio::test]
async fn manifest_delegates_to_inner_and_returns_its_entries() {
    let inner = Arc::new(StubProvider::with_manifest(vec![
        file_state("file-a.txt"),
        file_state("sub/dir/file-b.txt"),
    ]));
    let scoped = ScopedProvider::new(inner.clone(), "space-A/");
    let entries = scoped.manifest().await.expect("manifest");
    assert_eq!(entries.len(), 2);
    let calls = inner.calls();
    assert_eq!(
        calls.iter().filter(|(m, _)| m == "manifest").count(),
        1,
        "expected exactly one delegated manifest() call"
    );
}

#[tokio::test]
async fn read_file_delegates_when_path_is_inside_prefix() {
    let inner = Arc::new(StubProvider::new());
    let scoped = ScopedProvider::new(inner.clone(), "space-A/");
    let _ = scoped
        .read_file("inside/foo.txt")
        .await
        .expect("read must succeed");
    let calls = inner.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "read_file");
    assert_eq!(calls[0].1, "inside/foo.txt");
}

#[tokio::test]
async fn read_file_rejects_dotdot_escape() {
    let inner = Arc::new(StubProvider::new());
    let scoped = ScopedProvider::new(inner.clone(), "space-A/");
    let err = scoped
        .read_file("../space-B/foo")
        .await
        .expect_err("must reject");
    assert!(
        matches!(err, SyncProviderError::PathTraversal { .. }),
        "expected PathTraversal, got {err:?}"
    );
    assert!(inner.calls().is_empty(), "inner must not be invoked");
}

#[tokio::test]
async fn read_file_rejects_absolute_unix_path() {
    let inner = Arc::new(StubProvider::new());
    let scoped = ScopedProvider::new(inner.clone(), "space-A/");
    let err = scoped
        .read_file("/etc/passwd")
        .await
        .expect_err("must reject");
    assert!(matches!(err, SyncProviderError::PathTraversal { .. }));
    assert!(inner.calls().is_empty());
}

#[tokio::test]
async fn read_file_rejects_absolute_windows_path() {
    let inner = Arc::new(StubProvider::new());
    let scoped = ScopedProvider::new(inner.clone(), "space-A/");
    let err = scoped
        .read_file("\\Windows\\System32")
        .await
        .expect_err("must reject");
    assert!(matches!(err, SyncProviderError::PathTraversal { .. }));
    assert!(inner.calls().is_empty());
}

#[tokio::test]
async fn read_file_rejects_backslash_segment_dotdot() {
    let inner = Arc::new(StubProvider::new());
    let scoped = ScopedProvider::new(inner.clone(), "space-A/");
    let err = scoped
        .read_file("foo\\..\\bar")
        .await
        .expect_err("must reject");
    assert!(matches!(err, SyncProviderError::PathTraversal { .. }));
    assert!(inner.calls().is_empty());
}

#[tokio::test]
async fn write_file_rejects_dotdot_escape() {
    let inner = Arc::new(StubProvider::new());
    let scoped = ScopedProvider::new(inner.clone(), "space-A/");
    let err = scoped
        .write_file("../leak.txt", b"x")
        .await
        .expect_err("must reject");
    assert!(matches!(err, SyncProviderError::PathTraversal { .. }));
    assert!(inner.calls().is_empty());
}

#[tokio::test]
async fn delete_file_rejects_dotdot_escape() {
    let inner = Arc::new(StubProvider::new());
    let scoped = ScopedProvider::new(inner.clone(), "space-A/");
    let err = scoped
        .delete_file("../leak.txt", false)
        .await
        .expect_err("must reject");
    assert!(matches!(err, SyncProviderError::PathTraversal { .. }));
    assert!(inner.calls().is_empty());
}

#[tokio::test]
async fn create_directory_rejects_dotdot_escape() {
    let inner = Arc::new(StubProvider::new());
    let scoped = ScopedProvider::new(inner.clone(), "space-A/");
    let err = scoped
        .create_directory("../evil")
        .await
        .expect_err("must reject");
    assert!(matches!(err, SyncProviderError::PathTraversal { .. }));
    assert!(inner.calls().is_empty());
}

#[tokio::test]
async fn manifest_filters_out_inner_entries_outside_prefix() {
    let inner = Arc::new(StubProvider::with_manifest(vec![
        file_state("inside/a.txt"),
        file_state("../other-scope/leaked.txt"),
        file_state("/etc/passwd"),
    ]));
    let scoped = ScopedProvider::new(inner, "space-A/");
    let entries = scoped.manifest().await.expect("manifest");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].relative_path, "inside/a.txt");
}

#[test]
fn display_name_wraps_inner() {
    let inner = Arc::new(StubProvider::new());
    let scoped = ScopedProvider::new(inner, "space-A/");
    assert_eq!(scoped.display_name(), "scoped(stub)");
}

#[tokio::test]
async fn read_file_rejects_segment_dotdot_inside_path() {
    // `foo/../../bar` normalises to `../bar`, so a naive check that only
    // looks at the leading segment (or only splits on `/`) would let this
    // through. Pin the segment-scan behaviour.
    let inner = Arc::new(StubProvider::new());
    let scoped = ScopedProvider::new(inner.clone(), "space-A/");
    let err = scoped
        .read_file("foo/../../bar")
        .await
        .expect_err("must reject");
    assert!(matches!(err, SyncProviderError::PathTraversal { .. }));
    assert!(inner.calls().is_empty());
}

#[tokio::test]
async fn safe_dotdot_inside_filename_is_accepted() {
    // A literal `..` inside a filename (`data..backup`) is NOT a segment
    // and must not trip the guard. Symmetric to `provider::validate_relative_path`'s
    // `dotdot_in_filename_accepted` test.
    let inner = Arc::new(StubProvider::new());
    let scoped = ScopedProvider::new(inner.clone(), "space-A/");
    let _ = scoped
        .read_file("data..backup")
        .await
        .expect("filename with .. is legal");
    assert_eq!(inner.calls().len(), 1);
}

#[tokio::test]
async fn empty_prefix_still_rejects_path_escape_and_accepts_safe_path() {
    // Owner-only path may pass an empty prefix — the guard still works
    // (path-escape check is prefix-independent). Replaces an earlier
    // tautology that only asserted `display_name()` — a regression that
    // short-circuited `check()` on empty-prefix would have passed that.
    let inner = Arc::new(StubProvider::new());
    let scoped = ScopedProvider::new(inner.clone(), "");
    // Path escape still rejected even with empty prefix.
    let err = scoped
        .read_file("../evil")
        .await
        .expect_err("empty-prefix must still reject escapes");
    assert!(matches!(err, SyncProviderError::PathTraversal { .. }));
    assert!(inner.calls().is_empty());
    // Safe path still forwarded.
    let _ = scoped.read_file("foo.txt").await;
    let calls = inner.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "read_file");
    assert_eq!(calls[0].1, "foo.txt");
}

#[tokio::test]
async fn read_file_rejects_null_byte_in_path() {
    // C-style APIs down the stack null-truncate on `\0`, so `foo\0bar`
    // could otherwise collapse to `foo`. Guard must reject before delegate.
    // The path is deliberately free of `..` segments so this test isolates
    // the null-byte reject — a removal of the `\0` guard MUST fail this,
    // even though the segment-split scan still catches `foo\0../bar`.
    let inner = Arc::new(StubProvider::new());
    let scoped = ScopedProvider::new(inner.clone(), "space-A/");
    let err = scoped.read_file("foo\0bar").await.expect_err("must reject");
    assert!(matches!(err, SyncProviderError::PathTraversal { .. }));
    assert!(inner.calls().is_empty(), "inner must not be invoked");
}

#[test]
fn new_normalizes_prefix_by_appending_slash() {
    // Pins the `if raw.is_empty() || raw.ends_with('/') { raw } else { format!("{raw}/") }`
    // branch — a flipped condition would land silently otherwise. Accesses
    // the private `prefix` field via the child-module visibility path.
    let inner = Arc::new(StubProvider::new());

    let empty = ScopedProvider::new(inner.clone(), "");
    assert_eq!(empty.prefix, "", "empty prefix must stay empty");

    let with_slash = ScopedProvider::new(inner.clone(), "space-A/");
    assert_eq!(
        with_slash.prefix, "space-A/",
        "trailing-slash prefix must stay unchanged"
    );

    let no_slash = ScopedProvider::new(inner.clone(), "space-A");
    assert_eq!(
        no_slash.prefix, "space-A/",
        "prefix without trailing slash must have one appended"
    );
}

#[tokio::test]
async fn read_file_with_progress_rejects_dotdot_escape() {
    let inner = Arc::new(StubProvider::new());
    let scoped = ScopedProvider::new(inner.clone(), "space-A/");
    let noop: Arc<dyn Fn(u64, u64) + Send + Sync> = Arc::new(|_, _| {});
    let err = scoped
        .read_file_with_progress("../leak.bin", noop)
        .await
        .expect_err("must reject");
    assert!(matches!(err, SyncProviderError::PathTraversal { .. }));
    assert!(inner.calls().is_empty());
}

#[tokio::test]
async fn read_file_to_path_rejects_dotdot_escape() {
    let inner = Arc::new(StubProvider::new());
    let scoped = ScopedProvider::new(inner.clone(), "space-A/");
    let target = tempfile::NamedTempFile::new().unwrap();
    let noop: Arc<dyn Fn(u64, u64) + Send + Sync> = Arc::new(|_, _| {});
    let expected: Option<ChunkedHash> = None;
    let err = scoped
        .read_file_to_path("../leak.bin", target.path(), expected, noop)
        .await
        .expect_err("must reject");
    assert!(matches!(err, SyncProviderError::PathTraversal { .. }));
    assert!(inner.calls().is_empty());
}

#[tokio::test]
async fn write_file_from_path_rejects_dotdot_escape() {
    let inner = Arc::new(StubProvider::new());
    let scoped = ScopedProvider::new(inner.clone(), "space-A/");
    // Source path never gets read — the guard rejects before delegate.
    let src = Path::new("/nonexistent/source.bin");
    let err = scoped
        .write_file_from_path("../leak.bin", src)
        .await
        .expect_err("must reject");
    assert!(matches!(err, SyncProviderError::PathTraversal { .. }));
    assert!(inner.calls().is_empty());
}
