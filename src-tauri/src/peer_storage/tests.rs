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

    /// Mint a read-capable UCAN for `space_id`, signed by the audience key.
    /// Mirrors the test helper used by `ucan::verify::tests::make_test_token`,
    /// kept inline here so the peer_storage tests have no cross-module test
    /// dependency.
    fn read_ucan(signer: &SigningKey, space_id: &str) -> String {
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
            "cap": { format!("space:{}", space_id): "space/read" },
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
}
