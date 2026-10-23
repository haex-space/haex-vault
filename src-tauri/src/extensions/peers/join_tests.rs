//! The joining side's own checks (spec 057 T052, FR-023, FR-044): what it refuses before
//! dialing, and how a host that is not reached answers.

use serde_json::json;

use super::*;
use crate::extensions::peers::test_support::{probe, Admit, Vault};
use crate::sync::keys::random_bytes;

async fn join_as(vault: &Vault, code: &str, name: &str, payload: &Value) -> Option<PeerError> {
    join(
        &vault.host.peers,
        vault.extension_id,
        probe(),
        code,
        name,
        payload,
    )
    .await
    .err()
}

fn some_code() -> String {
    JoinCode {
        v: VERSION,
        endpoint_id: *iroh::SecretKey::from_bytes(&random_bytes::<32>())
            .public()
            .as_bytes(),
        relay: None,
        lobby: [1; 16],
        secret: random_bytes::<16>(),
    }
    .text()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_join_beyond_its_limits_is_refused_before_anything_else() {
    let vault = Vault::start(Admit::AtOnce).await;
    let code = some_code();
    let fine = json!({});
    for name in ["", &"x".repeat(65)] {
        assert_eq!(
            join_as(&vault, &code, name, &fine).await,
            Some(PeerError::InvalidInput),
            "{name:?}"
        );
    }
    let large = json!("x".repeat(PAYLOAD_BYTES));
    assert_eq!(
        join_as(&vault, &code, "Ana", &large).await,
        Some(PeerError::TooLarge {
            limit: PAYLOAD_BYTES
        })
    );
    vault.shutdown().await;
}

/// An unreadable code is `lobby_closed` before the device even looks for its endpoint: the same
/// call without a running endpoint still says `lobby_closed`, a readable code
/// `network_unavailable`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unreadable_code_never_leaves_the_device() {
    let vault = Vault::start(Admit::AtOnce).await;
    vault.host.peers.clear();
    let fine = json!({});
    for code in [
        "",
        "haex-vault:join:",
        "haex-vault:join:AAAA",
        "doom://lobby/1",
    ] {
        assert_eq!(
            join_as(&vault, code, "Ana", &fine).await,
            Some(PeerError::LobbyClosed),
            "{code:?}"
        );
    }
    assert_eq!(
        join_as(&vault, &some_code(), "Ana", &fine).await,
        Some(PeerError::NetworkUnavailable)
    );
    vault.shutdown().await;
}

/// A display name counts characters, not bytes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_display_name_of_sixty_four_characters_passes_the_check() {
    let vault = Vault::start(Admit::AtOnce).await;
    vault.host.peers.clear();
    let name = "ä".repeat(64);
    assert_eq!(
        join_as(&vault, &some_code(), &name, &json!({})).await,
        Some(PeerError::NetworkUnavailable),
        "past the checks, stopped only by the missing endpoint"
    );
    vault.shutdown().await;
}

/// A host whose endpoint cannot be found is `network_unavailable`, within the join's limit.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_host_that_is_not_reached_is_network_unavailable() {
    let vault = Vault::start(Admit::AtOnce).await;
    let started = std::time::Instant::now();
    assert_eq!(
        join_as(&vault, &some_code(), "Ana", &json!({})).await,
        Some(PeerError::NetworkUnavailable)
    );
    assert!(started.elapsed() <= JOIN_LIMIT + std::time::Duration::from_secs(1));
    vault.shutdown().await;
}
