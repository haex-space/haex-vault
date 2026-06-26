//! Tests for the `PeerProvider` SyncProvider adapter.
//!
//! ## What this exercises
//!
//! `PeerProvider` is a thin shim over an iroh `PeerEndpoint`: every method opens a
//! QUIC stream and exchanges typed `Request` / `Response` frames. Spinning up a
//! real iroh endpoint inside a unit test is expensive and flaky (relay
//! discovery, OS-level UDP sockets, async accept loops), and there is no trait
//! abstraction over `PeerEndpoint` we could swap with a mock. So the unit
//! coverage here is intentionally narrow: it locks down the behaviour that
//! runs **before** the first network I/O — display-name shape, capability
//! flags, and the path-traversal guard ordering.
//!
//! Real end-to-end traffic is covered by `tests/peer_storage_e2e.rs` and the
//! haex-e2e-tests companion repo.
//!
//! ## Why the path-traversal tests don't hang
//!
//! Every public path-taking method calls `validate_relative_path(...)` as its
//! first statement; on `Err` it short-circuits before opening a stream. We
//! exploit that ordering: build a `PeerProvider` over a never-started
//! `PeerEndpoint`, hand it an obviously-invalid path, and assert we get back
//! `SyncProviderError::PathTraversal { .. }`. If the order were ever reversed,
//! we'd block on `open_stream` against the dead endpoint instead and the test
//! would time out — which is itself the regression signal.

use std::sync::Arc;

use iroh::SecretKey;

use crate::file_sync::peer_provider::PeerProvider;
use crate::file_sync::provider::{SyncProvider, SyncProviderError};
use crate::peer_storage::endpoint::PeerEndpoint;

/// Build a PeerProvider against a fresh (un-started) PeerEndpoint. The
/// endpoint has no iroh socket bound, so any actual stream open would fail —
/// but the path-traversal guard runs before any I/O is attempted, which is
/// what these tests pin.
fn make_provider(remote_base_path: &str) -> PeerProvider {
    // `rand::random()` per repo convention — no literal seeds in test keys.
    let secret_bytes: [u8; 32] = rand::random();
    let local_secret = SecretKey::from_bytes(&secret_bytes);
    let remote_secret_bytes: [u8; 32] = rand::random();
    let remote_secret = SecretKey::from_bytes(&remote_secret_bytes);

    let endpoint = Arc::new(tokio::sync::RwLock::new(PeerEndpoint::new(local_secret)));
    let remote_id = remote_secret.public();

    PeerProvider::new(
        endpoint,
        remote_id,
        None,
        remote_base_path.to_string(),
        "ucan-test-token".to_string(),
    )
}

#[test]
fn display_name_uses_peer_prefix_and_remote_id() {
    let provider = make_provider("/remote/base");
    let name = provider.display_name();
    assert!(
        name.starts_with("peer:"),
        "display_name must be tagged with the `peer:` prefix so log lines can \
         be filtered by provider kind; got {name:?}"
    );
    // The remote id formatted into the name is what makes each peer instance
    // distinguishable in operator logs.
    assert!(
        name.len() > "peer:".len(),
        "display_name must include the remote endpoint id, got {name:?}"
    );
}

#[test]
fn supports_streaming_is_true() {
    // PeerProvider streams large reads (multipart download) and writes
    // (`pipe_reader_to_send`); the engine uses this flag to pick the
    // streaming path. Pinning it here so a refactor can't silently disable
    // streaming and fall back to bulk-load (OOM on large files).
    let provider = make_provider("/remote/base");
    assert!(provider.supports_streaming());
}

#[test]
fn supports_trash_is_false() {
    // The peer protocol's Delete is a hard remove — there is no trash bucket
    // on the remote side. Engine queries this flag to decide whether to use
    // a soft-delete path; pinning it false so we don't silently lose data.
    let provider = make_provider("/remote/base");
    assert!(!provider.supports_trash());
}

#[tokio::test]
async fn read_file_rejects_path_traversal_before_network_io() {
    // `../etc/passwd` must trip `validate_relative_path` before `open_stream`
    // is reached. If the guard ever moved below the stream open, this test
    // would block on connect against the dead endpoint instead of returning
    // a PathTraversal error.
    let provider = make_provider("/remote/base");
    let err = provider
        .read_file("../etc/passwd")
        .await
        .expect_err("traversal path must be rejected");
    assert!(
        matches!(err, SyncProviderError::PathTraversal { .. }),
        "expected PathTraversal, got {err:?}",
    );
}

#[tokio::test]
async fn write_file_rejects_path_traversal_before_network_io() {
    let provider = make_provider("/remote/base");
    let err = provider
        .write_file("../../etc/shadow", b"payload")
        .await
        .expect_err("traversal path must be rejected");
    assert!(
        matches!(err, SyncProviderError::PathTraversal { .. }),
        "expected PathTraversal, got {err:?}",
    );
}

#[tokio::test]
async fn delete_file_rejects_path_traversal_before_network_io() {
    let provider = make_provider("/remote/base");
    let err = provider
        .delete_file("../escape", false)
        .await
        .expect_err("traversal path must be rejected");
    assert!(
        matches!(err, SyncProviderError::PathTraversal { .. }),
        "expected PathTraversal, got {err:?}",
    );
}

#[tokio::test]
async fn create_directory_rejects_path_traversal_before_network_io() {
    let provider = make_provider("/remote/base");
    let err = provider
        .create_directory("../newdir")
        .await
        .expect_err("traversal path must be rejected");
    assert!(
        matches!(err, SyncProviderError::PathTraversal { .. }),
        "expected PathTraversal, got {err:?}",
    );
}

#[tokio::test]
async fn write_file_from_path_rejects_path_traversal_before_filesystem_io() {
    // `write_file_from_path` would `tokio::fs::metadata(source_path)` after
    // path validation — but validation comes first, so we never touch the
    // filesystem when the *relative* (remote) path is bad. Pin that ordering.
    let provider = make_provider("/remote/base");
    let err = provider
        .write_file_from_path("../escape", std::path::Path::new("/tmp/does-not-exist"))
        .await
        .expect_err("traversal path must be rejected");
    assert!(
        matches!(err, SyncProviderError::PathTraversal { .. }),
        "expected PathTraversal, got {err:?}",
    );
}
