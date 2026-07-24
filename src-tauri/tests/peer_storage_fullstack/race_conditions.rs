//! Race-condition tests: share removed mid-session, files deleted on disk.

use std::collections::{HashMap, HashSet};
use tokio::time::{sleep, Duration};

use super::common::*;
use haex_vault_lib::peer_storage::endpoint::PeerEndpoint;
use haex_vault_lib::peer_storage::protocol::{Request, Response};

#[tokio::test]
async fn share_removed_while_client_browsing() {
    let mut server = PeerEndpoint::new_ephemeral();
    let mut client = PeerEndpoint::new_ephemeral();
    let client_did = install_test_identities(&mut server, &mut [&mut client]).await;

    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("data.txt"), b"important").unwrap();
    server
        .add_share(
            "s1".to_string(),
            "Volatile".to_string(),
            tmp.path().to_string_lossy().to_string(),
            test_space_id("space-1"),
        )
        .await;

    let mut allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert(test_space_id("space-1"));
    allowed.insert(client.endpoint_id().to_string(), spaces);
    server.set_allowed_peers(allowed).await;
    let mut owner_dids = HashMap::new();
    owner_dids.insert(client.endpoint_id().to_string(), client_did);
    server.set_peer_owner_dids(owner_dids).await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let server_addr = server.endpoint_ref().unwrap().addr();

    // First access succeeds
    let resp = send_request(
        &client_ep,
        server_addr.clone(),
        &Request::List {
            path: "/Volatile".to_string(),
            ucan_token: test_ucan_token("space-1"),
        },
    )
    .await
    .unwrap();
    match &resp {
        Response::List { entries } => assert_eq!(entries.len(), 1),
        other => panic!("Expected List, got: {:?}", other),
    }

    // Remove the share mid-session
    server.remove_share("s1").await;
    sleep(Duration::from_millis(50)).await;

    // Root listing should be empty
    let resp = send_request(
        &client_ep,
        server_addr,
        &Request::List {
            path: "/".to_string(),
            ucan_token: test_ucan_token("space-1"),
        },
    )
    .await
    .unwrap();
    match resp {
        Response::List { entries } => {
            assert!(
                !entries.iter().any(|e| e.name == "Volatile"),
                "Removed share must not appear"
            );
        }
        other => panic!("Expected List, got: {:?}", other),
    }

    server.stop().await.ok();
}

#[tokio::test]
async fn file_deleted_on_disk_between_list_and_read() {
    let (mut server, client, addr, tmp) = setup_server_client(
        &[("keep.txt", b"stays"), ("gone.txt", b"vanishes")],
        &[],
        "DiskRace",
        "space-1",
    )
    .await;

    let client_ep = client.endpoint_ref().unwrap().clone();

    // Listing shows both
    let resp = send_request(
        &client_ep,
        addr.clone(),
        &Request::List {
            path: "/DiskRace".to_string(),
            ucan_token: test_ucan_token("space-1"),
        },
    )
    .await
    .unwrap();
    match &resp {
        Response::List { entries } => assert_eq!(entries.len(), 2),
        other => panic!("Expected 2 entries, got: {:?}", other),
    }

    // Delete from disk (external process)
    std::fs::remove_file(tmp.path().join("gone.txt")).unwrap();

    // Reading the deleted file should error, not crash
    let (header, _) = send_read_request(&client_ep, addr.clone(), "/DiskRace/gone.txt", None)
        .await
        .unwrap();
    match header {
        Response::Error { .. } => { /* expected */ }
        other => panic!("Reading deleted file should fail, got: {:?}", other),
    }

    // Other file still works
    let (_, data) = send_read_request(&client_ep, addr, "/DiskRace/keep.txt", None)
        .await
        .unwrap();
    assert_eq!(data, b"stays");

    server.stop().await.ok();
}
