//! E2E integration tests for P2P Storage access control.
//!
//! Tests two real iroh endpoints connecting on localhost.
//! Server shares folders, client tries to access them.
//! Verifies grant, revoke, cross-space isolation.
//!
//! Run: cargo test --test peer_storage_e2e

use std::collections::{HashMap, HashSet};
use tokio::time::{sleep, Duration};

use iroh::Endpoint;
use haex_vault_lib::peer_storage::endpoint::PeerEndpoint;
use haex_vault_lib::peer_storage::protocol::ALPN;

/// Create two endpoints that can discover each other on localhost
async fn create_connected_pair() -> (PeerEndpoint, PeerEndpoint, iroh::EndpointAddr) {
    let mut server = PeerEndpoint::new_ephemeral();
    let mut client = PeerEndpoint::new_ephemeral();

    server.start().await.unwrap();
    client.start().await.unwrap();

    // Get server's full address (ID + local socket addrs) for direct connection
    let server_addr = server.endpoint_ref().unwrap().addr();

    (server, client, server_addr)
}

/// Low-level: connect to a server and send a LIST request, return the response
async fn raw_list(client_ep: &Endpoint, server_addr: iroh::EndpointAddr) -> Result<String, String> {
    let conn = tokio::time::timeout(
        Duration::from_secs(5),
        client_ep.connect(server_addr, ALPN),
    )
    .await
    .map_err(|_| "connect timeout".to_string())?
    .map_err(|e| format!("connect error: {e}"))?;

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
    let (mut server, mut client, server_addr) = create_connected_pair().await;

    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("test.txt"), b"hello").unwrap();

    server.add_share(
        "s1".to_string(), "Test".to_string(),
        tmp.path().to_path_buf(), "space-1".to_string(),
    ).await;

    // Allow client
    let mut allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert("space-1".to_string());
    allowed.insert(client.endpoint_id().to_string(), spaces);
    server.set_allowed_peers(allowed).await;

    // Connect and send LIST
    let client_ep = client.endpoint_ref().unwrap().clone();
    let result = raw_list(&client_ep, server_addr).await;

    assert!(result.is_ok(), "allowed peer should get a response, got: {:?}", result);

    let _ = client.stop().await;
    let _ = server.stop().await;
}

#[tokio::test]
async fn unknown_peer_is_rejected() {
    let (mut server, mut attacker, server_addr) = create_connected_pair().await;

    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("secret.txt"), b"secret").unwrap();

    server.add_share(
        "s1".to_string(), "Secrets".to_string(),
        tmp.path().to_path_buf(), "space-1".to_string(),
    ).await;

    // No peers allowed
    server.set_allowed_peers(HashMap::new()).await;

    let attacker_ep = attacker.endpoint_ref().unwrap().clone();
    let result = raw_list(&attacker_ep, server_addr).await;

    assert!(result.is_err(), "unknown peer must be rejected, got: {:?}", result);

    let _ = attacker.stop().await;
    let _ = server.stop().await;
}

#[tokio::test]
async fn access_revoked_mid_session() {
    let (mut server, mut client, server_addr) = create_connected_pair().await;

    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("data.txt"), b"secret").unwrap();

    server.add_share(
        "s1".to_string(), "Data".to_string(),
        tmp.path().to_path_buf(), "space-1".to_string(),
    ).await;

    // Allow client
    let mut allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert("space-1".to_string());
    allowed.insert(client.endpoint_id().to_string(), spaces);
    server.set_allowed_peers(allowed).await;

    let client_ep = client.endpoint_ref().unwrap().clone();

    // First request succeeds
    let result = raw_list(&client_ep, server_addr.clone()).await;
    assert!(result.is_ok(), "should succeed before revocation");

    // Revoke
    server.set_allowed_peers(HashMap::new()).await;
    sleep(Duration::from_millis(100)).await;

    // Second request should fail
    let result = raw_list(&client_ep, server_addr).await;
    assert!(result.is_err(), "should fail after revocation, got: {:?}", result);

    let _ = client.stop().await;
    let _ = server.stop().await;
}

#[tokio::test]
async fn partial_revoke_only_blocks_target() {
    let mut server = PeerEndpoint::new_ephemeral();
    let mut good = PeerEndpoint::new_ephemeral();
    let mut evil = PeerEndpoint::new_ephemeral();

    server.start().await.unwrap();
    good.start().await.unwrap();
    evil.start().await.unwrap();

    let server_addr = server.endpoint_ref().unwrap().addr();

    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("file.txt"), b"content").unwrap();

    server.add_share(
        "s1".to_string(), "Files".to_string(),
        tmp.path().to_path_buf(), "space-1".to_string(),
    ).await;

    // Allow both
    let mut allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert("space-1".to_string());
    allowed.insert(good.endpoint_id().to_string(), spaces.clone());
    allowed.insert(evil.endpoint_id().to_string(), spaces);
    server.set_allowed_peers(allowed).await;

    let good_ep = good.endpoint_ref().unwrap().clone();
    let evil_ep = evil.endpoint_ref().unwrap().clone();

    // Both succeed
    assert!(raw_list(&good_ep, server_addr.clone()).await.is_ok());
    assert!(raw_list(&evil_ep, server_addr.clone()).await.is_ok());

    // Revoke only evil
    let mut new_allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert("space-1".to_string());
    new_allowed.insert(good.endpoint_id().to_string(), spaces);
    server.set_allowed_peers(new_allowed).await;
    sleep(Duration::from_millis(100)).await;

    // Evil denied, good still works
    let evil_result = raw_list(&evil_ep, server_addr.clone()).await;
    let good_result = raw_list(&good_ep, server_addr).await;

    assert!(evil_result.is_err(), "evil should be denied");
    assert!(good_result.is_ok(), "good should still have access");

    let _ = good.stop().await;
    let _ = evil.stop().await;
    let _ = server.stop().await;
}

// Cross-space isolation is tested in unit tests (share_only_accessible_in_matching_space)
// since the raw protocol format differs from what raw_list sends.
