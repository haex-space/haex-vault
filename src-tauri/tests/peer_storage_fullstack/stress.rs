//! Stress: bulk listing and rapid sequential requests.

use std::collections::HashSet;

use super::common::*;
use haex_vault_lib::peer_storage::protocol::{Request, Response};

#[tokio::test]
async fn listing_100_files() {
    let files: Vec<(String, Vec<u8>)> = (0..100)
        .map(|i| {
            (
                format!("file_{:03}.txt", i),
                format!("content_{i}").into_bytes(),
            )
        })
        .collect();
    let file_refs: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(n, c)| (n.as_str(), c.as_slice()))
        .collect();

    let (mut server, client, addr, _tmp) =
        setup_server_client(&file_refs, &[], "Bulk", "space-1").await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let resp = send_request(
        &client_ep,
        addr,
        &Request::List {
            path: "/Bulk".to_string(),
            ucan_token: test_ucan_token("space-1"),
        },
    )
    .await
    .unwrap();

    match resp {
        Response::List { entries } => {
            assert_eq!(entries.len(), 100);
            let names: HashSet<String> = entries.iter().map(|e| e.name.clone()).collect();
            assert_eq!(names.len(), 100, "All 100 filenames unique");
        }
        other => panic!("Expected 100 entries, got: {:?}", other),
    }

    server.stop().await.ok();
}

#[tokio::test]
async fn rapid_20_sequential_requests() {
    let (mut server, client, addr, _tmp) =
        setup_server_client(&[("data.txt", b"rapid")], &[], "Rapid", "space-1").await;

    let client_ep = client.endpoint_ref().unwrap().clone();

    for i in 0..20 {
        let resp = send_request(
            &client_ep,
            addr.clone(),
            &Request::List {
                path: "/".to_string(),
                ucan_token: test_ucan_token("space-1"),
            },
        )
        .await;
        assert!(resp.is_ok(), "Request {i}/20 failed: {:?}", resp.err());
    }

    for i in 0..10 {
        let (_, data) = send_read_request(&client_ep, addr.clone(), "/Rapid/data.txt", None)
            .await
            .unwrap_or_else(|e| panic!("Read {i}/10 failed: {e}"));
        assert_eq!(data, b"rapid");
    }

    server.stop().await.ok();
}
