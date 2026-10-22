//! The frame of the admission (spec 057 T020, FR-031, FR-032): who is cut when.

use std::time::Duration;

use super::*;
use crate::extensions::peers::test_support::{exchange_cards, Admit, Vault, LIMIT};
use crate::sync::foreign_auth::Trust;

/// A device of a vault that is no contact proves itself and is cut right away: no public lobby
/// is open on this delivery.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_device_that_is_not_verified_is_cut_after_the_exchange() {
    let host = Vault::start(Admit::Real(Duration::from_secs(30))).await;
    let stranger = Vault::start(Admit::AtOnce).await;

    let (connection, _send, _recv, verified) = stranger.dial(&host).await;
    // The host cuts the connection right after its proof, which may or may not still arrive.
    if let Ok(verified) = verified {
        assert_eq!(verified.trust, Trust::Unverified);
    }
    let closed = tokio::time::timeout(LIMIT, connection.closed())
        .await
        .expect("cut long before the admission limit");
    assert!(
        matches!(&closed, iroh::endpoint::ConnectionError::ApplicationClosed(close) if close.reason.is_empty()),
        "closed without a reason: {closed:?}"
    );
    host.shutdown().await;
    stranger.shutdown().await;
}

/// A contact that asks for nothing is cut once the admission limit passed, not before.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_contact_that_asks_for_nothing_is_cut_after_the_limit() {
    let limit = Duration::from_millis(300);
    let host = Vault::start(Admit::Real(limit)).await;
    let guest = Vault::start(Admit::AtOnce).await;
    exchange_cards(&host, &guest).await;

    let (connection, _send, _recv, verified) = guest.dial(&host).await;
    let started = std::time::Instant::now();
    assert!(matches!(
        verified.expect("verified").trust,
        Trust::Contact { .. }
    ));
    tokio::time::timeout(LIMIT, connection.closed())
        .await
        .expect("cut in time");
    assert!(
        started.elapsed() >= limit / 2,
        "cut after {:?}, before the limit of {limit:?}",
        started.elapsed()
    );
    host.shutdown().await;
    guest.shutdown().await;
}

/// A device that does not even open the first stream is dropped within the exchange's limit.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_device_that_says_nothing_is_dropped_within_the_exchange_limit() {
    let host = Vault::start(Admit::Real(Duration::from_secs(30))).await;
    let mute = Vault::start(Admit::AtOnce).await;

    let connection = mute
        .node
        .dial_alpn(host.node.addr(), crate::sync::wire::EXT_PEER_ALPN)
        .await
        .expect("dial");
    tokio::time::timeout(
        foreign_auth::HANDSHAKE_LIMIT + Duration::from_secs(5),
        connection.closed(),
    )
    .await
    .expect("dropped after the exchange's limit");
    host.shutdown().await;
    mute.shutdown().await;
}
