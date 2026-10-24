//! Spec 049, managing clients from the settings against a real vault and real sockets (US5, T054):
//! the lists, changing, revoking and unblocking, with and without a running bridge, and moving to
//! another port (FR-002, FR-011, FR-037, FR-038, SC-007).

#[path = "common/bridge_fixture.rs"]
mod bridge_fixture;

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use bridge_fixture::*;
use holzi_lib::browser_bridge::admin::{BridgeClientUpdate, BridgeClientView, ClientAdmin};
use holzi_lib::browser_bridge::approvals::{self, Status};
use holzi_lib::browser_bridge::declaration::BridgeAccess;
use holzi_lib::browser_bridge::prompts::{
    ApprovalAnswer, BridgeAnswer, BridgeQuestion, WriteAnswer,
};
use holzi_lib::browser_bridge::protocol::PROTOCOL_VERSION;
use holzi_lib::browser_bridge::runtime::Notice;
use holzi_lib::browser_bridge::server::Running;
use holzi_lib::error::HolziError;
use holzi_lib::passwords::access::Caller;
use holzi_lib::passwords::model::ItemInput;

fn admin(fx: &Fixture, bridge: Option<&Running>) -> ClientAdmin {
    ClientAdmin {
        db: fx.db.clone(),
        device: fx.device,
        core: bridge.map(|bridge| Arc::clone(bridge.core())),
    }
}

fn answer(
    allow: bool,
    remember: bool,
    access: BridgeAccess,
    scope: Option<&[&str]>,
) -> BridgeAnswer {
    BridgeAnswer::Approval(ApprovalAnswer {
        allow,
        remember,
        access: allow.then_some(access),
        scope_tags: scope.map(|tags| tags.iter().map(|t| (*t).to_owned()).collect()),
        default_tag: None,
    })
}

/// Connects with `handshake`, answers the approval question and returns the socket after the
/// `authorizationUpdate`.
async fn decided(
    fx: &mut Fixture,
    port: u16,
    handshake: Value,
    decision: BridgeAnswer,
) -> (Socket, Value) {
    let mut socket = extension(port).await;
    send(&mut socket, handshake).await;
    let first = recv(&mut socket).await.expect("answer");
    assert_eq!(handshake_answer(&first), (false, true));
    let (id, _) = fx.next_question().await;
    fx.answer_with(&id, decision);
    let update = recv(&mut socket).await.expect("update");
    assert_eq!(update["type"], "authorizationUpdate");
    (socket, first)
}

fn find<'a>(list: &'a [BridgeClientView], client: &TestClient) -> &'a BridgeClientView {
    list.iter()
        .find(|view| view.client_id == client.id)
        .unwrap_or_else(|| panic!("{} is listed", client.id))
}

fn update(client: &TestClient, access: BridgeAccess, scope: Option<&[&str]>) -> BridgeClientUpdate {
    BridgeClientUpdate {
        client_id: client.id.clone(),
        access,
        scope_tags: scope.map(|tags| tags.iter().map(|t| (*t).to_owned()).collect()),
        default_tag: None,
    }
}

async fn seed(fx: &Fixture, title: &str, tag: &str) -> String {
    fx.service()
        .create_item(
            &Caller::User,
            &[],
            ItemInput {
                title: Some(title.to_owned()),
                url: Some("https://example.de".to_owned()),
                password: Some("pw".to_owned()),
                tags: vec![tag.to_owned()],
                ..ItemInput::default()
            },
            None,
        )
        .await
        .expect("create")
}

/// Waits a moment for a notice the bridge sends from another task.
async fn notice(fx: &Fixture, wanted: &Notice) {
    for _ in 0..50 {
        if fx.notices.lock().expect("notices").contains(wanted) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("no {wanted:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_lists_show_remembered_and_session_decisions_and_who_is_connected() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    let port = bridge.port();
    let remembered = TestClient::new(40);
    let (_kept_open, _) = approved(&mut fx, port, &remembered).await;
    let session = TestClient::new(41);
    let (_session_socket, _) = decided(
        &mut fx,
        port,
        session.handshake(),
        answer(true, false, BridgeAccess::Read, Some(&["web"])),
    )
    .await;
    let blocked = TestClient::new(42);
    decided(
        &mut fx,
        port,
        blocked.handshake(),
        answer(false, true, BridgeAccess::Read, None),
    )
    .await;
    let session_block = TestClient::new(43);
    decided(
        &mut fx,
        port,
        session_block.handshake(),
        answer(false, false, BridgeAccess::Read, None),
    )
    .await;

    let lists = admin(&fx, Some(&bridge))
        .overview()
        .await
        .expect("overview");
    assert_eq!(lists.clients.len(), 2);
    assert_eq!(lists.blocked.len(), 2);
    let first = find(&lists.clients, &remembered);
    assert_eq!(first.client_name, "haex-pass Browser Extension");
    assert_eq!(first.origin, "chrome-extension://abcdefghijklmnop");
    assert_eq!(first.access, Some(BridgeAccess::ReadWrite));
    assert_eq!(first.scope_tags, None);
    assert!(first.remembered && first.connected && first.declares_write);
    assert_eq!(
        first.declared.as_ref().map(|d| d.access),
        Some(BridgeAccess::ReadWrite)
    );
    let seen = first.last_seen_at.expect("seen at the approval");
    let second = find(&lists.clients, &session);
    assert_eq!(second.access, Some(BridgeAccess::Read));
    assert_eq!(second.scope_tags, Some(vec!["web".to_owned()]));
    assert!(!second.remembered && second.connected);
    assert_eq!(second.last_seen_at, None);
    let third = find(&lists.blocked, &blocked);
    assert!(third.remembered && !third.connected);
    assert_eq!(third.access, None);
    assert!(!find(&lists.blocked, &session_block).remembered);

    // An approved handshake notes when the client was last seen.
    tokio::time::sleep(Duration::from_millis(5)).await;
    let mut again = extension(port).await;
    send(&mut again, remembered.handshake()).await;
    assert_eq!(
        handshake_answer(&recv(&mut again).await.expect("answer")),
        (true, false)
    );
    let stored = fx.stored(&remembered.id).await.expect("row");
    assert!(stored.last_seen_at.expect("seen") > seen);

    // Without a running bridge the remembered decisions stay listed, nobody connected.
    bridge.stop().await;
    let lists = admin(&fx, None).overview().await.expect("overview");
    assert_eq!(lists.clients.len(), 1);
    assert_eq!(lists.blocked.len(), 1);
    assert!(!lists.clients[0].connected);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn revoking_closes_the_connection_ends_its_question_and_asks_again() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    let client = TestClient::new(44);
    let (mut socket, key) = approved_as(
        &mut fx,
        bridge.port(),
        &client,
        BridgeAccess::Read,
        None,
        None,
    )
    .await;
    send(
        &mut socket,
        client.request(
            &key,
            "create-item",
            json!({ "title": "a", "requestId": "w" }),
        ),
    )
    .await;
    let (prompt, question) = fx.next_question().await;
    assert!(matches!(question, BridgeQuestion::Write(_)));

    let started = Instant::now();
    admin(&fx, Some(&bridge))
        .revoke(&client.id)
        .await
        .expect("revoke");
    assert_eq!(
        recv(&mut socket).await.expect("update"),
        json!({ "type": "authorizationUpdate", "authorized": false })
    );
    assert!(started.elapsed() < Duration::from_secs(1), "SC-007");
    closed(&mut socket).await;
    notice(&fx, &Notice::PromptCancelled(prompt)).await;
    assert!(fx.stored(&client.id).await.is_none());

    let mut socket = extension(bridge.port()).await;
    send(&mut socket, client.handshake()).await;
    assert_eq!(
        handshake_answer(&recv(&mut socket).await.expect("answer")),
        (false, true)
    );
    fx.next_question().await;
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_change_holds_from_the_next_request_and_forgets_the_answers_about_writing() {
    let mut fx = fixture();
    let web = seed(&fx, "web", "web").await;
    seed(&fx, "work", "work").await;
    let bridge = fx.bridge().await;
    let client = TestClient::new(45);
    let (mut socket, key) = approved_as(
        &mut fx,
        bridge.port(),
        &client,
        BridgeAccess::Read,
        None,
        None,
    )
    .await;
    let lookup = json!({ "url": "https://example.de" });
    let found = call(&mut socket, &client, &key, "get-items", lookup.clone()).await;
    assert_eq!(
        found["data"]["entries"].as_array().expect("entries").len(),
        2
    );

    // Writing allowed for this session only.
    send(
        &mut socket,
        client.request(
            &key,
            "create-item",
            json!({ "title": "a", "requestId": "w1" }),
        ),
    )
    .await;
    let (id, _) = fx.next_question().await;
    fx.answer_with(
        &id,
        BridgeAnswer::Write(WriteAnswer {
            allow: true,
            remember: false,
            default_tag: None,
        }),
    );
    let saved = client.open(&recv(&mut socket).await.expect("response"));
    assert_eq!(saved["success"], true, "{saved}");

    admin(&fx, Some(&bridge))
        .update(update(&client, BridgeAccess::Read, Some(&["web"])))
        .await
        .expect("update");
    let found = call(&mut socket, &client, &key, "get-items", lookup).await;
    let entries = found["data"]["entries"].as_array().expect("entries");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["id"], json!(web));
    send(
        &mut socket,
        client.request(
            &key,
            "create-item",
            json!({ "title": "b", "requestId": "w2" }),
        ),
    )
    .await;
    let (id, question) = fx.next_question().await;
    assert!(matches!(question, BridgeQuestion::Write(_)), "asked again");
    fx.answer_with(
        &id,
        BridgeAnswer::Write(WriteAnswer {
            allow: false,
            remember: false,
            default_tag: None,
        }),
    );
    let refused = client.open(&recv(&mut socket).await.expect("response"));
    assert_eq!(refused["errorCode"], "PERMISSION_DENIED");
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_change_wider_than_the_declaration_or_for_nobody_is_refused() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    let client = TestClient::new(46);
    let reader = client.handshake_with(
        PROTOCOL_VERSION,
        json!({ "core": { "passwords": [{ "target": "web", "operation": "read" }] } }),
    );
    let (_socket, _) = decided(
        &mut fx,
        bridge.port(),
        reader,
        answer(true, true, BridgeAccess::Read, Some(&["web"])),
    )
    .await;
    let admin = admin(&fx, Some(&bridge));
    for wider in [
        update(&client, BridgeAccess::ReadWrite, Some(&["web"])),
        update(&client, BridgeAccess::Read, None),
        update(&client, BridgeAccess::Read, Some(&["web", "work"])),
        update(&client, BridgeAccess::Read, Some(&[])),
    ] {
        let refused = admin.update(wider).await;
        assert!(
            matches!(refused, Err(HolziError::InvalidInput { .. })),
            "{refused:?}"
        );
    }
    let unknown = admin
        .update(update(&TestClient::new(47), BridgeAccess::Read, None))
        .await;
    assert!(matches!(
        unknown,
        Err(HolziError::BrowserBridgeClientNotFound)
    ));
    assert!(matches!(
        admin.unblock(&client.id).await,
        Err(HolziError::BrowserBridgeClientNotFound)
    ));
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unblocking_asks_again_with_and_without_a_running_bridge() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    let remembered = TestClient::new(48);
    decided(
        &mut fx,
        bridge.port(),
        remembered.handshake(),
        answer(false, true, BridgeAccess::Read, None),
    )
    .await;
    let session = TestClient::new(49);
    decided(
        &mut fx,
        bridge.port(),
        session.handshake(),
        answer(false, false, BridgeAccess::Read, None),
    )
    .await;
    assert!(matches!(
        admin(&fx, Some(&bridge)).revoke(&session.id).await,
        Err(HolziError::BrowserBridgeClientNotFound)
    ));
    admin(&fx, Some(&bridge))
        .unblock(&session.id)
        .await
        .expect("unblock");
    let mut socket = extension(bridge.port()).await;
    send(&mut socket, session.handshake()).await;
    assert_eq!(
        handshake_answer(&recv(&mut socket).await.expect("answer")),
        (false, true)
    );
    fx.next_question().await;
    bridge.stop().await;

    admin(&fx, None)
        .unblock(&remembered.id)
        .await
        .expect("unblock without a bridge");
    assert!(fx.stored(&remembered.id).await.is_none());
    let bridge = fx.bridge().await;
    let mut socket = extension(bridge.port()).await;
    send(&mut socket, remembered.handshake()).await;
    assert_eq!(
        handshake_answer(&recv(&mut socket).await.expect("answer")),
        (false, true)
    );
    fx.next_question().await;
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_remembered_approval_changes_and_goes_without_a_running_bridge() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    let client = TestClient::new(50);
    approved(&mut fx, bridge.port(), &client).await;
    bridge.stop().await;

    let stored = fx.stored(&client.id).await.expect("row");
    let identity = stored.identity.clone();
    let approval = match stored.status {
        Status::Allowed(approval) => approval,
        Status::Blocked => panic!("the client is allowed"),
    };

    let admin = admin(&fx, None);
    admin
        .update(update(&client, BridgeAccess::Read, Some(&["web"])))
        .await
        .expect("update");
    let lists = admin.overview().await.expect("overview");
    assert_eq!(lists.clients[0].access, Some(BridgeAccess::Read));
    assert_eq!(lists.clients[0].scope_tags, Some(vec!["web".to_owned()]));
    admin.revoke(&client.id).await.expect("revoke");
    assert!(fx.stored(&client.id).await.is_none());

    // A stale update from before the revoke must not recreate the remembered approval.
    let device = fx.device;
    let recreated = fx
        .db
        .write(move |tx| approvals::update_allowed(tx, device, &identity, &approval))
        .await
        .expect("conditional update");
    assert!(!recreated);
    assert!(fx.stored(&client.id).await.is_none());

    assert!(matches!(
        admin.revoke(&client.id).await,
        Err(HolziError::BrowserBridgeClientNotFound)
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn moving_to_another_port_frees_the_old_one_and_keeps_the_approvals() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    let old = bridge.port();
    let client = TestClient::new(51);
    let (mut socket, _) = approved(&mut fx, old, &client).await;
    bridge.stop().await;
    closed(&mut socket).await;
    drop(std::net::TcpListener::bind(("127.0.0.1", old)).expect("the old port is free"));

    let moved = fx.bridge().await;
    let mut socket = extension(moved.port()).await;
    send(&mut socket, client.handshake()).await;
    assert_eq!(
        handshake_answer(&recv(&mut socket).await.expect("answer")),
        (true, false)
    );
    fx.no_question().await;
    moved.stop().await;
}
