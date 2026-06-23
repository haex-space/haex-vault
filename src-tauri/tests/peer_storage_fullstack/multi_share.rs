//! Multi-share tests: multiple shares served from the same space.

use std::collections::{HashMap, HashSet};

use super::common::*;
use haex_vault_lib::peer_storage::endpoint::PeerEndpoint;
use haex_vault_lib::peer_storage::protocol::{Request, Response};

#[tokio::test]
async fn multiple_shares_in_same_space() {
    let mut server = PeerEndpoint::new_ephemeral();
    let mut client = PeerEndpoint::new_ephemeral();

    let client_did = install_test_identities(&mut server, &mut [&mut client]).await;

    let tmp1 = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp1.path().join("doc.txt"), b"document").unwrap();

    let tmp2 = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp2.path().join("photo.jpg"), b"\xFF\xD8\xFF\xE0").unwrap();

    server
        .add_share(
            "s1".to_string(),
            "Documents".to_string(),
            tmp1.path().to_string_lossy().to_string(),
            "shared-space".to_string(),
        )
        .await;
    server
        .add_share(
            "s2".to_string(),
            "Photos".to_string(),
            tmp2.path().to_string_lossy().to_string(),
            "shared-space".to_string(),
        )
        .await;

    let mut allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert("shared-space".to_string());
    allowed.insert(client.endpoint_id().to_string(), spaces);
    server.set_allowed_peers(allowed).await;
    let mut owner_dids = HashMap::new();
    owner_dids.insert(client.endpoint_id().to_string(), client_did);
    server.set_peer_owner_dids(owner_dids).await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let server_addr = server.endpoint_ref().unwrap().addr();

    // List root — should show both shares. UCAN claims shared-space (the
    // peer's only authorized space).
    let resp = send_request(
        &client_ep,
        server_addr.clone(),
        &Request::List {
            path: "/".to_string(),
            ucan_token: test_ucan_token("shared-space"),
        },
    )
    .await
    .unwrap();
    match resp {
        Response::List { entries } => {
            assert_eq!(entries.len(), 2);
            let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
            assert!(names.contains(&"Documents"));
            assert!(names.contains(&"Photos"));
        }
        other => panic!("Expected List, got: {:?}", other),
    }

    // Read file from each share
    let (_, doc_data) = send_read_request_for_space(
        &client_ep,
        server_addr.clone(),
        "/Documents/doc.txt",
        None,
        "shared-space",
    )
    .await
    .unwrap();
    assert_eq!(doc_data, b"document");

    let (_, photo_data) = send_read_request_for_space(
        &client_ep,
        server_addr,
        "/Photos/photo.jpg",
        None,
        "shared-space",
    )
    .await
    .unwrap();
    assert_eq!(photo_data, b"\xFF\xD8\xFF\xE0");

    let _ = server.stop().await;
}
