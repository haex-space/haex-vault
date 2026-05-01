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
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

use iroh::{Endpoint, EndpointAddr, EndpointId, RelayMode, RelayUrl, SecretKey};

const DEFAULT_RELAY_URL: &str = "https://relay.sync.haex.space";
const CONTENT_URI_ANDROID_ONLY: &str = "Content URIs are only supported on Android";

use crate::peer_storage::error::PeerStorageError;
use crate::peer_storage::protocol::{self, FileEntry, Request, Response, ALPN};

// ============================================================================
// Delivery connection handler trait
// ============================================================================

/// Trait for handling space delivery connections. Implemented by space_delivery module.
pub trait DeliveryConnectionHandler: Send + Sync {
    fn handle_connection(&self, conn: iroh::endpoint::Connection) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>>;
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
            .field("app_handle", &self.app_handle.as_ref().map(|_| "Some(AppHandle)"))
            .field("delivery_handler", &self.delivery_handler.as_ref().map(|_| "Some(DeliveryConnectionHandler)"))
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
    pub async fn start(&mut self, relay_url: Option<String>) -> Result<EndpointId, PeerStorageError> {
        if self.endpoint.is_some() {
            return Err(PeerStorageError::EndpointAlreadyRunning);
        }

        let effective_relay = relay_url
            .filter(|s| !s.is_empty())
            .or_else(|| std::env::var("HAEX_RELAY_URL").ok().filter(|s| !s.is_empty()))
            .unwrap_or_else(|| DEFAULT_RELAY_URL.to_string());

        let relay_mode = match effective_relay.parse::<RelayUrl>() {
            Ok(parsed) => {
                eprintln!("[PeerStorage] Using relay: {effective_relay}");
                self.configured_relay_url = Some(parsed.clone());
                RelayMode::custom([parsed])
            }
            Err(e) => {
                eprintln!("[PeerStorage] Invalid relay URL '{effective_relay}': {e} — falling back to iroh default");
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
                iroh::address_lookup::MdnsAddressLookup::builder()
                    .service_name("haex-peer"),
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

        self.endpoint = Some(endpoint);
        self.accept_task = Some(accept_task);

        Ok(id)
    }

    /// Stop the endpoint
    pub async fn stop(&mut self) -> Result<(), PeerStorageError> {
        if let Some(task) = self.accept_task.take() {
            task.abort();
        }

        if let Some(endpoint) = self.endpoint.take() {
            endpoint.close().await;
            eprintln!("[PeerStorage] Endpoint stopped");
        }

        Ok(())
    }

    /// Add a shared folder
    pub async fn add_share(&self, id: String, name: String, local_path: String, space_id: String) {
        eprintln!("[PeerStorage] Added share '{name}' at {local_path} (space: {space_id})");
        self.state.write().await.shares.insert(id, SharedFolder { name, local_path, space_id });
    }

    /// Remove a shared folder
    pub async fn remove_share(&self, id: &str) -> bool {
        self.state.write().await.shares.remove(id).is_some()
    }

    /// List shared folders
    pub async fn list_shares(&self) -> Vec<(String, SharedFolder)> {
        self.state.read().await.shares.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
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
        eprintln!("[PeerStorage] Updated allowed peers: {} peers across spaces", allowed.len());
        self.state.write().await.allowed_peers = allowed;
    }

    /// Open a bidirectional QUIC stream to a remote peer.
    /// Centralises the endpoint check, address build, connect, and open_bi calls.
    async fn open_peer_stream(
        &self,
        remote_id: EndpointId,
        relay_url: Option<RelayUrl>,
    ) -> Result<(iroh::endpoint::SendStream, iroh::endpoint::RecvStream), PeerStorageError> {
        let endpoint = self
            .endpoint
            .as_ref()
            .ok_or(PeerStorageError::EndpointNotRunning)?;
        let addr = match relay_url {
            Some(url) => EndpointAddr::new(remote_id).with_relay_url(url),
            None => EndpointAddr::new(remote_id),
        };
        let conn = endpoint
            .connect(addr, ALPN)
            .await
            .map_err(|e| PeerStorageError::ConnectionFailed { reason: e.to_string() })?;
        conn.open_bi()
            .await
            .map_err(|e| PeerStorageError::ConnectionFailed { reason: e.to_string() })
    }

    /// Connect to a remote peer and list a directory
    pub async fn remote_list(
        &self,
        remote_id: EndpointId,
        relay_url: Option<RelayUrl>,
        path: &str,
        ucan_token: &str,
    ) -> Result<Vec<FileEntry>, PeerStorageError> {
        let (mut send, mut recv) = self.open_peer_stream(remote_id, relay_url).await?;
        let req = Request::List { path: path.to_string(), ucan_token: ucan_token.to_string() };
        match send_and_receive(&mut send, &mut recv, &req).await? {
            Response::List { entries } => Ok(entries),
            Response::Error { message } => Err(PeerStorageError::ProtocolError { reason: message }),
            _ => Err(PeerStorageError::ProtocolError {
                reason: "Unexpected response type".to_string(),
            }),
        }
    }

    /// Connect to a remote peer and download a file directly to disk.
    /// Streams chunks from the iroh connection directly into the output file
    /// without buffering the entire file in memory.
    /// Returns the total file size on success.
    pub async fn remote_read_to_file(
        &self,
        remote_id: EndpointId,
        relay_url: Option<RelayUrl>,
        path: &str,
        output_path: &std::path::Path,
        range: Option<[u64; 2]>,
        on_progress: Option<Box<dyn Fn(u64, u64) + Send>>,
        cancel_token: Option<tokio_util::sync::CancellationToken>,
        pause_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
        ucan_token: &str,
    ) -> Result<u64, PeerStorageError> {
        let (mut send, mut recv) = self.open_peer_stream(remote_id, relay_url).await?;
        let req = Request::Read { path: path.to_string(), range, ucan_token: ucan_token.to_string() };
        match send_and_receive(&mut send, &mut recv, &req).await? {
            Response::ReadHeader { size } => {
                // Stream chunks directly to file — no full-file RAM buffering
                use tokio::io::AsyncWriteExt;

                let mut file = tokio::fs::File::create(output_path).await
                    .map_err(|e| PeerStorageError::ProtocolError {
                        reason: format!("Failed to create output file: {e}"),
                    })?;

                let mut bytes_written: u64 = 0;
                let mut buf = [0u8; 64 * 1024]; // 64KB chunks

                loop {
                    // Check cancellation before each chunk
                    if let Some(ref token) = cancel_token {
                        if token.is_cancelled() {
                            // Clean up partial file
                            let _ = tokio::fs::remove_file(output_path).await;
                            return Err(PeerStorageError::ProtocolError {
                                reason: "Transfer cancelled".to_string(),
                            });
                        }
                    }

                    // Wait while paused
                    if let Some(ref flag) = pause_flag {
                        while flag.load(std::sync::atomic::Ordering::Relaxed) {
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                            if let Some(ref token) = cancel_token {
                                if token.is_cancelled() {
                                    let _ = tokio::fs::remove_file(output_path).await;
                                    return Err(PeerStorageError::ProtocolError {
                                        reason: "Transfer cancelled".to_string(),
                                    });
                                }
                            }
                        }
                    }

                    let chunk = recv.read(&mut buf).await
                        .map_err(|e| PeerStorageError::ConnectionFailed {
                            reason: format!("Failed to read from stream: {e}"),
                        })?;
                    match chunk {
                        Some(n) => {
                            file.write_all(&buf[..n]).await
                                .map_err(|e| PeerStorageError::ProtocolError {
                                    reason: format!("Failed to write to file: {e}"),
                                })?;
                            bytes_written += n as u64;
                            if let Some(ref cb) = on_progress {
                                cb(bytes_written, size);
                            }
                        }
                        None => break,
                    }
                }

                file.flush().await.map_err(|e| PeerStorageError::ProtocolError {
                    reason: format!("Failed to flush file: {e}"),
                })?;

                Ok(size)
            }
            Response::Error { message } => {
                Err(PeerStorageError::ProtocolError { reason: message })
            }
            _ => Err(PeerStorageError::ProtocolError {
                reason: "Unexpected response type".to_string(),
            }),
        }
    }

    /// Connect to a remote peer and get a recursive file manifest.
    pub async fn remote_manifest(
        &self,
        remote_id: EndpointId,
        relay_url: Option<RelayUrl>,
        path: &str,
        ucan_token: &str,
    ) -> Result<Vec<crate::file_sync::types::FileState>, PeerStorageError> {
        let (mut send, mut recv) = self.open_peer_stream(remote_id, relay_url).await?;
        let req = Request::Manifest { path: path.to_string(), ucan_token: ucan_token.to_string() };
        match send_and_receive(&mut send, &mut recv, &req).await? {
            Response::Manifest { entries } => Ok(entries),
            Response::Error { message } => Err(PeerStorageError::ProtocolError { reason: message }),
            _ => Err(PeerStorageError::ProtocolError {
                reason: "Unexpected response type".to_string(),
            }),
        }
    }

    /// Connect to a remote peer and read a file into memory.
    /// For large files prefer `remote_read_to_file`; this is for sync-sized reads.
    pub async fn remote_read_bytes(
        &self,
        remote_id: EndpointId,
        relay_url: Option<RelayUrl>,
        path: &str,
        ucan_token: &str,
    ) -> Result<Vec<u8>, PeerStorageError> {
        let (mut send, mut recv) = self.open_peer_stream(remote_id, relay_url).await?;
        let req = Request::Read { path: path.to_string(), range: None, ucan_token: ucan_token.to_string() };
        match send_and_receive(&mut send, &mut recv, &req).await? {
            Response::ReadHeader { size } => {
                let mut data = Vec::with_capacity(size as usize);
                let mut buf = [0u8; 64 * 1024];
                loop {
                    let chunk = recv.read(&mut buf).await
                        .map_err(|e| PeerStorageError::ConnectionFailed {
                            reason: format!("Failed to read from stream: {e}"),
                        })?;
                    match chunk {
                        Some(n) => data.extend_from_slice(&buf[..n]),
                        None => break,
                    }
                }
                Ok(data)
            }
            Response::Error { message } => Err(PeerStorageError::ProtocolError { reason: message }),
            _ => Err(PeerStorageError::ProtocolError {
                reason: "Unexpected response type".to_string(),
            }),
        }
    }

    /// Connect to a remote peer and write a file.
    /// Sends the Write request header, then streams the file data.
    pub async fn remote_write_file(
        &self,
        remote_id: EndpointId,
        relay_url: Option<RelayUrl>,
        path: &str,
        data: &[u8],
        ucan_token: &str,
    ) -> Result<(), PeerStorageError> {
        let (mut send, mut recv) = self.open_peer_stream(remote_id, relay_url).await?;

        // Send request header without finishing — data follows
        let req = Request::Write { path: path.to_string(), size: data.len() as u64, ucan_token: ucan_token.to_string() };
        let req_bytes = protocol::encode_request(&req)
            .map_err(|e| PeerStorageError::ProtocolError { reason: e.to_string() })?;
        send.write_all(&req_bytes)
            .await
            .map_err(|e| PeerStorageError::ConnectionFailed { reason: e.to_string() })?;

        // Stream file data then close send side
        send.write_all(data)
            .await
            .map_err(|e| PeerStorageError::ConnectionFailed { reason: e.to_string() })?;
        send.finish()
            .map_err(|e| PeerStorageError::ConnectionFailed { reason: e.to_string() })?;

        let response: Response = protocol::read_response(&mut recv)
            .await
            .map_err(|e| PeerStorageError::ProtocolError { reason: e.to_string() })?;
        match response {
            Response::WriteOk => Ok(()),
            Response::Error { message } => Err(PeerStorageError::ProtocolError { reason: message }),
            _ => Err(PeerStorageError::ProtocolError {
                reason: "Unexpected response type".to_string(),
            }),
        }
    }

    /// Connect to a remote peer and delete a file.
    pub async fn remote_delete_file(
        &self,
        remote_id: EndpointId,
        relay_url: Option<RelayUrl>,
        path: &str,
        to_trash: bool,
        ucan_token: &str,
    ) -> Result<(), PeerStorageError> {
        let (mut send, mut recv) = self.open_peer_stream(remote_id, relay_url).await?;
        let req = Request::Delete { path: path.to_string(), to_trash, ucan_token: ucan_token.to_string() };
        match send_and_receive(&mut send, &mut recv, &req).await? {
            Response::DeleteOk => Ok(()),
            Response::Error { message } => Err(PeerStorageError::ProtocolError { reason: message }),
            _ => Err(PeerStorageError::ProtocolError {
                reason: "Unexpected response type".to_string(),
            }),
        }
    }

    /// Connect to a remote peer and create a directory.
    pub async fn remote_create_directory(
        &self,
        remote_id: EndpointId,
        relay_url: Option<RelayUrl>,
        path: &str,
        ucan_token: &str,
    ) -> Result<(), PeerStorageError> {
        let (mut send, mut recv) = self.open_peer_stream(remote_id, relay_url).await?;
        let req = Request::CreateDirectory { path: path.to_string(), ucan_token: ucan_token.to_string() };
        match send_and_receive(&mut send, &mut recv, &req).await? {
            Response::CreateDirectoryOk => Ok(()),
            Response::Error { message } => Err(PeerStorageError::ProtocolError { reason: message }),
            _ => Err(PeerStorageError::ProtocolError {
                reason: "Unexpected response type".to_string(),
            }),
        }
    }
}

// ============================================================================
// Protocol helpers
// ============================================================================

/// Encode and send a request, finish the stream, then read the response.
/// Use this for all request–response calls where no data follows the request.
async fn send_and_receive(
    send: &mut iroh::endpoint::SendStream,
    recv: &mut iroh::endpoint::RecvStream,
    req: &Request,
) -> Result<Response, PeerStorageError> {
    let bytes = protocol::encode_request(req)
        .map_err(|e| PeerStorageError::ProtocolError { reason: e.to_string() })?;
    send.write_all(&bytes)
        .await
        .map_err(|e| PeerStorageError::ConnectionFailed { reason: e.to_string() })?;
    send.finish()
        .map_err(|e| PeerStorageError::ConnectionFailed { reason: e.to_string() })?;
    protocol::read_response(recv)
        .await
        .map_err(|e| PeerStorageError::ProtocolError { reason: e.to_string() })
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
                                eprintln!("[PeerStorage] Accepted connection from {remote} (access to {} spaces)", spaces.len());
                                handle_connection(conn, state).await;
                            }
                            _ => {
                                eprintln!("[PeerStorage] Rejected connection from {remote}: not registered in any shared space");
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
                                eprintln!("[SpaceDelivery] Accepted delivery connection from {remote}");
                                h.handle_connection(conn).await;
                            }
                            None => {
                                eprintln!("[SpaceDelivery] Rejected delivery connection from {remote}: no handler registered");
                            }
                        }
                    } else {
                        eprintln!("[Endpoint] Rejected connection from {remote}: unknown ALPN {:?}", String::from_utf8_lossy(&alpn));
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
) {
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
                    if let Err(e) = handle_stream(send, &mut recv, &state, &allowed_spaces).await {
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

async fn handle_stream(
    mut send: iroh::endpoint::SendStream,
    recv: &mut iroh::endpoint::RecvStream,
    state: &RwLock<PeerState>,
    allowed_spaces: &HashSet<String>,
) -> Result<(), PeerStorageError> {
    let request = protocol::read_request(recv)
        .await
        .map_err(|e| PeerStorageError::ProtocolError {
            reason: e.to_string(),
        })?;

    // ── Layer 1 (first line of defense): validate UCAN signature + expiry ──
    let validated_ucan = match crate::ucan::validate_token(request.ucan_token()) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[PeerStorage] UCAN validation failed: {e}");
            let resp = Response::Error { message: format!("UCAN validation failed: {e}") };
            let bytes = protocol::encode_response(&resp)
                .map_err(|e| PeerStorageError::ProtocolError { reason: e.to_string() })?;
            send.write_all(&bytes).await.ok();
            send.finish().ok();
            return Ok(());
        }
    };

    // ── Layer 2 (source of truth): check capability matches operation ──
    // Determine which space this request targets
    let target_space_id = {
        let s = state.read().await;
        let path = match &request {
            Request::List { path, .. } | Request::Stat { path, .. }
            | Request::Read { path, .. } | Request::Manifest { path, .. }
            | Request::Write { path, .. } | Request::Delete { path, .. }
            | Request::CreateDirectory { path, .. } => path.as_str(),
        };
        find_space_for_path(&s.shares, allowed_spaces, path)
    };

    if let Some(space_id) = &target_space_id {
        let required = if request.requires_write() {
            crate::ucan::CapabilityLevel::Write
        } else {
            crate::ucan::CapabilityLevel::Read
        };

        if let Err(e) = crate::ucan::require_capability(&validated_ucan, space_id, required) {
            eprintln!("[PeerStorage] UCAN capability check failed: {e}");
            let resp = Response::Error { message: format!("Access denied: {e}") };
            let bytes = protocol::encode_response(&resp)
                .map_err(|e| PeerStorageError::ProtocolError { reason: e.to_string() })?;
            send.write_all(&bytes).await.ok();
            send.finish().ok();
            return Ok(());
        }
    }

    let response = match request {
        Request::List { path, .. } => handle_list(state, &path, allowed_spaces).await,
        Request::Stat { path, .. } => handle_stat(state, &path, allowed_spaces).await,
        Request::Manifest { path, .. } => handle_manifest(state, &path, allowed_spaces).await,
        Request::Read { path, range, .. } => {
            if let Err(e) = handle_read(&mut send, state, &path, range, allowed_spaces).await {
                eprintln!("[PeerStorage] Read error for '{path}': {e}");
                let error_resp = Response::Error { message: format!("{e}") };
                if let Ok(bytes) = protocol::encode_response(&error_resp) {
                    let _ = send.write_all(&bytes).await;
                    let _ = send.finish();
                }
                return Err(e);
            }
            return Ok(());
        }
        Request::Write { path, size, .. } => {
            if let Err(e) = handle_write(&mut send, recv, state, &path, size, allowed_spaces).await {
                eprintln!("[PeerStorage] Write error for '{path}': {e}");
                let error_resp = Response::Error { message: format!("{e}") };
                if let Ok(bytes) = protocol::encode_response(&error_resp) {
                    let _ = send.write_all(&bytes).await;
                    let _ = send.finish();
                }
                return Err(e);
            }
            return Ok(());
        }
        Request::Delete { path, to_trash, .. } => {
            handle_delete(state, &path, to_trash, allowed_spaces).await
        }
        Request::CreateDirectory { path, .. } => {
            handle_create_directory(state, &path, allowed_spaces).await
        }
    };

    let resp_bytes = protocol::encode_response(&response)
        .map_err(|e| PeerStorageError::ProtocolError {
            reason: e.to_string(),
        })?;
    send.write_all(&resp_bytes)
        .await
        .map_err(|e| PeerStorageError::ConnectionFailed {
            reason: e.to_string(),
        })?;
    send.finish()
        .map_err(|e| PeerStorageError::ConnectionFailed {
            reason: e.to_string(),
        })?;

    Ok(())
}

// ============================================================================
// Request handlers (with space-based access control)
// ============================================================================

/// Filter shares to only those the remote peer is allowed to access
fn filter_shares<'a>(
    shares: &'a HashMap<String, SharedFolder>,
    allowed_spaces: &HashSet<String>,
) -> HashMap<&'a String, &'a SharedFolder> {
    shares
        .iter()
        .filter(|(_, share)| allowed_spaces.contains(&share.space_id))
        .collect()
}

/// Find a share by name (or ID) and extract the sub-path within it.
fn find_share_and_subpath<'a>(
    shares: &'a HashMap<String, SharedFolder>,
    allowed_spaces: &HashSet<String>,
    request_path: &str,
) -> Result<(&'a SharedFolder, String), Response> {
    let trimmed = request_path.trim_start_matches('/');
    let (share_name, sub_path) = trimmed.split_once('/').unwrap_or((trimmed, ""));

    let share = shares.values()
        .find(|s| s.name == share_name && allowed_spaces.contains(&s.space_id))
        .or_else(|| shares.get(share_name).filter(|s| allowed_spaces.contains(&s.space_id)))
        .ok_or_else(|| Response::Error {
            message: format!("Share not found: {share_name}"),
        })?;

    Ok((share, sub_path.to_string()))
}

/// Resolve a request path to a local filesystem path (desktop / standard paths).
fn resolve_path_filtered(
    shares: &HashMap<String, SharedFolder>,
    allowed_spaces: &HashSet<String>,
    request_path: &str,
) -> Result<PathBuf, Response> {
    let (share, sub_path) = find_share_and_subpath(shares, allowed_spaces, request_path)?;

    crate::filesystem::reject_path_traversal(&sub_path).map_err(|message| Response::Error { message })?;

    let full_path = PathBuf::from(&share.local_path).join(&sub_path);

    let canonical = full_path.canonicalize().map_err(|_| Response::Error {
        message: "Path not found".to_string(),
    })?;
    let share_canonical = PathBuf::from(&share.local_path).canonicalize().map_err(|_| Response::Error {
        message: "Share path invalid".to_string(),
    })?;

    if !canonical.starts_with(&share_canonical) {
        return Err(Response::Error {
            message: "Access denied: path outside share".to_string(),
        });
    }

    Ok(canonical)
}

async fn handle_list(
    state: &RwLock<PeerState>,
    path: &str,
    allowed_spaces: &HashSet<String>,
) -> Response {
    let state = state.read().await;

    if path.is_empty() || path == "/" {
        // Only list shares the peer is allowed to access
        let filtered = filter_shares(&state.shares, allowed_spaces);
        let entries: Vec<FileEntry> = filtered
            .iter()
            .map(|(_id, share)| FileEntry {
                name: share.name.clone(),
                size: 0,
                is_dir: true,
                modified: None,
            })
            .collect();
        return Response::List { entries };
    }

    // Check if the target share uses Content URIs (Android)
    if let Ok((share, _sub)) = find_share_and_subpath(&state.shares, allowed_spaces, path) {
        if is_content_uri(&share.local_path) {
            #[cfg(target_os = "android")]
            {
                let app_handle = match &state.app_handle {
                    Some(h) => h.clone(),
                    None => return Response::Error { message: "AppHandle not available".to_string() },
                };
                let root_uri = share.local_path.clone();
                let sub_path = _sub;
                // Drop state lock before blocking JNI call
                drop(state);
                return match tokio::task::spawn_blocking(move || {
                    list_content_uri(&app_handle, &root_uri, &sub_path)
                }).await {
                    Ok(Ok(entries)) => Response::List { entries },
                    Ok(Err(e)) => Response::Error { message: e },
                    Err(e) => Response::Error { message: format!("Task failed: {e}") },
                };
            }
            #[cfg(not(target_os = "android"))]
            return Response::Error { message: CONTENT_URI_ANDROID_ONLY.to_string() };
        }
    }

    // Standard filesystem path
    let local_path = match resolve_path_filtered(&state.shares, allowed_spaces, path) {
        Ok(p) => p,
        Err(resp) => return resp,
    };

    if !local_path.is_dir() {
        return Response::Error {
            message: "Not a directory".to_string(),
        };
    }

    match read_dir_entries(&local_path).await {
        Ok(entries) => Response::List { entries },
        Err(e) => Response::Error {
            message: format!("Failed to list directory: {e}"),
        },
    }
}

async fn handle_manifest(
    state: &RwLock<PeerState>,
    path: &str,
    allowed_spaces: &HashSet<String>,
) -> Response {
    let state = state.read().await;

    // Manifest requires a share path (no root-level manifest)
    if path.is_empty() || path == "/" {
        return Response::Error {
            message: "Manifest requires a share path".to_string(),
        };
    }

    // Content URIs require recursive scan via android_fs
    if let Ok((share, _sub)) = find_share_and_subpath(&state.shares, allowed_spaces, path) {
        if is_content_uri(&share.local_path) {
            #[cfg(target_os = "android")]
            {
                let app_handle = match &state.app_handle {
                    Some(h) => h.clone(),
                    None => return Response::Error { message: "AppHandle not available".to_string() },
                };
                let root_uri = share.local_path.clone();
                let sub_path = _sub;
                drop(state);
                return match tokio::task::spawn_blocking(move || {
                    manifest_content_uri(&app_handle, &root_uri, &sub_path)
                }).await {
                    Ok(Ok(entries)) => Response::Manifest { entries },
                    Ok(Err(e)) => Response::Error { message: e },
                    Err(e) => Response::Error { message: format!("Task failed: {e}") },
                };
            }
            #[cfg(not(target_os = "android"))]
            return Response::Error { message: CONTENT_URI_ANDROID_ONLY.to_string() };
        }
    }

    let local_path = match resolve_path_filtered(&state.shares, allowed_spaces, path) {
        Ok(p) => p,
        Err(resp) => return resp,
    };

    if !local_path.is_dir() {
        return Response::Error {
            message: "Not a directory".to_string(),
        };
    }

    // Use the same recursive scan as LocalProvider
    match tokio::task::spawn_blocking({
        let base = local_path.clone();
        move || scan_directory_recursive(&local_path, &base)
    })
    .await
    {
        Ok(Ok(entries)) => Response::Manifest { entries },
        Ok(Err(e)) => Response::Error {
            message: format!("Failed to scan directory: {e}"),
        },
        Err(e) => Response::Error {
            message: format!("Task failed: {e}"),
        },
    }
}

/// Recursively scan a directory and collect FileState entries for the manifest.
fn scan_directory_recursive(
    dir: &Path,
    base: &Path,
) -> Result<Vec<crate::file_sync::types::FileState>, std::io::Error> {
    let mut entries = Vec::new();
    let read_dir = std::fs::read_dir(dir)?;

    for entry in read_dir {
        let entry = entry?;
        let metadata = entry.metadata()?;

        let relative = entry
            .path()
            .strip_prefix(base)
            .unwrap_or(&entry.path())
            .to_string_lossy()
            .to_string()
            .replace('\\', "/");

        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        entries.push(crate::file_sync::types::FileState {
            relative_path: relative,
            size: if metadata.is_dir() { 0 } else { metadata.len() },
            modified_at,
            is_directory: metadata.is_dir(),
        });

        if metadata.is_dir() {
            entries.extend(scan_directory_recursive(&entry.path(), base)?);
        }
    }

    Ok(entries)
}

async fn handle_stat(
    state: &RwLock<PeerState>,
    path: &str,
    allowed_spaces: &HashSet<String>,
) -> Response {
    let state = state.read().await;

    // Check if the target share uses Content URIs
    if let Ok((share, _sub)) = find_share_and_subpath(&state.shares, allowed_spaces, path) {
        if is_content_uri(&share.local_path) {
            #[cfg(target_os = "android")]
            {
                let app_handle = match &state.app_handle {
                    Some(h) => h.clone(),
                    None => return Response::Error { message: "AppHandle not available".to_string() },
                };
                let root_uri = share.local_path.clone();
                let sub_path = _sub;
                drop(state);
                return match tokio::task::spawn_blocking(move || {
                    stat_content_uri(&app_handle, &root_uri, &sub_path)
                }).await {
                    Ok(Ok(entry)) => Response::Stat { entry },
                    Ok(Err(e)) => Response::Error { message: e },
                    Err(e) => Response::Error { message: format!("Task failed: {e}") },
                };
            }
            #[cfg(not(target_os = "android"))]
            return Response::Error { message: CONTENT_URI_ANDROID_ONLY.to_string() };
        }
    }

    let local_path = match resolve_path_filtered(&state.shares, allowed_spaces, path) {
        Ok(p) => p,
        Err(resp) => return resp,
    };

    match file_entry_from_path(&local_path) {
        Ok(entry) => Response::Stat { entry },
        Err(e) => Response::Error {
            message: format!("Failed to stat: {e}"),
        },
    }
}

async fn handle_read(
    send: &mut iroh::endpoint::SendStream,
    state: &RwLock<PeerState>,
    path: &str,
    range: Option<[u64; 2]>,
    allowed_spaces: &HashSet<String>,
) -> Result<(), PeerStorageError> {
    // Check if the target share uses Content URIs
    let content_uri_info = {
        let state = state.read().await;
        if let Ok((share, sub_path)) = find_share_and_subpath(&state.shares, allowed_spaces, path) {
            if is_content_uri(&share.local_path) {
                Some((
                    share.local_path.clone(),
                    sub_path,
                    state.app_handle.clone(),
                ))
            } else {
                None
            }
        } else {
            None
        }
    };

    if let Some((root_uri, sub_path, app_handle_opt)) = content_uri_info {
        #[cfg(target_os = "android")]
        {
            let app_handle = app_handle_opt.ok_or_else(|| PeerStorageError::ProtocolError {
                reason: "AppHandle not available".to_string(),
            })?;
            return handle_read_content_uri(send, &app_handle, &root_uri, &sub_path, range).await;
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = (root_uri, sub_path, app_handle_opt);
            let resp = Response::Error { message: CONTENT_URI_ANDROID_ONLY.to_string() };
            let resp_bytes = protocol::encode_response(&resp)
                .map_err(|e| PeerStorageError::ProtocolError { reason: e.to_string() })?;
            send.write_all(&resp_bytes).await.ok();
            send.finish().ok();
            return Ok(());
        }
    }

    // Standard filesystem read
    let local_path = {
        let state = state.read().await;
        match resolve_path_filtered(&state.shares, allowed_spaces, path) {
            Ok(p) => p,
            Err(resp) => {
                let resp_bytes = protocol::encode_response(&resp)
                    .map_err(|e| PeerStorageError::ProtocolError { reason: e.to_string() })?;
                send.write_all(&resp_bytes).await.ok();
                send.finish().ok();
                return Ok(());
            }
        }
    };

    if !local_path.is_file() {
        let resp = Response::Error {
            message: "Not a file".to_string(),
        };
        let resp_bytes = protocol::encode_response(&resp)
            .map_err(|e| PeerStorageError::ProtocolError { reason: e.to_string() })?;
        send.write_all(&resp_bytes).await.ok();
        send.finish().ok();
        return Ok(());
    }

    stream_file_to_send(send, &local_path, range).await
}

/// Stream a local file to the QUIC send stream in 64KB chunks.
async fn stream_file_to_send(
    send: &mut iroh::endpoint::SendStream,
    local_path: &Path,
    range: Option<[u64; 2]>,
) -> Result<(), PeerStorageError> {
    use tokio::io::AsyncReadExt;

    let mut file = tokio::fs::File::open(local_path)
        .await
        .map_err(PeerStorageError::Io)?;

    let metadata = file.metadata().await.map_err(PeerStorageError::Io)?;
    let file_size = metadata.len();

    let (offset, read_size) = match range {
        Some([start, end]) => {
            let end = end.min(file_size);
            (start, end - start)
        }
        None => (0, file_size),
    };

    // Send header
    let header = Response::ReadHeader { size: read_size };
    let header_bytes = protocol::encode_response(&header)
        .map_err(|e| PeerStorageError::ProtocolError { reason: e.to_string() })?;
    send.write_all(&header_bytes)
        .await
        .map_err(|e| PeerStorageError::ConnectionFailed { reason: e.to_string() })?;

    if offset > 0 {
        use tokio::io::AsyncSeekExt;
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(PeerStorageError::Io)?;
    }

    // Stream file data in chunks (64 KB)
    let mut remaining = read_size;
    let mut buf = vec![0u8; 64 * 1024];

    while remaining > 0 {
        let to_read = (remaining as usize).min(buf.len());
        let n = file
            .read(&mut buf[..to_read])
            .await
            .map_err(PeerStorageError::Io)?;
        if n == 0 {
            break;
        }
        send.write_all(&buf[..n])
            .await
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: e.to_string(),
            })?;
        remaining -= n as u64;
    }

    send.finish()
        .map_err(|e| PeerStorageError::ConnectionFailed {
            reason: e.to_string(),
        })?;

    Ok(())
}

// ============================================================================
// Android Content URI helpers
// ============================================================================

/// Resolve a relative sub-path within a Content URI tree by navigating directory
/// by directory. Returns the target Content URI JSON string and whether it's a dir.
#[cfg(target_os = "android")]
fn resolve_content_uri_subpath(
    app_handle: &tauri::AppHandle,
    root_uri_json: &str,
    sub_path: &str,
) -> Result<(tauri_plugin_android_fs::FileUri, bool), String> {
    use tauri_plugin_android_fs::{AndroidFsExt, FileUri};

    crate::filesystem::reject_path_traversal(sub_path)?;

    let api = app_handle.android_fs();
    let root = FileUri::from_json_str(root_uri_json)
        .map_err(|e| format!("Invalid Content URI: {e:?}"))?;

    let segments: Vec<&str> = sub_path
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();

    if segments.is_empty() {
        // Root of the share
        let is_dir = api.get_type(&root)
            .map(|t| t == tauri_plugin_android_fs::EntryType::Dir)
            .unwrap_or(true);
        return Ok((root, is_dir));
    }

    let mut current = root;

    for (i, segment) in segments.iter().enumerate() {
        let entries = api.read_dir(&current)
            .map_err(|e| format!("Failed to read dir: {e:?}"))?;

        let found = entries
            .into_iter()
            .find(|entry| entry.name() == *segment);

        match found {
            Some(entry) => {
                let is_dir = entry.is_dir();
                let is_last = i == segments.len() - 1;
                current = entry.uri().clone();

                if !is_last && !is_dir {
                    return Err(format!("Path segment '{}' is not a directory", segment));
                }

                if is_last {
                    return Ok((current, is_dir));
                }
            }
            None => return Err(format!("Not found: {}", segment)),
        }
    }

    unreachable!()
}

/// List directory contents via Content URI.
#[cfg(target_os = "android")]
fn list_content_uri(
    app_handle: &tauri::AppHandle,
    root_uri_json: &str,
    sub_path: &str,
) -> Result<Vec<FileEntry>, String> {
    use tauri_plugin_android_fs::AndroidFsExt;

    let (target_uri, is_dir) = resolve_content_uri_subpath(app_handle, root_uri_json, sub_path)?;

    if !is_dir {
        return Err("Not a directory".to_string());
    }

    let api = app_handle.android_fs();
    let dir_entries = api.read_dir(&target_uri)
        .map_err(|e| format!("Failed to read dir: {e:?}"))?;

    let mut entries: Vec<FileEntry> = dir_entries
        .into_iter()
        .map(|entry| {
            let modified = entry.last_modified()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_secs());

            FileEntry {
                name: entry.name().to_string(),
                size: entry.file_len().unwrap_or(0),
                is_dir: entry.is_dir(),
                modified,
            }
        })
        .collect();

    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(entries)
}

/// Get file/dir metadata via Content URI.
#[cfg(target_os = "android")]
fn stat_content_uri(
    app_handle: &tauri::AppHandle,
    root_uri_json: &str,
    sub_path: &str,
) -> Result<FileEntry, String> {
    use tauri_plugin_android_fs::AndroidFsExt;

    let (target_uri, is_dir) = resolve_content_uri_subpath(app_handle, root_uri_json, sub_path)?;
    let api = app_handle.android_fs();

    let info = api.get_info(&target_uri)
        .map_err(|e| format!("Failed to get info: {e:?}"))?;

    let modified = info.last_modified()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs());

    Ok(FileEntry {
        name: info.name().to_string(),
        size: info.file_len().unwrap_or(0),
        is_dir,
        modified,
    })
}

/// Stream a file via Content URI to the QUIC send stream.
///
/// The JNI file descriptor stays entirely within a `spawn_blocking` thread that
/// reads chunks and sends them over an `mpsc` channel.  The async side receives
/// chunks and writes them to QUIC.  This avoids fd-lifetime issues that arise
/// when converting a JNI `std::fs::File` to `tokio::fs::File` (the
/// `ParcelFileDescriptor` on the Java side can be GC'd while the async read is
/// still in progress, invalidating the fd).
#[cfg(target_os = "android")]
async fn handle_read_content_uri(
    send: &mut iroh::endpoint::SendStream,
    app_handle: &tauri::AppHandle,
    root_uri_json: &str,
    sub_path: &str,
    range: Option<[u64; 2]>,
) -> Result<(), PeerStorageError> {
    use tauri_plugin_android_fs::AndroidFsExt;
    use std::io::Read;

    let app = app_handle.clone();
    let root = root_uri_json.to_string();
    let sub = sub_path.to_string();

    // Step 1: Resolve Content URI and get file size (blocking JNI)
    let (file_size, target_root, target_sub) = tokio::task::spawn_blocking({
        let app = app.clone();
        let root = root.clone();
        let sub = sub.clone();
        move || -> Result<(u64, String, String), PeerStorageError> {
            let api = app.android_fs();
            let (target_uri, is_dir) = resolve_content_uri_subpath(&app, &root, &sub)
                .map_err(|e| PeerStorageError::ProtocolError { reason: e })?;

            if is_dir {
                return Err(PeerStorageError::ProtocolError { reason: "Not a file".to_string() });
            }

            let size = api.get_len(&target_uri).unwrap_or(0);
            eprintln!("[PeerStorage] Content URI read: size={size}, path={sub}");
            Ok((size, root, sub))
        }
    }).await.map_err(|e| PeerStorageError::ProtocolError {
        reason: format!("Task failed: {e}"),
    })??;

    let (offset, read_size) = match range {
        Some([start, end]) => {
            let end = end.min(file_size);
            (start, end - start)
        }
        None => (0, file_size),
    };

    // Step 2: Send header
    let header = Response::ReadHeader { size: read_size };
    let header_bytes = protocol::encode_response(&header)
        .map_err(|e| PeerStorageError::ProtocolError { reason: e.to_string() })?;
    send.write_all(&header_bytes)
        .await
        .map_err(|e| PeerStorageError::ConnectionFailed { reason: e.to_string() })?;

    if read_size == 0 {
        send.finish()
            .map_err(|e| PeerStorageError::ConnectionFailed { reason: e.to_string() })?;
        return Ok(());
    }

    // Step 3: Channel-based streaming — fd stays in the blocking thread
    // 4 chunks in-flight ≈ 256KB buffer, provides backpressure
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, String>>(4);

    let reader_handle = tokio::task::spawn_blocking(move || {
        let api = app.android_fs();

        let (target_uri, _) = match resolve_content_uri_subpath(&app, &target_root, &target_sub) {
            Ok(v) => v,
            Err(e) => {
                let _ = tx.blocking_send(Err(format!("Failed to resolve URI: {e}")));
                return;
            }
        };

        let mut file = match api.open_file_readable(&target_uri) {
            Ok(f) => f,
            Err(e) => {
                let _ = tx.blocking_send(Err(format!("Failed to open file: {e:?}")));
                return;
            }
        };

        // Seek to offset if needed
        if offset > 0 {
            use std::io::Seek;
            if let Err(e) = file.seek(std::io::SeekFrom::Start(offset)) {
                let _ = tx.blocking_send(Err(format!("Failed to seek: {e}")));
                return;
            }
        }

        let mut remaining = read_size;
        let mut buf = vec![0u8; 64 * 1024];
        let mut chunks_sent: u64 = 0;

        while remaining > 0 {
            let to_read = (remaining as usize).min(buf.len());
            match file.read(&mut buf[..to_read]) {
                Ok(0) => {
                    eprintln!("[PeerStorage] Content URI read: EOF after {chunks_sent} chunks, {remaining} bytes remaining");
                    break;
                }
                Ok(n) => {
                    remaining -= n as u64;
                    chunks_sent += 1;
                    // Send chunk; if receiver dropped (QUIC error), stop reading
                    if tx.blocking_send(Ok(buf[..n].to_vec())).is_err() {
                        eprintln!("[PeerStorage] Content URI read: receiver dropped after {chunks_sent} chunks");
                        return;
                    }
                }
                Err(e) => {
                    eprintln!("[PeerStorage] Content URI read: IO error after {chunks_sent} chunks: {e}");
                    let _ = tx.blocking_send(Err(format!("Read error: {e}")));
                    return;
                }
            }
        }

        eprintln!("[PeerStorage] Content URI read: complete, {chunks_sent} chunks sent");
    });

    // Step 4: Receive chunks and write to QUIC stream
    while let Some(result) = rx.recv().await {
        match result {
            Ok(chunk) => {
                send.write_all(&chunk)
                    .await
                    .map_err(|e| PeerStorageError::ConnectionFailed {
                        reason: e.to_string(),
                    })?;
            }
            Err(e) => {
                // Reader reported an error — send error response to client
                eprintln!("[PeerStorage] Content URI streaming error: {e}");
                return Err(PeerStorageError::ProtocolError { reason: e });
            }
        }
    }

    // Wait for reader thread to finish
    let _ = reader_handle.await;

    send.finish()
        .map_err(|e| PeerStorageError::ConnectionFailed {
            reason: e.to_string(),
        })?;

    Ok(())
}

/// Write a file via Content URI. Receives file data from QUIC stream.
#[cfg(target_os = "android")]
async fn handle_write_content_uri(
    send: &mut iroh::endpoint::SendStream,
    recv: &mut iroh::endpoint::RecvStream,
    app_handle: &tauri::AppHandle,
    root_uri_json: &str,
    sub_path: &str,
    size: u64,
) -> Result<(), PeerStorageError> {
    use tauri_plugin_android_fs::AndroidFsExt;
    use std::io::Write;

    // Read all data from the stream first
    let mut data = Vec::with_capacity(size as usize);
    let mut buf = [0u8; 64 * 1024];
    let mut remaining = size;
    while remaining > 0 {
        let to_read = (remaining as usize).min(buf.len());
        let chunk = recv.read(&mut buf[..to_read]).await
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: format!("Failed to read file data: {e}"),
            })?;
        match chunk {
            Some(n) => {
                data.extend_from_slice(&buf[..n]);
                remaining -= n as u64;
            }
            None => break,
        }
    }

    // Write via android_fs (blocking JNI)
    let app = app_handle.clone();
    let root = root_uri_json.to_string();
    let sub = sub_path.to_string();

    let result = tokio::task::spawn_blocking(move || -> Result<(), String> {
        let api = app.android_fs();

        // Navigate to parent directory, creating as needed
        let path = std::path::Path::new(&sub);
        let file_name = path.file_name()
            .and_then(|n| n.to_str())
            .ok_or("Invalid file name")?;

        let (parent_uri, _) = resolve_content_uri_subpath(&app, &root, path.parent().and_then(|p| p.to_str()).unwrap_or(""))
            .map_err(|e| format!("Failed to resolve parent: {e}"))?;

        // Create file in parent directory
        let file_uri = api.create_new_file(&parent_uri, file_name, None)
            .map_err(|e| format!("Failed to create file: {e:?}"))?;

        // Write data
        let mut file = api.open_file_writable(&file_uri)
            .map_err(|e| format!("Failed to open for writing: {e:?}"))?;
        file.write_all(&data)
            .map_err(|e| format!("Failed to write: {e}"))?;

        Ok(())
    }).await.map_err(|e| PeerStorageError::ProtocolError {
        reason: format!("Task failed: {e}"),
    })?;

    let resp = match result {
        Ok(()) => Response::WriteOk,
        Err(e) => Response::Error { message: e },
    };

    let resp_bytes = protocol::encode_response(&resp)
        .map_err(|e| PeerStorageError::ProtocolError { reason: e.to_string() })?;
    send.write_all(&resp_bytes).await
        .map_err(|e| PeerStorageError::ConnectionFailed { reason: e.to_string() })?;
    send.finish()
        .map_err(|e| PeerStorageError::ConnectionFailed { reason: e.to_string() })?;

    Ok(())
}

/// Delete a file/directory via Content URI.
#[cfg(target_os = "android")]
fn delete_content_uri(
    app_handle: &tauri::AppHandle,
    root_uri_json: &str,
    sub_path: &str,
    _to_trash: bool,
) -> Result<(), String> {
    use tauri_plugin_android_fs::AndroidFsExt;

    let (target_uri, is_dir) = resolve_content_uri_subpath(app_handle, root_uri_json, sub_path)?;
    let api = app_handle.android_fs();
    if is_dir {
        api.remove_dir_all(&target_uri)
    } else {
        api.remove_file(&target_uri)
    }
    .map_err(|e| format!("Failed to delete: {e:?}"))
}

/// Recursively scan a Content URI directory and return FileState entries.
#[cfg(target_os = "android")]
fn manifest_content_uri(
    app_handle: &tauri::AppHandle,
    root_uri_json: &str,
    sub_path: &str,
) -> Result<Vec<crate::file_sync::types::FileState>, String> {
    let (target_uri, is_dir) = resolve_content_uri_subpath(app_handle, root_uri_json, sub_path)?;

    if !is_dir {
        return Err("Not a directory".to_string());
    }

    let mut entries = Vec::new();
    collect_manifest_recursive(app_handle, &target_uri, "", &mut entries)?;
    Ok(entries)
}

#[cfg(target_os = "android")]
fn collect_manifest_recursive(
    app_handle: &tauri::AppHandle,
    dir_uri: &tauri_plugin_android_fs::FileUri,
    prefix: &str,
    entries: &mut Vec<crate::file_sync::types::FileState>,
) -> Result<(), String> {
    use tauri_plugin_android_fs::AndroidFsExt;

    let api = app_handle.android_fs();
    let dir_entries = api.read_dir(dir_uri)
        .map_err(|e| format!("Failed to read dir: {e:?}"))?;

    for entry in dir_entries {
        let name = entry.name().to_string();
        let relative_path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{}/{}", prefix, name)
        };

        let modified_at = entry
            .last_modified()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let is_directory = entry.is_dir();
        let size = if is_directory { 0 } else { entry.file_len().unwrap_or(0) };

        entries.push(crate::file_sync::types::FileState {
            relative_path: relative_path.clone(),
            size,
            modified_at,
            is_directory,
        });

        if is_directory {
            let child_uri = entry.uri().clone();
            collect_manifest_recursive(app_handle, &child_uri, &relative_path, entries)?;
        }
    }

    Ok(())
}

/// Create a directory via Content URI.
#[cfg(target_os = "android")]
fn create_directory_content_uri(
    app_handle: &tauri::AppHandle,
    root_uri_json: &str,
    sub_path: &str,
) -> Result<(), String> {
    use tauri_plugin_android_fs::AndroidFsExt;

    crate::filesystem::reject_path_traversal(sub_path)?;

    let trimmed = sub_path.trim_start_matches('/');
    if trimmed.is_empty() {
        return Ok(()); // Root already exists
    }

    let api = app_handle.android_fs();
    let root = tauri_plugin_android_fs::FileUri::from_json_str(root_uri_json)
        .map_err(|e| format!("Invalid Content URI: {e:?}"))?;

    api.create_dir_all(&root, trimmed)
        .map_err(|e| format!("Failed to create directory '{}': {e:?}", trimmed))?;

    Ok(())
}

// ============================================================================
// Filesystem helpers
// ============================================================================

async fn read_dir_entries(dir: &Path) -> Result<Vec<FileEntry>, std::io::Error> {
    let mut entries = Vec::new();
    let mut read_dir = tokio::fs::read_dir(dir).await?;

    while let Some(entry) = read_dir.next_entry().await? {
        if let Ok(fe) = file_entry_from_dir_entry(&entry).await {
            entries.push(fe);
        }
    }

    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(entries)
}

async fn file_entry_from_dir_entry(
    entry: &tokio::fs::DirEntry,
) -> Result<FileEntry, std::io::Error> {
    let metadata = entry.metadata().await?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    Ok(FileEntry {
        name: entry.file_name().to_string_lossy().to_string(),
        size: metadata.len(),
        is_dir: metadata.is_dir(),
        modified,
    })
}

fn file_entry_from_path(path: &Path) -> Result<FileEntry, std::io::Error> {
    let metadata = std::fs::metadata(path)?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    Ok(FileEntry {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        size: metadata.len(),
        is_dir: metadata.is_dir(),
        modified,
    })
}

// ============================================================================
// UCAN helpers
// ============================================================================

/// Determine which space a request path targets, for UCAN capability checking.
fn find_space_for_path(
    shares: &HashMap<String, SharedFolder>,
    allowed_spaces: &HashSet<String>,
    request_path: &str,
) -> Option<String> {
    if request_path.is_empty() || request_path == "/" {
        return None; // Root listing — no specific space
    }
    find_share_and_subpath(shares, allowed_spaces, request_path)
        .ok()
        .map(|(share, _)| share.space_id.clone())
}

// ============================================================================
// Write handlers
// ============================================================================

/// Handle an incoming Write request: read file data from the stream, write to disk.
async fn handle_write(
    send: &mut iroh::endpoint::SendStream,
    recv: &mut iroh::endpoint::RecvStream,
    state: &RwLock<PeerState>,
    path: &str,
    size: u64,
    allowed_spaces: &HashSet<String>,
) -> Result<(), PeerStorageError> {
    // Check for Content URI shares (Android)
    let content_uri_info = {
        let state = state.read().await;
        if let Ok((share, sub_path)) = find_share_and_subpath(&state.shares, allowed_spaces, path) {
            if is_content_uri(&share.local_path) {
                Some((share.local_path.clone(), sub_path, state.app_handle.clone()))
            } else {
                None
            }
        } else {
            None
        }
    };

    if let Some((_root_uri, _sub_path, _app_handle_opt)) = content_uri_info {
        #[cfg(target_os = "android")]
        {
            return handle_write_content_uri(
                send, recv, &_app_handle_opt.ok_or_else(|| PeerStorageError::ProtocolError {
                    reason: "AppHandle not available".to_string(),
                })?,
                &_root_uri, &_sub_path, size,
            ).await;
        }
        #[cfg(not(target_os = "android"))]
        {
            let resp = Response::Error { message: CONTENT_URI_ANDROID_ONLY.to_string() };
            let resp_bytes = protocol::encode_response(&resp)
                .map_err(|e| PeerStorageError::ProtocolError { reason: e.to_string() })?;
            send.write_all(&resp_bytes).await.ok();
            send.finish().ok();
            return Ok(());
        }
    }

    let local_path = {
        let state = state.read().await;
        match resolve_path_for_write(&state.shares, allowed_spaces, path) {
            Ok(p) => p,
            Err(resp) => {
                let resp_bytes = protocol::encode_response(&resp)
                    .map_err(|e| PeerStorageError::ProtocolError { reason: e.to_string() })?;
                send.write_all(&resp_bytes).await.ok();
                send.finish().ok();
                return Ok(());
            }
        }
    };

    // Create parent directories
    if let Some(parent) = local_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(PeerStorageError::Io)?;
    }

    // Read file data from the stream and write to disk
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::File::create(&local_path)
        .await
        .map_err(PeerStorageError::Io)?;

    let mut remaining = size;
    let mut buf = [0u8; 64 * 1024];

    while remaining > 0 {
        let to_read = (remaining as usize).min(buf.len());
        let chunk = recv.read(&mut buf[..to_read]).await
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: format!("Failed to read file data: {e}"),
            })?;
        match chunk {
            Some(n) => {
                file.write_all(&buf[..n]).await.map_err(PeerStorageError::Io)?;
                remaining -= n as u64;
            }
            None => break,
        }
    }

    file.flush().await.map_err(PeerStorageError::Io)?;

    // Send success response
    let resp = Response::WriteOk;
    let resp_bytes = protocol::encode_response(&resp)
        .map_err(|e| PeerStorageError::ProtocolError { reason: e.to_string() })?;
    send.write_all(&resp_bytes)
        .await
        .map_err(|e| PeerStorageError::ConnectionFailed { reason: e.to_string() })?;
    send.finish()
        .map_err(|e| PeerStorageError::ConnectionFailed { reason: e.to_string() })?;

    Ok(())
}

/// Handle an incoming Delete request.
async fn handle_delete(
    state: &RwLock<PeerState>,
    path: &str,
    to_trash: bool,
    allowed_spaces: &HashSet<String>,
) -> Response {
    // Check for Content URI shares (Android)
    if let Ok((share, _sub)) = {
        let s = state.read().await;
        find_share_and_subpath(&s.shares, allowed_spaces, path).map(|(sh, sub)| (sh.clone(), sub))
    } {
        if is_content_uri(&share.local_path) {
            #[cfg(target_os = "android")]
            {
                let app_handle = {
                    let s = state.read().await;
                    match &s.app_handle {
                        Some(h) => h.clone(),
                        None => return Response::Error { message: "AppHandle not available".to_string() },
                    }
                };
                let root_uri = share.local_path.clone();
                return match tokio::task::spawn_blocking(move || {
                    delete_content_uri(&app_handle, &root_uri, &_sub, to_trash)
                }).await {
                    Ok(Ok(())) => Response::DeleteOk,
                    Ok(Err(e)) => Response::Error { message: e },
                    Err(e) => Response::Error { message: format!("Task failed: {e}") },
                };
            }
            #[cfg(not(target_os = "android"))]
            return Response::Error { message: CONTENT_URI_ANDROID_ONLY.to_string() };
        }
    }

    let local_path = {
        let state = state.read().await;
        match resolve_path_filtered(&state.shares, allowed_spaces, path) {
            Ok(p) => p,
            Err(resp) => return resp,
        }
    };

    if !local_path.exists() {
        return Response::Error {
            message: "File not found".to_string(),
        };
    }

    if to_trash {
        #[cfg(not(target_os = "android"))]
        {
            if let Err(e) = trash::delete(&local_path) {
                return Response::Error {
                    message: format!("Failed to trash: {e}"),
                };
            }
        }
        #[cfg(target_os = "android")]
        {
            if let Err(e) = tokio::fs::remove_file(&local_path).await {
                return Response::Error {
                    message: format!("Failed to delete: {e}"),
                };
            }
        }
    } else if local_path.is_dir() {
        if let Err(e) = tokio::fs::remove_dir_all(&local_path).await {
            return Response::Error {
                message: format!("Failed to delete directory: {e}"),
            };
        }
    } else if let Err(e) = tokio::fs::remove_file(&local_path).await {
        return Response::Error {
            message: format!("Failed to delete file: {e}"),
        };
    }

    Response::DeleteOk
}

/// Handle an incoming CreateDirectory request.
async fn handle_create_directory(
    state: &RwLock<PeerState>,
    path: &str,
    allowed_spaces: &HashSet<String>,
) -> Response {
    // Check for Content URI shares (Android)
    if let Ok((share, _sub)) = {
        let s = state.read().await;
        find_share_and_subpath(&s.shares, allowed_spaces, path).map(|(sh, sub)| (sh.clone(), sub))
    } {
        if is_content_uri(&share.local_path) {
            #[cfg(target_os = "android")]
            {
                let app_handle = {
                    let s = state.read().await;
                    match &s.app_handle {
                        Some(h) => h.clone(),
                        None => return Response::Error { message: "AppHandle not available".to_string() },
                    }
                };
                let root_uri = share.local_path.clone();
                return match tokio::task::spawn_blocking(move || {
                    create_directory_content_uri(&app_handle, &root_uri, &_sub)
                }).await {
                    Ok(Ok(())) => Response::CreateDirectoryOk,
                    Ok(Err(e)) => Response::Error { message: e },
                    Err(e) => Response::Error { message: format!("Task failed: {e}") },
                };
            }
            #[cfg(not(target_os = "android"))]
            return Response::Error { message: CONTENT_URI_ANDROID_ONLY.to_string() };
        }
    }

    let local_path = {
        let state = state.read().await;
        match resolve_path_for_write(&state.shares, allowed_spaces, path) {
            Ok(p) => p,
            Err(resp) => return resp,
        }
    };

    match tokio::fs::create_dir_all(&local_path).await {
        Ok(()) => Response::CreateDirectoryOk,
        Err(e) => Response::Error {
            message: format!("Failed to create directory: {e}"),
        },
    }
}

/// Resolve a request path for write operations.
/// Similar to `resolve_path_filtered` but doesn't require the path to exist yet
/// (needed for creating new files/directories).
fn resolve_path_for_write(
    shares: &HashMap<String, SharedFolder>,
    allowed_spaces: &HashSet<String>,
    request_path: &str,
) -> Result<PathBuf, Response> {
    let (share, sub_path) = find_share_and_subpath(shares, allowed_spaces, request_path)?;

    crate::filesystem::check_relative_path(&sub_path).map_err(|message| Response::Error { message })?;

    let full_path = PathBuf::from(&share.local_path).join(&sub_path);

    // Canonicalize the share root and check the target is within it
    let share_canonical = PathBuf::from(&share.local_path).canonicalize().map_err(|_| Response::Error {
        message: "Share path invalid".to_string(),
    })?;

    // For new files, canonicalize the parent (which must exist) and verify containment
    let parent = full_path.parent().unwrap_or(&full_path);
    if parent.exists() {
        let parent_canonical = parent.canonicalize().map_err(|_| Response::Error {
            message: "Parent path invalid".to_string(),
        })?;
        if !parent_canonical.starts_with(&share_canonical) {
            return Err(Response::Error {
                message: "Access denied: path outside share".to_string(),
            });
        }
    }

    Ok(full_path)
}
