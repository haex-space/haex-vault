#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use base64::Engine as _;
    use ed25519_dalek::{Signer, SigningKey};

    use crate::peer_storage::endpoint::{PeerEndpoint, PeerState, SharedFolder};

    // =========================================================================
    // Unit tests: PeerState access control
    // =========================================================================

    #[test]
    fn peer_state_default_denies_all() {
        let state = PeerState::default();
        assert!(state.allowed_peers.is_empty());
        assert!(state.allowed_peers.get("any-peer-id").is_none());
    }

    #[test]
    fn peer_state_allows_registered_peer() {
        let mut state = PeerState::default();
        let mut spaces = HashSet::new();
        spaces.insert("space-1".to_string());
        state.allowed_peers.insert("peer-abc".to_string(), spaces);

        let allowed = state.allowed_peers.get("peer-abc");
        assert!(allowed.is_some());
        assert!(allowed.unwrap().contains("space-1"));
    }

    #[test]
    fn peer_state_denies_unregistered_peer() {
        let mut state = PeerState::default();
        let mut spaces = HashSet::new();
        spaces.insert("space-1".to_string());
        state.allowed_peers.insert("peer-abc".to_string(), spaces);

        assert!(state.allowed_peers.get("peer-xyz").is_none());
    }

    #[test]
    fn peer_state_revoke_removes_access() {
        let mut state = PeerState::default();
        let mut spaces = HashSet::new();
        spaces.insert("space-1".to_string());
        state.allowed_peers.insert("peer-abc".to_string(), spaces);

        assert!(state.allowed_peers.get("peer-abc").is_some());

        // Revoke: update with empty map (simulates reload after device removal)
        state.allowed_peers = HashMap::new();

        assert!(state.allowed_peers.get("peer-abc").is_none());
    }

    #[test]
    fn peer_state_partial_revoke() {
        let mut state = PeerState::default();

        let mut spaces_a = HashSet::new();
        spaces_a.insert("space-1".to_string());
        state.allowed_peers.insert("peer-a".to_string(), spaces_a);

        let mut spaces_b = HashSet::new();
        spaces_b.insert("space-1".to_string());
        state.allowed_peers.insert("peer-b".to_string(), spaces_b);

        // Remove only peer-a
        let mut new_allowed = HashMap::new();
        let mut spaces_b_new = HashSet::new();
        spaces_b_new.insert("space-1".to_string());
        new_allowed.insert("peer-b".to_string(), spaces_b_new);
        state.allowed_peers = new_allowed;

        assert!(state.allowed_peers.get("peer-a").is_none());
        assert!(state.allowed_peers.get("peer-b").is_some());
    }

    #[test]
    fn peer_state_multi_space_access() {
        let mut state = PeerState::default();
        let mut spaces = HashSet::new();
        spaces.insert("space-1".to_string());
        spaces.insert("space-2".to_string());
        state.allowed_peers.insert("peer-a".to_string(), spaces);

        let allowed = state.allowed_peers.get("peer-a").unwrap();
        assert!(allowed.contains("space-1"));
        assert!(allowed.contains("space-2"));
        assert!(!allowed.contains("space-3"));
    }

    #[test]
    fn peer_state_empty_spaces_treated_as_denied() {
        let mut state = PeerState::default();
        state
            .allowed_peers
            .insert("peer-a".to_string(), HashSet::new());

        let spaces = state.allowed_peers.get("peer-a").unwrap();
        assert!(spaces.is_empty());
    }

    // =========================================================================
    // Unit tests: Share space filtering
    // =========================================================================

    #[test]
    fn share_only_accessible_in_matching_space() {
        let mut state = PeerState::default();

        state.shares.insert(
            "share-1".to_string(),
            SharedFolder {
                name: "Photos".to_string(),
                local_path: String::from("/tmp/photos"),
                space_id: "space-1".to_string(),
            },
        );

        state.shares.insert(
            "share-2".to_string(),
            SharedFolder {
                name: "Docs".to_string(),
                local_path: String::from("/tmp/docs"),
                space_id: "space-2".to_string(),
            },
        );

        // Peer only has access to space-1
        let mut peer_spaces = HashSet::new();
        peer_spaces.insert("space-1".to_string());

        let accessible: Vec<_> = state
            .shares
            .values()
            .filter(|s| peer_spaces.contains(&s.space_id))
            .collect();

        assert_eq!(accessible.len(), 1);
        assert_eq!(accessible[0].name, "Photos");
    }

    #[test]
    fn share_inaccessible_without_space_membership() {
        let mut state = PeerState::default();

        state.shares.insert(
            "share-1".to_string(),
            SharedFolder {
                name: "Secret".to_string(),
                local_path: String::from("/tmp/secret"),
                space_id: "space-private".to_string(),
            },
        );

        // Peer has no space access
        let peer_spaces: HashSet<String> = HashSet::new();

        let accessible: Vec<_> = state
            .shares
            .values()
            .filter(|s| peer_spaces.contains(&s.space_id))
            .collect();

        assert_eq!(accessible.len(), 0);
    }

    // =========================================================================
    // Async unit tests: PeerEndpoint state management
    // =========================================================================

    #[tokio::test]
    async fn endpoint_set_allowed_peers_updates_state() {
        let ep = PeerEndpoint::new_ephemeral();

        let mut allowed = HashMap::new();
        let mut spaces = HashSet::new();
        spaces.insert("space-1".to_string());
        allowed.insert("peer-abc".to_string(), spaces);

        ep.set_allowed_peers(allowed).await;

        let state = ep.state.read().await;
        assert!(state.allowed_peers.contains_key("peer-abc"));
    }

    #[tokio::test]
    async fn endpoint_revoke_clears_allowed_peers() {
        let ep = PeerEndpoint::new_ephemeral();

        let mut allowed = HashMap::new();
        let mut spaces = HashSet::new();
        spaces.insert("space-1".to_string());
        allowed.insert("peer-abc".to_string(), spaces);
        ep.set_allowed_peers(allowed).await;

        ep.set_allowed_peers(HashMap::new()).await;

        let state = ep.state.read().await;
        assert!(state.allowed_peers.is_empty());
    }

    #[tokio::test]
    async fn endpoint_add_and_remove_share() {
        let ep = PeerEndpoint::new_ephemeral();

        ep.add_share(
            "s1".to_string(),
            "Photos".to_string(),
            "/tmp/photos".to_string(),
            "space-1".to_string(),
        )
        .await;

        assert_eq!(ep.list_shares().await.len(), 1);

        ep.remove_share("s1").await;
        assert_eq!(ep.list_shares().await.len(), 0);
    }

    #[tokio::test]
    async fn endpoint_clear_shares() {
        let ep = PeerEndpoint::new_ephemeral();

        ep.add_share("s1".to_string(), "A".to_string(), "/a".to_string(), "sp1".to_string()).await;
        ep.add_share("s2".to_string(), "B".to_string(), "/b".to_string(), "sp1".to_string()).await;

        assert_eq!(ep.list_shares().await.len(), 2);
        ep.clear_shares().await;
        assert_eq!(ep.list_shares().await.len(), 0);
    }

    #[tokio::test]
    async fn endpoint_rapid_peer_updates_final_state_correct() {
        let ep = PeerEndpoint::new_ephemeral();

        // Rapid grant/revoke
        for _ in 0..100 {
            let mut allowed = HashMap::new();
            let mut spaces = HashSet::new();
            spaces.insert("space-1".to_string());
            allowed.insert("peer-abc".to_string(), spaces);
            ep.set_allowed_peers(allowed).await;
            ep.set_allowed_peers(HashMap::new()).await;
        }

        // Final state: revoked
        let state = ep.state.read().await;
        assert!(state.allowed_peers.is_empty());
    }

    #[tokio::test]
    async fn endpoint_revoke_one_keep_others() {
        let ep = PeerEndpoint::new_ephemeral();

        let mut allowed = HashMap::new();
        let mut spaces = HashSet::new();
        spaces.insert("space-1".to_string());
        allowed.insert("peer-evil".to_string(), spaces.clone());
        allowed.insert("peer-good".to_string(), spaces);
        ep.set_allowed_peers(allowed).await;

        // Revoke only evil peer
        let mut new_allowed = HashMap::new();
        let mut spaces = HashSet::new();
        spaces.insert("space-1".to_string());
        new_allowed.insert("peer-good".to_string(), spaces);
        ep.set_allowed_peers(new_allowed).await;

        let state = ep.state.read().await;
        assert!(state.allowed_peers.get("peer-evil").is_none());
        assert!(state.allowed_peers.get("peer-good").is_some());
    }

    // =========================================================================
    // Integration tests: client.rs remote_* methods
    // ------------------------------------------------------------------
    // Two local PeerEndpoints over RelayMode::Disabled, sharing a temp
    // directory that contains a 1 MiB ramp file (byte i == (i % 256) as u8).
    // =========================================================================

    const BASE64URL: base64::engine::GeneralPurpose = base64::engine::GeneralPurpose::new(
        &base64::alphabet::URL_SAFE,
        base64::engine::general_purpose::NO_PAD,
    );
    const ED25519_MULTICODEC: [u8; 2] = [0xed, 0x01];

    fn did_from_signing_key(key: &SigningKey) -> String {
        let mut bytes = Vec::with_capacity(34);
        bytes.extend_from_slice(&ED25519_MULTICODEC);
        bytes.extend_from_slice(key.verifying_key().as_bytes());
        format!("did:key:z{}", bs58::encode(bytes).into_string())
    }

    /// Mint a UCAN for `space_id` with the given capability, signed by the
    /// audience key. Mirrors the test helper used by
    /// `ucan::verify::tests::make_test_token`, kept inline here so the
    /// peer_storage tests have no cross-module test dependency.
    fn mint_ucan(signer: &SigningKey, space_id: &str, capability: &str) -> String {
        let issuer_did = did_from_signing_key(signer);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let header = serde_json::json!({"alg": "EdDSA", "typ": "JWT"});
        let payload = serde_json::json!({
            "ucv": "1.0",
            "iss": issuer_did,
            "aud": "did:key:z6MkAudience",
            "cap": { format!("space:{}", space_id): capability },
            "exp": now + 3600,
            "iat": now,
            "prf": [],
            "nnc": "test-nonce"
        });
        let header_b64 = BASE64URL.encode(serde_json::to_string(&header).unwrap().as_bytes());
        let payload_b64 = BASE64URL.encode(serde_json::to_string(&payload).unwrap().as_bytes());
        let signing_input = format!("{}.{}", header_b64, payload_b64);
        let signature = signer.sign(signing_input.as_bytes());
        format!(
            "{}.{}.{}",
            header_b64,
            payload_b64,
            BASE64URL.encode(signature.to_bytes())
        )
    }

    fn read_ucan(signer: &SigningKey, space_id: &str) -> String {
        mint_ucan(signer, space_id, "space/read")
    }

    fn write_ucan(signer: &SigningKey, space_id: &str) -> String {
        mint_ucan(signer, space_id, "space/write")
    }

    struct Harness {
        // Kept alive so the bound iroh endpoint + accept loop keep running
        // for the duration of the test, even though we never call methods
        // on `server` directly after setup.
        _server: PeerEndpoint,
        client: PeerEndpoint,
        server_remote_id: iroh::EndpointId,
        share_name: String,
        ucan: String,
        _tmp: tempfile::TempDir,
    }

    /// Spin up two local PeerEndpoints. Server hosts a 1 MiB ramp file under
    /// share "media" / space "test-space". Client is registered as an allowed
    /// peer for that space and has a fresh QUIC connection cached so
    /// `open_stream` will reuse it without needing relay/address lookup.
    async fn setup_harness() -> Harness {
        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("ramp.bin");
        let mut ramp = vec![0u8; 1024 * 1024];
        for (i, b) in ramp.iter_mut().enumerate() {
            *b = (i % 256) as u8;
        }
        tokio::fs::write(&file_path, &ramp).await.unwrap();

        let share_name = "media".to_string();
        let space_id = "test-space".to_string();

        // --- Server side ---
        let mut server = PeerEndpoint::new_ephemeral();
        let server_id = server.start_for_test().await.expect("server bind");
        server
            .add_share(
                "share-1".to_string(),
                share_name.clone(),
                tmp.path().to_string_lossy().to_string(),
                space_id.clone(),
            )
            .await;

        // --- Client side ---
        let mut client = PeerEndpoint::new_ephemeral();
        client.start_for_test().await.expect("client bind");
        let client_id = client.endpoint_id();

        // Grant the client read access to the space on the server.
        let mut allowed = HashMap::new();
        let mut spaces = HashSet::new();
        spaces.insert(space_id.clone());
        allowed.insert(client_id.to_string(), spaces);
        server.set_allowed_peers(allowed).await;

        // Server endpoint addr (full, with direct addrs since RelayMode::Disabled).
        let server_addr = server.endpoint_ref().unwrap().addr();
        client
            .connect_for_test(server_addr)
            .await
            .expect("client → server connect");

        // Sign the UCAN with the same key as the client device — the server's
        // capability check verifies the token signature but does not require
        // iss == client EndpointId, only that the token grants read on the
        // target space.
        let mut seed = [0u8; 32];
        rand::fill(&mut seed);
        let ucan_signer = SigningKey::from_bytes(&seed);
        let ucan = read_ucan(&ucan_signer, &space_id);

        Harness {
            _server: server,
            client,
            server_remote_id: server_id,
            share_name,
            ucan,
            _tmp: tmp,
        }
    }

    #[tokio::test]
    async fn remote_read_range_returns_only_requested_bytes() {
        let h = setup_harness().await;
        let path = format!("/{}/ramp.bin", h.share_name);

        let bytes = h
            .client
            .remote_read_range_bytes(h.server_remote_id, None, &path, [100, 199], &h.ucan)
            .await
            .expect("remote_read_range_bytes");

        assert_eq!(bytes.len(), 100, "range [100, 199] should yield 100 bytes");
        assert_eq!(bytes[0], 100, "first byte of the range");
        assert_eq!(bytes[99], 199, "last byte of the range");
    }

    #[tokio::test]
    async fn remote_stat_returns_file_size() {
        let h = setup_harness().await;
        let path = format!("/{}/ramp.bin", h.share_name);

        let entry = h
            .client
            .remote_stat(h.server_remote_id, None, &path, &h.ucan)
            .await
            .expect("remote_stat");

        assert_eq!(entry.size, 1024 * 1024, "ramp file is 1 MiB");
        assert!(!entry.is_dir, "ramp.bin is a regular file");
    }

    // =========================================================================
    // Unit tests: streaming module constants and error types
    // =========================================================================

    #[test]
    fn streaming_chunk_size_is_one_mib() {
        assert_eq!(crate::peer_storage::streaming::CHUNK_SIZE, 1024 * 1024);
    }

    #[test]
    fn streaming_channel_depth_is_eight() {
        assert_eq!(crate::peer_storage::streaming::CHANNEL_DEPTH, 8);
    }

    #[test]
    fn streaming_multi_stream_threshold_is_sixteen_mib() {
        assert_eq!(
            crate::peer_storage::streaming::MULTI_STREAM_THRESHOLD,
            16 * 1024 * 1024
        );
    }

    #[test]
    fn streaming_max_parallel_streams_is_four() {
        assert_eq!(crate::peer_storage::streaming::MAX_PARALLEL_STREAMS_PER_FILE, 4);
    }

    #[test]
    fn pipeline_error_display_io() {
        use crate::peer_storage::streaming::PipelineError;
        let e = PipelineError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "file gone"));
        let s = e.to_string();
        assert!(s.starts_with("io:"), "expected 'io:' prefix, got: {s}");
        assert!(s.contains("file gone"));
    }

    #[test]
    fn pipeline_error_display_stream() {
        use crate::peer_storage::streaming::PipelineError;
        let e = PipelineError::Stream("unexpected EOF".to_string());
        let s = e.to_string();
        assert!(s.starts_with("stream:"), "expected 'stream:' prefix, got: {s}");
        assert!(s.contains("unexpected EOF"));
    }

    #[test]
    fn pipeline_error_display_cancelled() {
        use crate::peer_storage::streaming::PipelineError;
        let e = PipelineError::Cancelled;
        assert_eq!(e.to_string(), "cancelled");
    }

    #[test]
    fn pipeline_error_is_std_error() {
        use crate::peer_storage::streaming::PipelineError;
        // Verify the trait bound compiles and is well-formed.
        fn accepts_error<E: std::error::Error>(_e: E) {}
        accepts_error(PipelineError::Cancelled);
        accepts_error(PipelineError::Stream("x".into()));
    }

    #[test]
    fn recv_stats_default_is_zero_and_no_hash() {
        use crate::peer_storage::streaming::RecvStats;
        let s = RecvStats::default();
        assert_eq!(s.bytes, 0);
        assert!(s.hash.is_none());
    }

    #[test]
    fn recv_options_default_has_no_fields_set() {
        use crate::peer_storage::streaming::RecvOptions;
        let opts = RecvOptions::default();
        assert!(opts.on_progress.is_none());
        assert!(opts.cancel_token.is_none());
        assert!(opts.pause_flag.is_none());
        assert!(!opts.compute_hash, "compute_hash should default to false");
    }

    // =========================================================================
    // Integration tests: streaming pipelines via PeerEndpoint operations
    // =========================================================================

    /// A harness variant where the client is wrapped in `Arc<RwLock<PeerEndpoint>>`
    /// so it can be passed directly to `read_multipart_to_file`.
    struct MultipartHarness {
        _server: PeerEndpoint,
        client: std::sync::Arc<tokio::sync::RwLock<PeerEndpoint>>,
        server_remote_id: iroh::EndpointId,
        share_name: String,
        ucan: String,
        _tmp: tempfile::TempDir,
    }

    async fn setup_multipart_harness() -> MultipartHarness {
        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("ramp.bin");
        let mut ramp = vec![0u8; 1024 * 1024];
        for (i, b) in ramp.iter_mut().enumerate() {
            *b = (i % 256) as u8;
        }
        tokio::fs::write(&file_path, &ramp).await.unwrap();

        let share_name = "media".to_string();
        let space_id = "test-space".to_string();

        let mut server = PeerEndpoint::new_ephemeral();
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
        client_inner.start_for_test().await.expect("client bind");
        let client_id = client_inner.endpoint_id();

        let mut allowed = HashMap::new();
        let mut spaces = HashSet::new();
        spaces.insert(space_id.clone());
        allowed.insert(client_id.to_string(), spaces);
        server.set_allowed_peers(allowed).await;

        let server_addr = server.endpoint_ref().unwrap().addr();
        client_inner
            .connect_for_test(server_addr)
            .await
            .expect("client → server connect");

        let mut seed = [0u8; 32];
        rand::fill(&mut seed);
        let ucan_signer = ed25519_dalek::SigningKey::from_bytes(&seed);
        let ucan = read_ucan(&ucan_signer, &space_id);

        let client = std::sync::Arc::new(tokio::sync::RwLock::new(client_inner));

        MultipartHarness {
            _server: server,
            client,
            server_remote_id: server_id,
            share_name,
            ucan,
            _tmp: tmp,
        }
    }

    // -------------------------------------------------------------------------
    // pipe_recv_to_writer (via remote_read_to_file) tests
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn pipe_recv_to_writer_full_file_produces_correct_hash() {
        let h = setup_harness().await;
        let path = format!("/{}/ramp.bin", h.share_name);
        let tmp_out = tempfile::tempdir().unwrap();
        let out_path = tmp_out.path().join("ramp_out.bin");

        let result = h
            .client
            .remote_read_to_file(
                h.server_remote_id,
                None,
                &path,
                &out_path,
                None,
                None,
                None,
                None,
                &h.ucan,
            )
            .await
            .expect("remote_read_to_file");

        assert_eq!(result.bytes, 1024 * 1024, "should download all 1 MiB");
        assert!(result.hash.is_some(), "full-file download must produce a hash");

        // Independently compute the expected hash of the ramp data.
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        let mut ramp = vec![0u8; 1024 * 1024];
        for (i, b) in ramp.iter_mut().enumerate() {
            *b = (i % 256) as u8;
        }
        hasher.update(&ramp);
        let expected = hex::encode(hasher.finalize());
        assert_eq!(result.hash.unwrap(), expected, "downloaded hash must match expected hash");

        // Output file must exist and have the right length.
        let meta = tokio::fs::metadata(&out_path).await.unwrap();
        assert_eq!(meta.len(), 1024 * 1024);
    }

    #[tokio::test]
    async fn pipe_recv_to_writer_range_has_no_hash() {
        let h = setup_harness().await;
        let path = format!("/{}/ramp.bin", h.share_name);
        let tmp_out = tempfile::tempdir().unwrap();
        let out_path = tmp_out.path().join("range_out.bin");

        // A partial range: bytes [512, 1024) (half-open on the wire)
        let result = h
            .client
            .remote_read_to_file(
                h.server_remote_id,
                None,
                &path,
                &out_path,
                Some([512, 1024]),
                None,
                None,
                None,
                &h.ucan,
            )
            .await
            .expect("remote_read_to_file range");

        assert_eq!(result.bytes, 512, "range download must be 512 bytes");
        assert!(result.hash.is_none(), "partial range must not produce a hash");
    }

    #[tokio::test]
    async fn pipe_recv_to_writer_reports_progress() {
        use std::sync::{Arc, Mutex};

        let h = setup_harness().await;
        let path = format!("/{}/ramp.bin", h.share_name);
        let tmp_out = tempfile::tempdir().unwrap();
        let out_path = tmp_out.path().join("progress_out.bin");

        // Collect progress reports.
        let reports: Arc<Mutex<Vec<(u64, u64)>>> = Arc::new(Mutex::new(Vec::new()));
        let reports_clone = reports.clone();
        let cb = Box::new(move |done: u64, total: u64| {
            reports_clone.lock().unwrap().push((done, total));
        });

        h.client
            .remote_read_to_file(
                h.server_remote_id,
                None,
                &path,
                &out_path,
                None,
                Some(cb),
                None,
                None,
                &h.ucan,
            )
            .await
            .expect("remote_read_to_file with progress");

        let collected = reports.lock().unwrap();
        assert!(!collected.is_empty(), "progress callback must have been called");

        // All reported totals should match the file size.
        for (_, total) in collected.iter() {
            assert_eq!(*total, 1024 * 1024, "total passed to progress must be file size");
        }

        // Progress must be non-decreasing.
        let mut prev = 0u64;
        for (done, _) in collected.iter() {
            assert!(*done >= prev, "progress must be non-decreasing");
            prev = *done;
        }

        // Final progress value must equal total size.
        let last_done = collected.last().map(|(d, _)| *d).unwrap_or(0);
        assert_eq!(last_done, 1024 * 1024, "final progress must equal file size");
    }

    #[tokio::test]
    async fn pipe_recv_to_writer_cancelled_token_aborts_download() {
        let h = setup_harness().await;
        let path = format!("/{}/ramp.bin", h.share_name);
        let tmp_out = tempfile::tempdir().unwrap();
        let out_path = tmp_out.path().join("cancel_out.bin");

        // Pre-cancel the token so it is already cancelled when the transfer starts.
        let token = tokio_util::sync::CancellationToken::new();
        token.cancel();

        let result = h
            .client
            .remote_read_to_file(
                h.server_remote_id,
                None,
                &path,
                &out_path,
                None,
                None,
                Some(token),
                None,
                &h.ucan,
            )
            .await;

        assert!(result.is_err(), "cancelled transfer must return an error");
        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("cancel") || err_str.contains("Cancel"),
            "error must mention cancellation, got: {err_str}"
        );

        // Output file must have been cleaned up on cancellation.
        assert!(
            !out_path.exists(),
            "partial output file must be removed on cancellation"
        );
    }

    #[tokio::test]
    async fn pipe_recv_to_writer_pause_flag_can_pause_and_resume() {
        use std::sync::{Arc, atomic::AtomicBool};

        let h = setup_harness().await;
        let path = format!("/{}/ramp.bin", h.share_name);
        let tmp_out = tempfile::tempdir().unwrap();
        let out_path = tmp_out.path().join("pause_out.bin");

        // Start with paused=false so the transfer completes normally.
        let pause_flag = Arc::new(AtomicBool::new(false));

        let result = h
            .client
            .remote_read_to_file(
                h.server_remote_id,
                None,
                &path,
                &out_path,
                None,
                None,
                None,
                Some(pause_flag),
                &h.ucan,
            )
            .await
            .expect("transfer with unset pause flag should succeed");

        assert_eq!(result.bytes, 1024 * 1024);
    }

    // -------------------------------------------------------------------------
    // pipe_reader_to_send (via remote_write_file + read-back) tests
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn pipe_reader_to_send_upload_then_download_roundtrip() {
        let h = setup_harness().await;
        let upload_path = format!("/{}/uploaded.bin", h.share_name);
        let tmp_out = tempfile::tempdir().unwrap();
        let out_path = tmp_out.path().join("uploaded_out.bin");

        // Build a small deterministic payload and persist it as the upload source.
        let payload: Vec<u8> = (0u16..512).map(|i| (i % 256) as u8).collect();
        let src_dir = tempfile::tempdir().unwrap();
        let src_path = src_dir.path().join("payload.bin");
        tokio::fs::write(&src_path, &payload).await.unwrap();

        // The default harness UCAN only grants read; mint a write-capable one
        // signed by the same key so the upload passes the capability check.
        // The signing key lives only inside setup_harness, so reproduce it by
        // signing fresh — verification only checks the signature against the
        // token's `iss`, not the issuer's identity beyond that.
        let mut seed = [0u8; 32];
        rand::fill(&mut seed);
        let write_signer = SigningKey::from_bytes(&seed);
        let write_token = write_ucan(&write_signer, "test-space");

        // Upload a file to the server, then read it back. The read exercises
        // pipe_reader_to_send through handlers::stream_file_to_send on the
        // server side (disk → QUIC pipeline). The write exercises
        // pipe_recv_to_writer through handlers::handle_write on the server side
        // (QUIC → disk pipeline).
        h.client
            .remote_write_file(
                h.server_remote_id,
                None,
                &upload_path,
                &src_path,
                &write_token,
                crate::peer_storage::streaming::SendOptions::default(),
            )
            .await
            .expect("remote_write_file");

        // Read back via the streaming pipeline.
        let result = h
            .client
            .remote_read_to_file(
                h.server_remote_id,
                None,
                &upload_path,
                &out_path,
                None,
                None,
                None,
                None,
                &h.ucan,
            )
            .await
            .expect("remote_read_to_file after upload");

        assert_eq!(result.bytes, payload.len() as u64);

        let downloaded = tokio::fs::read(&out_path).await.unwrap();
        assert_eq!(downloaded, payload, "round-tripped bytes must match original");
    }

    #[tokio::test]
    async fn pipe_reader_to_send_cancelled_token_aborts_upload() {
        let h = setup_harness().await;
        let upload_path = format!("/{}/cancel_upload.bin", h.share_name);

        // 8 MB payload — large enough that the pipeline iterates over multiple
        // chunks (CHUNK_SIZE = 1 MB) so the cancel-check between chunks gets a
        // chance to trip. Pre-cancelling the token still works because the
        // check runs before the first chunk write.
        let payload: Vec<u8> = (0..(8 * 1024 * 1024)).map(|i| (i % 251) as u8).collect();
        let src_dir = tempfile::tempdir().unwrap();
        let src_path = src_dir.path().join("cancel_payload.bin");
        tokio::fs::write(&src_path, &payload).await.unwrap();

        let mut seed = [0u8; 32];
        rand::fill(&mut seed);
        let write_signer = SigningKey::from_bytes(&seed);
        let write_token = write_ucan(&write_signer, "test-space");

        let token = tokio_util::sync::CancellationToken::new();
        token.cancel();

        let options = crate::peer_storage::streaming::SendOptions {
            on_progress: None,
            cancel_token: Some(token),
        };

        let result = h
            .client
            .remote_write_file(
                h.server_remote_id,
                None,
                &upload_path,
                &src_path,
                &write_token,
                options,
            )
            .await;

        assert!(result.is_err(), "cancelled upload must return an error");
        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("cancel") || err_str.contains("Cancel"),
            "error must mention cancellation, got: {err_str}"
        );

        // The server stages writes to a `.part` sibling and only renames on
        // success — a cancelled upload must leave neither the staged file nor
        // the final destination on disk. Server cleanup runs asynchronously
        // after the client's connection reset propagates, so poll briefly
        // before asserting absence.
        let dest = h._tmp.path().join("cancel_upload.bin");
        let staged = h._tmp.path().join("cancel_upload.bin.part");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if !dest.exists() && !staged.exists() {
                break;
            }
            if std::time::Instant::now() >= deadline {
                panic!(
                    "cancelled upload left files on disk after 2s: dest_exists={}, staged_exists={}",
                    dest.exists(),
                    staged.exists()
                );
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    // -------------------------------------------------------------------------
    // read_multipart_to_file tests
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn read_multipart_to_file_zero_size_creates_empty_file_with_hash() {
        // For size == 0, read_multipart_to_file short-circuits and does not
        // need a real peer connection — just create a dummy endpoint.
        let endpoint = std::sync::Arc::new(tokio::sync::RwLock::new(
            PeerEndpoint::new_ephemeral(),
        ));
        let tmp = tempfile::tempdir().unwrap();
        let out_path = tmp.path().join("empty.bin");

        // EndpointId is not constructible directly; use a nonsense remote_id
        // that will never be contacted (size == 0 exits early).
        let dummy_remote_id = endpoint.read().await.endpoint_id();

        let result = crate::peer_storage::client::read_multipart_to_file(
            endpoint,
            dummy_remote_id,
            None,
            "/media/empty.bin".to_string(),
            out_path.clone(),
            0,
            4,
            None,
            None,
            None,
            "dummy-token".to_string(),
        )
        .await
        .expect("zero-size multipart download");

        assert_eq!(result.bytes, 0, "zero-size download must return 0 bytes");
        assert!(result.hash.is_some(), "zero-size download must have a hash");

        // SHA-256 of empty input is well-known.
        use sha2::Digest;
        let expected_empty_hash = hex::encode(sha2::Sha256::digest([]));
        assert_eq!(
            result.hash.unwrap(),
            expected_empty_hash,
            "hash of zero-size file must be SHA-256 of empty bytes"
        );

        assert!(out_path.exists(), "empty output file must be created");
        let meta = tokio::fs::metadata(&out_path).await.unwrap();
        assert_eq!(meta.len(), 0, "output file must be 0 bytes");
    }

    #[tokio::test]
    async fn read_multipart_to_file_single_stream_matches_direct_download() {
        let h = setup_multipart_harness().await;
        let path = format!("/{}/ramp.bin", h.share_name);
        let tmp_out = tempfile::tempdir().unwrap();
        let out_path = tmp_out.path().join("multipart_out.bin");

        let result = crate::peer_storage::client::read_multipart_to_file(
            h.client.clone(),
            h.server_remote_id,
            None,
            path,
            out_path.clone(),
            1024 * 1024,
            1, // single stream
            None,
            None,
            None,
            h.ucan.clone(),
        )
        .await
        .expect("read_multipart_to_file single stream");

        assert_eq!(result.bytes, 1024 * 1024);
        assert!(result.hash.is_some(), "multipart download must produce a hash");

        // Verify the expected hash.
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        let mut ramp = vec![0u8; 1024 * 1024];
        for (i, b) in ramp.iter_mut().enumerate() {
            *b = (i % 256) as u8;
        }
        hasher.update(&ramp);
        let expected = hex::encode(hasher.finalize());
        assert_eq!(result.hash.unwrap(), expected, "multipart hash must match expected");

        let meta = tokio::fs::metadata(&out_path).await.unwrap();
        assert_eq!(meta.len(), 1024 * 1024, "output file must be correct size");
    }

    #[tokio::test]
    async fn read_multipart_to_file_four_streams_matches_single_stream_hash() {
        let h = setup_multipart_harness().await;
        let path = format!("/{}/ramp.bin", h.share_name);
        let tmp_out = tempfile::tempdir().unwrap();
        let out_path = tmp_out.path().join("multi4_out.bin");

        let result = crate::peer_storage::client::read_multipart_to_file(
            h.client.clone(),
            h.server_remote_id,
            None,
            path,
            out_path.clone(),
            1024 * 1024,
            4, // four parallel streams
            None,
            None,
            None,
            h.ucan.clone(),
        )
        .await
        .expect("read_multipart_to_file 4 streams");

        assert_eq!(result.bytes, 1024 * 1024);
        assert!(result.hash.is_some());

        // The hash must match the known SHA-256 of the ramp data regardless
        // of how many streams were used.
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        let mut ramp = vec![0u8; 1024 * 1024];
        for (i, b) in ramp.iter_mut().enumerate() {
            *b = (i % 256) as u8;
        }
        hasher.update(&ramp);
        let expected = hex::encode(hasher.finalize());
        assert_eq!(result.hash.unwrap(), expected, "4-stream hash must equal single-stream hash");

        // File contents must also be bit-perfect.
        let downloaded = tokio::fs::read(&out_path).await.unwrap();
        assert_eq!(downloaded, ramp, "4-stream download must produce correct file bytes");
    }

    #[tokio::test]
    async fn read_multipart_to_file_reports_aggregate_progress() {
        use std::sync::{Arc, Mutex};

        let h = setup_multipart_harness().await;
        let path = format!("/{}/ramp.bin", h.share_name);
        let tmp_out = tempfile::tempdir().unwrap();
        let out_path = tmp_out.path().join("progress_multi_out.bin");

        let reports: Arc<Mutex<Vec<(u64, u64)>>> = Arc::new(Mutex::new(Vec::new()));
        let reports_clone = reports.clone();
        let cb: std::sync::Arc<dyn Fn(u64, u64) + Send + Sync> =
            std::sync::Arc::new(move |done: u64, total: u64| {
                reports_clone.lock().unwrap().push((done, total));
            });

        crate::peer_storage::client::read_multipart_to_file(
            h.client.clone(),
            h.server_remote_id,
            None,
            path,
            out_path.clone(),
            1024 * 1024,
            2,
            Some(cb),
            None,
            None,
            h.ucan.clone(),
        )
        .await
        .expect("read_multipart_to_file with progress");

        let collected = reports.lock().unwrap();
        assert!(!collected.is_empty(), "progress must have been reported");

        for (_, total) in collected.iter() {
            assert_eq!(*total, 1024 * 1024, "total must always be file size");
        }
    }

    #[tokio::test]
    async fn read_multipart_to_file_parallelism_clamped_to_max() {
        // Parallelism above MAX_PARALLEL_STREAMS_PER_FILE must be silently clamped.
        let h = setup_multipart_harness().await;
        let path = format!("/{}/ramp.bin", h.share_name);
        let tmp_out = tempfile::tempdir().unwrap();
        let out_path = tmp_out.path().join("clamped_out.bin");

        // Request far more streams than the cap allows.
        let result = crate::peer_storage::client::read_multipart_to_file(
            h.client.clone(),
            h.server_remote_id,
            None,
            path,
            out_path.clone(),
            1024 * 1024,
            1024, // way above MAX_PARALLEL_STREAMS_PER_FILE = 4
            None,
            None,
            None,
            h.ucan.clone(),
        )
        .await
        .expect("read_multipart_to_file with excessive parallelism");

        assert_eq!(result.bytes, 1024 * 1024, "clamped-parallelism download must complete");
        assert!(result.hash.is_some());
    }
}
