//! The bridge methods of channels (spec 057 T058, FR-002, FR-006, FR-041, FR-044, SC-006): every
//! call goes through the allowlist of the bridge, as a frame's call does, on a blocking thread.

use std::sync::Arc;

use super::*;
use crate::extensions::bridge::dispatch::{call, CallContext, METHODS};
use crate::extensions::peers::channel::MESSAGE;
use crate::extensions::peers::lobby::Openness;
use crate::extensions::peers::test_scene::Scene;
use crate::extensions::peers::test_support::{self, install, Admit, Vault};

/// The context of a call from a frame of `extension_id` on `vault`.
pub(super) fn context(vault: &Vault, extension_id: Uuid) -> Arc<CallContext> {
    let session = vault
        .host
        .frames
        .of_extension(extension_id)
        .into_iter()
        .next()
        .unwrap_or_else(|| {
            vault
                .host
                .frames
                .open(extension_id, Uuid::new_v4(), "tab-1")
        });
    Arc::new(CallContext {
        db: vault.db.clone(),
        host: Arc::clone(&vault.host),
        session,
        device: vault.db.device_id(),
        emitter: vault.window.clone(),
    })
}

/// The context of the vault's test extension.
pub(super) fn own(vault: &Vault) -> Arc<CallContext> {
    context(vault, vault.extension_id)
}

pub(super) async fn bridge(
    ctx: &Arc<CallContext>,
    method: &str,
    params: Value,
) -> Result<Value, BridgeError> {
    let (ctx, method) = (Arc::clone(ctx), method.to_owned());
    tokio::task::spawn_blocking(move || call(&ctx, &method, &params))
        .await
        .expect("the call ran")
}

pub(super) fn reason(error: &BridgeError) -> Option<&str> {
    error.details.as_ref()?.get("reason")?.as_str()
}

pub(super) fn bytes(data: &[u8]) -> Value {
    json!({ "$bytes": STANDARD.encode(data) })
}

/// One call of each method, with parameters that are well-formed.
fn every_call() -> Vec<(&'static str, Value)> {
    let id = Uuid::new_v4().to_string();
    vec![
        (
            "extension_peers_invite",
            json!({ "contactId": "someone", "payload": {} }),
        ),
        ("extension_peers_invite_cancel", json!({ "inviteId": id })),
        (
            "extension_peers_lobby_open",
            json!({ "lobbyId": "arena", "visibility": "contacts" }),
        ),
        ("extension_peers_lobby_close", json!({ "lobbyId": "arena" })),
        (
            "extension_peers_join",
            json!({ "joinCode": "haex-vault:join:AAAA", "displayName": "Ben", "payload": null }),
        ),
        ("extension_peers_remove", json!({ "channelId": id })),
        (
            "extension_peers_send",
            json!({ "channelId": id, "data": bytes(b"x") }),
        ),
        ("extension_peers_close", json!({ "channelId": id })),
    ]
}

/// A vault with a frame of its test extension and the permission `status`, or none.
pub(super) async fn vault_with(status: Option<&str>) -> Vault {
    let vault = Vault::start(Admit::AtOnce).await;
    if let Some(status) = status {
        vault.grant_peers(status);
    }
    vault
}

#[test]
fn the_allowlist_lists_the_eight_methods_of_this_module_and_no_way_to_answer_an_invitation() {
    let ours: Vec<&str> = METHODS
        .iter()
        .filter(|m| m.module == MODULE)
        .map(|m| m.name)
        .collect();
    assert_eq!(
        ours,
        [
            "extension_peers_invite",
            "extension_peers_invite_cancel",
            "extension_peers_lobby_open",
            "extension_peers_lobby_close",
            "extension_peers_join",
            "extension_peers_remove",
            "extension_peers_send",
            "extension_peers_close",
        ]
    );
    assert!(
        METHODS
            .iter()
            .all(|m| !m.name.contains("accept") && !m.name.contains("decline")),
        "an invitation is answered only in holzi's prompt (FR-016)"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_denied_permission_answers_1002_and_nothing_starts() {
    let vault = vault_with(Some("denied")).await;
    let ctx = own(&vault);
    for (method, params) in every_call() {
        let error = bridge(&ctx, method, params).await.unwrap_err();
        assert_eq!(error.code, ExtensionErrorCode::PermissionDenied, "{method}");
        assert_eq!(error.message, "permission required");
    }
    assert_eq!(vault.host.peers.openness(), Openness::None);
    assert!(vault.host.peers.extensions().is_empty());
    assert_eq!(vault.window.all_frame_events(), 0);
    vault.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_undecided_permission_asks_with_the_question_and_starts_nothing() {
    let vault = vault_with(None).await;
    let ctx = own(&vault);
    for (method, params) in every_call() {
        let error = bridge(&ctx, method, params).await.unwrap_err();
        assert_eq!(
            error.code,
            ExtensionErrorCode::PermissionPromptRequired,
            "{method}"
        );
        assert_eq!(
            error.details,
            Some(json!({ "resourceType": "peers", "action": "connect", "target": "*" })),
            "{method}"
        );
    }
    // One question for the user, however often the extension asks.
    let questions = vault.window.window_events("extension-permission-request");
    assert_eq!(questions.len(), 1);
    assert_eq!(questions[0]["kind"], "peers");
    assert_eq!(questions[0]["action"], "connect");
    assert_eq!(questions[0]["target"], "*");
    assert_eq!(vault.host.peers.openness(), Openness::None);
    assert!(vault.host.peers.extensions().is_empty());
    vault.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_channel_of_another_extension_answers_like_an_unknown_one() {
    let a = vault_with(Some("granted")).await;
    let b = Vault::start(Admit::AtOnce).await;
    let (near, _far) = test_support::open_test_channel(&a, &b).await;
    // Another extension of the same vault holds the permission too.
    let other = install(&a.db, "other-probe");
    a.grant_to(other, "peers", "connect", "granted");
    let intruder = context(&a, other);

    let foreign = near.id.to_string();
    let missing = Uuid::new_v4().to_string();
    for method in [
        "extension_peers_close",
        "extension_peers_remove",
        "extension_peers_send",
    ] {
        let params = |id: &str| json!({ "channelId": id, "data": bytes(b"x") });
        let seen = bridge(&intruder, method, params(&foreign))
            .await
            .unwrap_err();
        let unknown = bridge(&intruder, method, params(&missing))
            .await
            .unwrap_err();
        assert_eq!(seen, unknown, "{method}");
        assert_eq!(seen.code, ExtensionErrorCode::NotFound, "{method}");
        assert_eq!(reason(&seen), Some("channel_unknown"), "{method}");
    }
    assert!(!near.is_closed(), "the owner's channel is untouched");
    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_invitation_is_cancelled_only_by_the_extension_that_sent_it() {
    let scene = Scene::start().await;
    scene.a.grant_peers("granted");
    let intruder_id = install(&scene.a.db, "other-probe");
    scene.a.grant_to(intruder_id, "peers", "connect", "granted");
    let intruder = context(&scene.a, intruder_id);
    let owner = own(&scene.a);

    let sent = bridge(
        &owner,
        "extension_peers_invite",
        json!({ "contactId": scene.b_on_a, "payload": { "map": 1 } }),
    )
    .await
    .expect("invited");
    let invite = sent["inviteId"].as_str().expect("an id").to_owned();
    let foreign = bridge(
        &intruder,
        "extension_peers_invite_cancel",
        json!({ "inviteId": invite }),
    )
    .await
    .unwrap_err();
    assert_eq!(reason(&foreign), Some("invite_not_open"));
    assert_eq!(foreign.code, ExtensionErrorCode::NotFound);
    assert_eq!(
        bridge(
            &owner,
            "extension_peers_invite_cancel",
            json!({ "inviteId": invite })
        )
        .await
        .expect("the sender cancels"),
        json!({})
    );
    let again = bridge(
        &owner,
        "extension_peers_invite_cancel",
        json!({ "inviteId": invite }),
    )
    .await
    .unwrap_err();
    assert_eq!(reason(&again), Some("invite_not_open"));
    scene.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_message_over_its_limit_is_too_large_and_one_at_it_is_sent() {
    let a = vault_with(Some("granted")).await;
    let b = Vault::start(Admit::AtOnce).await;
    let (near, _far) = test_support::open_test_channel(&a, &b).await;
    let ctx = own(&a);
    let id = near.id.to_string();
    let send = |data: usize, reliable: bool| json!({ "channelId": id, "data": bytes(&vec![7; data]), "reliable": reliable });

    let fast = bridge(&ctx, "extension_peers_send", send(1025, false))
        .await
        .unwrap_err();
    assert_eq!(fast.code, ExtensionErrorCode::LimitExceeded);
    assert_eq!(
        fast.details,
        Some(json!({ "reason": "too_large", "limit": 1024 }))
    );
    let reliable = bridge(&ctx, "extension_peers_send", send(65_537, true))
        .await
        .unwrap_err();
    assert_eq!(
        reliable.details,
        Some(json!({ "reason": "too_large", "limit": 65_536 }))
    );
    // The same size is fine as a reliable message, and each limit itself goes out as it is.
    for (size, reliable) in [(1024, false), (1025, true), (65_536, true)] {
        let sent = bridge(&ctx, "extension_peers_send", send(size, reliable)).await;
        assert_eq!(sent.expect("sent"), json!({}), "{size} {reliable}");
    }
    b.window
        .wait_until(|w| w.frame_events(MESSAGE).len() == 3)
        .await;
    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn malformed_parameters_are_invalid_input() {
    let a = vault_with(Some("granted")).await;
    let b = Vault::start(Admit::AtOnce).await;
    let (near, _far) = test_support::open_test_channel(&a, &b).await;
    let ctx = own(&a);
    let id = near.id.to_string();
    let wrong = |method: &str, params: Value| {
        let ctx = Arc::clone(&ctx);
        let method = method.to_owned();
        async move {
            let error = bridge(&ctx, &method, params).await.unwrap_err();
            (error.code, reason(&error).map(str::to_owned))
        }
    };
    let invalid = (ExtensionErrorCode::Validation, Some("invalid_input".into()));

    for (method, params) in [
        (
            "extension_peers_send",
            json!({ "channelId": "no-uuid", "data": bytes(b"x") }),
        ),
        ("extension_peers_send", json!({ "data": bytes(b"x") })),
        ("extension_peers_send", json!({ "channelId": id })),
        (
            "extension_peers_send",
            json!({ "channelId": id, "data": "AAAA" }),
        ),
        (
            "extension_peers_send",
            json!({ "channelId": id, "data": [1, 2, 3] }),
        ),
        (
            "extension_peers_send",
            json!({ "channelId": id, "data": { "$bytes": "not base64!" } }),
        ),
        (
            "extension_peers_send",
            json!({ "channelId": id, "data": { "$bytes": 1 } }),
        ),
        (
            "extension_peers_send",
            json!({ "channelId": id, "data": bytes(b"x"), "reliable": "yes" }),
        ),
        ("extension_peers_close", json!({ "channelId": 5 })),
        ("extension_peers_remove", json!({})),
        ("extension_peers_invite_cancel", json!({ "inviteId": "x" })),
        ("extension_peers_invite", json!({ "payload": {} })),
        (
            "extension_peers_invite",
            json!({ "contactId": "", "payload": {} }),
        ),
        ("extension_peers_lobby_open", json!({ "lobbyId": "arena" })),
        (
            "extension_peers_lobby_open",
            json!({ "lobbyId": "arena", "visibility": "friends" }),
        ),
        (
            "extension_peers_lobby_open",
            json!({ "lobbyId": "has space", "visibility": "public" }),
        ),
        (
            "extension_peers_lobby_open",
            json!({ "lobbyId": "", "visibility": "public" }),
        ),
        ("extension_peers_lobby_close", json!({})),
        ("extension_peers_join", json!({ "displayName": "Ben" })),
        (
            "extension_peers_join",
            json!({ "joinCode": "haex-vault:join:AAAA", "displayName": "" }),
        ),
    ] {
        assert_eq!(
            wrong(method, params.clone()).await,
            invalid,
            "{method} {params}"
        );
    }
    a.shutdown().await;
    b.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_limits_per_extension_answer_7000_limit() {
    let vault = vault_with(Some("granted")).await;
    let ctx = own(&vault);
    for n in 0..4 {
        let opened = bridge(
            &ctx,
            "extension_peers_lobby_open",
            json!({ "lobbyId": format!("lobby-{n}"), "visibility": "contacts" }),
        )
        .await
        .expect("a lobby");
        assert!(opened["joinCode"]
            .as_str()
            .is_some_and(|code| code.starts_with("haex-vault:join:")));
    }
    let fifth = bridge(
        &ctx,
        "extension_peers_lobby_open",
        json!({ "lobbyId": "lobby-4", "visibility": "contacts" }),
    )
    .await
    .unwrap_err();
    assert_eq!(fifth.code, ExtensionErrorCode::LimitExceeded);
    assert_eq!(
        fifth.details,
        Some(json!({ "reason": "limit", "limit": 4 }))
    );
    // The same id again is no limit but a mistake; a closed lobby gives its place back, and
    // closing an unknown one is no error.
    let again = bridge(
        &ctx,
        "extension_peers_lobby_open",
        json!({ "lobbyId": "lobby-0", "visibility": "public" }),
    )
    .await
    .unwrap_err();
    assert_eq!(reason(&again), Some("invalid_input"));
    for lobby in ["lobby-0", "lobby-0", "never-opened"] {
        let closed = bridge(
            &ctx,
            "extension_peers_lobby_close",
            json!({ "lobbyId": lobby }),
        )
        .await;
        assert_eq!(closed.expect("closed"), json!({}));
    }
    bridge(
        &ctx,
        "extension_peers_lobby_open",
        json!({ "lobbyId": "lobby-4", "visibility": "public" }),
    )
    .await
    .expect("room again");

    // A payload over 4 KiB never starts an invitation or a join.
    let big = json!({ "text": "x".repeat(4100) });
    let invite = bridge(
        &ctx,
        "extension_peers_invite",
        json!({ "contactId": "someone", "payload": big }),
    )
    .await
    .unwrap_err();
    assert_eq!(
        invite.details,
        Some(json!({ "reason": "too_large", "limit": 4096 }))
    );
    vault.shutdown().await;
}
