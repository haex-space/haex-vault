//! Spec 049, the External Bridge against a real vault and real sockets: connecting and approving
//! (US1, T024), and keeping web pages and forged clients out (US6, T038). A small client in this
//! file speaks the protocol of haex-pass-browser (`contracts/wire-protocol.md`).

// These tests read the remembered decisions directly.
#![allow(clippy::disallowed_methods)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use curve25519_dalek::montgomery::MontgomeryPoint;
use futures::{SinkExt, StreamExt};
use haex_crdt::{Database, DatabaseConfig, NoopSignatureProvider, SqlCipherKey};
use serde_json::{json, Value};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use holzi_lib::browser_bridge::approvals::{self, Status};
use holzi_lib::browser_bridge::declaration::BridgeAccess;
use holzi_lib::browser_bridge::envelope::{client_id_of, seal_with, ServerKeys};
use holzi_lib::browser_bridge::prompts::{
    ApprovalAnswer, BridgeAnswer, BridgePrompts, BridgeQuestion,
};
use holzi_lib::browser_bridge::runtime::{Limits, Notice};
use holzi_lib::browser_bridge::server::{start, Running, StartArgs, StartError};
use holzi_lib::identity::{
    holzi_migration_source, installation_id_path, read_or_mint_installation_uuid, HolziBootstrap,
    HOLZI_TRIGGER_VERSION,
};
use holzi_lib::prompt_park::PromptPark;
use holzi_lib::storage::known_devices;
use holzi_lib::vault_gate::{VaultDb, VaultGate};

const WAIT: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// The vault and the bridge
// ---------------------------------------------------------------------------

struct Fixture {
    _dir: tempfile::TempDir,
    db: VaultDb,
    device: Uuid,
    prompts: BridgePrompts,
    asked: mpsc::UnboundedReceiver<(String, BridgeQuestion)>,
    notices: Arc<Mutex<Vec<Notice>>>,
    session: CancellationToken,
}

fn fixture() -> Fixture {
    fixture_with_prompt_timeout(Duration::from_secs(120))
}

fn fixture_with_prompt_timeout(timeout: Duration) -> Fixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let database = Database::open(DatabaseConfig {
        path: dir.path().join("vault.db"),
        key: SqlCipherKey::new("browser-bridge"),
        create_if_missing: true,
        bootstrap: Arc::new(HolziBootstrap::new(installation_id_path(dir.path()))),
        signature_provider: Arc::new(NoopSignatureProvider),
        migration_source: holzi_migration_source(),
        trigger_version: HOLZI_TRIGGER_VERSION,
        max_transaction_bytes: haex_crdt::MAX_CRDT_TRANSACTION_BYTES,
        max_value_bytes: haex_crdt::MAX_VALUE_BYTES,
    })
    .expect("open the vault");
    let installation =
        read_or_mint_installation_uuid(&installation_id_path(dir.path())).expect("installation");
    let device = holzi_lib::storage::query::read(&database, |q| {
        known_devices::get_vault_device_uuid(q, installation)
    })
    .expect("read")
    .expect("this device is registered");
    let db = VaultGate::new()
        .vault_db(Arc::new(database))
        .expect("open the gate");
    let prompts: BridgePrompts = PromptPark::new(timeout);
    let (tell, asked) = mpsc::unbounded_channel();
    prompts.set_emitter(Arc::new(move |id: &str, question: &BridgeQuestion| {
        tell.send((id.to_owned(), question.clone())).is_ok()
    }));
    Fixture {
        _dir: dir,
        db,
        device,
        prompts,
        asked,
        notices: Arc::new(Mutex::new(Vec::new())),
        session: CancellationToken::new(),
    }
}

fn short_deadline() -> Limits {
    Limits {
        handshake_deadline: Duration::from_millis(400),
        ..Limits::default()
    }
}

impl Fixture {
    async fn start(&self, port: u16, limits: Limits) -> Result<Running, StartError> {
        let notices = Arc::clone(&self.notices);
        start(StartArgs {
            db: self.db.clone(),
            device: self.device,
            port,
            parent: self.session.clone(),
            prompts: self.prompts.clone(),
            notify: Arc::new(move |notice| notices.lock().expect("notices").push(notice)),
            limits,
        })
        .await
    }

    async fn bridge(&self) -> Running {
        self.start(0, short_deadline()).await.expect("start")
    }

    /// The next question to the window, within [`WAIT`].
    async fn next_question(&mut self) -> (String, BridgeQuestion) {
        tokio::time::timeout(WAIT, self.asked.recv())
            .await
            .expect("a question within the wait")
            .expect("the emitter is alive")
    }

    /// No question arrives for a short while.
    async fn no_question(&mut self) {
        let quiet = tokio::time::timeout(Duration::from_millis(300), self.asked.recv()).await;
        assert!(quiet.is_err(), "no question expected, got {quiet:?}");
    }

    fn answer(&self, id: &str, allow: bool, remember: bool) {
        assert!(self.prompts.answer(
            id,
            BridgeAnswer::Approval(ApprovalAnswer {
                allow,
                remember,
                access: allow.then_some(BridgeAccess::ReadWrite),
                scope_tags: None,
                default_tag: None,
            }),
        ));
    }

    async fn stored(&self, client_id: &str) -> Option<approvals::StoredClient> {
        let device = self.device;
        let client_id = client_id.to_owned();
        self.db
            .read(move |q| approvals::load(q, device, &client_id))
            .await
            .expect("read")
    }
}

// ---------------------------------------------------------------------------
// A client that speaks like haex-pass-browser
// ---------------------------------------------------------------------------

struct TestClient {
    secret: [u8; 32],
    public: [u8; 32],
    id: String,
}

impl TestClient {
    fn new(seed: u8) -> Self {
        let secret = [seed; 32];
        let public = MontgomeryPoint::mul_base_clamped(secret).to_bytes();
        let id = client_id_of(&public).expect("id");
        Self { secret, public, id }
    }

    fn handshake_with(&self, version: u32, permissions: Value) -> Value {
        json!({
            "type": "handshake",
            "version": version,
            "client": {
                "clientId": self.id,
                "clientName": "haex-pass Browser Extension",
                "publicKey": STANDARD.encode(self.public),
                "requestedExtensions": [{ "name": "core", "extensionPublicKey": "__core__" }],
                "permissions": permissions,
            }
        })
    }

    fn handshake(&self) -> Value {
        self.handshake_with(
            2,
            json!({ "core": { "passwords": [{ "target": "*", "operation": "readWrite" }] } }),
        )
    }

    /// A request sealed to the server key, as `background/crypto.ts` seals it.
    fn request(&self, server_public: &str, action: &str, payload: Value) -> Value {
        let server: [u8; 32] = STANDARD
            .decode(server_public)
            .expect("b64")
            .try_into()
            .expect("32 bytes");
        let mut ephemeral = [0u8; 32];
        let mut iv = [0u8; 12];
        getrandom::fill(&mut ephemeral).expect("random");
        getrandom::fill(&mut iv).expect("random");
        let sealed =
            seal_with(ephemeral, iv, &server, payload.to_string().as_bytes()).expect("seal");
        json!({
            "type": "request", "action": action,
            "message": sealed.message, "iv": sealed.iv, "clientId": self.id,
            "publicKey": sealed.public_key,
            "extensionPublicKey": "__core__", "extensionName": "core"
        })
    }

    /// Opens an answer with the client's own key.
    fn open(&self, response: &Value) -> Value {
        let plain = ServerKeys::from_secret(self.secret)
            .open_request(
                response["publicKey"].as_str().expect("publicKey"),
                response["iv"].as_str().expect("iv"),
                response["message"].as_str().expect("message"),
            )
            .expect("open the answer");
        serde_json::from_slice(&plain).expect("json")
    }
}

type Socket = WebSocketStream<TcpStream>;

async fn connect(port: u16, origin: Option<&str>) -> Result<Socket, String> {
    let mut request = format!("ws://127.0.0.1:{port}/")
        .into_client_request()
        .expect("request");
    if let Some(origin) = origin {
        request
            .headers_mut()
            .insert("Origin", origin.parse().expect("header"));
    }
    let stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .map_err(|e| e.to_string())?;
    tokio_tungstenite::client_async(request, stream)
        .await
        .map(|(socket, _)| socket)
        .map_err(|e| e.to_string())
}

async fn extension(port: u16) -> Socket {
    connect(port, Some("chrome-extension://abcdefghijklmnop"))
        .await
        .expect("an extension connects")
}

async fn send(socket: &mut Socket, frame: Value) {
    socket
        .send(Message::text(frame.to_string()))
        .await
        .expect("send");
}

/// The next JSON frame, or `None` when the server closed.
async fn recv(socket: &mut Socket) -> Option<Value> {
    loop {
        match tokio::time::timeout(WAIT, socket.next())
            .await
            .expect("a frame within the wait")
        {
            Some(Ok(Message::Text(text))) => {
                return Some(serde_json::from_str(&text).expect("json"))
            }
            Some(Ok(Message::Ping(_) | Message::Pong(_))) => continue,
            _ => return None,
        }
    }
}

async fn closed(socket: &mut Socket) {
    assert_eq!(recv(socket).await, None, "the server closes the connection");
}

fn handshake_answer(frame: &Value) -> (bool, bool) {
    assert_eq!(frame["type"], "handshakeResponse", "{frame}");
    (
        frame["authorized"].as_bool().expect("authorized"),
        frame["pendingApproval"].as_bool().expect("pending"),
    )
}

/// Connects, approves with "remember" and returns the open, approved socket and the server key.
async fn approved(fx: &mut Fixture, port: u16, client: &TestClient) -> (Socket, String) {
    let mut socket = extension(port).await;
    send(&mut socket, client.handshake()).await;
    let first = recv(&mut socket).await.expect("answer");
    assert_eq!(handshake_answer(&first), (false, true));
    let (id, _) = fx.next_question().await;
    fx.answer(&id, true, true);
    let update = recv(&mut socket).await.expect("update");
    assert_eq!(
        update,
        json!({ "type": "authorizationUpdate", "authorized": true })
    );
    let key = first["serverPublicKey"].as_str().expect("key").to_owned();
    (socket, key)
}

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
    let BridgeQuestion::Approval(question) = question;
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

    // Until the password methods arrive, every method answers NOT_SUPPORTED, sealed to the client.
    let key = answer["serverPublicKey"].as_str().expect("key");
    let request = client.request(
        key,
        "get-items",
        json!({ "url": "https://example.de", "requestId": "r1" }),
    );
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
        2,
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
async fn handshakes_without_a_declaration_or_below_v2_are_refused() {
    let mut fx = fixture();
    let bridge = fx.bridge().await;
    let client = TestClient::new(8);
    let passwords =
        json!({ "core": { "passwords": [{ "target": "*", "operation": "readWrite" }] } });
    for handshake in [
        client.handshake_with(1, passwords),
        client.handshake_with(2, json!({ "core": { "database": [{ "target": "*" }] } })),
        client.handshake_with(2, Value::Null),
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
