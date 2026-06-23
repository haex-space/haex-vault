//! Security: path traversal and cross-space isolation.

use std::collections::{HashMap, HashSet};

use super::common::*;
use haex_vault_lib::peer_storage::endpoint::PeerEndpoint;
use haex_vault_lib::peer_storage::protocol::{Request, Response};

#[tokio::test]
async fn path_traversal_is_blocked() {
    let (mut server, client, addr, _tmp) =
        setup_server_client(&[("safe.txt", b"safe")], &[], "Secure", "space-1").await;

    let client_ep = client.endpoint_ref().unwrap().clone();

    // Try to escape the share with ../
    let resp = send_request(
        &client_ep,
        addr,
        &Request::List {
            path: "/Secure/../../../etc".to_string(),
            ucan_token: test_ucan_token("space-1"),
        },
    )
    .await
    .unwrap();

    match resp {
        Response::Error { message } => {
            assert!(
                message.contains("denied")
                    || message.contains("outside")
                    || message.contains("not found"),
                "Path traversal should be blocked, got: {}",
                message,
            );
        }
        other => panic!("Path traversal should return Error, got: {:?}", other),
    }

    let _ = server.stop().await;
}

#[tokio::test]
async fn cross_space_isolation() {
    let mut server = PeerEndpoint::new_ephemeral();
    let mut client = PeerEndpoint::new_ephemeral();

    let client_did = install_test_identities(&mut server, &mut [&mut client]).await;

    let tmp1 = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp1.path().join("public.txt"), b"public").unwrap();

    let tmp2 = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp2.path().join("secret.txt"), b"secret").unwrap();

    // Add two shares in different spaces
    server
        .add_share(
            "s1".to_string(),
            "Public".to_string(),
            tmp1.path().to_string_lossy().to_string(),
            "space-public".to_string(),
        )
        .await;
    server
        .add_share(
            "s2".to_string(),
            "Private".to_string(),
            tmp2.path().to_string_lossy().to_string(),
            "space-private".to_string(),
        )
        .await;

    // Client only has access to space-public
    let mut allowed = HashMap::new();
    let mut spaces = HashSet::new();
    spaces.insert("space-public".to_string());
    allowed.insert(client.endpoint_id().to_string(), spaces);
    server.set_allowed_peers(allowed).await;
    let mut owner_dids = HashMap::new();
    owner_dids.insert(client.endpoint_id().to_string(), client_did);
    server.set_peer_owner_dids(owner_dids).await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let server_addr = server.endpoint_ref().unwrap().addr();

    // List root — should only show Public. UCAN claims space-public (the
    // space the peer is registered in); the new per-UCAN-space gate now
    // requires this to overlap with allowed_spaces.
    let resp = send_request(
        &client_ep,
        server_addr.clone(),
        &Request::List {
            path: "/".to_string(),
            ucan_token: test_ucan_token("space-public"),
        },
    )
    .await
    .unwrap();
    match resp {
        Response::List { entries } => {
            let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
            assert!(names.contains(&"Public"), "Should see Public share");
            assert!(
                !names.contains(&"Private"),
                "Should NOT see Private share, got: {:?}",
                names
            );
        }
        other => panic!("Expected List, got: {:?}", other),
    }

    // Try to access Private directly with the space-public UCAN — should
    // fail at the UCAN capability check (UCAN has no rights for space-private).
    let resp = send_request(
        &client_ep,
        server_addr,
        &Request::List {
            path: "/Private".to_string(),
            ucan_token: test_ucan_token("space-public"),
        },
    )
    .await
    .unwrap();
    match resp {
        Response::Error { .. } => { /* expected */ }
        other => panic!("Accessing Private share should fail, got: {:?}", other),
    }

    let _ = server.stop().await;
}
