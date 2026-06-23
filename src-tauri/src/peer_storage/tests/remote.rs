use std::collections::{HashMap, HashSet};

use ed25519_dalek::SigningKey;

use crate::peer_storage::endpoint::{OwnIdentity, PeerEndpoint};

use super::helpers::*;

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

    let stat = h
        .client
        .remote_stat(h.server_remote_id, None, &path, &h.ucan)
        .await
        .expect("remote_stat");

    assert_eq!(stat.entry.size, 1024 * 1024, "ramp file is 1 MiB");
    assert!(!stat.entry.is_dir, "ramp.bin is a regular file");
    let chunks = stat.chunks.expect("file stat must include chunks");
    assert_eq!(
        chunks.chunk_size,
        crate::file_sync::hashing::CHUNK_HASH_SIZE
    );
    assert_eq!(
        chunks.chunk_hashes.len(),
        1,
        "1 MiB file with 1 MiB chunk size = exactly 1 chunk"
    );
    let expected_file_hash = blake3::hash(
        &(0..1024u32 * 1024u32)
            .map(|i| (i % 256) as u8)
            .collect::<Vec<u8>>(),
    )
    .to_hex()
    .to_string();
    assert_eq!(chunks.file_hash, expected_file_hash);
}

/// Defense-in-depth regression: when the DB's expected owner DID for a
/// peer (`peer_owner_dids`) disagrees with the cryptographically verified
/// DID from the handshake, the connection must close. The handshake on
/// its own only proves the peer holds the private key for the DID it
/// claims; the cross-check ensures that DID is also the one our vault
/// recorded for this endpoint id when the row was synced through CRDT.
/// A drift between the two layers is treated as a vault-internal
/// inconsistency, not a recoverable state.
#[tokio::test]
async fn connection_closes_when_peer_owner_did_disagrees_with_handshake() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("file.txt"), b"x").unwrap();
    let space_id = "test-space".to_string();

    let mut server = PeerEndpoint::new_ephemeral();
    server.set_random_test_identity();
    let server_id = server.start_for_test().await.expect("server bind");
    server
        .add_share(
            "share-1".to_string(),
            "media".to_string(),
            tmp.path().to_string_lossy().to_string(),
            space_id.clone(),
        )
        .await;

    // Build the client's identity ourselves so we keep a handle on the
    // signing key — we'll use it below to mint a *valid* UCAN. With a
    // valid UCAN, any rejection can only come from the
    // peer_owner_did / handshake cross-check (the thing this test
    // actually exercises), not from cheap "token is garbage" failure.
    let client_seed: [u8; 32] = rand::random();
    let client_signing_key = SigningKey::from_bytes(&client_seed);
    let client_did = did_from_signing_key(&client_signing_key);
    let valid_ucan = read_ucan(&client_signing_key, &space_id, &client_did);
    let mut client = PeerEndpoint::new_ephemeral();
    client.set_own_identity(OwnIdentity {
        did: client_did,
        signing_key: client_signing_key,
    });
    client.start_for_test().await.expect("client bind");
    let client_id = client.endpoint_id();

    // Allow the client through the coarse access gate ...
    let mut allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert(space_id.clone());
    allowed.insert(client_id.to_string(), spaces);
    server.set_allowed_peers(allowed).await;

    // ... but tell the server that this endpoint id is supposed to
    // belong to a completely different DID. The handshake will return
    // the client's actual DID; the cross-check must then reject.
    let mut wrong_owner_dids = HashMap::new();
    wrong_owner_dids.insert(
        client_id.to_string(),
        "did:key:z6MkUnrelatedExpectedOwner".to_string(),
    );
    server.set_peer_owner_dids(wrong_owner_dids).await;

    // Run the connect through `connect_for_test`, which completes the
    // handshake locally; the server-side cross-check then closes the
    // connection so the next request open_bi fails.
    let server_addr = server.endpoint_ref().unwrap().addr();
    // `connect_for_test` may or may not see the close before its own
    // handshake completes — either way, the subsequent operation must
    // not succeed.
    let _ = client.connect_for_test(server_addr).await;

    let path = "/media/file.txt".to_string();
    let result = client
        .remote_list(server_id, None, &path, &valid_ucan)
        .await;
    assert!(
        result.is_err(),
        "request must fail when DB owner_did disagrees with handshake (UCAN is valid, so any rejection IS the cross-check), got: {result:?}"
    );
}

/// Layer 1.25 security regression: a UCAN whose audience does not match
/// the verified peer DID must be rejected, even when the signature is
/// valid and the capability + space match. Without this check the entire
/// quic_did_auth handshake would be theatrical — a peer could replay any
/// UCAN it managed to obtain by other means.
#[tokio::test]
async fn remote_list_rejects_ucan_for_foreign_did() {
    let h = setup_harness().await;

    // Mint a UCAN whose audience is a fresh, unrelated DID — not the
    // client's verified DID. The signature is valid (issuer signs over
    // the payload) but the audience is wrong, so handle_stream's Layer
    // 1.25 check should fire.
    let foreign_seed: [u8; 32] = rand::random();
    let foreign_signer = SigningKey::from_bytes(&foreign_seed);
    let foreign_did = did_from_signing_key(&foreign_signer);

    let issuer_seed: [u8; 32] = rand::random();
    let issuer = SigningKey::from_bytes(&issuer_seed);
    let mismatched_ucan = read_ucan(&issuer, "test-space", &foreign_did);

    let result = h
        .client
        .remote_list(h.server_remote_id, None, "/", &mismatched_ucan)
        .await;

    let err = result.expect_err("foreign-audience UCAN must be rejected");
    let msg = format!("{err}");
    assert!(
        msg.contains("audience"),
        "error should mention audience mismatch, got: {msg}"
    );
}
