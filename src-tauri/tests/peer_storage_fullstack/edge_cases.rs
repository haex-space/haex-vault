//! Edge cases: empty files, special filenames, deep nesting.

use super::common::*;
use haex_vault_lib::peer_storage::protocol::{Request, Response};

#[tokio::test]
async fn empty_file_read_returns_zero_bytes() {
    let (mut server, client, addr, _tmp) =
        setup_server_client(&[("empty.txt", b"")], &[], "EmptyFile", "space-1").await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let (header, data) = send_read_request(&client_ep, addr, "/EmptyFile/empty.txt", None)
        .await
        .unwrap();

    match header {
        Response::ReadHeader { size } => assert_eq!(size, 0),
        other => panic!("Expected ReadHeader, got: {:?}", other),
    }
    assert!(data.is_empty());

    server.stop().await.ok();
}

#[tokio::test]
async fn filenames_with_spaces_and_special_chars() {
    let (mut server, client, addr, _tmp) = setup_server_client(
        &[
            ("file with spaces.txt", b"spaces"),
            ("file-with-dashes.txt", b"dashes"),
            ("file_under_scores.txt", b"underscores"),
            ("file.multiple.dots.txt", b"dots"),
        ],
        &[],
        "SpecialNames",
        "space-1",
    )
    .await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let resp = send_request(
        &client_ep,
        addr.clone(),
        &Request::List {
            path: "/SpecialNames".to_string(),
            ucan_token: test_ucan_token("space-1"),
        },
    )
    .await
    .unwrap();
    match &resp {
        Response::List { entries } => {
            assert_eq!(entries.len(), 4);
            let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
            assert!(names.contains(&"file with spaces.txt"));
            assert!(names.contains(&"file.multiple.dots.txt"));
        }
        other => panic!("Expected List, got: {:?}", other),
    }

    // Read file with spaces in name
    let (_, data) = send_read_request(&client_ep, addr, "/SpecialNames/file with spaces.txt", None)
        .await
        .unwrap();
    assert_eq!(data, b"spaces");

    server.stop().await.ok();
}

#[tokio::test]
async fn deeply_nested_10_levels() {
    let mut path = String::new();
    for i in 0..10 {
        if !path.is_empty() {
            path.push('/');
        }
        path.push_str(&format!("level{i}"));
    }
    let file_path = format!("{path}/deep.txt");

    let (mut server, client, addr, _tmp) =
        setup_server_client(&[(&file_path, b"found me!")], &[], "DeepNest", "space-1").await;

    let client_ep = client.endpoint_ref().unwrap().clone();

    // Read the deep file
    let deep_file = format!("/DeepNest/{file_path}");
    let (header, data) = send_read_request(&client_ep, addr, &deep_file, None)
        .await
        .unwrap();
    match header {
        Response::ReadHeader { size } => assert_eq!(size, 9), // "found me!"
        other => panic!("Expected ReadHeader, got: {:?}", other),
    }
    assert_eq!(data, b"found me!");

    server.stop().await.ok();
}

#[tokio::test]
async fn empty_directory_listing_returns_zero_entries() {
    let (mut server, client, addr, _tmp) =
        setup_server_client(&[], &["emptydir"], "EmptyDir", "space-1").await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let resp = send_request(
        &client_ep,
        addr,
        &Request::List {
            path: "/EmptyDir/emptydir".to_string(),
            ucan_token: test_ucan_token("space-1"),
        },
    )
    .await
    .unwrap();
    match resp {
        Response::List { entries } => assert!(entries.is_empty()),
        other => panic!("Expected empty List, got: {:?}", other),
    }

    server.stop().await.ok();
}
