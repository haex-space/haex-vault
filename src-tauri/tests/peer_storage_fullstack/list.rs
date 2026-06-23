//! LIST request tests.

use super::common::*;
use haex_vault_lib::peer_storage::protocol::{Request, Response};

#[tokio::test]
async fn list_root_shows_shared_folders() {
    let (mut server, client, addr, _tmp) =
        setup_server_client(&[("readme.txt", b"hello")], &[], "Documents", "space-1").await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let resp = send_request(
        &client_ep,
        addr,
        &Request::List {
            path: "/".to_string(),
            ucan_token: test_ucan_token("space-1"),
        },
    )
    .await
    .unwrap();

    match resp {
        Response::List { entries } => {
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].name, "Documents");
            assert!(entries[0].is_dir);
        }
        other => panic!("Expected List, got: {:?}", other),
    }

    let _ = server.stop().await;
}

#[tokio::test]
async fn list_share_shows_files_and_dirs() {
    let (mut server, client, addr, _tmp) = setup_server_client(
        &[
            ("file1.txt", b"content1"),
            ("file2.md", b"# Header"),
            ("subdir/nested.txt", b"nested"),
        ],
        &["emptydir"],
        "MyShare",
        "space-1",
    )
    .await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let resp = send_request(
        &client_ep,
        addr,
        &Request::List {
            path: "/MyShare".to_string(),
            ucan_token: test_ucan_token("space-1"),
        },
    )
    .await
    .unwrap();

    match resp {
        Response::List { entries } => {
            let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
            assert!(
                names.contains(&"file1.txt"),
                "missing file1.txt, got: {:?}",
                names
            );
            assert!(
                names.contains(&"file2.md"),
                "missing file2.md, got: {:?}",
                names
            );
            assert!(
                names.contains(&"subdir"),
                "missing subdir, got: {:?}",
                names
            );
            assert!(
                names.contains(&"emptydir"),
                "missing emptydir, got: {:?}",
                names
            );

            let file1 = entries.iter().find(|e| e.name == "file1.txt").unwrap();
            assert!(!file1.is_dir);
            assert_eq!(file1.size, 8); // "content1"

            let subdir = entries.iter().find(|e| e.name == "subdir").unwrap();
            assert!(subdir.is_dir);
        }
        other => panic!("Expected List, got: {:?}", other),
    }

    let _ = server.stop().await;
}

#[tokio::test]
async fn list_nested_directory() {
    let (mut server, client, addr, _tmp) = setup_server_client(
        &[
            ("deep/level1/level2/file.txt", b"deep file"),
            ("deep/level1/sibling.txt", b"sibling"),
        ],
        &[],
        "DeepShare",
        "space-1",
    )
    .await;

    let client_ep = client.endpoint_ref().unwrap().clone();

    // List /DeepShare/deep/level1
    let resp = send_request(
        &client_ep,
        addr,
        &Request::List {
            path: "/DeepShare/deep/level1".to_string(),
            ucan_token: test_ucan_token("space-1"),
        },
    )
    .await
    .unwrap();

    match resp {
        Response::List { entries } => {
            let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
            assert!(names.contains(&"level2"), "missing level2 dir");
            assert!(names.contains(&"sibling.txt"), "missing sibling.txt");
        }
        other => panic!("Expected List, got: {:?}", other),
    }

    let _ = server.stop().await;
}

#[tokio::test]
async fn list_nonexistent_path_returns_error() {
    let (mut server, client, addr, _tmp) =
        setup_server_client(&[("file.txt", b"x")], &[], "Share", "space-1").await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let resp = send_request(
        &client_ep,
        addr,
        &Request::List {
            path: "/Share/nonexistent".to_string(),
            ucan_token: test_ucan_token("space-1"),
        },
    )
    .await
    .unwrap();

    match resp {
        Response::Error { message } => {
            assert!(
                message.contains("not found") || message.contains("Not a directory"),
                "Unexpected error: {}",
                message
            );
        }
        other => panic!("Expected Error, got: {:?}", other),
    }

    let _ = server.stop().await;
}
