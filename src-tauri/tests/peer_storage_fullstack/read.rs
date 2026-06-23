//! READ request tests.

use super::common::*;
use haex_vault_lib::peer_storage::protocol::Response;

#[tokio::test]
async fn read_small_file() {
    let content = b"Hello, P2P World!";
    let (mut server, client, addr, _tmp) =
        setup_server_client(&[("greeting.txt", content)], &[], "ReadTest", "space-1").await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let (header, data) = send_read_request(&client_ep, addr, "/ReadTest/greeting.txt", None)
        .await
        .unwrap();

    match header {
        Response::ReadHeader { size } => {
            assert_eq!(size, content.len() as u64);
        }
        other => panic!("Expected ReadHeader, got: {:?}", other),
    }

    assert_eq!(data, content);

    let _ = server.stop().await;
}

#[tokio::test]
async fn read_large_file_chunked() {
    // 256 KB file — will be sent in multiple 64 KB chunks
    let content: Vec<u8> = (0..256 * 1024).map(|i| (i % 256) as u8).collect();
    let (mut server, client, addr, _tmp) =
        setup_server_client(&[("large.bin", &content)], &[], "LargeFile", "space-1").await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let (header, data) = send_read_request(&client_ep, addr, "/LargeFile/large.bin", None)
        .await
        .unwrap();

    match header {
        Response::ReadHeader { size } => {
            assert_eq!(size, content.len() as u64);
        }
        other => panic!("Expected ReadHeader, got: {:?}", other),
    }

    assert_eq!(data.len(), content.len());
    assert_eq!(data, content);

    let _ = server.stop().await;
}

#[tokio::test]
async fn read_with_byte_range() {
    let content = b"0123456789ABCDEF";
    let (mut server, client, addr, _tmp) =
        setup_server_client(&[("range.txt", content)], &[], "RangeTest", "space-1").await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    // Read bytes 4..12 (8 bytes: "4567890A" — wait, "89AB")
    let (header, data) = send_read_request(&client_ep, addr, "/RangeTest/range.txt", Some([4, 12]))
        .await
        .unwrap();

    match header {
        Response::ReadHeader { size } => {
            assert_eq!(size, 8); // 12 - 4
        }
        other => panic!("Expected ReadHeader, got: {:?}", other),
    }

    assert_eq!(data, b"456789AB");

    let _ = server.stop().await;
}

#[tokio::test]
async fn read_nonexistent_file_returns_error() {
    let (mut server, client, addr, _tmp) =
        setup_server_client(&[("exists.txt", b"x")], &[], "ReadErr", "space-1").await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let (header, _) = send_read_request(&client_ep, addr, "/ReadErr/missing.txt", None)
        .await
        .unwrap();

    match header {
        Response::Error { message } => {
            assert!(
                message.contains("not found") || message.contains("Not a file"),
                "Unexpected error: {}",
                message
            );
        }
        other => panic!("Expected Error, got: {:?}", other),
    }

    let _ = server.stop().await;
}

#[tokio::test]
async fn read_directory_returns_error() {
    let (mut server, client, addr, _tmp) =
        setup_server_client(&[("subdir/file.txt", b"x")], &[], "ReadDir", "space-1").await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let (header, _) = send_read_request(&client_ep, addr, "/ReadDir/subdir", None)
        .await
        .unwrap();

    match header {
        Response::Error { message } => {
            assert!(
                message.contains("Not a file"),
                "Unexpected error: {}",
                message
            );
        }
        other => panic!("Expected Error, got: {:?}", other),
    }

    let _ = server.stop().await;
}
