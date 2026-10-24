//! The admission (spec 057 T020, T053, FR-031–FR-034, ADR 0016): who is cut when, and who may
//! join while a public lobby is open.

use std::time::Duration;

use serde_json::Value;

use super::*;
use crate::extensions::peers::lobby::{close, Visibility, JOINED};
use crate::extensions::peers::test_support::{exchange_cards, probe, Admit, Vault, Window, LIMIT};
use crate::extensions::peers::throttle::ATTEMPTS_PER_ENDPOINT;
use crate::extensions::peers::wire::{join_proof, JoinCode, VERSION};
use crate::extensions::peers::PeerError;
use crate::sync::foreign_auth::Trust;

fn joined(window: &Window) -> Vec<Value> {
    window
        .frame_events(JOINED)
        .into_iter()
        .map(|(_, _, data)| data)
        .collect()
}

/// A stranger: a vault that is no contact of `host` and knows where it listens.
async fn stranger_of(host: &Vault) -> Vault {
    let stranger = Vault::start(Admit::AtOnce).await;
    stranger.learn(host).await;
    stranger
}

/// Sends `message` on the first stream of a fresh connection from `from` to `to`, after the
/// exchange; whatever comes back before the connection ends.
async fn send_after_exchange(
    from: &Vault,
    to: &Vault,
    message: impl FnOnce(&crate::sync::foreign_auth::Verified) -> Message,
) -> Option<Message> {
    let (connection, mut send, mut recv, verified) = from.dial(to).await;
    let verified = verified.expect("the exchange passes");
    // The host may reject the request and close the connection before the write completes.
    // That is still the expected no-answer outcome for this helper.
    if wire::write(&mut send, &message(&verified), ADMISSION_FRAME_LIMIT)
        .await
        .is_err()
    {
        tokio::time::timeout(LIMIT, connection.closed())
            .await
            .expect("the host ends the connection");
        return None;
    }
    let reply = tokio::time::timeout(LIMIT, wire::read(&mut recv, ADMISSION_FRAME_LIMIT))
        .await
        .expect("an answer or an end in time")
        .ok()
        .flatten();
    tokio::time::timeout(LIMIT, connection.closed())
        .await
        .expect("the host ends the connection");
    reply
}

/// A device of a vault that is no contact proves itself and is cut right away while no lobby
/// is open.
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

/// With only a lobby for contacts, a stranger may send one bounded join: a valid proof ends with
/// `NotAdmitted` (lobby_tests), a wrong one or an oversized join without any answer (FR-031).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn with_only_a_lobby_for_contacts_a_stranger_gets_no_answer_without_a_valid_proof() {
    let host = Vault::start(Admit::Real(Duration::from_secs(30))).await;
    let stranger = stranger_of(&host).await;
    let code = JoinCode::parse(&host.open_lobby("coop", Visibility::Contacts)).expect("code");

    let wrong = send_after_exchange(&stranger, &host, |verified| Message::Join {
        v: VERSION,
        lobby: code.lobby,
        proof: join_proof(&[0; 16], &verified.nonces, &code.lobby),
        extension: probe(),
        display_name: "Dave".to_owned(),
        payload: b"{}".to_vec(),
    })
    .await;
    assert_eq!(wrong, None, "a wrong proof is not answered");
    let oversized = send_after_exchange(&stranger, &host, |verified| Message::Join {
        v: VERSION,
        lobby: code.lobby,
        proof: join_proof(&code.secret, &verified.nonces, &code.lobby),
        extension: probe(),
        display_name: "Dave".to_owned(),
        payload: format!("\"{}\"", "x".repeat(8 * 1024)).into_bytes(),
    })
    .await;
    assert_eq!(oversized, None, "a join beyond the bound is not read");
    assert!(joined(&host.window).is_empty());
    host.shutdown().await;
    stranger.shutdown().await;
}

/// ADR 0016: a public lobby admits a stranger, shown as not verified with its own name.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_public_lobby_admits_a_stranger_as_not_verified() {
    let host = Vault::start(Admit::Real(Duration::from_secs(5))).await;
    let stranger = stranger_of(&host).await;
    host.grant("contacts", "read", "granted");
    let code = host.open_lobby("party", Visibility::Public);

    let joined_d = stranger.join(&code, "Dave").await.expect("joined");
    assert_eq!(joined_d.lobby_id, "party");
    assert_eq!(
        joined_d.channel.remote.trust,
        Trust::Unverified,
        "nor is the host to Dave"
    );
    host.window.wait_until(|w| !joined(w).is_empty()).await;
    let event = joined(&host.window).remove(0);
    assert_eq!(event["verified"], false);
    assert_eq!(event["displayName"], "Dave");
    assert!(event.get("contactId").is_none());
    host.shutdown().await;
    stranger.shutdown().await;
}

/// US5 scenario 5: a contact in a public lobby is still a verified contact.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_contact_in_a_public_lobby_is_verified() {
    let host = Vault::start(Admit::Real(Duration::from_secs(5))).await;
    let guest = stranger_of(&host).await;
    let (guest_on_host, _) = exchange_cards(&host, &guest).await;
    host.grant("contacts", "read", "granted");
    let code = host.open_lobby("party", Visibility::Public);

    guest.join(&code, "Ben").await.expect("joined");
    host.window.wait_until(|w| !joined(w).is_empty()).await;
    let event = joined(&host.window).remove(0);
    assert_eq!(event["verified"], true);
    assert_eq!(event["contactId"], guest_on_host);
    host.shutdown().await;
    guest.shutdown().await;
}

/// FR-032 for a stranger: admitted to ask, but cut once the limit passed without a join.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_stranger_that_asks_for_nothing_is_cut_after_the_limit() {
    let limit = Duration::from_millis(300);
    let host = Vault::start(Admit::Real(limit)).await;
    let stranger = stranger_of(&host).await;
    host.open_lobby("party", Visibility::Public);

    let (connection, _send, _recv, verified) = stranger.dial(&host).await;
    let started = std::time::Instant::now();
    assert_eq!(verified.expect("verified").trust, Trust::Unverified);
    tokio::time::timeout(LIMIT, connection.closed())
        .await
        .expect("cut in time");
    assert!(
        started.elapsed() >= limit / 2,
        "cut after {:?}, before the limit of {limit:?}",
        started.elapsed()
    );
    host.shutdown().await;
    stranger.shutdown().await;
}

/// FR-031: once the public lobby closed, a stranger is cut after the exchange again; its
/// channel from before stays.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn after_the_public_lobby_closed_a_stranger_is_cut_after_the_exchange() {
    let host = Vault::start(Admit::Real(Duration::from_secs(30))).await;
    let stranger = stranger_of(&host).await;
    let code = host.open_lobby("party", Visibility::Public);
    let channel = stranger.join(&code, "Dave").await.expect("joined").channel;

    close(&host.host.peers, host.extension_id, "party");
    assert_eq!(
        stranger.join(&code, "Dave").await.err(),
        Some(PeerError::LobbyClosed)
    );
    let (connection, _send, _recv, _verified) = stranger.dial(&host).await;
    tokio::time::timeout(Duration::from_secs(10), connection.closed())
        .await
        .expect("cut long before the admission limit");
    assert!(!channel.is_closed(), "the channel from before stays");
    host.shutdown().await;
    stranger.shutdown().await;
}

/// FR-033: a sixth connection of one endpoint within the window is refused before the exchange,
/// for a contact too.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_throttle_refuses_a_sixth_connection_of_an_endpoint_before_the_exchange() {
    let host = Vault::start(Admit::Real(Duration::from_secs(30))).await;
    let guest = Vault::start(Admit::AtOnce).await;
    exchange_cards(&host, &guest).await;
    for _ in 0..ATTEMPTS_PER_ENDPOINT {
        let (connection, _send, _recv, verified) = guest.dial(&host).await;
        verified.expect("within the count");
        connection.close(0u32.into(), b"");
    }
    // The sixth is closed at once; without the throttle the host would wait out the exchange's
    // limit for a device that says nothing.
    let connection = guest
        .node
        .dial_alpn(host.node.addr(), crate::sync::wire::EXT_PEER_ALPN)
        .await
        .expect("dial");
    let started = std::time::Instant::now();
    tokio::time::timeout(LIMIT, connection.closed())
        .await
        .expect("refused");
    assert!(
        started.elapsed() < foreign_auth::HANDSHAKE_LIMIT / 2,
        "refused before the exchange, after {:?}",
        started.elapsed()
    );
    host.shutdown().await;
    guest.shutdown().await;
}

/// FR-033: three joins that were answered `Closed` ban the endpoint, and during the ban even a
/// join with the right proof is answered `Closed` (no difference from outside).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn three_failed_joins_ban_the_endpoint_and_a_valid_join_stays_closed() {
    let host = Vault::start(Admit::Real(Duration::from_secs(30))).await;
    let guest = Vault::start(Admit::AtOnce).await;
    guest.learn(&host).await;
    exchange_cards(&host, &guest).await;
    let code = host.open_lobby("coop", Visibility::Contacts);
    let mut wrong = JoinCode::parse(&code).expect("code");
    wrong.secret = crate::sync::keys::random_bytes::<16>();

    for _ in 0..crate::extensions::peers::throttle::FAILED_JOINS {
        assert_eq!(
            guest.join(&wrong.text(), "Ben").await.err(),
            Some(PeerError::LobbyClosed)
        );
    }
    assert_eq!(
        guest.join(&code, "Ben").await.err(),
        Some(PeerError::LobbyClosed),
        "banned: the right code is answered like a wrong one"
    );
    assert!(joined(&host.window).is_empty());
    host.shutdown().await;
    guest.shutdown().await;
}
