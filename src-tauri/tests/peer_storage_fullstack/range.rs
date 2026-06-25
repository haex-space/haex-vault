//! Byte-range edge cases.

use super::common::*;
use haex_vault_lib::peer_storage::protocol::Response;

#[tokio::test]
async fn range_beyond_file_size_is_clamped() {
    let content = b"short";
    let (mut server, client, addr, _tmp) =
        setup_server_client(&[("short.txt", content)], &[], "Clamp", "space-1").await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let (header, data) =
        send_read_request(&client_ep, addr, "/Clamp/short.txt", Some([0, 1_000_000]))
            .await
            .unwrap();

    match header {
        Response::ReadHeader { size } => {
            assert_eq!(size, content.len() as u64, "Should clamp to file size");
        }
        other => panic!("Expected ReadHeader, got: {:?}", other),
    }
    assert_eq!(data, content);

    server.stop().await.ok();
}

#[tokio::test]
async fn range_start_equals_end_returns_zero() {
    let (mut server, client, addr, _tmp) =
        setup_server_client(&[("hello.txt", b"hello")], &[], "ZeroRange", "space-1").await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let (header, data) = send_read_request(&client_ep, addr, "/ZeroRange/hello.txt", Some([3, 3]))
        .await
        .unwrap();

    match header {
        Response::ReadHeader { size } => assert_eq!(size, 0),
        other => panic!("Expected ReadHeader size=0, got: {:?}", other),
    }
    assert!(data.is_empty());

    server.stop().await.ok();
}
