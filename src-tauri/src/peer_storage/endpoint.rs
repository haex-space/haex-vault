//! iroh Endpoint management
//!
//! Manages the iroh QUIC endpoint: starting, stopping, accepting connections,
//! and handling incoming file requests. Access control ensures only peers
//! registered in the same Space can access shared folders.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

use iroh::{Endpoint, EndpointAddr, EndpointId, RelayMode, RelayUrl, SecretKey};

const DEFAULT_RELAY_URL: &str = "https://relay.sync.haex.space";

use crate::peer_storage::error::PeerStorageError;
use crate::peer_storage::protocol::{self, FileEntry, Request, Response, ALPN};

// ============================================================================
// Shared state
// ============================================================================

/// A folder shared with peers
#[derive(Debug, Clone)]
pub struct SharedFolder {
    /// Display name
    pub name: String,
    /// Local filesystem path
    pub local_path: PathBuf,
    /// Space this share belongs to (for access control)
    pub space_id: String,
}

/// State shared between PeerEndpoint methods and the accept loop
#[derive(Debug, Default)]
pub struct PeerState {
    /// Shared folders (share_id -> folder)
    pub shares: HashMap<String, SharedFolder>,
    /// Access control: remote EndpointId (string) -> set of space_ids they may access
    pub allowed_peers: HashMap<String, HashSet<String>>,
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
}

impl PeerEndpoint {
    /// Create a new PeerEndpoint with a persistent device key.
    pub fn new(secret_key: SecretKey) -> Self {
        Self {
            endpoint: None,
            secret_key,
            state: Arc::new(RwLock::new(PeerState::default())),
            accept_task: None,
        }
    }

    /// Create a PeerEndpoint with a temporary random key (for testing or pre-init state).
    pub fn new_ephemeral() -> Self {
        let mut bytes = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut bytes);
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

    /// Get the public EndpointId
    pub fn endpoint_id(&self) -> EndpointId {
        self.secret_key.public()
    }

    /// Check if the endpoint is running
    pub fn is_running(&self) -> bool {
        self.endpoint.is_some()
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
                RelayMode::custom([parsed])
            }
            Err(e) => {
                eprintln!("[PeerStorage] Invalid relay URL '{effective_relay}': {e} — falling back to iroh default");
                RelayMode::Default
            }
        };

        let endpoint = Endpoint::builder()
            .secret_key(self.secret_key.clone())
            .alpns(vec![ALPN.to_vec()])
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
    pub async fn add_share(&self, id: String, name: String, local_path: PathBuf, space_id: String) {
        eprintln!("[PeerStorage] Added share '{name}' at {} (space: {space_id})", local_path.display());
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

    /// Connect to a remote peer and list a directory
    pub async fn remote_list(
        &self,
        remote_id: EndpointId,
        relay_url: Option<RelayUrl>,
        path: &str,
    ) -> Result<Vec<FileEntry>, PeerStorageError> {
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
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: e.to_string(),
            })?;

        let (mut send, mut recv) = conn
            .open_bi()
            .await
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: e.to_string(),
            })?;

        // Send LIST request
        let req = Request::List { path: path.to_string() };
        let req_bytes = protocol::encode_request(&req)
            .map_err(|e| PeerStorageError::ProtocolError { reason: e.to_string() })?;
        send.write_all(&req_bytes)
            .await
            .map_err(|e| PeerStorageError::ConnectionFailed { reason: e.to_string() })?;
        send.finish()
            .map_err(|e| PeerStorageError::ConnectionFailed { reason: e.to_string() })?;

        // Read response
        let response: Response = protocol::read_response(&mut recv)
            .await
            .map_err(|e| PeerStorageError::ProtocolError { reason: e.to_string() })?;

        match response {
            Response::List { entries } => Ok(entries),
            Response::Error { message } => Err(PeerStorageError::ProtocolError { reason: message }),
            _ => Err(PeerStorageError::ProtocolError {
                reason: "Unexpected response type".to_string(),
            }),
        }
    }

    /// Connect to a remote peer and read a file
    pub async fn remote_read(
        &self,
        remote_id: EndpointId,
        relay_url: Option<RelayUrl>,
        path: &str,
        range: Option<[u64; 2]>,
    ) -> Result<(u64, Vec<u8>), PeerStorageError> {
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
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: e.to_string(),
            })?;

        let (mut send, mut recv) = conn
            .open_bi()
            .await
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: e.to_string(),
            })?;

        // Send READ request
        let req = Request::Read {
            path: path.to_string(),
            range,
        };
        let req_bytes = protocol::encode_request(&req)
            .map_err(|e| PeerStorageError::ProtocolError { reason: e.to_string() })?;
        send.write_all(&req_bytes)
            .await
            .map_err(|e| PeerStorageError::ConnectionFailed { reason: e.to_string() })?;
        send.finish()
            .map_err(|e| PeerStorageError::ConnectionFailed { reason: e.to_string() })?;

        // Read response header
        let response: Response = protocol::read_response(&mut recv)
            .await
            .map_err(|e| PeerStorageError::ProtocolError { reason: e.to_string() })?;

        match response {
            Response::ReadHeader { size } => {
                // Read file bytes from stream
                let data = recv
                    .read_to_end(size as usize)
                    .await
                    .map_err(|e| PeerStorageError::ConnectionFailed {
                        reason: e.to_string(),
                    })?;
                Ok((size, data))
            }
            Response::Error { message } => {
                Err(PeerStorageError::ProtocolError { reason: message })
            }
            _ => Err(PeerStorageError::ProtocolError {
                reason: "Unexpected response type".to_string(),
            }),
        }
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
                    let remote = conn.remote_id();
                    let remote_str = remote.to_string();

                    // Check access control: which spaces is this peer allowed to access?
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
                            // Connection will be dropped, closing it
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[PeerStorage] Failed to accept connection: {e}");
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

    let response = match request {
        Request::List { path } => handle_list(state, &path, allowed_spaces).await,
        Request::Stat { path } => handle_stat(state, &path, allowed_spaces).await,
        Request::Read { path, range } => {
            return handle_read(&mut send, state, &path, range, allowed_spaces).await;
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

fn resolve_path_filtered(
    shares: &HashMap<String, SharedFolder>,
    allowed_spaces: &HashSet<String>,
    request_path: &str,
) -> Result<PathBuf, Response> {
    let trimmed = request_path.trim_start_matches('/');
    let (share_name, sub_path) = trimmed.split_once('/').unwrap_or((trimmed, ""));

    // Look up share by name (used in directory listing) or by ID (legacy)
    let share = shares.values()
        .find(|s| s.name == share_name && allowed_spaces.contains(&s.space_id))
        .or_else(|| shares.get(share_name).filter(|s| allowed_spaces.contains(&s.space_id)))
        .ok_or_else(|| Response::Error {
            message: format!("Share not found: {share_name}"),
        })?;

    let full_path = share.local_path.join(sub_path);

    // Prevent path traversal
    let canonical = full_path.canonicalize().map_err(|_| Response::Error {
        message: "Path not found".to_string(),
    })?;
    let share_canonical = share.local_path.canonicalize().map_err(|_| Response::Error {
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

async fn handle_stat(
    state: &RwLock<PeerState>,
    path: &str,
    allowed_spaces: &HashSet<String>,
) -> Response {
    let state = state.read().await;
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

    use tokio::io::AsyncReadExt;

    let mut file = tokio::fs::File::open(&local_path)
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
