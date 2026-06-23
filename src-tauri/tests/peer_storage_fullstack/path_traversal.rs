//! Path traversal: comprehensive attack vectors.

use super::common::*;
use haex_vault_lib::peer_storage::protocol::{Request, Response};

#[tokio::test]
async fn path_traversal_attack_vectors() {
    let (mut server, client, addr, _tmp) =
        setup_server_client(&[("safe.txt", b"safe")], &[], "Fort", "space-1").await;

    let client_ep = client.endpoint_ref().unwrap().clone();

    let attacks = [
        "/Fort/../../../etc/passwd",
        "/Fort/../../..",
        "/Fort/./../../etc/shadow",
        "/Fort/....//....//etc/passwd",
        "/Fort/subdir/../../..",
    ];

    for path in &attacks {
        let resp = send_request(
            &client_ep,
            addr.clone(),
            &Request::List {
                path: path.to_string(),
                ucan_token: test_ucan_token("space-1"),
            },
        )
        .await;
        match resp {
            Ok(Response::Error { .. }) => { /* blocked */ }
            Ok(Response::List { entries }) => {
                for entry in &entries {
                    assert!(
                        entry.name != "passwd" && entry.name != "shadow" && entry.name != "etc",
                        "Path traversal '{}' leaked: '{}'",
                        path,
                        entry.name,
                    );
                }
            }
            Err(_) => { /* connection closed — also fine */ }
            other => panic!("Unexpected for '{}': {:?}", path, other),
        }
    }

    // Also attack via READ
    for path in &attacks {
        let read_path = path.replace("etc/passwd", "etc/hostname");
        let result = send_read_request(&client_ep, addr.clone(), &read_path, None).await;
        match result {
            Ok((Response::Error { .. }, _)) => { /* blocked */ }
            Ok((Response::ReadHeader { .. }, data)) => {
                // If somehow it read something, it must be from within the share
                assert!(
                    data.is_empty() || data == b"safe",
                    "Path traversal READ leaked data for '{}'",
                    path
                );
            }
            Err(_) => { /* connection error — fine */ }
            other => panic!("Unexpected READ for '{}': {:?}", path, other),
        }
    }

    server.stop().await.ok();
}
