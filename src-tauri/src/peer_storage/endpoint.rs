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

use iroh::{Endpoint, EndpointAddr, EndpointId, RelayMode, RelayUrl, SecretKey};

const DEFAULT_RELAY_URL: &str = "https://relay.sync.haex.space";

use tauri::Emitter;

use crate::peer_storage::error::PeerStorageError;
use crate::peer_storage::protocol::{self, Request, Response, ALPN};

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
}

impl Default for PeerState {
    fn default() -> Self {
        Self {
            shares: HashMap::new(),
            allowed_peers: HashMap::new(),
            app_handle: None,
            delivery_handler: None,
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
    /// Configured relay URL (set at start, available even before relay connection is established)
    configured_relay_url: Option<RelayUrl>,
    /// Cached connections to remote peers. Reusing a single QUIC connection for
    /// multiple streams avoids per-request TLS handshakes and the race condition
    /// where a closing connection interferes with a subsequent connect() call.
    connections: Mutex<HashMap<EndpointId, iroh::endpoint::Connection>>,
}

impl PeerEndpoint {
    /// Create a new PeerEndpoint with a persistent device key.
    pub fn new(secret_key: SecretKey) -> Self {
        Self {
            endpoint: None,
            secret_key,
            state: Arc::new(RwLock::new(PeerState::default())),
            accept_task: None,
            configured_relay_url: None,
            connections: Mutex::new(HashMap::new()),
        }
    }

    /// Create a PeerEndpoint with a temporary random key (for testing or pre-init state).
    pub fn new_ephemeral() -> Self {
        let mut bytes = [0u8; 32];
        rand::fill(&mut bytes);
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

        let endpoint = Endpoint::builder(iroh::endpoint::presets::N0)
            .secret_key(self.secret_key.clone())
            .alpns(vec![
                ALPN.to_vec(),
                crate::space_delivery::local::protocol::ALPN.to_vec(),
            ])
            .relay_mode(relay_mode)
            .address_lookup(
                iroh::address_lookup::MdnsAddressLookup::builder().service_name("haex-peer"),
            )
            .bind()
            .await
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: format!("Failed to bind endpoint: {e}"),
            })?;

        let id = endpoint.id();
        eprintln!("[PeerStorage] Endpoint started with ID: {id}");

        // Spawn accept loop with shared state
        let ep = endpoint.clone();
        let state = self.state.clone();

        let accept_task = tokio::spawn(async move {
            accept_loop(ep, state).await;
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
        tokio::spawn(async move {
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
                let _ = app.emit(
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

        Ok(id)
    }

    /// Stop the endpoint
    pub async fn stop(&mut self) -> Result<(), PeerStorageError> {
        if let Ok(mut cache) = self.connections.lock() {
            cache.clear();
        }

        if let Some(task) = self.accept_task.take() {
            task.abort();
        }

        if let Some(endpoint) = self.endpoint.take() {
            endpoint.close().await;
            eprintln!("[PeerStorage] Endpoint stopped");
        }

        Ok(())
    }

    /// Get a cached QUIC connection or establish a new one, then open a
    /// bidirectional stream. If a cached connection is stale, it is evicted
    /// and a fresh one is created automatically.
    pub(super) async fn open_stream(
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
            // Stale or corrupted — evict. Explicit close tells the peer now
            // instead of waiting for QUIC idle timeout.
            if let Ok(mut cache) = self.connections.lock() {
                if let Some(evicted) = cache.remove(&remote_id) {
                    evicted.close(0u32.into(), b"stale-cache-evicted");
                }
            }
        }

        // Establish a new connection
        let addr = match relay_url {
            Some(url) => EndpointAddr::new(remote_id).with_relay_url(url),
            None => EndpointAddr::new(remote_id),
        };

        let conn = endpoint
            .connect(addr, ALPN)
            .await
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: e.to_string(),
            })?;

        let streams = conn
            .open_bi()
            .await
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: e.to_string(),
            })?;

        if let Ok(mut cache) = self.connections.lock() {
            cache.insert(remote_id, conn);
        }
        Ok(streams)
    }

    /// Encode a request, send it on the stream, signal end-of-send, and read the response.
    pub(super) async fn send_request(
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
    pub(super) async fn send_request_header(
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
}

// ============================================================================
// Accept loop — handles incoming connections with access control
// ============================================================================

async fn accept_loop(endpoint: Endpoint, state: Arc<RwLock<PeerState>>) {
    while let Some(incoming) = endpoint.accept().await {
        let state = state.clone();
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
                                handle_connection(conn, state).await;
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

async fn handle_connection(conn: iroh::endpoint::Connection, state: Arc<RwLock<PeerState>>) {
    let remote = conn.remote_id();
    let remote_str = remote.to_string();

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
                    return;
                };

                let state = state.clone();
                tokio::spawn(async move {
                    if let Err(e) =
                        super::handlers::handle_stream(send, &mut recv, &state, &allowed_spaces)
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
}
