//! STAT request tests.

use super::common::*;
use haex_vault_lib::peer_storage::protocol::{Request, Response};

#[tokio::test]
async fn stat_file_returns_metadata() {
    let content = b"Hello, World! This is a test file.";
    let (mut server, client, addr, _tmp) =
        setup_server_client(&[("hello.txt", content)], &[], "StatTest", "space-1").await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let resp = send_request(
        &client_ep,
        addr,
        &Request::Stat {
            path: "/StatTest/hello.txt".to_string(),
            ucan_token: test_ucan_token("space-1"),
        },
    )
    .await
    .unwrap();

    match resp {
        Response::Stat { entry, chunks } => {
            assert_eq!(entry.name, "hello.txt");
            assert!(!entry.is_dir);
            assert_eq!(entry.size, content.len() as u64);
            assert!(entry.modified.is_some());
            // File stats carry the chunked BLAKE3 manifest so the receiver
            // can verify the bytes it pulls over the wire.
            let chunks = chunks.expect("file stat must include chunks");
            assert_eq!(chunks.file_hash, blake3::hash(content).to_hex().to_string());
        }
        other => panic!("Expected Stat, got: {:?}", other),
    }

    let _ = server.stop().await;
}

#[tokio::test]
async fn stat_directory_returns_metadata() {
    let (mut server, client, addr, _tmp) =
        setup_server_client(&[("subdir/file.txt", b"x")], &[], "StatDir", "space-1").await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let resp = send_request(
        &client_ep,
        addr,
        &Request::Stat {
            path: "/StatDir/subdir".to_string(),
            ucan_token: test_ucan_token("space-1"),
        },
    )
    .await
    .unwrap();

    match resp {
        Response::Stat { entry, chunks } => {
            assert_eq!(entry.name, "subdir");
            assert!(entry.is_dir);
            assert_eq!(chunks, None, "directory stats must omit chunks");
        }
        other => panic!("Expected Stat, got: {:?}", other),
    }

    let _ = server.stop().await;
}
