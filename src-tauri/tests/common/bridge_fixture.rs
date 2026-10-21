//! The fixture of the External Bridge tests (spec 049): a real vault, a bridge on a free port with
//! its questions caught, and a client that speaks the protocol of haex-pass-browser
//! (`contracts/wire-protocol.md`). Included per binary via
//! `#[path = "common/bridge_fixture.rs"] mod bridge_fixture;`.
#![allow(dead_code, clippy::disallowed_methods)]

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

use holzi_lib::browser_bridge::approvals;
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
use holzi_lib::passwords::service::PasswordsService;
use holzi_lib::prompt_park::PromptPark;
use holzi_lib::storage::known_devices;
use holzi_lib::vault_gate::{VaultDb, VaultGate};

pub const WAIT: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// The vault and the bridge
// ---------------------------------------------------------------------------

pub struct Fixture {
    pub _dir: tempfile::TempDir,
    pub db: VaultDb,
    pub device: Uuid,
    pub prompts: BridgePrompts,
    pub asked: mpsc::UnboundedReceiver<(String, BridgeQuestion)>,
    pub notices: Arc<Mutex<Vec<Notice>>>,
    pub session: CancellationToken,
}

pub fn fixture() -> Fixture {
    fixture_with_prompt_timeout(Duration::from_secs(120))
}

pub fn fixture_with_prompt_timeout(timeout: Duration) -> Fixture {
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

pub fn short_deadline() -> Limits {
    Limits {
        handshake_deadline: Duration::from_millis(400),
        ..Limits::default()
    }
}

impl Fixture {
    pub async fn start(&self, port: u16, limits: Limits) -> Result<Running, StartError> {
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

    pub async fn bridge(&self) -> Running {
        self.start(0, short_deadline()).await.expect("start")
    }

    /// The next question to the window, within [`WAIT`].
    pub async fn next_question(&mut self) -> (String, BridgeQuestion) {
        tokio::time::timeout(WAIT, self.asked.recv())
            .await
            .expect("a question within the wait")
            .expect("the emitter is alive")
    }

    /// No question arrives for a short while.
    pub async fn no_question(&mut self) {
        let quiet = tokio::time::timeout(Duration::from_millis(300), self.asked.recv()).await;
        assert!(quiet.is_err(), "no question expected, got {quiet:?}");
    }

    pub fn answer(&self, id: &str, allow: bool, remember: bool) {
        self.answer_with(
            id,
            BridgeAnswer::Approval(ApprovalAnswer {
                allow,
                remember,
                access: allow.then_some(BridgeAccess::ReadWrite),
                scope_tags: None,
                default_tag: None,
            }),
        );
    }

    pub fn answer_with(&self, id: &str, answer: BridgeAnswer) {
        assert!(self.prompts.answer(id, answer), "the question waits");
    }

    /// The password service over the same vault, to seed entries and read what a client wrote.
    pub fn service(&self) -> PasswordsService {
        PasswordsService::new(self.db.clone())
    }

    pub async fn stored(&self, client_id: &str) -> Option<approvals::StoredClient> {
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

pub struct TestClient {
    pub secret: [u8; 32],
    pub public: [u8; 32],
    pub id: String,
}

impl TestClient {
    pub fn new(seed: u8) -> Self {
        let secret = [seed; 32];
        let public = MontgomeryPoint::mul_base_clamped(secret).to_bytes();
        let id = client_id_of(&public).expect("id");
        Self { secret, public, id }
    }

    pub fn handshake_with(&self, version: u32, permissions: Value) -> Value {
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

    pub fn handshake(&self) -> Value {
        self.handshake_with(
            2,
            json!({ "core": { "passwords": [{ "target": "*", "operation": "readWrite" }] } }),
        )
    }

    /// A request sealed to the server key, as `background/crypto.ts` seals it.
    pub fn request(&self, server_public: &str, action: &str, payload: Value) -> Value {
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
    pub fn open(&self, response: &Value) -> Value {
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

pub type Socket = WebSocketStream<TcpStream>;

pub async fn connect(port: u16, origin: Option<&str>) -> Result<Socket, String> {
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

pub async fn extension(port: u16) -> Socket {
    connect(port, Some("chrome-extension://abcdefghijklmnop"))
        .await
        .expect("an extension connects")
}

pub async fn send(socket: &mut Socket, frame: Value) {
    socket
        .send(Message::text(frame.to_string()))
        .await
        .expect("send");
}

/// The next JSON frame, or `None` when the server closed.
pub async fn recv(socket: &mut Socket) -> Option<Value> {
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

pub async fn closed(socket: &mut Socket) {
    assert_eq!(recv(socket).await, None, "the server closes the connection");
}

pub fn handshake_answer(frame: &Value) -> (bool, bool) {
    assert_eq!(frame["type"], "handshakeResponse", "{frame}");
    (
        frame["authorized"].as_bool().expect("authorized"),
        frame["pendingApproval"].as_bool().expect("pending"),
    )
}

/// Connects, approves reading and writing everything with "remember" and returns the open,
/// approved socket and the server key.
pub async fn approved(fx: &mut Fixture, port: u16, client: &TestClient) -> (Socket, String) {
    approved_as(fx, port, client, BridgeAccess::ReadWrite, None, None).await
}

/// Connects and approves with "remember" and the given grant.
pub async fn approved_as(
    fx: &mut Fixture,
    port: u16,
    client: &TestClient,
    access: BridgeAccess,
    scope_tags: Option<&[&str]>,
    default_tag: Option<&str>,
) -> (Socket, String) {
    let mut socket = extension(port).await;
    send(&mut socket, client.handshake()).await;
    let first = recv(&mut socket).await.expect("answer");
    assert_eq!(handshake_answer(&first), (false, true));
    let (id, _) = fx.next_question().await;
    fx.answer_with(
        &id,
        BridgeAnswer::Approval(ApprovalAnswer {
            allow: true,
            remember: true,
            access: Some(access),
            scope_tags: scope_tags.map(|tags| tags.iter().map(|t| (*t).to_owned()).collect()),
            default_tag: default_tag.map(str::to_owned),
        }),
    );
    let update = recv(&mut socket).await.expect("update");
    assert_eq!(
        update,
        json!({ "type": "authorizationUpdate", "authorized": true })
    );
    let key = first["serverPublicKey"].as_str().expect("key").to_owned();
    (socket, key)
}

/// Sends one request and returns the opened answer.
pub async fn call(
    socket: &mut Socket,
    client: &TestClient,
    key: &str,
    action: &str,
    mut payload: Value,
) -> Value {
    let request_id = format!("r-{action}");
    payload["requestId"] = json!(request_id);
    send(socket, client.request(key, action, payload)).await;
    let response = recv(socket).await.expect("a response");
    assert_eq!(response["type"], "response", "{response}");
    let reply = client.open(&response);
    assert_eq!(reply["requestId"], json!(request_id));
    reply
}
