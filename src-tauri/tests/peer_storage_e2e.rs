//! E2E integration tests for P2P Storage access control.
//!
//! Tests two real iroh endpoints connecting on localhost.
//! Server shares folders, client tries to access them.
//! Verifies grant, revoke, cross-space isolation.
//!
//! Run: cargo test --test peer_storage_e2e

use std::collections::{HashMap, HashSet};
use tokio::time::{sleep, Duration};

use ed25519_dalek::SigningKey;
use iroh::Endpoint;
use haex_vault_lib::peer_storage::endpoint::{OwnIdentity, PeerEndpoint};
use haex_vault_lib::peer_storage::protocol::ALPN;
use haex_vault_lib::quic_did_auth;

const ED25519_MULTICODEC: [u8; 2] = [0xed, 0x01];

/// Build a fresh OwnIdentity with a random ed25519 keypair. Mirrors the
/// `PeerEndpoint::set_random_test_identity` helper used by lib-level unit
/// tests, but lives here so the integration crate doesn't depend on test-cfg
/// symbols.
fn random_test_identity() -> OwnIdentity {
    let mut seed = [0u8; 32];
    rand::fill(&mut seed);
    let signing_key = SigningKey::from_bytes(&seed);
    let mut did_bytes = Vec::with_capacity(34);
    did_bytes.extend_from_slice(&ED25519_MULTICODEC);
    did_bytes.extend_from_slice(signing_key.verifying_key().as_bytes());
    let did = format!("did:key:z{}", bs58::encode(did_bytes).into_string());
    OwnIdentity { did, signing_key }
}

/// Create two endpoints that can discover each other on localhost. Both get
/// a fresh identity installed before `start` so the quic_did_auth handshake
/// can complete on each accepted connection.
async fn create_connected_pair() -> (PeerEndpoint, PeerEndpoint, iroh::EndpointAddr, OwnIdentity) {
    let mut server = PeerEndpoint::new_ephemeral();
    let mut client = PeerEndpoint::new_ephemeral();

    server.set_own_identity(random_test_identity());
    let client_identity = random_test_identity();
    client.set_own_identity(client_identity.clone());

    server.start(None).await.unwrap();
    client.start(None).await.unwrap();

    // Get server's full address (ID + local socket addrs) for direct connection
    let server_addr = server.endpoint_ref().unwrap().addr();

    (server, client, server_addr, client_identity)
}

/// Low-level: connect to a server and send a LIST request, return the
/// response.
///
/// Must complete the server-initiated quic_did_auth handshake on the first
/// bidirectional stream before opening the request stream. The handshake
/// runs once per connection on the server side (`handle_connection`),
/// regardless of how many requests follow.
async fn raw_list(
    client_ep: &Endpoint,
    server_addr: iroh::EndpointAddr,
    client_identity: &OwnIdentity,
) -> Result<String, String> {
    let conn = tokio::time::timeout(
        Duration::from_secs(5),
        client_ep.connect(server_addr, ALPN),
    )
    .await
    .map_err(|_| "connect timeout".to_string())?
    .map_err(|e| format!("connect error: {e}"))?;

    // Phase 1: server-initiated DID-auth handshake (server opens the
    // bidirectional auth stream, client accepts and signs the response).
    let (mut auth_send, mut auth_recv) = tokio::time::timeout(
        Duration::from_secs(5),
        conn.accept_bi(),
    )
    .await
    .map_err(|_| "auth accept_bi timeout".to_string())?
    .map_err(|e| format!("auth accept_bi error: {e}"))?;

    quic_did_auth::respond_to_challenge(
        &mut auth_send,
        &mut auth_recv,
        &client_identity.did,
        &client_identity.signing_key,
        &client_ep.id().to_string(),
    )
    .await
    .map_err(|e| format!("did-auth error: {e}"))?;
    let _ = auth_send.finish();

    // Phase 2: the actual LIST request on a client-initiated stream.
    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .map_err(|e| format!("open_bi error: {e}"))?;

    // Send LIST request
    let request = serde_json::to_vec(&serde_json::json!({"type": "LIST", "path": "/"}))
        .map_err(|e| format!("serialize: {e}"))?;

    send.write_all(&request)
        .await
        .map_err(|e| format!("write error: {e}"))?;
    send.finish()
        .map_err(|e| format!("finish error: {e}"))?;

    let response = recv
        .read_to_end(1024 * 1024)
        .await
        .map_err(|e| format!("read error: {e}"))?;

    String::from_utf8(response).map_err(|e| format!("utf8 error: {e}"))
}

// =============================================================================
// Tests
// =============================================================================

#[tokio::test]
async fn allowed_peer_can_connect() {
    let (mut server, mut client, server_addr, client_identity) = create_connected_pair().await;

    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("test.txt"), b"hello").unwrap();

    server.add_share(
        "s1".to_string(), "Test".to_string(),
        tmp.path().to_string_lossy().to_string(), "space-1".to_string(),
    ).await;

    // Allow client + mirror the DB-side `(endpoint_id -> owner_did)`
    // expectation that handle_connection cross-checks against the
    // crypto-verified DID from the handshake.
    let mut allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert("space-1".to_string());
    allowed.insert(client.endpoint_id().to_string(), spaces);
    server.set_allowed_peers(allowed).await;
    let mut owner_dids = HashMap::new();
    owner_dids.insert(client.endpoint_id().to_string(), client_identity.did.clone());
    server.set_peer_owner_dids(owner_dids).await;

    // Connect and send LIST
    let client_ep = client.endpoint_ref().unwrap().clone();
    let result = raw_list(&client_ep, server_addr, &client_identity).await;

    assert!(result.is_ok(), "allowed peer should get a response, got: {:?}", result);

    let _ = client.stop().await;
    let _ = server.stop().await;
}

#[tokio::test]
async fn unknown_peer_is_rejected() {
    let (mut server, mut attacker, server_addr, attacker_identity) = create_connected_pair().await;

    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("secret.txt"), b"secret").unwrap();

    server.add_share(
        "s1".to_string(), "Secrets".to_string(),
        tmp.path().to_string_lossy().to_string(), "space-1".to_string(),
    ).await;

    // No peers allowed — the server's accept loop closes the connection
    // before the auth handshake even starts.
    server.set_allowed_peers(HashMap::new()).await;

    let attacker_ep = attacker.endpoint_ref().unwrap().clone();
    let result = raw_list(&attacker_ep, server_addr, &attacker_identity).await;

    assert!(result.is_err(), "unknown peer must be rejected, got: {:?}", result);

    let _ = attacker.stop().await;
    let _ = server.stop().await;
}

#[tokio::test]
async fn access_revoked_mid_session() {
    let (mut server, mut client, server_addr, client_identity) = create_connected_pair().await;

    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("data.txt"), b"secret").unwrap();

    server.add_share(
        "s1".to_string(), "Data".to_string(),
        tmp.path().to_string_lossy().to_string(), "space-1".to_string(),
    ).await;

    // Allow client + matching owner_did expectation.
    let mut allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert("space-1".to_string());
    allowed.insert(client.endpoint_id().to_string(), spaces);
    server.set_allowed_peers(allowed).await;
    let mut owner_dids = HashMap::new();
    owner_dids.insert(client.endpoint_id().to_string(), client_identity.did.clone());
    server.set_peer_owner_dids(owner_dids).await;

    let client_ep = client.endpoint_ref().unwrap().clone();

    // First request succeeds
    let result = raw_list(&client_ep, server_addr.clone(), &client_identity).await;
    assert!(result.is_ok(), "should succeed before revocation");

    // Revoke
    server.set_allowed_peers(HashMap::new()).await;
    server.set_peer_owner_dids(HashMap::new()).await;
    sleep(Duration::from_millis(100)).await;

    // Second request should fail
    let result = raw_list(&client_ep, server_addr, &client_identity).await;
    assert!(result.is_err(), "should fail after revocation, got: {:?}", result);

    let _ = client.stop().await;
    let _ = server.stop().await;
}

#[tokio::test]
async fn partial_revoke_only_blocks_target() {
    let mut server = PeerEndpoint::new_ephemeral();
    let mut good = PeerEndpoint::new_ephemeral();
    let mut evil = PeerEndpoint::new_ephemeral();

    server.set_own_identity(random_test_identity());
    let good_identity = random_test_identity();
    good.set_own_identity(good_identity.clone());
    let evil_identity = random_test_identity();
    evil.set_own_identity(evil_identity.clone());

    server.start(None).await.unwrap();
    good.start(None).await.unwrap();
    evil.start(None).await.unwrap();

    let server_addr = server.endpoint_ref().unwrap().addr();

    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("file.txt"), b"content").unwrap();

    server.add_share(
        "s1".to_string(), "Files".to_string(),
        tmp.path().to_string_lossy().to_string(), "space-1".to_string(),
    ).await;

    // Allow both + matching owner_did expectations.
    let mut allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert("space-1".to_string());
    allowed.insert(good.endpoint_id().to_string(), spaces.clone());
    allowed.insert(evil.endpoint_id().to_string(), spaces);
    server.set_allowed_peers(allowed).await;
    let mut owner_dids = HashMap::new();
    owner_dids.insert(good.endpoint_id().to_string(), good_identity.did.clone());
    owner_dids.insert(evil.endpoint_id().to_string(), evil_identity.did.clone());
    server.set_peer_owner_dids(owner_dids).await;

    let good_ep = good.endpoint_ref().unwrap().clone();
    let evil_ep = evil.endpoint_ref().unwrap().clone();

    // Both succeed
    assert!(raw_list(&good_ep, server_addr.clone(), &good_identity).await.is_ok());
    assert!(raw_list(&evil_ep, server_addr.clone(), &evil_identity).await.is_ok());

    // Revoke only evil — drop from both maps in lock-step.
    let mut new_allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert("space-1".to_string());
    new_allowed.insert(good.endpoint_id().to_string(), spaces);
    server.set_allowed_peers(new_allowed).await;
    let mut new_owner_dids = HashMap::new();
    new_owner_dids.insert(good.endpoint_id().to_string(), good_identity.did.clone());
    server.set_peer_owner_dids(new_owner_dids).await;
    sleep(Duration::from_millis(100)).await;

    // Evil denied, good still works
    let evil_result = raw_list(&evil_ep, server_addr.clone(), &evil_identity).await;
    let good_result = raw_list(&good_ep, server_addr, &good_identity).await;

    assert!(evil_result.is_err(), "evil should be denied");
    assert!(good_result.is_ok(), "good should still have access");

    let _ = good.stop().await;
    let _ = evil.stop().await;
    let _ = server.stop().await;
}

// Cross-space isolation is tested in unit tests (share_only_accessible_in_matching_space)
// since the raw protocol format differs from what raw_list sends.
