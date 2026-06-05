//! iroh Endpoint management
//!
//! Manages the iroh QUIC endpoint: starting, stopping, accepting connections,
//! and handling incoming file requests. Access control ensures only peers
//! registered in the same Space can access shared folders.
//!
//! On Android, shared folders may use Content URIs (from the Storage Access
//! Framework). These are opaque URIs that require `tauri_plugin_android_fs` for
//! reading — standard `std::fs` calls do not work. The handlers detect Content
//! URI shares (JSON strings starting with `{`) and delegate to the android_fs
//! plugin via the `AppHandle` stored in `PeerState`.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;

use iroh::{
    endpoint::{QuicTransportConfig, VarInt},
    Endpoint, EndpointAddr, EndpointId, RelayMode, RelayUrl, SecretKey,
};

const DEFAULT_RELAY_URL: &str = "https://relay.sync.haex.space";

use tauri::Emitter;

use ed25519_dalek::SigningKey;

use crate::peer_storage::error::PeerStorageError;
use crate::peer_storage::protocol::{self, Request, Response, ALPN};

/// Identity material the endpoint uses to prove its DID on outbound
/// connections and to challenge inbound peers. Loaded once at start from
/// `haex_devices.owner_did` + `haex_identities.private_key`. Held in
/// `PeerEndpoint`, NOT in `PeerState` — the secret key never leaves the
/// endpoint struct.
#[derive(Clone)]
pub struct OwnIdentity {
    pub did: String,
    pub signing_key: SigningKey,
}

impl std::fmt::Debug for OwnIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnIdentity")
            .field("did", &self.did)
            .field("signing_key", &"<redacted>")
            .finish()
    }
}

// ============================================================================
// Delivery connection handler trait
// ============================================================================

/// Trait for handling space delivery connections. Implemented by space_delivery module.
pub trait DeliveryConnectionHandler: Send + Sync {
    fn handle_connection(
        &self,
        conn: iroh::endpoint::Connection,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>>;
}

// ============================================================================
// Shared state
// ============================================================================

/// A folder shared with peers
#[derive(Debug, Clone)]
pub struct SharedFolder {
    /// Display name
    pub name: String,
    /// Local filesystem path or Android Content URI (JSON string starting with `{`)
    pub local_path: String,
    /// Space this share belongs to (for access control)
    pub space_id: String,
}

/// Check if a path string is an Android Content URI (JSON-encoded)
pub fn is_content_uri(path: &str) -> bool {
    path.starts_with('{')
}

/// What kind of network path a QUIC connection is currently using.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PathType {
    /// Hole-punched/LAN — packets travel directly between the two endpoints.
    Direct,
    /// Relayed via a relay server — every packet round-trips through the relay.
    Relay,
    /// Connection exists but the path type is not classifiable.
    Unknown,
    /// Connection has already been closed.
    Closed,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionDiagnostics {
    pub path_type: PathType,
    pub remote_addr: Option<String>,
    pub rtt_ms: Option<f64>,
}

/// State shared between PeerEndpoint methods and the accept loop
pub struct PeerState {
    /// Shared folders (share_id -> folder)
    pub shares: HashMap<String, SharedFolder>,
    /// Access control: remote EndpointId (string) -> set of space_ids they may access
    pub allowed_peers: HashMap<String, HashSet<String>>,
    /// Tauri AppHandle for android_fs operations (set on Android before start)
    pub app_handle: Option<tauri::AppHandle>,
    /// Handler for incoming space delivery connections (set by space_delivery module)
    pub delivery_handler: Option<Arc<dyn DeliveryConnectionHandler>>,
    /// Verified DID per connected remote endpoint id, populated by the
    /// quic_did_auth handshake at connection-accept time. Used by request
    /// handlers to enforce UCAN audience match. Cleared when the connection
    /// closes.
    pub endpoint_dids: HashMap<String, String>,
    /// Expected owner DID per remote endpoint id, loaded from
    /// `haex_devices.owner_did` (which the `haex_space_devices_ensure_refs`
    /// trigger populates from UCAN-attributed CRDT rows). Cross-checked
    /// against the cryptographically verified DID from the quic_did_auth
    /// handshake — any mismatch is treated as a vault-internal inconsistency
    /// (database drift, partial sync, or worse) and the connection is closed.
    pub peer_owner_dids: HashMap<String, String>,
    /// Live connection-watcher count per remote endpoint. A peer can have
    /// several concurrent connections (inbound + outbound, or a stale one
    /// lingering across a reconnect); the watcher only emits a `Closed`
    /// diagnostic once the LAST one is torn down, so a transient connection
    /// dropping never flips a still-live peer offline in the UI.
    pub connection_watchers: HashMap<EndpointId, u32>,
}

impl Default for PeerState {
    fn default() -> Self {
        Self {
            shares: HashMap::new(),
            allowed_peers: HashMap::new(),
            app_handle: None,
            delivery_handler: None,
            endpoint_dids: HashMap::new(),
            peer_owner_dids: HashMap::new(),
            connection_watchers: HashMap::new(),
        }
    }
}

// Manual Debug impl because tauri::AppHandle doesn't implement Debug
impl std::fmt::Debug for PeerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PeerState")
            .field("shares", &self.shares)
            .field("allowed_peers", &self.allowed_peers)
            .field(
                "app_handle",
                &self.app_handle.as_ref().map(|_| "Some(AppHandle)"),
            )
            .field(
                "delivery_handler",
                &self
                    .delivery_handler
                    .as_ref()
                    .map(|_| "Some(DeliveryConnectionHandler)"),
            )
            .finish()
    }
}

/// Peer storage endpoint state
pub struct PeerEndpoint {
    /// The iroh endpoint (None if not running)
    endpoint: Option<Endpoint>,
    /// Secret key for this node
    secret_key: SecretKey,
    /// Shared state (accessible by both endpoint methods and accept loop)
    pub(crate) state: Arc<RwLock<PeerState>>,
    /// Handle to the accept loop task
    accept_task: Option<tokio::task::JoinHandle<()>>,
    /// Handle to the endpoint-closed watcher task; aborted on user-initiated stop
    /// so it does not emit a spurious "endpoint-closed" event that would trigger
    /// the TS auto-restart handler.
    watcher_task: Option<tokio::task::JoinHandle<()>>,
    /// Configured relay URL (set at start, available even before relay connection is established)
    configured_relay_url: Option<RelayUrl>,
    /// Cached connections to remote peers. Reusing a single QUIC connection for
    /// multiple streams avoids per-request TLS handshakes and the race condition
    /// where a closing connection interferes with a subsequent connect() call.
    ///
    /// **Cached connections are always already-authenticated**: the
    /// `quic_did_auth` handshake runs once on the first opened/accepted
    /// bi-stream of a fresh connection. Subsequent stream opens on the same
    /// connection skip the handshake.
    connections: Mutex<HashMap<EndpointId, iroh::endpoint::Connection>>,
    /// Identity used to prove our DID to remote peers (outbound) and to
    /// challenge inbound peers. Set via `set_own_identity` before `start`.
    /// `None` until set — used only by the quic_did_auth handshake; other
    /// peer_storage paths do not depend on it.
    ///
    /// Held in an `Arc<Mutex<_>>` so the accept loop and concurrent `open_stream`
    /// calls can read it without going through the outer endpoint reference.
    own_identity: Arc<Mutex<Option<OwnIdentity>>>,
}

impl PeerEndpoint {
    /// Create a new PeerEndpoint with a persistent device key.
    pub fn new(secret_key: SecretKey) -> Self {
        Self {
            endpoint: None,
            secret_key,
            state: Arc::new(RwLock::new(PeerState::default())),
            accept_task: None,
            watcher_task: None,
            configured_relay_url: None,
            connections: Mutex::new(HashMap::new()),
            own_identity: Arc::new(Mutex::new(None)),
        }
    }

    /// Set the DID + signing key used by the quic_did_auth handshake.
    ///
    /// Must be called before `start` — swapping the identity while the
    /// endpoint is running would leave cached connections authenticated
    /// against the *old* DID while new connections authenticate against the
    /// new one, producing a single endpoint that effectively serves two
    /// mixed identities. Panics in that case (programmer error, not a
    /// recoverable runtime condition).
    pub fn set_own_identity(&self, identity: OwnIdentity) {
        assert!(
            self.endpoint.is_none(),
            "set_own_identity must be called before start (endpoint is already running)"
        );
        if let Ok(mut slot) = self.own_identity.lock() {
            *slot = Some(identity);
        }
    }

    /// Clone the configured identity, if any. Used by the accept loop and by
    /// `open_stream` to drive the handshake.
    fn own_identity(&self) -> Option<OwnIdentity> {
        self.own_identity.lock().ok().and_then(|g| g.clone())
    }

    /// Create a PeerEndpoint with a temporary random key (for testing or pre-init state).
    pub fn new_ephemeral() -> Self {
        let bytes: [u8; 32] = rand::random();
        Self::new(SecretKey::from_bytes(&bytes))
    }

    /// Replace the secret key with a persistent device key.
    /// Must be called before starting the endpoint. Panics if endpoint is running.
    pub fn replace_key(&mut self, secret_key: SecretKey) {
        assert!(
            self.endpoint.is_none(),
            "Cannot replace key while endpoint is running"
        );
        self.secret_key = secret_key;
    }

    /// Store the Tauri AppHandle for android_fs operations.
    /// Must be called before start() on Android so Content URI shares can be served.
    pub async fn set_app_handle(&self, app_handle: tauri::AppHandle) {
        self.state.write().await.app_handle = Some(app_handle);
    }

    /// Register a handler for space delivery connections.
    pub async fn set_delivery_handler(&self, handler: Arc<dyn DeliveryConnectionHandler>) {
        self.state.write().await.delivery_handler = Some(handler);
    }

    /// Get the public EndpointId
    pub fn endpoint_id(&self) -> EndpointId {
        self.secret_key.public()
    }

    /// Check if the endpoint is running
    pub fn is_running(&self) -> bool {
        self.endpoint.is_some()
    }

    /// Get the configured relay URL (available even before relay connection is established)
    pub fn configured_relay_url(&self) -> Option<&RelayUrl> {
        self.configured_relay_url.as_ref()
    }

    /// Start the iroh endpoint and begin accepting connections.
    /// `relay_url` — optional relay URL from vault settings; falls back to
    /// `HAEX_RELAY_URL` env var, then iroh's default relay servers.
    pub async fn start(
        &mut self,
        relay_url: Option<String>,
    ) -> Result<EndpointId, PeerStorageError> {
        if self.endpoint.is_some() {
            return Err(PeerStorageError::EndpointAlreadyRunning);
        }

        let effective_relay = relay_url
            .filter(|s| !s.is_empty())
            .or_else(|| {
                std::env::var("HAEX_RELAY_URL")
                    .ok()
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or_else(|| DEFAULT_RELAY_URL.to_string());

        let relay_mode = match effective_relay.parse::<RelayUrl>() {
            Ok(parsed) => {
                eprintln!("[PeerStorage] Using relay: {effective_relay}");
                self.configured_relay_url = Some(parsed.clone());
                RelayMode::custom([parsed])
            }
            Err(e) => {
                eprintln!(
                    "[PeerStorage] Invalid relay URL '{effective_relay}': {e} \
                     — falling back to iroh default"
                );
                RelayMode::Default
            }
        };

        // Tune QUIC transport for LAN/WAN bulk transfers. iroh's noq-default
        // sizes its windows for 100 Mbps × 100 ms = 1.25 MB per stream, which
        // becomes the bottleneck for large files: the receiver's stream-level
        // flow control fills up while disk writes are in flight, the sender
        // stalls waiting for window updates, and per-stream throughput pegs
        // around 1 MB/s regardless of link capacity. Sizing the window for
        // ~1 Gbps × 50 ms (worst-case LAN+jitter) gives room for ~6 MB in
        // flight per stream without ballooning RAM (worst-case usage =
        // max_concurrent_bidi_streams × stream_receive_window).
        let transport_config = QuicTransportConfig::builder()
            .stream_receive_window(VarInt::from_u32(16 * 1024 * 1024))
            .send_window(64 * 1024 * 1024)
            .max_concurrent_bidi_streams(VarInt::from_u32(256))
            .build();

        // iroh's `.bind()` can hang indefinitely if relay-URL DNS lookup
        // stalls or socket binding loops on transient OS errors. Cap at 15s
        // so a hung start surfaces as a fast error and the caller can retry,
        // instead of wedging the whole frontend (observed in CI as 30s+
        // playwright timeouts on `peer_storage_start`).
        let bind_future = Endpoint::builder(iroh::endpoint::presets::N0)
            .secret_key(self.secret_key.clone())
            .alpns(vec![
                ALPN.to_vec(),
                crate::space_delivery::local::protocol::ALPN.to_vec(),
            ])
            .relay_mode(relay_mode)
            .address_lookup(
                iroh::address_lookup::MdnsAddressLookup::builder().service_name("haex-peer"),
            )
            .transport_config(transport_config)
            .bind();
        let endpoint = tokio::time::timeout(std::time::Duration::from_secs(15), bind_future)
            .await
            .map_err(|_| PeerStorageError::ConnectionFailed {
                reason: "Endpoint bind timed out after 15s".to_string(),
            })?
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: format!("Failed to bind endpoint: {e}"),
            })?;

        let id = endpoint.id();
        eprintln!("[PeerStorage] Endpoint started with ID: {id}");

        // Spawn accept loop with shared state and (a handle to) the own-
        // identity slot so the quic_did_auth handshake can find it. Sharing
        // the Arc<Mutex<_>> (rather than a snapshot) means a late
        // `set_own_identity` after `start` still reaches the accept loop.
        let ep = endpoint.clone();
        let state = self.state.clone();
        let own_identity = self.own_identity.clone();

        let accept_task = tokio::spawn(async move {
            accept_loop(ep, state, own_identity).await;
        });

        // Endpoint death watcher (diag/multi-leader-quic-logging branch).
        // The iroh endpoint can become "closed" without us calling stop()
        // — typically when an internal task (relay actor, socket
        // transport) gives up after unrecoverable errors. The sync loop
        // then spins forever in exponential-backoff reconnect with
        // "Endpoint is closed", and we currently have zero visibility
        // into the why or the when.
        //
        // This watcher resolves when iroh signals the endpoint as
        // closed, and writes one entry to haex_logs with the elapsed
        // uptime. Correlate the timestamp with the stderr stream
        // around it (sync_loop, multi_leader, relay) to spot the
        // trigger event.
        let watch_endpoint = endpoint.clone();
        let watch_state = self.state.clone();
        let started_at = std::time::Instant::now();
        let endpoint_id_short = id.fmt_short();
        let watcher_task = tokio::spawn(async move {
            watch_endpoint.closed().await;
            let uptime = started_at.elapsed();
            let msg = format!(
                "iroh endpoint reported closed after {}s {}ms uptime (id={})",
                uptime.as_secs(),
                uptime.subsec_millis(),
                endpoint_id_short,
            );
            eprintln!("[Endpoint] {msg}");
            let app_handle = watch_state.read().await.app_handle.clone();
            if let Some(app) = app_handle {
                if let Some(state) = <tauri::AppHandle as tauri::Manager<tauri::Wry>>::try_state::<crate::AppState>(&app) {
                    let _ = crate::logging::insert_log(
                        &state, "error", "Endpoint", None, &msg, None, "rust",
                    );
                }
                let _ = app.emit_to(
                    "main",
                    crate::event_names::EVENT_PEER_STORAGE_STATE_CHANGED,
                    serde_json::json!({
                        "running": false,
                        "reason": "endpoint-closed",
                        "uptimeSecs": uptime.as_secs(),
                    }),
                );
            }
        });

        self.endpoint = Some(endpoint);
        self.accept_task = Some(accept_task);
        self.watcher_task = Some(watcher_task);

        Ok(id)
    }

    /// Stop the endpoint
    pub async fn stop(&mut self) -> Result<(), PeerStorageError> {
        if let Ok(mut cache) = self.connections.lock() {
            cache.clear();
        }

        // Abort the watcher before closing so it cannot emit a spurious
        // "endpoint-closed" event that would trigger the TS auto-restart handler.
        if let Some(task) = self.watcher_task.take() {
            task.abort();
        }

        if let Some(task) = self.accept_task.take() {
            task.abort();
        }

        if let Some(endpoint) = self.endpoint.take() {
            // iroh's graceful close waits for peers to ACK QUIC CLOSE frames.
            // With default RTT estimates this can block up to ~30s when a peer
            // is unreachable (network switch, peer offline) — long enough for
            // the user-facing logout/lock to feel hung. Bound it: healthy
            // peers complete in well under a second, dead ones fall through.
            let close_timeout = std::time::Duration::from_secs(2);
            match tokio::time::timeout(close_timeout, endpoint.close()).await {
                Ok(()) => eprintln!("[PeerStorage] Endpoint stopped"),
                Err(_) => eprintln!(
                    "[PeerStorage] Endpoint close exceeded {}s, peer ACKs abandoned",
                    close_timeout.as_secs()
                ),
            }
        }

        Ok(())
    }

    /// Inspect the cached connection to a peer and report whether it currently
    /// runs over a direct LAN/WAN path or via the relay. Returns `None` if
    /// there is no live connection — call only after the engine has issued at
    /// least one stream against the peer.
    ///
    /// This exists primarily to debug the "iroh fell back to relay" failure
    /// mode, which presents as a steady ~1 MB/s ceiling per stream and looks
    /// like a code-tuning problem until you check the path type.
    ///
    /// For push updates when the path changes, the frontend should listen for
    /// `peer-storage:connection-changed` events emitted by
    /// `spawn_connection_watcher` (started for every connection that lands in
    /// the cache).
    pub fn diagnose_connection(&self, remote_id: EndpointId) -> Option<ConnectionDiagnostics> {
        self.connections
            .lock()
            .ok()
            .and_then(|cache| cache.get(&remote_id).cloned())
            .map(|conn| compute_diagnostics(&conn))
    }

    /// Get a cached QUIC connection or establish a new one, then open a
    /// bidirectional stream. If a cached connection is stale, it is evicted
    /// and a fresh one is created automatically.
    pub(crate) async fn open_stream(
        &self,
        remote_id: EndpointId,
        relay_url: Option<RelayUrl>,
    ) -> Result<(iroh::endpoint::SendStream, iroh::endpoint::RecvStream), PeerStorageError> {
        let endpoint = self
            .endpoint
            .as_ref()
            .ok_or(PeerStorageError::EndpointNotRunning)?;

        // Try the cached connection first
        let cached = self
            .connections
            .lock()
            .ok()
            .and_then(|cache| cache.get(&remote_id).cloned());

        if let Some(conn) = cached {
            // A cached connection can be half-closed after the remote revokes
            // authorization: open_bi() may optimistically succeed, then the
            // subsequent read hangs until QUIC's idle timeout (~41s). Detect
            // stale connections via close_reason(), and bound open_bi() so a
            // connection that has silently died cannot stall the caller.
            if conn.close_reason().is_none() {
                if let Ok(Ok(streams)) = tokio::time::timeout(
                    std::time::Duration::from_secs(3),
                    conn.open_bi(),
                )
                .await
                {
                    return Ok(streams);
                }
            }
            // Stale or corrupted — evict from cache. Do NOT call .close() here:
            // parallel tasks may still hold streams on this connection, and an
            // explicit close would tear them down mid-transfer.
            if let Ok(mut cache) = self.connections.lock() {
                cache.remove(&remote_id);
            }
        }

        // Establish a new connection
        let addr = match relay_url {
            Some(url) => EndpointAddr::new(remote_id).with_relay_url(url),
            None => EndpointAddr::new(remote_id),
        };

        // iroh's connect() has no caller-visible timeout; if the peer is
        // unreachable the QUIC handshake hangs ~30s before failing. That
        // makes file-sync feel hung when a peer just isn't online. 8s is
        // generous for LAN (<100ms), hole-punched WAN (~1-3s), and relay
        // (~1-2s) paths while failing fast on truly dead peers.
        let conn = match tokio::time::timeout(
            std::time::Duration::from_secs(8),
            endpoint.connect(addr, ALPN),
        )
        .await
        {
            Ok(Ok(conn)) => conn,
            Ok(Err(e)) => {
                return Err(PeerStorageError::ConnectionFailed {
                    reason: e.to_string(),
                });
            }
            Err(_) => {
                return Err(PeerStorageError::ConnectionFailed {
                    reason: "connect handshake timed out after 8s".to_string(),
                });
            }
        };

        // -- Phase 1: DID handshake on a server-initiated bi-stream --
        //
        // Server `handle_connection` calls `open_bi` and writes the Challenge
        // first; we await it here with `accept_bi`. Doing it that direction
        // avoids a both-sides-blocked-on-read deadlock — `open_bi` alone does
        // not materialise the stream on the wire, so client-initiated +
        // server-initiated reads would both block forever.
        let identity = self.own_identity().ok_or_else(|| PeerStorageError::ConnectionFailed {
            reason: "own identity not configured — call set_own_identity before open_stream".into(),
        })?;
        let own_endpoint_id_str = endpoint.id().to_string();

        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            conn.accept_bi(),
        )
        .await
        {
            Ok(Ok((mut auth_send, mut auth_recv))) => {
                if let Err(e) = crate::quic_did_auth::respond_to_challenge(
                    &mut auth_send,
                    &mut auth_recv,
                    &identity.did,
                    &identity.signing_key,
                    &own_endpoint_id_str,
                )
                .await
                {
                    return Err(PeerStorageError::ConnectionFailed {
                        reason: format!("DID-auth handshake failed: {e}"),
                    });
                }
                // Close the auth stream cleanly so the server sees end-of-send
                // and can hand off to its Phase 2 accept loop without delay.
                let _ = auth_send.finish();
            }
            Ok(Err(e)) => {
                return Err(PeerStorageError::ConnectionFailed {
                    reason: format!("accept auth stream: {e}"),
                });
            }
            Err(_) => {
                return Err(PeerStorageError::ConnectionFailed {
                    reason: "accept auth stream timed out after 5s".to_string(),
                });
            }
        };

        // -- Phase 2: open the actual request stream --
        //
        // Same rationale as the cached-path open_bi: never let stream open
        // outlast the connect bound.
        let streams = match tokio::time::timeout(
            std::time::Duration::from_secs(3),
            conn.open_bi(),
        )
        .await
        {
            Ok(Ok(streams)) => streams,
            Ok(Err(e)) => {
                return Err(PeerStorageError::ConnectionFailed {
                    reason: e.to_string(),
                });
            }
            Err(_) => {
                return Err(PeerStorageError::ConnectionFailed {
                    reason: "open_bi timed out after 3s".to_string(),
                });
            }
        };

        if let Ok(mut cache) = self.connections.lock() {
            cache.insert(remote_id, conn.clone());
        }
        // Push connection-changed events on path switches (direct↔relay) and
        // drop. Cheaper than periodic polling from the frontend and gives the
        // UI a real-time signal without a setInterval.
        spawn_connection_watcher(remote_id, conn, self.state.clone());
        Ok(streams)
    }

    /// Encode a request, send it on the stream, signal end-of-send, and read the response.
    pub(crate) async fn send_request(
        send: &mut iroh::endpoint::SendStream,
        recv: &mut iroh::endpoint::RecvStream,
        req: &Request,
    ) -> Result<Response, PeerStorageError> {
        let req_bytes = protocol::encode_request(req)
            .map_err(|e| PeerStorageError::ProtocolError {
                reason: e.to_string(),
            })?;
        send.write_all(&req_bytes)
            .await
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: e.to_string(),
            })?;
        send.finish()
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: e.to_string(),
            })?;

        protocol::read_response(recv)
            .await
            .map_err(|e| PeerStorageError::ProtocolError {
                reason: e.to_string(),
            })
    }

    /// Send a request header without finishing the send side (caller will stream more data).
    pub(crate) async fn send_request_header(
        send: &mut iroh::endpoint::SendStream,
        req: &Request,
    ) -> Result<(), PeerStorageError> {
        let req_bytes = protocol::encode_request(req)
            .map_err(|e| PeerStorageError::ProtocolError {
                reason: e.to_string(),
            })?;
        send.write_all(&req_bytes)
            .await
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: e.to_string(),
            })?;
        Ok(())
    }

    /// Add a shared folder
    pub async fn add_share(&self, id: String, name: String, local_path: String, space_id: String) {
        eprintln!("[PeerStorage] Added share '{name}' at {local_path} (space: {space_id})");
        self.state.write().await.shares.insert(
            id,
            SharedFolder {
                name,
                local_path,
                space_id,
            },
        );
    }

    /// Remove a shared folder
    pub async fn remove_share(&self, id: &str) -> bool {
        self.state.write().await.shares.remove(id).is_some()
    }

    /// List shared folders
    pub async fn list_shares(&self) -> Vec<(String, SharedFolder)> {
        self.state
            .read()
            .await
            .shares
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Clear all shares (used before reloading from DB)
    pub async fn clear_shares(&self) {
        self.state.write().await.shares.clear();
    }

    /// Get a reference to the underlying iroh endpoint
    pub fn endpoint_ref(&self) -> Option<&Endpoint> {
        self.endpoint.as_ref()
    }

    /// Update the allowed peers map (remote EndpointId -> set of space_ids)
    pub async fn set_allowed_peers(&self, allowed: HashMap<String, HashSet<String>>) {
        eprintln!(
            "[PeerStorage] Updated allowed peers: {} peers across spaces",
            allowed.len()
        );
        self.state.write().await.allowed_peers = allowed;
    }

    /// Update the expected `(endpoint_id -> owner_did)` map used as a
    /// defense-in-depth cross-check against the cryptographically verified
    /// DID from the handshake. Loaded from `haex_devices.owner_did` for
    /// every endpoint we expect to see — keep this in sync with
    /// `allowed_peers`, since a peer that passes `allowed_peers` but has no
    /// entry here will be rejected by `handle_connection`.
    pub async fn set_peer_owner_dids(&self, dids: HashMap<String, String>) {
        eprintln!(
            "[PeerStorage] Updated peer owner DIDs: {} entries",
            dids.len()
        );
        self.state.write().await.peer_owner_dids = dids;
    }

    /// Local-only endpoint start for unit tests. Binds with `RelayMode::Disabled`
    /// and no address-lookup service, so the test does not depend on DNS or relay
    /// servers. Spawns the accept loop; omits the production endpoint-closed
    /// watcher (which depends on a Tauri AppHandle).
    #[cfg(test)]
    pub(crate) async fn start_for_test(&mut self) -> Result<EndpointId, PeerStorageError> {
        if self.endpoint.is_some() {
            return Err(PeerStorageError::EndpointAlreadyRunning);
        }

        let endpoint = Endpoint::builder(iroh::endpoint::presets::Minimal)
            .secret_key(self.secret_key.clone())
            .alpns(vec![ALPN.to_vec()])
            .relay_mode(RelayMode::Disabled)
            .bind()
            .await
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: format!("Failed to bind test endpoint: {e}"),
            })?;

        let id = endpoint.id();

        let ep = endpoint.clone();
        let state = self.state.clone();
        let own_identity = self.own_identity.clone();
        let accept_task = tokio::spawn(async move {
            accept_loop(ep, state, own_identity).await;
        });

        self.endpoint = Some(endpoint);
        self.accept_task = Some(accept_task);

        Ok(id)
    }

    /// Generate a fresh ed25519 keypair and install it as the endpoint's
    /// own identity. Returns the generated DID so tests can mint UCANs whose
    /// audience matches the verified peer DID checked in handle_stream.
    #[cfg(test)]
    pub(crate) fn set_random_test_identity(&self) -> String {
        let seed: [u8; 32] = rand::random();
        let signing_key = SigningKey::from_bytes(&seed);
        let mut did_bytes = Vec::with_capacity(34);
        did_bytes.extend_from_slice(&[0xed, 0x01]);
        did_bytes.extend_from_slice(signing_key.verifying_key().as_bytes());
        let did = format!("did:key:z{}", bs58::encode(did_bytes).into_string());
        self.set_own_identity(OwnIdentity {
            did: did.clone(),
            signing_key,
        });
        did
    }

    /// Pre-populate the connection cache with a direct-address QUIC connection
    /// to `remote_addr`. After this returns, `open_stream(remote_id, None)` will
    /// reuse the cached connection. Used by tests to bypass the relay /
    /// address-lookup path that production `open_stream` relies on, since unit
    /// tests run with `RelayMode::Disabled` and no DNS publishing.
    ///
    /// Runs the quic_did_auth handshake on the first opened bi-stream so the
    /// cached connection is fully authenticated (matching production
    /// `open_stream`). Callers must have called `set_random_test_identity` (or
    /// `set_own_identity`) on the client side, and the server-side endpoint
    /// must also have a configured identity for the handshake to complete.
    #[cfg(test)]
    pub(crate) async fn connect_for_test(
        &self,
        remote_addr: EndpointAddr,
    ) -> Result<(), PeerStorageError> {
        let endpoint = self
            .endpoint
            .as_ref()
            .ok_or(PeerStorageError::EndpointNotRunning)?;
        let remote_id = remote_addr.id;
        let conn = endpoint
            .connect(remote_addr, ALPN)
            .await
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: format!("connect_for_test: {e}"),
            })?;

        // Server-initiated auth bi-stream (see open_stream for protocol
        // reasoning). Client awaits on accept_bi, then responds.
        let identity = self.own_identity().ok_or_else(|| PeerStorageError::ConnectionFailed {
            reason: "connect_for_test: own identity not configured".into(),
        })?;
        let own_endpoint_id_str = endpoint.id().to_string();
        let (mut auth_send, mut auth_recv) = conn.accept_bi().await.map_err(|e| {
            PeerStorageError::ConnectionFailed {
                reason: format!("connect_for_test accept auth stream: {e}"),
            }
        })?;
        crate::quic_did_auth::respond_to_challenge(
            &mut auth_send,
            &mut auth_recv,
            &identity.did,
            &identity.signing_key,
            &own_endpoint_id_str,
        )
        .await
        .map_err(|e| PeerStorageError::ConnectionFailed {
            reason: format!("connect_for_test DID-auth: {e}"),
        })?;
        let _ = auth_send.finish();

        if let Ok(mut cache) = self.connections.lock() {
            cache.insert(remote_id, conn.clone());
        }
        spawn_connection_watcher(remote_id, conn, self.state.clone());
        Ok(())
    }
}

// ============================================================================
// Connection diagnostics — shared by on-demand query + push-event watcher
// ============================================================================

/// Extract `ConnectionDiagnostics` from a live iroh `Connection`. Shared
/// between `diagnose_connection` (on-demand query) and the watcher task that
/// emits push events when iroh switches the selected path.
fn compute_diagnostics(conn: &iroh::endpoint::Connection) -> ConnectionDiagnostics {
    if conn.close_reason().is_some() {
        return ConnectionDiagnostics {
            path_type: PathType::Closed,
            remote_addr: None,
            rtt_ms: None,
        };
    }

    let info = conn.to_info();
    let (path_type, remote_addr, rtt_ms) = match info.selected_path() {
        Some(path) => {
            let path_type = if path.is_relay() {
                PathType::Relay
            } else if path.is_ip() {
                PathType::Direct
            } else {
                PathType::Unknown
            };
            let rtt_ms = path.rtt().map(|d| d.as_secs_f64() * 1000.0);
            let remote_addr = Some(format!("{:?}", path.remote_addr()));
            (path_type, remote_addr, rtt_ms)
        }
        None => (PathType::Unknown, None, None),
    };

    ConnectionDiagnostics {
        path_type,
        remote_addr,
        rtt_ms,
    }
}

/// Spawn a task that watches `conn.paths()` and emits a Tauri
/// `peer-storage:connection-changed` event each time iroh switches the selected
/// network path (direct↔relay) or the connection is dropped. The task
/// self-terminates when the watcher returns `Disconnected` — i.e., when the
/// underlying iroh `Watchable` is dropped (connection torn down end to end), so
/// there is no explicit lifecycle to manage from the caller side.
///
/// A peer can have more than one connection alive at once (inbound + outbound,
/// or a stale connection lingering across a reconnect), each with its own
/// watcher. We track a per-peer live count in `PeerState::connection_watchers`
/// and only emit the `Closed` diagnostic when the LAST connection goes away —
/// otherwise a transient connection dropping would overwrite a still-valid
/// `online` state and flip the UI offline.
///
/// This replaces a frontend `setInterval` poll: the UI listens once, gets
/// real-time updates, and incurs zero CPU when nothing changes.
fn spawn_connection_watcher(
    remote_id: EndpointId,
    conn: iroh::endpoint::Connection,
    state: Arc<RwLock<PeerState>>,
) {
    let node_id_str = remote_id.to_string();
    tokio::spawn(async move {
        use iroh::Watcher;
        let mut watcher = conn.paths();

        // Register this connection before emitting anything, so a concurrent
        // disconnect of a sibling connection can see we're still here.
        {
            let mut s = state.write().await;
            *s.connection_watchers.entry(remote_id).or_insert(0) += 1;
        }

        // Initial snapshot — a frontend that subscribed after the connection
        // was established still gets a value without waiting for the first
        // path switch.
        emit_connection_changed(&state, &node_id_str, &conn).await;

        loop {
            match watcher.updated().await {
                Ok(_paths) => emit_connection_changed(&state, &node_id_str, &conn).await,
                Err(_disconnected) => break,
            }
        }

        // This connection is gone. Deregister and only signal `Closed` once
        // the peer has no live connection left — a stale or duplicate watcher
        // must not clobber the `online` state another connection still owns.
        // Emitting `Closed` explicitly (rather than recomputing from `conn`)
        // avoids the window where `close_reason()` hasn't propagated yet and
        // `compute_diagnostics` would report a stale `online`/path snapshot.
        let remaining = {
            let mut s = state.write().await;
            let count = s.connection_watchers.entry(remote_id).or_insert(0);
            *count = count.saturating_sub(1);
            let remaining = *count;
            if remaining == 0 {
                s.connection_watchers.remove(&remote_id);
            }
            remaining
        };
        if remaining == 0 {
            emit_diagnostics(
                &state,
                &node_id_str,
                ConnectionDiagnostics {
                    path_type: PathType::Closed,
                    remote_addr: None,
                    rtt_ms: None,
                },
            )
            .await;
        }
    });
}

async fn emit_connection_changed(
    state: &Arc<RwLock<PeerState>>,
    node_id_str: &str,
    conn: &iroh::endpoint::Connection,
) {
    emit_diagnostics(state, node_id_str, compute_diagnostics(conn)).await;
}

async fn emit_diagnostics(
    state: &Arc<RwLock<PeerState>>,
    node_id_str: &str,
    diagnostics: ConnectionDiagnostics,
) {
    let app_handle = { state.read().await.app_handle.clone() };
    let Some(app) = app_handle else {
        // App handle is set on Android start and during normal vault boot.
        // Absence means we're running in a context without a frontend (tests,
        // pre-init) — silently skip the emit.
        return;
    };
    let _ = app.emit_to(
        "main",
        crate::event_names::EVENT_PEER_CONNECTION_CHANGED,
        serde_json::json!({
            "nodeId": node_id_str,
            "diagnostics": diagnostics,
        }),
    );
}

// ============================================================================
// Accept loop — handles incoming connections with access control
// ============================================================================

async fn accept_loop(
    endpoint: Endpoint,
    state: Arc<RwLock<PeerState>>,
    own_identity: Arc<Mutex<Option<OwnIdentity>>>,
) {
    while let Some(incoming) = endpoint.accept().await {
        let state = state.clone();
        let own_identity = own_identity.clone();
        let own_endpoint_id = endpoint.id().to_string();
        tokio::spawn(async move {
            match incoming.await {
                Ok(conn) => {
                    let alpn = conn.alpn();
                    let alpn_bytes: &[u8] = &alpn;
                    let remote = conn.remote_id();

                    if alpn_bytes == ALPN {
                        // --- Peer storage protocol ---
                        let remote_str = remote.to_string();

                        let allowed_spaces = {
                            let s = state.read().await;
                            s.allowed_peers.get(&remote_str).cloned()
                        };

                        match allowed_spaces {
                            Some(spaces) if !spaces.is_empty() => {
                                eprintln!(
                                    "[PeerStorage] Accepted connection from {remote} \
                                     (access to {} spaces)",
                                    spaces.len()
                                );
                                // Watch inbound connections too — when a remote
                                // peer reaches out, the UI's online dot should
                                // flip without waiting for an outbound retry.
                                spawn_connection_watcher(remote, conn.clone(), state.clone());
                                handle_connection(conn, state, own_identity, own_endpoint_id).await;
                            }
                            _ => {
                                eprintln!(
                                    "[PeerStorage] Rejected connection from {remote}: \
                                     not registered in any shared space"
                                );
                            }
                        }
                    } else if alpn_bytes == crate::space_delivery::local::protocol::ALPN {
                        // --- Space delivery protocol ---
                        let handler = {
                            let s = state.read().await;
                            s.delivery_handler.clone()
                        };

                        match handler {
                            Some(h) => {
                                eprintln!(
                                    "[SpaceDelivery] Accepted delivery connection from {remote}"
                                );
                                spawn_connection_watcher(remote, conn.clone(), state.clone());
                                h.handle_connection(conn).await;
                            }
                            None => {
                                eprintln!(
                                    "[SpaceDelivery] Rejected delivery connection from {remote}: \
                                     no handler registered"
                                );
                            }
                        }
                    } else {
                        eprintln!(
                            "[Endpoint] Rejected connection from {remote}: unknown ALPN {:?}",
                            String::from_utf8_lossy(&alpn)
                        );
                    }
                }
                Err(e) => {
                    eprintln!("[Endpoint] Failed to accept connection: {e}");
                }
            }
        });
    }
}

async fn handle_connection(
    conn: iroh::endpoint::Connection,
    state: Arc<RwLock<PeerState>>,
    own_identity: Arc<Mutex<Option<OwnIdentity>>>,
    own_endpoint_id: String,
) {
    let remote = conn.remote_id();
    let remote_str = remote.to_string();

    // -- Phase 1: DID challenge --
    //
    // The first accepted bi-stream of every connection is the quic_did_auth
    // handshake. Until it succeeds we hold no state for this peer; on
    // success we cache (endpoint_id -> DID) in PeerState so subsequent
    // request handlers can enforce UCAN audience == this DID.
    let identity_snapshot = own_identity.lock().ok().and_then(|g| g.clone());
    let Some(_own_identity) = identity_snapshot else {
        eprintln!(
            "[PeerStorage] Rejecting connection from {remote}: own identity not configured \
             (set_own_identity must run before start)"
        );
        conn.close(3u32.into(), b"no own identity");
        return;
    };

    // The server initiates the auth stream so it can write the Challenge
    // first — `open_bi` materialises the stream on the wire as soon as the
    // server writes, which avoids a both-sides-blocked-on-read deadlock that
    // would otherwise occur if both endpoints tried to read first.
    let verified_did = match conn.open_bi().await {
        Ok((mut send, mut recv)) => {
            match crate::quic_did_auth::challenge_and_verify(
                &mut send,
                &mut recv,
                &own_endpoint_id,
                &remote_str,
            )
            .await
            {
                Ok(did) => did,
                Err(e) => {
                    eprintln!("[PeerStorage] DID-auth failed for {remote}: {e}");
                    conn.close(2u32.into(), b"did-auth failed");
                    return;
                }
            }
        }
        Err(e) => {
            eprintln!("[PeerStorage] Failed to open auth stream to {remote}: {e}");
            return;
        }
    };

    let verified_short: String = verified_did.chars().take(24).collect();
    eprintln!("[PeerStorage] DID-auth ok: {remote} -> {verified_short}");

    // -- Phase 1.5: defense in depth — cross-check the crypto-verified DID
    // against the (endpoint_id -> owner_did) map we loaded from haex_devices.
    // The handshake alone proves "this peer holds the private key for the
    // DID it claims". The DB-side expectation proves "this DID is the one
    // we recorded as the owner of this endpoint id when the row was synced
    // through CRDT with UCAN audience attribution". Either layer alone is
    // sufficient on the happy path; together they make any single-layer
    // compromise (crypto bug, DB drift, partial sync, schema regression)
    // detectable rather than silent.
    {
        let s = state.read().await;
        match s.peer_owner_dids.get(&remote_str) {
            Some(expected) if expected == &verified_did => {
                // happy path — DB and crypto agree
            }
            Some(expected) => {
                let expected_short: String = expected.chars().take(24).collect();
                eprintln!(
                    "[PeerStorage] Closing connection to {remote}: verified DID does not match \
                     haex_devices.owner_did (verified={verified_short} db={expected_short})"
                );
                conn.close(4u32.into(), b"did/owner_did mismatch");
                return;
            }
            None => {
                // A peer that cleared allowed_peers must also have an entry
                // here — the two maps are loaded from the same DB pass. A
                // missing entry means inconsistent state and we reject
                // rather than accept the crypto-only proof.
                eprintln!(
                    "[PeerStorage] Closing connection to {remote}: no haex_devices.owner_did \
                     entry for verified DID {verified_short}"
                );
                conn.close(5u32.into(), b"no owner_did mapping");
                return;
            }
        }
    }

    state
        .write()
        .await
        .endpoint_dids
        .insert(remote_str.clone(), verified_did.clone());

    // -- Phase 2: normal request loop --

    loop {
        match conn.accept_bi().await {
            Ok((send, mut recv)) => {
                // Re-check access on every request — if peer was removed, close immediately
                let allowed_spaces = {
                    let s = state.read().await;
                    s.allowed_peers.get(&remote_str).cloned()
                };

                let Some(allowed_spaces) = allowed_spaces.filter(|s| !s.is_empty()) else {
                    eprintln!("[PeerStorage] Closing connection to {remote}: access revoked");
                    conn.close(1u32.into(), b"access revoked");
                    break;
                };

                let state = state.clone();
                let verified_did = verified_did.clone();
                tokio::spawn(async move {
                    if let Err(e) = super::handlers::handle_stream(
                        send,
                        &mut recv,
                        &state,
                        &allowed_spaces,
                        &verified_did,
                    )
                    .await
                    {
                        eprintln!("[PeerStorage] Stream error from {remote}: {e}");
                    }
                });
            }
            Err(_) => {
                eprintln!("[PeerStorage] Connection from {remote} closed");
                break;
            }
        }
    }

    // Drop the cached DID when the connection ends — once the QUIC stream
    // is gone the (endpoint_id -> DID) binding from this handshake no longer
    // applies. A future reconnect repeats the handshake.
    state.write().await.endpoint_dids.remove(&remote_str);
}
