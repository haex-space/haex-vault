use std::collections::{HashMap, HashSet};

use crate::peer_storage::endpoint::PeerEndpoint;

use super::helpers::*;

/// Pre-seeded resume scenario. A previous attempt left a sidecar +
/// partial-bytes file with chunk 0 already verified; this attempt must
/// download only chunks 1+2, reconcile bytes into the surviving partial
/// file, atomic-rename to the final path, and clear the sidecar.
///
/// Uses the simpler "seed the post-failure state" shape from the plan —
/// it exercises the load-sidecar + missing-ranges + range-Read +
/// reconcile logic without needing a fail-mid-stream harness extension.
#[tokio::test]
async fn single_stream_resumes_after_failure() {
    // 3 MiB ramp file, 3 chunks of 1 MiB. Stays under
    // MULTI_STREAM_THRESHOLD so download_file_to_path picks the
    // single-stream branch which is the one Task 8 patches.
    let tmp = tempfile::tempdir().unwrap();
    let chunk_size = crate::file_sync::hashing::CHUNK_HASH_SIZE as usize;
    let payload_len = chunk_size * 3;
    let file_path = tmp.path().join("resume.bin");
    let mut ramp = vec![0u8; payload_len];
    for (i, b) in ramp.iter_mut().enumerate() {
        *b = ((i * 17 + 5) % 256) as u8;
    }
    tokio::fs::write(&file_path, &ramp).await.unwrap();

    let share_name = "media".to_string();
    let (ucan_signer, space_id) = mint_test_root_and_space();

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

    let ucan = read_ucan(&ucan_signer, &space_id, &client_did);
    let client = std::sync::Arc::new(tokio::sync::RwLock::new(client_inner));

    // Build the manifest off the same bytes the server holds.
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

    // Simulate the post-failure on-disk state Task 7 would have left.
    // The partial bytes file holds the file's first `chunk_size` bytes
    // (chunk 0 — already verified) followed by zeros for chunks 1+2.
    // The sidecar bitmap records [true, false, false] so missing_ranges
    // returns [(chunk_size, 3*chunk_size)] — i.e. chunks 1 and 2.
    let tmp_out = tempfile::tempdir().unwrap();
    let out_path = tmp_out.path().join("out_resume.bin");
    let partial_path = crate::peer_storage::resume::PartialState::partial_path(&out_path);
    let mut partial_bytes = vec![0u8; payload_len];
    partial_bytes[..chunk_size].copy_from_slice(&ramp[..chunk_size]);
    tokio::fs::write(&partial_path, &partial_bytes)
        .await
        .unwrap();

    let sidecar = crate::peer_storage::resume::PartialState {
        file_hash: expected_file_hash.clone(),
        chunk_size: crate::file_sync::hashing::CHUNK_HASH_SIZE,
        completed: vec![true, false, false],
    };
    sidecar.save(&out_path).await.unwrap();

    // Sanity-check the pre-state: sidecar + partial bytes file exist.
    let meta_path = {
        let mut p = out_path.as_os_str().to_owned();
        p.push(".haex-partial.meta");
        std::path::PathBuf::from(p)
    };
    assert!(meta_path.exists(), "sidecar meta seeded");
    assert!(partial_path.exists(), "partial bytes seeded");
    assert_eq!(
        sidecar.missing_ranges(),
        vec![(chunk_size as u64, 3 * chunk_size as u64)]
    );

    // Capture progress events to assert the resume loop emits at least
    // one (>0, file_size) update — guards against a regression where
    // on_progress is dropped on the resume path (finding I5).
    let progress_log: std::sync::Arc<std::sync::Mutex<Vec<(u64, u64)>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let progress_log_cb = progress_log.clone();
    let on_progress: std::sync::Arc<dyn Fn(u64, u64) + Send + Sync> =
        std::sync::Arc::new(move |done, total| {
            progress_log_cb
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((done, total));
        });

    let path = format!("/{share_name}/resume.bin");
    let result = crate::peer_storage::client::download_file_to_path(
        client,
        server_id,
        None,
        path,
        out_path.clone(),
        Some(manifest),
        Some(on_progress),
        None,
        None,
        ucan,
    )
    .await
    .expect("resume download must succeed");

    // The resume path always reports the manifest BLAKE3 file_hash.
    assert_eq!(result.bytes, payload_len as u64);
    assert_eq!(
        result.hash.as_deref(),
        Some(expected_file_hash.as_str()),
        "resume returns the manifest BLAKE3 file_hash"
    );

    // Final file is bit-perfect and the sidecar machinery is cleaned up.
    let on_disk = tokio::fs::read(&out_path).await.unwrap();
    assert_eq!(on_disk, ramp, "resumed bytes match the source verbatim");
    assert!(
        !meta_path.exists(),
        "sidecar metadata cleared after successful resume: {meta_path:?}"
    );
    assert!(
        !partial_path.exists(),
        "partial bytes file renamed away after successful resume: {partial_path:?}"
    );

    // Progress: the resume loop must surface at least one event whose
    // `done` is positive and `total` matches `file_size`. Before the I5
    // fix the callback was dropped via `let _ = on_progress;` so this
    // assertion would catch any regression.
    let events = progress_log
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    assert!(
        events
            .iter()
            .any(|(done, total)| *done > 0 && *total == payload_len as u64),
        "expected at least one progress event with done>0 and total=file_size during resume; got {events:?}"
    );
}

/// Sidecar drift: when the sidecar's `file_hash` disagrees with the
/// manifest, `load_if_matches` returns `None` and the helper falls back
/// to Task 7's fresh-download path — so the partial bytes from the
/// stale attempt are overwritten and the download succeeds.
#[tokio::test]
async fn single_stream_ignores_sidecar_with_drifting_file_hash() {
    let h = setup_multipart_harness().await;
    let path = format!("/{}/ramp.bin", h.share_name);
    let tmp_out = tempfile::tempdir().unwrap();
    let out_path = tmp_out.path().join("out_drift.bin");

    // Seed a sidecar whose file_hash does NOT match the actual file.
    // load_if_matches → None → fresh download path.
    let stale = crate::peer_storage::resume::PartialState {
        file_hash: "deadbeef".repeat(8),
        chunk_size: crate::file_sync::hashing::CHUNK_HASH_SIZE,
        completed: vec![true],
    };
    stale.save(&out_path).await.unwrap();
    let partial_path = crate::peer_storage::resume::PartialState::partial_path(&out_path);
    // The stale partial-bytes file has nonsense data — the fresh download
    // path must overwrite it entirely.
    tokio::fs::write(&partial_path, b"junk").await.unwrap();

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
    .expect("download with drifting-hash sidecar falls back to fresh path");

    assert_eq!(result.bytes, 1024 * 1024);
    let on_disk = tokio::fs::read(&out_path).await.unwrap();
    let expected: Vec<u8> = (0..1024u32 * 1024u32).map(|i| (i % 256) as u8).collect();
    assert_eq!(on_disk, expected);
}
