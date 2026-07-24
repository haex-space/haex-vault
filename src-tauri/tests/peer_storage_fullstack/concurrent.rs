//! Concurrent connection tests.

use std::collections::{HashMap, HashSet};

use super::common::*;
use haex_vault_lib::peer_storage::endpoint::PeerEndpoint;

#[tokio::test]
async fn concurrent_clients_can_connect() {
    let mut server = PeerEndpoint::new_ephemeral();
    let mut client1 = PeerEndpoint::new_ephemeral();
    let mut client2 = PeerEndpoint::new_ephemeral();

    let client_did = install_test_identities(&mut server, &mut [&mut client1, &mut client2]).await;

    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("shared.txt"), b"shared content").unwrap();

    server
        .add_share(
            "s1".to_string(),
            "Shared".to_string(),
            tmp.path().to_string_lossy().to_string(),
            test_space_id("space-1"),
        )
        .await;

    let mut allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert(test_space_id("space-1"));
    allowed.insert(client1.endpoint_id().to_string(), spaces.clone());
    allowed.insert(client2.endpoint_id().to_string(), spaces);
    server.set_allowed_peers(allowed).await;
    let mut owner_dids = HashMap::new();
    owner_dids.insert(client1.endpoint_id().to_string(), client_did.clone());
    owner_dids.insert(client2.endpoint_id().to_string(), client_did);
    server.set_peer_owner_dids(owner_dids).await;

    let server_addr = server.endpoint_ref().unwrap().addr();
    let ep1 = client1.endpoint_ref().unwrap().clone();
    let ep2 = client2.endpoint_ref().unwrap().clone();

    // Both clients connect and read simultaneously
    let (r1, r2) = tokio::join!(
        send_read_request(&ep1, server_addr.clone(), "/Shared/shared.txt", None),
        send_read_request(&ep2, server_addr, "/Shared/shared.txt", None),
    );

    let (_, data1) = r1.unwrap();
    let (_, data2) = r2.unwrap();

    assert_eq!(data1, b"shared content");
    assert_eq!(data2, b"shared content");

    let _ = server.stop().await;
}
