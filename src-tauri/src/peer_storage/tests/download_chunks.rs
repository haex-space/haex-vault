use std::collections::{HashMap, HashSet};

use crate::peer_storage::endpoint::PeerEndpoint;

use super::helpers::*;

/// File-browser flow: caller passes `expected_chunks = None`. The
/// stat-probe's chunks must be adopted silently and the download must
/// complete.
#[tokio::test]
async fn download_file_to_path_uses_stat_probe_chunks_when_none_supplied() {
    let h = setup_multipart_harness().await;
    let path = format!("/{}/ramp.bin", h.share_name);
    let tmp_out = tempfile::tempdir().unwrap();
    let out_path = tmp_out.path().join("out_none.bin");

    let result = crate::peer_storage::client::download_file_to_path(
        h.client.clone(),
        h.server_remote_id,
        None,
        path,
        out_path.clone(),
        None,
        None,
        None,
        None,
        h.ucan.clone(),
    )
    .await
    .expect("file-browser flow (no manifest) must succeed");

    assert_eq!(result.bytes, 1024 * 1024);
    assert!(result.hash.is_some());
}

/// Sync-rule flow: caller supplies the manifest's `ChunkedHash`. With
/// the file unchanged on disk, the manifest equals the stat-probe's
/// chunks and the download proceeds.
#[tokio::test]
async fn download_file_to_path_uses_caller_supplied_chunks_when_provided() {
    let h = setup_multipart_harness().await;
    let path = format!("/{}/ramp.bin", h.share_name);
    let tmp_out = tempfile::tempdir().unwrap();
    let out_path = tmp_out.path().join("out_some.bin");

    // Pre-compute the chunked hash off the same bytes the harness wrote.
    let ramp: Vec<u8> = (0..1024u32 * 1024u32).map(|i| (i % 256) as u8).collect();
    let manifest_chunks = crate::file_sync::hashing::ChunkedHash {
        file_hash: blake3::hash(&ramp).to_hex().to_string(),
        chunk_size: crate::file_sync::hashing::CHUNK_HASH_SIZE,
        chunk_hashes: vec![blake3::hash(&ramp).to_hex().to_string()],
    };

    let result = crate::peer_storage::client::download_file_to_path(
        h.client.clone(),
        h.server_remote_id,
        None,
        path,
        out_path.clone(),
        Some(manifest_chunks.clone()),
        None,
        None,
        None,
        h.ucan.clone(),
    )
    .await
    .expect("sync-rule flow with matching manifest must succeed");

    assert_eq!(result.bytes, 1024 * 1024);
}

/// Zero-byte file end-to-end. The streaming hasher emits
/// `chunk_hashes: vec![]` for empty input, so manifests for empty files
/// have an empty `Vec<String>`. `FileState::chunked_hash()` must accept
/// that case (CR1) and the verifier must walk through the empty hash
/// slice without error — together they ensure zero-byte sync downloads
/// don't silently bypass manifest pinning.
#[tokio::test]
async fn download_file_to_path_handles_zero_byte_file() {
    let tmp = tempfile::tempdir().unwrap();
    let file_path = tmp.path().join("empty.bin");
    tokio::fs::write(&file_path, b"").await.unwrap();

    let share_name = "media".to_string();
    let space_id = "test-space".to_string();

    let mut server = PeerEndpoint::new_ephemeral();
    server.set_random_test_identity();
    let server_id = server.start_for_test().await.expect("server bind");
    server
        .add_share(
            "share-1".to_string(),
            share_name.clone(),
            tmp.path().to_string_lossy().to_string(),
            space_id.clone(),
        )
        .await;

    let mut client_inner = PeerEndpoint::new_ephemeral();
    let client_did = client_inner.set_random_test_identity();
    client_inner.start_for_test().await.expect("client bind");
    let client_id = client_inner.endpoint_id();

    let mut allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert(space_id.clone());
    allowed.insert(client_id.to_string(), spaces);
    server.set_allowed_peers(allowed).await;

    let mut owner_dids = HashMap::new();
    owner_dids.insert(client_id.to_string(), client_did.clone());
    server.set_peer_owner_dids(owner_dids).await;

    let server_addr = server.endpoint_ref().unwrap().addr();
    client_inner
        .connect_for_test(server_addr)
        .await
        .expect("client → server connect");

    let seed: [u8; 32] = rand::random();
    let ucan_signer = ed25519_dalek::SigningKey::from_bytes(&seed);
    let ucan = read_ucan(&ucan_signer, &space_id, &client_did);
    let client = std::sync::Arc::new(tokio::sync::RwLock::new(client_inner));

    let path = format!("/{share_name}/empty.bin");
    let tmp_out = tempfile::tempdir().unwrap();
    let out_path = tmp_out.path().join("out_empty.bin");

    // Manifest entry produced by the scanner for a zero-byte file:
    // `chunk_hashes` is an empty Vec because the streaming hasher's
    // tail-flush block never fires when nothing was read.
    let manifest = crate::file_sync::hashing::ChunkedHash {
        file_hash: blake3::hash(&[]).to_hex().to_string(),
        chunk_size: crate::file_sync::hashing::CHUNK_HASH_SIZE,
        chunk_hashes: vec![],
    };

    let result = crate::peer_storage::client::download_file_to_path(
        client,
        server_id,
        None,
        path,
        out_path.clone(),
        Some(manifest),
        None,
        None,
        None,
        ucan,
    )
    .await
    .expect("zero-byte download must succeed end-to-end");

    assert_eq!(result.bytes, 0);
    let on_disk = tokio::fs::read(&out_path).await.unwrap();
    assert!(on_disk.is_empty(), "zero-byte file must land empty on disk");

    // Sidecar + partial bytes must be cleaned up — the verifier writes
    // the sidecar after every successful chunk, but with zero chunks
    // the post-download clear() is still expected to run.
    let partial_path = crate::peer_storage::resume::PartialState::partial_path(&out_path);
    let meta_path = {
        let mut p = out_path.as_os_str().to_owned();
        p.push(".haex-partial.meta");
        std::path::PathBuf::from(p)
    };
    assert!(
        !meta_path.exists(),
        "sidecar metadata should be cleared after success: {meta_path:?}"
    );
    assert!(
        !partial_path.exists(),
        "partial bytes file should be renamed away after success: {partial_path:?}"
    );
}

/// Multi-chunk single-stream download. 3 MiB file spans 3 chunks but
/// stays below `MULTI_STREAM_THRESHOLD` (16 MiB), so it goes through
/// `receive_with_chunk_verification`. Verifies that:
/// - bytes on disk match the source exactly
/// - the returned hash is the manifest BLAKE3 file_hash (verified path
///   never falls back to SHA-256)
/// - the resume sidecar + partial bytes are cleared after success
#[tokio::test]
async fn download_file_to_path_verified_multi_chunk_clears_sidecar() {
    // Build a custom harness with a 3 MiB ramp file (3 chunks of 1 MiB).
    let tmp = tempfile::tempdir().unwrap();
    let chunk_size = crate::file_sync::hashing::CHUNK_HASH_SIZE as usize;
    let payload_len = chunk_size * 3;
    let file_path = tmp.path().join("big.bin");
    let mut ramp = vec![0u8; payload_len];
    for (i, b) in ramp.iter_mut().enumerate() {
        *b = ((i * 31 + 7) % 256) as u8;
    }
    tokio::fs::write(&file_path, &ramp).await.unwrap();

    let share_name = "media".to_string();
    let space_id = "test-space".to_string();

    let mut server = PeerEndpoint::new_ephemeral();
    server.set_random_test_identity();
    let server_id = server.start_for_test().await.expect("server bind");
    server
        .add_share(
            "share-1".to_string(),
            share_name.clone(),
            tmp.path().to_string_lossy().to_string(),
            space_id.clone(),
        )
        .await;

    let mut client_inner = PeerEndpoint::new_ephemeral();
    let client_did = client_inner.set_random_test_identity();
    client_inner.start_for_test().await.expect("client bind");
    let client_id = client_inner.endpoint_id();

    let mut allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert(space_id.clone());
    allowed.insert(client_id.to_string(), spaces);
    server.set_allowed_peers(allowed).await;

    let mut owner_dids = HashMap::new();
    owner_dids.insert(client_id.to_string(), client_did.clone());
    server.set_peer_owner_dids(owner_dids).await;

    let server_addr = server.endpoint_ref().unwrap().addr();
    client_inner
        .connect_for_test(server_addr)
        .await
        .expect("client → server connect");

    let seed: [u8; 32] = rand::random();
    let ucan_signer = ed25519_dalek::SigningKey::from_bytes(&seed);
    let ucan = read_ucan(&ucan_signer, &space_id, &client_did);
    let client = std::sync::Arc::new(tokio::sync::RwLock::new(client_inner));

    let path = format!("/{share_name}/big.bin");
    let tmp_out = tempfile::tempdir().unwrap();
    let out_path = tmp_out.path().join("out_big.bin");

    // Build the manifest ChunkedHash off the same bytes the server holds.
    let expected_file_hash = blake3::hash(&ramp).to_hex().to_string();
    let chunk_hashes: Vec<String> = (0..3)
        .map(|i| {
            let s = i * chunk_size;
            let e = (i + 1) * chunk_size;
            blake3::hash(&ramp[s..e]).to_hex().to_string()
        })
        .collect();
    let manifest = crate::file_sync::hashing::ChunkedHash {
        file_hash: expected_file_hash.clone(),
        chunk_size: crate::file_sync::hashing::CHUNK_HASH_SIZE,
        chunk_hashes,
    };

    let result = crate::peer_storage::client::download_file_to_path(
        client,
        server_id,
        None,
        path,
        out_path.clone(),
        Some(manifest),
        None,
        None,
        None,
        ucan,
    )
    .await
    .expect("verified multi-chunk download must succeed");

    assert_eq!(result.bytes, payload_len as u64);
    assert_eq!(
        result.hash.as_deref(),
        Some(expected_file_hash.as_str()),
        "verified path returns the manifest BLAKE3 file_hash"
    );

    let on_disk = tokio::fs::read(&out_path).await.unwrap();
    assert_eq!(on_disk, ramp, "bytes on disk match the source verbatim");

    // After a clean download the sidecar metadata + partial bytes file
    // must be gone — the engine should never see leftover resume state
    // for a completed download.
    let meta_path = {
        let mut p = out_path.as_os_str().to_owned();
        p.push(".haex-partial.meta");
        std::path::PathBuf::from(p)
    };
    let partial_path = crate::peer_storage::resume::PartialState::partial_path(&out_path);
    assert!(
        !meta_path.exists(),
        "sidecar metadata should be cleared after success: {meta_path:?}"
    );
    assert!(
        !partial_path.exists(),
        "partial bytes file should be renamed away after success: {partial_path:?}"
    );
}

/// Sync-rule flow with corrupted manifest: caller supplies a
/// ChunkedHash whose file_hash disagrees with what the stat-probe
/// reports. The download must abort with `ManifestHashMismatch` and
/// never touch the receive path — a signal that the file has changed on
/// the sender since the manifest was scanned.
#[tokio::test]
async fn download_file_to_path_rejects_when_manifest_and_stat_disagree() {
    let h = setup_multipart_harness().await;
    let path = format!("/{}/ramp.bin", h.share_name);
    let tmp_out = tempfile::tempdir().unwrap();
    let out_path = tmp_out.path().join("out_mismatch.bin");

    let bogus = "deadbeef".repeat(8);
    let bad_manifest = crate::file_sync::hashing::ChunkedHash {
        file_hash: bogus.clone(),
        chunk_size: crate::file_sync::hashing::CHUNK_HASH_SIZE,
        chunk_hashes: vec![bogus],
    };

    let err = crate::peer_storage::client::download_file_to_path(
        h.client.clone(),
        h.server_remote_id,
        None,
        path,
        out_path.clone(),
        Some(bad_manifest),
        None,
        None,
        None,
        h.ucan.clone(),
    )
    .await
    .expect_err("mismatched manifest must abort the download");

    assert!(
        matches!(
            err,
            crate::peer_storage::error::PeerStorageError::ManifestHashMismatch { .. }
        ),
        "expected ManifestHashMismatch, got {err:?}"
    );
}
