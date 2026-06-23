use std::collections::{HashMap, HashSet};

use crate::peer_storage::endpoint::PeerEndpoint;

use super::helpers::*;

/// The generic retry pool re-queues a failed range once and the
/// fetcher's second call for that range succeeds. Other ranges complete
/// on their first attempt — they're not retried, not cancelled, just
/// finished. Verifies the retry-only-the-failed-range contract without
/// needing a real flaky peer.
#[tokio::test]
async fn multi_stream_retry_pool_retries_only_failed_range() {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    // Four equal 8 MiB ranges; 0..8M succeeds, 8..16M fails once then
    // succeeds, 16..24M and 24..32M succeed. Track call counts per range
    // so we can assert the failed-range call count = 2 and siblings = 1.
    let ranges: Vec<(u64, u64, u32)> = vec![
        (24 * 1024 * 1024, 32 * 1024 * 1024, 0),
        (16 * 1024 * 1024, 24 * 1024 * 1024, 0),
        (8 * 1024 * 1024, 16 * 1024 * 1024, 0),
        (0, 8 * 1024 * 1024, 0),
    ];
    let pending = Arc::new(tokio::sync::Mutex::new(ranges.clone()));

    // Per-range call counts.
    let calls: Arc<std::sync::Mutex<std::collections::HashMap<(u64, u64), u32>>> =
        Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
    // Tracks the failed-range's "next call should fail" state.
    let flaky_attempts_remaining = Arc::new(AtomicU32::new(1));

    let calls_for_fetcher = calls.clone();
    let fetcher = Arc::new(move |start: u64, end: u64| {
        let calls = calls_for_fetcher.clone();
        let flaky = flaky_attempts_remaining.clone();
        Box::pin(async move {
            *calls.lock().unwrap().entry((start, end)).or_insert(0) += 1;
            let is_flaky_range = start == 8 * 1024 * 1024;
            if is_flaky_range && flaky.fetch_sub(1, Ordering::SeqCst) > 0 {
                Err(
                    crate::peer_storage::error::PeerStorageError::ConnectionFailed {
                        reason: "deterministic mid-stream abort".to_string(),
                    },
                )
            } else {
                Ok(())
            }
        })
            as std::pin::Pin<
                Box<
                    dyn std::future::Future<
                            Output = Result<(), crate::peer_storage::error::PeerStorageError>,
                        > + Send,
                >,
            >
    });

    // No on_retry hook needed for the pool-only test — the pool itself
    // handles re-queueing; the hook is only for caller-side bookkeeping.
    let first_err = crate::peer_storage::client::run_bounded_retry_pool(
        pending,
        4,
        crate::peer_storage::streaming::MAX_RANGE_RETRIES,
        fetcher,
        None,
    )
    .await;

    assert!(
        first_err.is_none(),
        "retry pool must succeed when failed range succeeds on retry: {first_err:?}"
    );

    let final_calls = calls.lock().unwrap();
    assert_eq!(
        final_calls.get(&(0, 8 * 1024 * 1024)).copied(),
        Some(1),
        "range [0, 8M) is never retried"
    );
    assert_eq!(
        final_calls
            .get(&(8 * 1024 * 1024, 16 * 1024 * 1024))
            .copied(),
        Some(2),
        "flaky range [8M, 16M) is called twice (fail + success)"
    );
    assert_eq!(
        final_calls
            .get(&(16 * 1024 * 1024, 24 * 1024 * 1024))
            .copied(),
        Some(1),
        "range [16M, 24M) is never retried"
    );
    assert_eq!(
        final_calls
            .get(&(24 * 1024 * 1024, 32 * 1024 * 1024))
            .copied(),
        Some(1),
        "range [24M, 32M) is never retried"
    );
}

/// Same shape as the previous test but the flaky range fails *every*
/// attempt. The pool must surface an Err after `MAX_RANGE_RETRIES + 1`
/// total calls to the failing range (initial + retries), while sibling
/// ranges still complete normally on their first call — proving sibling
/// workers keep draining the queue even after one range has bottomed out.
#[tokio::test]
async fn multi_stream_retry_pool_exhausts_and_returns_err() {
    use std::sync::Arc;

    let ranges: Vec<(u64, u64, u32)> = vec![
        (24 * 1024 * 1024, 32 * 1024 * 1024, 0),
        (16 * 1024 * 1024, 24 * 1024 * 1024, 0),
        (8 * 1024 * 1024, 16 * 1024 * 1024, 0),
        (0, 8 * 1024 * 1024, 0),
    ];
    let pending = Arc::new(tokio::sync::Mutex::new(ranges));

    let calls: Arc<std::sync::Mutex<std::collections::HashMap<(u64, u64), u32>>> =
        Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));

    let calls_for_fetcher = calls.clone();
    let fetcher = Arc::new(move |start: u64, end: u64| {
        let calls = calls_for_fetcher.clone();
        Box::pin(async move {
            *calls.lock().unwrap().entry((start, end)).or_insert(0) += 1;
            let is_doomed_range = start == 8 * 1024 * 1024;
            if is_doomed_range {
                Err(
                    crate::peer_storage::error::PeerStorageError::ConnectionFailed {
                        reason: "permanent fault".to_string(),
                    },
                )
            } else {
                Ok(())
            }
        })
            as std::pin::Pin<
                Box<
                    dyn std::future::Future<
                            Output = Result<(), crate::peer_storage::error::PeerStorageError>,
                        > + Send,
                >,
            >
    });

    let first_err = crate::peer_storage::client::run_bounded_retry_pool(
        pending,
        4,
        crate::peer_storage::streaming::MAX_RANGE_RETRIES,
        fetcher,
        None,
    )
    .await;

    assert!(
        first_err.is_some(),
        "retry pool must surface Err after exhausting retries on a permanently-failing range"
    );

    let final_calls = calls.lock().unwrap();
    let doomed_calls = final_calls
        .get(&(8 * 1024 * 1024, 16 * 1024 * 1024))
        .copied()
        .unwrap_or(0);
    let expected_total = 1 + crate::peer_storage::streaming::MAX_RANGE_RETRIES;
    assert_eq!(
        doomed_calls, expected_total,
        "doomed range gets initial attempt + MAX_RANGE_RETRIES retries = {expected_total} calls"
    );
    // Siblings must still have completed exactly once — no abort_all.
    for sibling in [
        (0, 8 * 1024 * 1024),
        (16 * 1024 * 1024, 24 * 1024 * 1024),
        (24 * 1024 * 1024, 32 * 1024 * 1024),
    ] {
        assert_eq!(
            final_calls.get(&sibling).copied(),
            Some(1),
            "sibling range {sibling:?} must complete in one call (no abort_all)"
        );
    }
}

/// Integration: `read_multipart_to_file` against a real harness, with a
/// manifest that intentionally lists a wrong hash for one chunk. That
/// chunk's range fails verification every attempt (the server keeps
/// serving the same correct bytes; only the manifest is wrong), so the
/// pool exhausts retries for that range while siblings complete. The
/// function must:
/// - Return Err (some retry-exhausted failure).
/// - Leave the partial bytes file + sidecar on disk for resume.
/// - Not produce a renamed output file.
#[tokio::test]
async fn multi_stream_chunk_hash_mismatch_exhausts_retries_and_preserves_partial() {
    // 4 MiB file → 4 chunks of 1 MiB → 4 parallel streams of 1 MiB each
    // (one chunk per range, so a wrong manifest hash on chunk N makes
    // exactly range N fail).
    let tmp = tempfile::tempdir().unwrap();
    let chunk_size = crate::file_sync::hashing::CHUNK_HASH_SIZE as usize;
    let payload_len = chunk_size * 4;
    let file_path = tmp.path().join("multi.bin");
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

    // Build a manifest where chunk 2's hash is intentionally wrong. The
    // file_hash itself is the real BLAKE3 of the file so the
    // pre-receive ManifestHashMismatch guard doesn't fire — only the
    // per-chunk verifier inside the worker should catch it.
    let real_file_hash = blake3::hash(&ramp).to_hex().to_string();
    let mut chunk_hashes: Vec<String> = (0..4)
        .map(|i| {
            let s = i * chunk_size;
            let e = (i + 1) * chunk_size;
            blake3::hash(&ramp[s..e]).to_hex().to_string()
        })
        .collect();
    // Sabotage chunk 2: any string of the right length and shape.
    chunk_hashes[2] = "00".repeat(32);
    let manifest = crate::file_sync::hashing::ChunkedHash {
        file_hash: real_file_hash,
        chunk_size: crate::file_sync::hashing::CHUNK_HASH_SIZE,
        chunk_hashes,
    };

    let path = format!("/{share_name}/multi.bin");
    let tmp_out = tempfile::tempdir().unwrap();
    let out_path = tmp_out.path().join("out_multi.bin");

    let err = crate::peer_storage::client::read_multipart_to_file(
        client,
        server_id,
        None,
        path,
        out_path.clone(),
        payload_len as u64,
        4,
        &manifest,
        None,
        None,
        None,
        ucan,
    )
    .await
    .expect_err("intentional chunk-hash sabotage must fail after retry exhaustion");

    // The surfaced error originates from the failing chunk's verifier.
    assert!(
        matches!(
            err,
            crate::peer_storage::error::PeerStorageError::ChunkHashMismatch { .. }
        ),
        "expected ChunkHashMismatch after retry exhaustion, got {err:?}"
    );

    // Resume contract: partial bytes file + sidecar must remain on disk
    // so a future attempt (with a corrected manifest) can pick up where
    // siblings left off.
    let partial_path = crate::peer_storage::resume::PartialState::partial_path(&out_path);
    let meta_path = {
        let mut p = out_path.as_os_str().to_owned();
        p.push(".haex-partial.meta");
        std::path::PathBuf::from(p)
    };
    assert!(
        partial_path.exists(),
        "partial bytes file must survive retry-exhaustion for resume: {partial_path:?}"
    );
    assert!(
        meta_path.exists(),
        "sidecar metadata must survive retry-exhaustion for resume: {meta_path:?}"
    );
    // The final output path must NOT exist — nothing was atomic-renamed
    // because the bitmap wasn't full.
    assert!(
        !out_path.exists(),
        "final output path must not be renamed when verification failed: {out_path:?}"
    );

    // The sidecar bitmap should record some-but-not-all chunks as
    // verified — sibling ranges that did complete should have flipped
    // their bits while range 2 stayed false.
    let state = crate::peer_storage::resume::PartialState::load(&out_path)
        .await
        .expect("sidecar must load")
        .expect("sidecar must exist");
    assert_eq!(state.completed.len(), 4);
    assert!(
        !state.completed[2],
        "sabotaged chunk 2 must not be marked complete"
    );
    let completed_count = state.completed.iter().filter(|c| **c).count();
    assert!(
        completed_count >= 1,
        "at least one sibling chunk must complete before retry-exhaustion: {:?}",
        state.completed
    );
}

/// Clean multi-stream download via the verified path. Confirms (a) all
/// chunks pass verification, (b) the partial file is atomic-renamed to
/// the final path, (c) sidecar + partial file are cleaned up, and
/// (d) the returned hash equals the manifest's file_hash (not a
/// post-download SHA-256 re-read).
#[tokio::test]
async fn multi_stream_verified_clean_download_clears_sidecar() {
    let tmp = tempfile::tempdir().unwrap();
    let chunk_size = crate::file_sync::hashing::CHUNK_HASH_SIZE as usize;
    let payload_len = chunk_size * 4;
    let file_path = tmp.path().join("clean_multi.bin");
    let mut ramp = vec![0u8; payload_len];
    for (i, b) in ramp.iter_mut().enumerate() {
        *b = ((i * 17 + 11) % 256) as u8;
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

    let file_hash = blake3::hash(&ramp).to_hex().to_string();
    let chunk_hashes: Vec<String> = (0..4)
        .map(|i| {
            let s = i * chunk_size;
            let e = (i + 1) * chunk_size;
            blake3::hash(&ramp[s..e]).to_hex().to_string()
        })
        .collect();
    let manifest = crate::file_sync::hashing::ChunkedHash {
        file_hash: file_hash.clone(),
        chunk_size: crate::file_sync::hashing::CHUNK_HASH_SIZE,
        chunk_hashes,
    };

    let path = format!("/{share_name}/clean_multi.bin");
    let tmp_out = tempfile::tempdir().unwrap();
    let out_path = tmp_out.path().join("out_clean_multi.bin");

    let result = crate::peer_storage::client::read_multipart_to_file(
        client,
        server_id,
        None,
        path,
        out_path.clone(),
        payload_len as u64,
        4,
        &manifest,
        None,
        None,
        None,
        ucan,
    )
    .await
    .expect("clean multi-stream download must succeed via verified path");

    assert_eq!(result.bytes, payload_len as u64);
    assert_eq!(
        result.hash.as_deref(),
        Some(file_hash.as_str()),
        "verified path returns the manifest BLAKE3 file_hash, not a post-download SHA-256"
    );

    let on_disk = tokio::fs::read(&out_path).await.unwrap();
    assert_eq!(on_disk, ramp, "downloaded bytes must equal source");

    let partial_path = crate::peer_storage::resume::PartialState::partial_path(&out_path);
    let meta_path = {
        let mut p = out_path.as_os_str().to_owned();
        p.push(".haex-partial.meta");
        std::path::PathBuf::from(p)
    };
    assert!(
        !partial_path.exists(),
        "partial bytes file must be renamed away after success: {partial_path:?}"
    );
    assert!(
        !meta_path.exists(),
        "sidecar metadata must be cleared after success: {meta_path:?}"
    );
}

/// Cross-invocation resume on the multi-stream branch (Task 10).
///
/// Pre-seeds the on-disk state a previous failed multi-stream attempt
/// would have left behind: a partial bytes file holding the file's real
/// content for some chunks (and zeros for the gaps) plus a sidecar bitmap
/// marking those chunks complete. The download is then invoked against
/// the same manifest — `read_multipart_to_file` must pick up the sidecar,
/// only request the missing ranges, and finish with the file
/// bit-perfectly on disk + sidecar cleared.
///
/// The seed uses TWO disjoint gaps (chunks [5, 9) and [20, 25)) so the
/// initial pending pool has more than one entry — proving
/// `missing_ranges()` is what drives the worker queue, not the
/// equal-N-way split.
#[tokio::test]
async fn multi_stream_resumes_across_invocations() {
    // 32 MiB file → 32 chunks of 1 MiB → comfortably above the 16 MiB
    // multi-stream threshold so download_file_to_path would pick this
    // branch in the real flow.
    let tmp = tempfile::tempdir().unwrap();
    let chunk_size = crate::file_sync::hashing::CHUNK_HASH_SIZE as usize;
    let total_chunks = 32usize;
    let payload_len = chunk_size * total_chunks;
    let file_path = tmp.path().join("multi_resume.bin");
    let mut ramp = vec![0u8; payload_len];
    for (i, b) in ramp.iter_mut().enumerate() {
        *b = ((i * 19 + 3) % 256) as u8;
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

    let file_hash = blake3::hash(&ramp).to_hex().to_string();
    let chunk_hashes: Vec<String> = (0..total_chunks)
        .map(|i| {
            let s = i * chunk_size;
            let e = (i + 1) * chunk_size;
            blake3::hash(&ramp[s..e]).to_hex().to_string()
        })
        .collect();
    let manifest = crate::file_sync::hashing::ChunkedHash {
        file_hash: file_hash.clone(),
        chunk_size: crate::file_sync::hashing::CHUNK_HASH_SIZE,
        chunk_hashes,
    };

    // Pre-seed the on-disk state of a half-finished prior attempt:
    // chunks 0..5, 9..20, and 25..32 are complete; gaps at 5..9 and 20..25.
    let tmp_out = tempfile::tempdir().unwrap();
    let out_path = tmp_out.path().join("out_multi_resume.bin");
    let partial_path = crate::peer_storage::resume::PartialState::partial_path(&out_path);
    let mut partial_bytes = vec![0u8; payload_len];
    let mut completed = vec![false; total_chunks];
    for i in 0..total_chunks {
        let in_gap_a = (5..9).contains(&i);
        let in_gap_b = (20..25).contains(&i);
        if !in_gap_a && !in_gap_b {
            let s = i * chunk_size;
            let e = (i + 1) * chunk_size;
            partial_bytes[s..e].copy_from_slice(&ramp[s..e]);
            completed[i] = true;
        }
    }
    tokio::fs::write(&partial_path, &partial_bytes)
        .await
        .unwrap();

    let sidecar = crate::peer_storage::resume::PartialState {
        file_hash: file_hash.clone(),
        chunk_size: crate::file_sync::hashing::CHUNK_HASH_SIZE,
        completed: completed.clone(),
    };
    sidecar.save(&out_path).await.unwrap();

    // Sanity: the seeded sidecar produces the two expected gaps.
    let missing = sidecar.missing_ranges();
    let cs = chunk_size as u64;
    assert_eq!(
        missing,
        vec![(5 * cs, 9 * cs), (20 * cs, 25 * cs)],
        "seeded sidecar must produce exactly two missing ranges"
    );

    let path = format!("/{share_name}/multi_resume.bin");
    let result = crate::peer_storage::client::read_multipart_to_file(
        client,
        server_id,
        None,
        path,
        out_path.clone(),
        payload_len as u64,
        4,
        &manifest,
        None,
        None,
        None,
        ucan,
    )
    .await
    .expect("multi-stream resume must succeed");

    assert_eq!(result.bytes, payload_len as u64);
    assert_eq!(
        result.hash.as_deref(),
        Some(file_hash.as_str()),
        "verified resume returns the manifest BLAKE3 file_hash"
    );

    let on_disk = tokio::fs::read(&out_path).await.unwrap();
    assert_eq!(
        on_disk, ramp,
        "resumed bytes must match the source verbatim"
    );

    let meta_path = {
        let mut p = out_path.as_os_str().to_owned();
        p.push(".haex-partial.meta");
        std::path::PathBuf::from(p)
    };
    assert!(
        !partial_path.exists(),
        "partial bytes file must be renamed away on resume success: {partial_path:?}"
    );
    assert!(
        !meta_path.exists(),
        "sidecar metadata must be cleared on resume success: {meta_path:?}"
    );
}

/// Drift guard for the multi-stream resume probe. When the sidecar's
/// `file_hash` matches the manifest but its `chunk_size` (or chunk count)
/// disagrees, `read_multipart_to_file` must treat the sidecar as garbage
/// and start fresh — otherwise the worker pool would align bytes against
/// the wrong chunk indices and silently corrupt the output. The drift
/// branch also re-truncates the partial file, so seeded junk bytes get
/// overwritten.
#[tokio::test]
async fn multi_stream_resume_ignores_sidecar_with_drifting_chunk_size() {
    let tmp = tempfile::tempdir().unwrap();
    let chunk_size = crate::file_sync::hashing::CHUNK_HASH_SIZE as usize;
    let payload_len = chunk_size * 4;
    let file_path = tmp.path().join("drift_multi.bin");
    let mut ramp = vec![0u8; payload_len];
    for (i, b) in ramp.iter_mut().enumerate() {
        *b = ((i * 23 + 9) % 256) as u8;
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

    let file_hash = blake3::hash(&ramp).to_hex().to_string();
    let chunk_hashes: Vec<String> = (0..4)
        .map(|i| {
            let s = i * chunk_size;
            let e = (i + 1) * chunk_size;
            blake3::hash(&ramp[s..e]).to_hex().to_string()
        })
        .collect();
    let manifest = crate::file_sync::hashing::ChunkedHash {
        file_hash: file_hash.clone(),
        chunk_size: crate::file_sync::hashing::CHUNK_HASH_SIZE,
        chunk_hashes,
    };

    // Sidecar agrees on `file_hash` but lies about `chunk_size` (half
    // the real value). The drift filter must reject it and the partial
    // file must be re-truncated, so the junk bytes we seed cannot
    // survive into the final output.
    let tmp_out = tempfile::tempdir().unwrap();
    let out_path = tmp_out.path().join("out_drift_multi.bin");
    let partial_path = crate::peer_storage::resume::PartialState::partial_path(&out_path);
    tokio::fs::write(&partial_path, vec![0xCCu8; payload_len])
        .await
        .unwrap();

    let stale = crate::peer_storage::resume::PartialState {
        file_hash: file_hash.clone(),
        chunk_size: crate::file_sync::hashing::CHUNK_HASH_SIZE / 2,
        completed: vec![true, true, true, true, true, true, true, true],
    };
    stale.save(&out_path).await.unwrap();

    let path = format!("/{share_name}/drift_multi.bin");
    let result = crate::peer_storage::client::read_multipart_to_file(
        client,
        server_id,
        None,
        path,
        out_path.clone(),
        payload_len as u64,
        4,
        &manifest,
        None,
        None,
        None,
        ucan,
    )
    .await
    .expect("drift sidecar must fall back to fresh download");

    assert_eq!(result.bytes, payload_len as u64);
    let on_disk = tokio::fs::read(&out_path).await.unwrap();
    assert_eq!(
        on_disk, ramp,
        "drift fallback must produce the real file, not the 0xCC junk seed"
    );
}
