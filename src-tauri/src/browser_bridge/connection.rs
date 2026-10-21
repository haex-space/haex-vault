//! One connection of a client (spec 049, data-model "Verbindung", research R4): the upgrade with
//! the origin check, the handshake, waiting for approval, and requests that run beside each other.

use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, OwnedSemaphorePermit, Semaphore};
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use tokio_util::sync::CancellationToken;

use super::approvals::{kept_name, ClientIdentity, Decision};
use super::declaration::{canonical, Declared};
use super::envelope::{client_id_of, decode_key, seal_response, KEY_LEN};
use super::methods;
use super::origin::extension_origin;
use super::protocol::{
    parse, render, ErrorCode, ErrorFrame, Handshake, HandshakeResponse, Incoming, Outgoing,
    RequestEnvelope, ResponseEnvelope, PROTOCOL_VERSION,
};
use super::runtime::{BridgeCore, Control};

/// How many frames may wait for the socket.
const OUTBOX: usize = 32;

/// Serves one accepted TCP connection until it ends; `_slot` holds its place among the
/// connections (research R1).
pub(super) async fn serve(core: Arc<BridgeCore>, stream: TcpStream, _slot: OwnedSemaphorePermit) {
    let mut origin = None;
    let config = WebSocketConfig::default()
        .max_message_size(Some(core.limits.max_message_bytes))
        .max_frame_size(Some(core.limits.max_message_bytes));
    // The shape of the result is tungstenite's `Callback`; its error is the HTTP answer itself.
    #[allow(clippy::result_large_err)]
    let callback = |request: &Request, response: Response| -> Result<Response, ErrorResponse> {
        let header = request
            .headers()
            .get("origin")
            .and_then(|value| value.to_str().ok());
        match extension_origin(header) {
            Some(found) => {
                origin = Some(found.to_owned());
                Ok(response)
            }
            None => {
                let mut refusal = ErrorResponse::new(None);
                *refusal.status_mut() = StatusCode::FORBIDDEN;
                Err(refusal)
            }
        }
    };
    let upgrade = tokio_tungstenite::accept_hdr_async_with_config(stream, callback, Some(config));
    let socket = tokio::select! {
        _ = core.token.cancelled() => return,
        result = tokio::time::timeout(core.limits.handshake_deadline, upgrade) => match result {
            Ok(Ok(socket)) => socket,
            _ => return,
        },
    };
    let Some(origin) = origin else { return };
    Connection::new(core, origin).run(socket).await;
}

/// What the handshake established.
#[derive(Clone)]
struct Peer {
    identity: ClientIdentity,
    public_key: [u8; KEY_LEN],
}

enum State {
    AwaitingHandshake,
    Pending(Peer),
    Authorized(Peer),
}

enum Flow {
    Continue,
    Close,
}

struct Connection {
    core: Arc<BridgeCore>,
    origin: String,
    token: CancellationToken,
    in_flight: Arc<Semaphore>,
    state: State,
}

impl Connection {
    fn new(core: Arc<BridgeCore>, origin: String) -> Self {
        Self {
            token: core.token.child_token(),
            in_flight: Arc::new(Semaphore::new(core.limits.max_in_flight)),
            core,
            origin,
            state: State::AwaitingHandshake,
        }
    }

    async fn run(mut self, socket: WebSocketStream<TcpStream>) {
        let (conn_id, mut control) = self.core.register();
        let (mut sink, mut source) = socket.split();
        let (out, mut outbox) = mpsc::channel::<Message>(OUTBOX);
        let writer = tokio::spawn(async move {
            while let Some(message) = outbox.recv().await {
                if sink.send(message).await.is_err() {
                    break;
                }
            }
            let _ = sink.close().await;
        });
        let deadline = tokio::time::sleep(self.core.limits.handshake_deadline);
        tokio::pin!(deadline);
        loop {
            let flow = tokio::select! {
                _ = self.token.cancelled() => Flow::Close,
                _ = &mut deadline, if matches!(self.state, State::AwaitingHandshake) => Flow::Close,
                Some(signal) = control.recv() => self.on_control(signal, &out).await,
                frame = source.next() => match frame {
                    Some(Ok(Message::Text(text))) => self.on_text(text.as_str(), conn_id, &out).await,
                    // Pings are answered by tungstenite; pongs and raw frames carry nothing.
                    Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_))) => Flow::Continue,
                    // Binary frames are no part of the protocol; close, an error or the end of the
                    // stream end the connection.
                    _ => Flow::Close,
                },
            };
            if matches!(flow, Flow::Close) {
                break;
            }
        }
        // Ends the requests and questions of this connection (FR-031: nothing after the end).
        self.token.cancel();
        self.core.unregister(conn_id);
        drop(out);
        let _ = writer.await;
    }

    async fn on_control(&mut self, signal: Control, out: &mpsc::Sender<Message>) -> Flow {
        match signal {
            Control::Authorized => {
                let state = std::mem::replace(&mut self.state, State::AwaitingHandshake);
                self.state = match state {
                    State::Pending(peer) => {
                        send(out, &Outgoing::AuthorizationUpdate { authorized: true }).await;
                        State::Authorized(peer)
                    }
                    other => other,
                };
                Flow::Continue
            }
            Control::Denied => {
                send(out, &Outgoing::AuthorizationUpdate { authorized: false }).await;
                Flow::Close
            }
        }
    }

    async fn on_text(&mut self, text: &str, conn_id: u64, out: &mpsc::Sender<Message>) -> Flow {
        match parse(text) {
            Ok(Incoming::Handshake(handshake)) => self.on_handshake(handshake, conn_id, out).await,
            Ok(Incoming::Request(request)) => {
                self.on_request(request, out).await;
                Flow::Continue
            }
            Ok(Incoming::Ping) => {
                send(out, &Outgoing::Pong).await;
                Flow::Continue
            }
            Ok(Incoming::Pong) => Flow::Continue,
            Err(code) => {
                send(out, &Outgoing::Error(ErrorFrame::new(code))).await;
                Flow::Continue
            }
        }
    }

    async fn on_handshake(
        &mut self,
        handshake: Handshake,
        conn_id: u64,
        out: &mpsc::Sender<Message>,
    ) -> Flow {
        let declared = match Declared::read(handshake.client.permissions.as_ref()) {
            Ok(declared) if handshake.version >= PROTOCOL_VERSION => declared,
            _ => {
                let frame = ErrorFrame::new(ErrorCode::PermissionsDeclarationRequired);
                send(out, &Outgoing::Error(frame)).await;
                self.answer(out, false, false).await;
                return Flow::Close;
            }
        };
        let client = &handshake.client;
        let public_key = match decode_key(&client.public_key) {
            Ok(key) if client_id_of(&key).as_deref() == Ok(client.client_id.as_str()) => key,
            _ => {
                log::info!("browser bridge: handshake with a fingerprint that is not its key");
                self.answer(out, false, false).await;
                return Flow::Close;
            }
        };
        let peer = Peer {
            identity: ClientIdentity {
                client_id: client.client_id.clone(),
                public_key: client.public_key.clone(),
                client_name: kept_name(client.client_name.as_deref()),
                origin: self.origin.clone(),
                declaration: canonical(
                    client.permissions.as_ref(),
                    client.requested_extensions.as_ref(),
                ),
            },
            public_key,
        };
        // The extension repeats its handshake while it waits; the same client gets the current
        // state, another client on this socket ends it (research R4).
        match &self.state {
            State::Pending(known) | State::Authorized(known)
                if known.identity.client_id != peer.identity.client_id =>
            {
                return Flow::Close;
            }
            _ => {}
        }
        self.core.bind(conn_id, &peer.identity.client_id);
        let decision = match self.core.decide(&peer.identity).await {
            Ok(decision) => decision,
            Err(error) => {
                log::warn!("browser bridge: could not read the decisions: {error}");
                self.answer(out, false, false).await;
                return Flow::Close;
            }
        };
        let short = short_id(&peer.identity.client_id);
        match decision {
            Decision::Blocked => {
                log::info!("browser bridge: client {short} blocked");
                self.answer(out, false, false).await;
                Flow::Close
            }
            Decision::Authorized { remembered, .. } => {
                if remembered {
                    self.core.touch(&peer.identity.client_id).await;
                }
                self.state = State::Authorized(peer);
                self.answer(out, true, false).await;
                Flow::Continue
            }
            Decision::Ask => {
                log::info!("browser bridge: client {short} waits for approval");
                self.core.request_approval(peer.identity.clone(), declared);
                self.state = State::Pending(peer);
                self.answer(out, false, true).await;
                Flow::Continue
            }
        }
    }

    async fn answer(&self, out: &mpsc::Sender<Message>, authorized: bool, pending: bool) {
        let frame = Outgoing::HandshakeResponse(HandshakeResponse {
            version: PROTOCOL_VERSION,
            server_public_key: self.core.keys.public_b64(),
            authorized,
            pending_approval: pending,
        });
        send(out, &frame).await;
    }

    async fn on_request(&self, request: RequestEnvelope, out: &mpsc::Sender<Message>) {
        let State::Authorized(peer) = &self.state else {
            let frame = ErrorFrame::new(ErrorCode::Unauthorized);
            send(out, &Outgoing::Error(frame)).await;
            return;
        };
        let opened = self
            .core
            .keys
            .open_request(&request.public_key, &request.iv, &request.message)
            .ok()
            .and_then(|plain| serde_json::from_slice::<Value>(&plain).ok());
        let Some(Value::Object(mut payload)) = opened else {
            let frame = ErrorFrame::new(ErrorCode::DecryptionError);
            send(out, &Outgoing::Error(frame)).await;
            return;
        };
        // Without a request id the client cannot match an answer; it is dropped (research R4).
        let request_id = match payload.remove("requestId") {
            Some(Value::String(id)) if !id.is_empty() => id,
            _ => return,
        };
        let Ok(slot) = Arc::clone(&self.in_flight).try_acquire_owned() else {
            let reply = methods::busy(request_id);
            reply_to(out, peer, &request.action, &reply).await;
            return;
        };
        let core = Arc::clone(&self.core);
        let peer = peer.clone();
        let out = out.clone();
        let token = self.token.clone();
        tokio::spawn(async move {
            let _slot = slot;
            let reply = methods::dispatch(
                &core,
                &peer.identity,
                &request,
                request_id,
                Value::Object(payload),
                &token,
            )
            .await;
            if !token.is_cancelled() {
                reply_to(&out, &peer, &request.action, &reply).await;
            }
        });
    }
}

async fn reply_to(
    out: &mpsc::Sender<Message>,
    peer: &Peer,
    action: &str,
    reply: &super::protocol::Reply,
) {
    let plaintext = serde_json::to_vec(reply).unwrap_or_default();
    let Ok(sealed) = seal_response(&peer.public_key, &plaintext) else {
        return;
    };
    let frame = Outgoing::Response(ResponseEnvelope {
        action: action.to_owned(),
        message: sealed.message,
        iv: sealed.iv,
        client_id: String::new(),
        public_key: sealed.public_key,
        extension_public_key: None,
        extension_name: None,
    });
    send(out, &frame).await;
}

async fn send(out: &mpsc::Sender<Message>, frame: &Outgoing) {
    let _ = out.send(Message::text(render(frame))).await;
}

/// The first 8 digits of a fingerprint: enough to tell clients apart in a log (FR-039).
pub(super) fn short_id(client_id: &str) -> &str {
    client_id.get(..8).unwrap_or(client_id)
}
