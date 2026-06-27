//! End-to-end coverage for Phase 2 DoS-defence layer enforcement.
//!
//! The pure decision logic (`accept_decision`, `StreamCounterGuard`) is
//! covered by the unit tests in `peer_storage::endpoint::lifecycle::phase2_tests`.
//! These tests verify the wiring across a real connection: that L2 cap
//! enforcement actually closes the connection and that L3 timeout aborts
//! a stalled handshake.

use std::time::Duration;

use super::common::*;
use haex_vault_lib::peer_storage::protocol::ALPN;
use haex_vault_lib::space_delivery::local::dos_defence::config::DosDefenceConfig;

/// L2 — when a connection holds more in-flight stream tasks than the
/// configured cap, the server closes the whole connection rather than
/// rejecting individual streams. With cap=2 and three stalled streams,
/// the connection must close within a short window.
#[tokio::test]
async fn l2_cap_closes_connection_when_in_flight_streams_exceed_cap() {
    let (mut server, client, addr, _tmp) =
        setup_server_client(&[("data.txt", b"x")], &[], "L2", "space-1").await;

    // Override the loose default — install a tight L2 cap for this test.
    server
        .set_dos_config(DosDefenceConfig {
            l2_max_streams_per_conn: 2,
            ..loose_dos_config()
        })
        .await;

    let client_ep = client.endpoint_ref().unwrap().clone();
    let conn = connect_and_handshake(&client_ep, addr).await.unwrap();

    // Open two bi-streams and send a single byte so the server's
    // `accept_bi` actually resolves (QUIC `open_bi` is lazy on the wire
    // until data arrives). The server's `handle_stream` then blocks
    // trying to read the rest of the length prefix, pinning both
    // in-flight slots.
    let (mut s1, _r1) = conn.open_bi().await.unwrap();
    s1.write_all(&[0u8]).await.unwrap();
    let (mut s2, _r2) = conn.open_bi().await.unwrap();
    s2.write_all(&[0u8]).await.unwrap();

    // Give the server accept-loop time to bump the in-flight counter for
    // both streams before we open the third.
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Third stream — server hits the cap and closes the connection.
    let (mut s3, _r3) = conn.open_bi().await.unwrap();
    let _ = s3.write_all(&[0u8]).await; // may already err on conn close

    // Wait for the connection to close. iroh wraps quinn so a server
    // `conn.close(code, reason)` surfaces here as `closed()` resolving.
    let close_reason = tokio::time::timeout(Duration::from_secs(2), conn.closed())
        .await
        .expect("connection did not close within 2s after L2 cap hit");

    let reason_str = format!("{close_reason:?}");
    assert!(
        reason_str.contains("stream cap") || reason_str.contains(" 9"),
        "expected stream-cap close (code 9 / reason 'stream cap exceeded'), got: {reason_str}"
    );

    server.stop().await.ok();
}

/// L3 — when a peer opens the server-initiated DID-auth stream but
/// never sends a response, the server must abort the handshake after
/// the configured timeout and close the connection. We exercise this
/// by accepting the stream client-side and dropping the responder.
#[tokio::test]
async fn l3_handshake_timeout_closes_connection_when_peer_stalls() {
    // Use `setup_server_client` so the server has `allowed_peers` +
    // `peer_owner_dids` wired — without those the accept-loop rejects
    // the connection before the auth stream is ever opened, which would
    // mask the L3 timeout we're trying to observe.
    let (mut server, client, addr, _tmp) =
        setup_server_client(&[("data.txt", b"x")], &[], "L3", "space-1").await;

    // Tight L3 timeout — 200ms is short enough to keep the test fast but
    // long enough that scheduling jitter doesn't false-positive on a
    // legitimate slow handshake.
    server
        .set_dos_config(DosDefenceConfig {
            l3_handshake_timeout: Duration::from_millis(200),
            ..loose_dos_config()
        })
        .await;

    let client_ep = client.endpoint_ref().unwrap().clone();

    let conn = client_ep.connect(addr, ALPN).await.unwrap();

    // Accept the server-initiated auth bi-stream so the server's
    // `open_bi` resolves and `challenge_and_verify` starts. Then drop
    // the responder without ever signing — the server should bail out
    // after `l3_handshake_timeout`.
    let _auth_stream = conn
        .accept_bi()
        .await
        .expect("server did not open the auth bi-stream");

    // Server `conn.close(7u32.into(), b"handshake timeout")` must fire
    // within ~200ms; allow generous slack for runtime scheduling.
    let close_reason = tokio::time::timeout(Duration::from_secs(2), conn.closed())
        .await
        .expect("connection did not close within 2s after L3 timeout");

    let reason_str = format!("{close_reason:?}");
    assert!(
        reason_str.contains("handshake timeout") || reason_str.contains(" 7"),
        "expected handshake-timeout close (code 7 / reason 'handshake timeout'), got: {reason_str}"
    );

    server.stop().await.ok();
}
