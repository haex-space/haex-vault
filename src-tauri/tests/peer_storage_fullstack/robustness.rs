//! Protocol robustness: malformed input, garbage data, oversized messages.

use tokio::time::{sleep, Duration};

use super::common::*;
use haex_vault_lib::peer_storage::protocol::{Request, ALPN};

#[tokio::test]
async fn malformed_json_request_does_not_crash_server() {
    let (mut server, client, server_addr, _tmp) =
        setup_server_client(&[("file.txt", b"x")], &[], "Robust", "space-1").await;

    let client_ep = client.endpoint_ref().unwrap().clone();

    // Send garbage instead of valid protocol message
    let conn = tokio::time::timeout(
        Duration::from_secs(5),
        client_ep.connect(server_addr.clone(), ALPN),
    )
    .await
    .unwrap()
    .unwrap();

    let (mut send, mut _recv) = conn.open_bi().await.unwrap();

    // Length prefix says 100 bytes, then send only 4 bytes of garbage
    send.write_all(&100u32.to_be_bytes()).await.unwrap();
    send.write_all(&[0xDE, 0xAD, 0xBE, 0xEF]).await.unwrap();
    send.finish().unwrap();

    // Server should not crash — connection just closes or returns error
    let _result = _recv.read_to_end(1024 * 1024).await;

    // Verify server is still alive by making a valid request
    let valid_resp = send_request(
        &client_ep,
        server_addr,
        &Request::List {
            path: "/".to_string(),
            ucan_token: test_ucan_token("space-1"),
        },
    )
    .await;
    assert!(
        valid_resp.is_ok(),
        "Server should still work after malformed request"
    );

    server.stop().await.ok();
}

#[tokio::test]
async fn oversized_length_prefix_is_rejected() {
    let (mut server, client, server_addr, _tmp) =
        setup_server_client(&[("file.txt", b"x")], &[], "Oversize", "space-1").await;

    let client_ep = client.endpoint_ref().unwrap().clone();

    let conn = tokio::time::timeout(
        Duration::from_secs(5),
        client_ep.connect(server_addr.clone(), ALPN),
    )
    .await
    .unwrap()
    .unwrap();

    let (mut send, mut _recv) = conn.open_bi().await.unwrap();

    // Claim 100 MB message (exceeds MAX_REQUEST_SIZE of 1 MB)
    send.write_all(&(100 * 1024 * 1024u32).to_be_bytes())
        .await
        .unwrap();
    send.write_all(b"{}").await.unwrap();
    send.finish().unwrap();

    let _result = _recv.read_to_end(1024 * 1024).await;

    // Server still alive
    let valid = send_request(
        &client_ep,
        server_addr,
        &Request::List {
            path: "/".to_string(),
            ucan_token: test_ucan_token("space-1"),
        },
    )
    .await;
    assert!(valid.is_ok(), "Server must survive oversized requests");

    server.stop().await.ok();
}

#[tokio::test]
async fn empty_stream_does_not_crash() {
    let (mut server, client, server_addr, _tmp) =
        setup_server_client(&[("file.txt", b"x")], &[], "Empty", "space-1").await;

    let client_ep = client.endpoint_ref().unwrap().clone();

    let conn = tokio::time::timeout(
        Duration::from_secs(5),
        client_ep.connect(server_addr.clone(), ALPN),
    )
    .await
    .unwrap()
    .unwrap();

    let (mut send, _recv) = conn.open_bi().await.unwrap();
    // Send nothing and close
    send.finish().unwrap();

    sleep(Duration::from_millis(100)).await;

    // Server still alive
    let valid = send_request(
        &client_ep,
        server_addr,
        &Request::List {
            path: "/".to_string(),
            ucan_token: test_ucan_token("space-1"),
        },
    )
    .await;
    assert!(valid.is_ok(), "Server must survive empty streams");

    server.stop().await.ok();
}
