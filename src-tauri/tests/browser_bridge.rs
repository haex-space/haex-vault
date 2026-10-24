//! Spec 049, the External Bridge against a real vault and real sockets: connecting and approving
//! (US1, T024), and keeping web pages and forged clients out (US6, T038).

#[path = "common/bridge_fixture.rs"]
mod bridge_fixture;

use std::time::Duration;

use futures::SinkExt;
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;

use bridge_fixture::*;
use holzi_lib::browser_bridge::approvals::Status;
use holzi_lib::browser_bridge::declaration::BridgeAccess;
use holzi_lib::browser_bridge::prompts::BridgeQuestion;
use holzi_lib::browser_bridge::protocol::PROTOCOL_VERSION;
use holzi_lib::browser_bridge::runtime::{Limits, Notice};
use holzi_lib::browser_bridge::server::StartError;

// ---------------------------------------------------------------------------
// User Story 1: connect and approve
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_client_is_asked_once_and_hears_the_approval_at_once() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    let client = TestClient::new(1);

    let mut socket = extension(bridge.port()).await;
    send(&mut socket, client.handshake()).await;
    let answer = recv(&mut socket).await.expect("answer");
    assert_eq!(handshake_answer(&answer), (false, true));
    assert!(answer["serverPublicKey"].is_string());

    let (id, question) = fx.next_question().await;
    let BridgeQuestion::Approval(question) = question else {
        panic!("an approval question");
    };
    assert_eq!(question.client.client_id, client.id);
    assert_eq!(question.client.client_name, "haex-pass Browser Extension");
    assert_eq!(
        question.client.origin,
        "chrome-extension://abcdefghijklmnop"
    );
    assert_eq!(question.declared.access, BridgeAccess::ReadWrite);
    assert_eq!(question.declared.scope_tags, None);

    // The extension repeats its handshake while it waits: no second question (FR-007).
    send(&mut socket, client.handshake()).await;
    assert_eq!(
        handshake_answer(&recv(&mut socket).await.expect("again")),
        (false, true)
    );
    fx.no_question().await;

    fx.answer(&id, true, true);
    assert_eq!(
        recv(&mut socket).await.expect("update"),
        json!({ "type": "authorizationUpdate", "authorized": true })
    );
    let stored = fx.stored(&client.id).await.expect("remembered");
    assert!(matches!(stored.status, Status::Allowed(_)));
    assert!(stored.last_seen_at.is_some());
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_remembered_client_connects_without_a_question_and_its_requests_are_answered() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    let client = TestClient::new(2);
    drop(approved(&mut fx, bridge.port(), &client).await);

    let mut socket = extension(bridge.port()).await;
    send(&mut socket, client.handshake()).await;
    let answer = recv(&mut socket).await.expect("answer");
    assert_eq!(handshake_answer(&answer), (true, false));
    fx.no_question().await;

    // An answer is sealed to the client; an unsupported method says so (FR-028).
    let key = answer["serverPublicKey"].as_str().expect("key");
    let request = client.request(key, "update-item", json!({ "id": "x", "requestId": "r1" }));
    send(&mut socket, request).await;
    let response = recv(&mut socket).await.expect("response");
    assert_eq!(response["type"], "response");
    assert_eq!(response["clientId"], "");
    assert_eq!(response["extensionPublicKey"], Value::Null);
    assert_eq!(
        client.open(&response),
        json!({ "requestId": "r1", "success": false, "error": "Not supported", "errorCode": "NOT_SUPPORTED" })
    );

    send(&mut socket, json!({ "type": "ping" })).await;
    assert_eq!(
        recv(&mut socket).await.expect("pong"),
        json!({ "type": "pong" })
    );
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_session_approval_ends_with_the_bridge() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    let client = TestClient::new(3);
    let mut socket = extension(bridge.port()).await;
    send(&mut socket, client.handshake()).await;
    assert_eq!(
        handshake_answer(&recv(&mut socket).await.expect("answer")),
        (false, true)
    );
    let (id, _) = fx.next_question().await;
    fx.answer(&id, true, false);
    assert_eq!(recv(&mut socket).await.expect("update")["authorized"], true);
    assert!(fx.stored(&client.id).await.is_none(), "not remembered");
    drop(socket);
    bridge.stop().await;

    let bridge = fx.bridge().await;
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
async fn a_remembered_denial_turns_the_client_away_without_a_question() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    let client = TestClient::new(4);
    let mut socket = extension(bridge.port()).await;
    send(&mut socket, client.handshake()).await;
    recv(&mut socket).await.expect("answer");
    let (id, _) = fx.next_question().await;
    fx.answer(&id, false, true);
    assert_eq!(
        recv(&mut socket).await.expect("update"),
        json!({ "type": "authorizationUpdate", "authorized": false })
    );
    closed(&mut socket).await;
    assert_eq!(
        fx.stored(&client.id).await.expect("remembered").status,
        Status::Blocked
    );

    let mut socket = extension(bridge.port()).await;
    send(&mut socket, client.handshake()).await;
    assert_eq!(
        handshake_answer(&recv(&mut socket).await.expect("answer")),
        (false, false)
    );
    closed(&mut socket).await;
    fx.no_question().await;
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_changed_declaration_is_asked_again_and_runs_nothing_until_then() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    let client = TestClient::new(5);
    let (_, key) = approved(&mut fx, bridge.port(), &client).await;

    let wider = client.handshake_with(
        PROTOCOL_VERSION,
        json!({ "core": {
            "passwords": [{ "target": "*", "operation": "readWrite" }],
            "bookmarks": [{ "target": "*", "operation": "readWrite" }]
        } }),
    );
    let mut socket = extension(bridge.port()).await;
    send(&mut socket, wider).await;
    assert_eq!(
        handshake_answer(&recv(&mut socket).await.expect("answer")),
        (false, true)
    );
    fx.next_question().await;
    send(
        &mut socket,
        client.request(&key, "get-items", json!({ "requestId": "r1" })),
    )
    .await;
    let refusal = recv(&mut socket).await.expect("error");
    assert_eq!(refusal["type"], "error");
    assert_eq!(refusal["code"], "UNAUTHORIZED");
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unanswered_question_ends_as_a_denial_and_closes_in_the_window() {
    let mut fx = fixture_with_prompt_timeout(Duration::from_millis(300));
    let bridge = fx.bridge().await;
    let client = TestClient::new(6);
    let mut socket = extension(bridge.port()).await;
    send(&mut socket, client.handshake()).await;
    recv(&mut socket).await.expect("answer");
    let (id, _) = fx.next_question().await;
    assert_eq!(
        recv(&mut socket).await.expect("update")["authorized"],
        false
    );
    closed(&mut socket).await;
    assert!(fx
        .notices
        .lock()
        .expect("notices")
        .contains(&Notice::PromptCancelled(id)));
    assert!(fx.stored(&client.id).await.is_none());
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stopping_frees_the_port_and_ends_the_connections() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    let port = bridge.port();
    let client = TestClient::new(7);
    let (mut socket, _) = approved(&mut fx, port, &client).await;
    let started = std::time::Instant::now();
    bridge.stop().await;
    closed(&mut socket).await;
    assert!(started.elapsed() < Duration::from_secs(2), "SC-008");
    assert!(connect(port, Some("chrome-extension://abc")).await.is_err());
    // The port is free again: the bridge starts on it.
    let again = fx
        .start(port, short_deadline())
        .await
        .expect("restart on the same port");
    again.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_taken_port_is_reported_not_moved() {
    let fx = fixture();
    let first = fx.bridge().await;
    let second = fx.start(first.port(), short_deadline()).await;
    assert_eq!(second.err(), Some(StartError::PortInUse));
    first.stop().await;
}

// ---------------------------------------------------------------------------
// User Story 6: web pages and forged clients stay out
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn web_pages_and_missing_origins_get_no_socket() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    for origin in [
        Some("https://evil.example"),
        Some("http://localhost"),
        Some("null"),
        None,
    ] {
        let refused = connect(bridge.port(), origin).await;
        assert!(refused.is_err(), "{origin:?} must not get a socket");
    }
    fx.no_question().await;
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn handshakes_without_a_declaration_or_below_v3_are_refused() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    let client = TestClient::new(8);
    let passwords =
        json!({ "core": { "passwords": [{ "target": "*", "operation": "readWrite" }] } });
    // Below protocol 3: no declaration check, the client is told to update (FR-049).
    for version in [1, 2] {
        let mut socket = extension(bridge.port()).await;
        send(
            &mut socket,
            client.handshake_with(version, passwords.clone()),
        )
        .await;
        let error = recv(&mut socket).await.expect("error");
        assert_eq!(error["code"], "PROTOCOL_UPDATE_REQUIRED");
        closed(&mut socket).await;
    }
    for handshake in [
        client.handshake_with(
            PROTOCOL_VERSION,
            json!({ "core": { "database": [{ "target": "*" }] } }),
        ),
        client.handshake_with(PROTOCOL_VERSION, Value::Null),
    ] {
        let mut socket = extension(bridge.port()).await;
        send(&mut socket, handshake).await;
        let error = recv(&mut socket).await.expect("error");
        assert_eq!(error["type"], "error");
        assert_eq!(error["code"], "PERMISSIONS_DECLARATION_REQUIRED");
        assert_eq!(
            handshake_answer(&recv(&mut socket).await.expect("answer")),
            (false, false)
        );
        closed(&mut socket).await;
    }
    fx.no_question().await;
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_fingerprint_that_is_not_the_key_is_refused_without_a_question() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    let client = TestClient::new(9);
    let mut forged = client.handshake();
    forged["client"]["clientId"] = json!(TestClient::new(10).id);
    let mut socket = extension(bridge.port()).await;
    send(&mut socket, forged).await;
    assert_eq!(
        handshake_answer(&recv(&mut socket).await.expect("answer")),
        (false, false)
    );
    closed(&mut socket).await;
    fx.no_question().await;
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hundred_handshakes_of_one_client_raise_one_question() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    let client = TestClient::new(11);
    for _ in 0..100 {
        let mut socket = extension(bridge.port()).await;
        send(&mut socket, client.handshake()).await;
        assert_eq!(
            handshake_answer(&recv(&mut socket).await.expect("answer")),
            (false, true)
        );
    }
    fx.next_question().await;
    fx.no_question().await;
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn oversized_binary_and_silent_connections_are_closed() {
    let fx = fixture();
    let bridge = fx.bridge().await;
    let port = bridge.port();

    let mut big = extension(port).await;
    let _ = big.send(Message::text("x".repeat(65 * 1024))).await;
    closed(&mut big).await;

    let mut binary = extension(port).await;
    binary
        .send(Message::binary(vec![1u8, 2, 3]))
        .await
        .expect("send");
    closed(&mut binary).await;

    // No handshake within the deadline (400 ms here, 10 s in holzi).
    let mut silent = extension(port).await;
    closed(&mut silent).await;

    // The bridge is still alive.
    let mut socket = extension(port).await;
    send(&mut socket, TestClient::new(12).handshake()).await;
    assert_eq!(
        handshake_answer(&recv(&mut socket).await.expect("answer")),
        (false, true)
    );
    bridge.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connections_beyond_the_limit_are_dropped() {
    let fx = fixture();
    let bridge = fx
        .start(
            0,
            Limits {
                max_connections: 3,
                ..Limits::default()
            },
        )
        .await
        .expect("start");
    let mut open = Vec::new();
    for _ in 0..3 {
        open.push(extension(bridge.port()).await);
    }
    assert!(
        connect(bridge.port(), Some("chrome-extension://abc"))
            .await
            .is_err(),
        "the fourth connection is dropped"
    );
    // A closed connection frees its place once the server has seen the close.
    let mut closing = open.pop().expect("a connection");
    closing.close(None).await.expect("close");
    let port = bridge.port();
    let reconnected = tokio::time::timeout(WAIT, async move {
        while connect(port, Some("chrome-extension://abc")).await.is_err() {
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(reconnected.is_ok(), "a freed place takes a new connection");
    bridge.stop().await;
}
