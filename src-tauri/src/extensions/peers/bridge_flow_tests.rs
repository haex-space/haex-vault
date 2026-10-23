//! Whole exchanges through the bridge (spec 057 T058, FR-044): a lobby, a join, messages, a removal
//! and a close, and a vault without an endpoint.

use super::tests::{bridge, bytes, own, reason, vault_with};
use super::*;
use crate::extensions::peers::channel::{CHANNEL_CLOSED, MESSAGE};
use crate::extensions::peers::lobby::JOINED;
use crate::extensions::peers::test_support::{self, exchange_cards, Admit, Vault};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn without_an_endpoint_the_methods_that_need_one_answer_network_unavailable() {
    let host = vault_with(Some("granted")).await;
    let code = host.open_lobby("arena", lobby::Visibility::Contacts);
    let vault = vault_with(Some("granted")).await;
    vault.host.peers.clear();
    let ctx = own(&vault);

    let answers = [
        (
            "extension_peers_lobby_open",
            json!({ "lobbyId": "arena", "visibility": "contacts" }),
        ),
        (
            "extension_peers_join",
            json!({ "joinCode": code, "displayName": "Ben", "payload": null }),
        ),
        (
            "extension_peers_invite",
            json!({ "contactId": "someone", "payload": {} }),
        ),
    ];
    for (method, params) in answers {
        let error = bridge(&ctx, method, params).await.unwrap_err();
        assert_eq!(error.code, ExtensionErrorCode::NotAvailable, "{method}");
        assert_eq!(reason(&error), Some("network_unavailable"), "{method}");
    }
    host.shutdown().await;
    vault.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_lobby_a_join_messages_a_removal_and_a_close_work_through_the_bridge() {
    let host = Vault::start(Admit::Real(test_support::LIMIT)).await;
    let guest = Vault::start(Admit::AtOnce).await;
    exchange_cards(&host, &guest).await;
    guest.learn(&host).await;
    host.grant_peers("granted");
    guest.grant_peers("granted");
    let (host_ctx, guest_ctx) = (own(&host), own(&guest));

    let opened = bridge(
        &host_ctx,
        "extension_peers_lobby_open",
        json!({ "lobbyId": "arena", "visibility": "contacts" }),
    )
    .await
    .expect("a lobby");
    let joined = bridge(
        &guest_ctx,
        "extension_peers_join",
        json!({ "joinCode": opened["joinCode"], "displayName": "Ben", "payload": { "skin": 2 } }),
    )
    .await
    .expect("joined");
    assert_eq!(joined["lobbyId"], "arena");
    let guest_channel = joined["channelId"].as_str().expect("a channel").to_owned();
    host.window
        .wait_until(|w| !w.frame_events(JOINED).is_empty())
        .await;
    let announced = host.window.frame_events(JOINED).remove(0).2;
    assert_eq!(announced["displayName"], "Ben");
    assert_eq!(announced["payload"], json!({ "skin": 2 }));
    let host_channel = announced["channelId"]
        .as_str()
        .expect("a channel")
        .to_owned();

    // Fast and reliable, both ways; bytes arrive as Base64 text.
    bridge(
        &guest_ctx,
        "extension_peers_send",
        json!({ "channelId": guest_channel, "data": bytes(b"hello") }),
    )
    .await
    .expect("fast");
    bridge(
        &guest_ctx,
        "extension_peers_send",
        json!({ "channelId": guest_channel, "data": bytes(b"safe"), "reliable": true }),
    )
    .await
    .expect("reliable");
    host.window
        .wait_until(|w| w.frame_events(MESSAGE).len() == 2)
        .await;
    let mut got: Vec<(bool, String)> = host
        .window
        .frame_events(MESSAGE)
        .into_iter()
        .map(|(_, _, data)| {
            (
                data["reliable"].as_bool().unwrap_or_default(),
                data["data"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect();
    got.sort();
    assert_eq!(
        got,
        [
            (false, STANDARD.encode(b"hello")),
            (true, STANDARD.encode(b"safe")),
        ]
    );
    bridge(
        &host_ctx,
        "extension_peers_send",
        json!({ "channelId": host_channel, "data": bytes(b"welcome"), "reliable": true }),
    )
    .await
    .expect("answer");
    guest
        .window
        .wait_until(|w| !w.frame_events(MESSAGE).is_empty())
        .await;

    // Closing the lobby keeps the channel; removing the guest ends it on both sides, and the
    // guest cannot send on it any more.
    bridge(
        &host_ctx,
        "extension_peers_lobby_close",
        json!({ "lobbyId": "arena" }),
    )
    .await
    .expect("lobby closed");
    assert_eq!(
        bridge(
            &host_ctx,
            "extension_peers_remove",
            json!({ "channelId": host_channel })
        )
        .await
        .expect("removed"),
        json!({})
    );
    host.window
        .wait_until(|w| !w.frame_events(CHANNEL_CLOSED).is_empty())
        .await;
    guest
        .window
        .wait_until(|w| !w.frame_events(CHANNEL_CLOSED).is_empty())
        .await;
    let gone = bridge(
        &guest_ctx,
        "extension_peers_send",
        json!({ "channelId": guest_channel, "data": bytes(b"late") }),
    )
    .await
    .unwrap_err();
    assert_eq!(reason(&gone), Some("channel_unknown"));
    host.shutdown().await;
    guest.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn closing_a_channel_tells_both_sides_once() {
    let a = vault_with(Some("granted")).await;
    let b = Vault::start(Admit::AtOnce).await;
    let (near, far) = test_support::open_test_channel(&a, &b).await;
    let ctx = own(&a);
    let closed = bridge(
        &ctx,
        "extension_peers_close",
        json!({ "channelId": near.id.to_string() }),
    )
    .await
    .expect("closed");
    assert_eq!(closed, json!({}));
    for vault in [&a, &b] {
        vault
            .window
            .wait_until(|w| !w.frame_events(CHANNEL_CLOSED).is_empty())
            .await;
        let ends = vault.window.frame_events(CHANNEL_CLOSED);
        assert_eq!(ends.len(), 1);
        assert_eq!(ends[0].2["reason"], "closed");
    }
    assert!(far.is_closed());
    a.shutdown().await;
    b.shutdown().await;
}
