//! iroh Endpoint management
//!
//! Manages the iroh QUIC endpoint: starting, stopping, accepting connections,
//! and handling incoming file requests.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

use iroh::{Endpoint, EndpointId, SecretKey};

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
}

/// Peer storage endpoint state
pub struct PeerEndpoint {
    /// The iroh endpoint (None if not running)
    endpoint: Option<Endpoint>,
    /// Secret key for this node
    secret_key: SecretKey,
    /// Shared folders (id -> folder)
    shares: HashMap<String, SharedFolder>,
    /// Handle to the accept loop task
    accept_task: Option<tokio::task::JoinHandle<()>>,
}

impl PeerEndpoint {
    /// Create a new PeerEndpoint with a persistent device key.
    pub fn new(secret_key: SecretKey) -> Self {
        Self {
            endpoint: None,
            secret_key,
            shares: HashMap::new(),
            accept_task: None,
        }
    }

    /// Create a PeerEndpoint with a temporary random key (for testing or pre-init state).
    pub fn new_ephemeral() -> Self {
        // TODO: Replace with SecretKey::generate() once p256 upgrades to rand_core 0.9
        // Currently iroh uses rand_core 0.9 but p256 uses rand_core 0.6 (via rand 0.8).
        // Track: when p256 >= 0.14 ships with rand_core 0.9, upgrade rand to 0.9 project-wide.
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

    /// Start the iroh endpoint and begin accepting connections
    pub async fn start(&mut self) -> Result<EndpointId, PeerStorageError> {
        if self.endpoint.is_some() {
            return Err(PeerStorageError::EndpointAlreadyRunning);
        }

        let endpoint = Endpoint::builder()
            .secret_key(self.secret_key.clone())
            .alpns(vec![ALPN.to_vec()])
            .bind()
            .await
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: format!("Failed to bind endpoint: {e}"),
            })?;

        let id = endpoint.id();
        eprintln!("[PeerStorage] Endpoint started with ID: {id}");

        // Spawn accept loop
        let ep = endpoint.clone();
        let shares = Arc::new(RwLock::new(self.shares.clone()));

        let accept_task = tokio::spawn(async move {
            accept_loop(ep, shares).await;
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
    pub fn add_share(&mut self, id: String, name: String, local_path: PathBuf) {
        eprintln!("[PeerStorage] Added share '{name}' at {}", local_path.display());
        self.shares.insert(id, SharedFolder { name, local_path });
    }

    /// Remove a shared folder
    pub fn remove_share(&mut self, id: &str) -> bool {
        self.shares.remove(id).is_some()
    }

    /// List shared folders
    pub fn list_shares(&self) -> Vec<(String, SharedFolder)> {
        self.shares.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    /// Clear all shares (used before reloading from DB)
    pub fn clear_shares(&mut self) {
        self.shares.clear();
    }

    /// Connect to a remote peer and list a directory
    pub async fn remote_list(
        &self,
        remote_id: EndpointId,
        path: &str,
    ) -> Result<Vec<FileEntry>, PeerStorageError> {
        let endpoint = self
            .endpoint
            .as_ref()
            .ok_or(PeerStorageError::EndpointNotRunning)?;

        let conn = endpoint
            .connect(remote_id, ALPN)
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
        path: &str,
        range: Option<[u64; 2]>,
    ) -> Result<(u64, Vec<u8>), PeerStorageError> {
        let endpoint = self
            .endpoint
            .as_ref()
            .ok_or(PeerStorageError::EndpointNotRunning)?;

        let conn = endpoint
            .connect(remote_id, ALPN)
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
// Accept loop — handles incoming connections
// ============================================================================

async fn accept_loop(endpoint: Endpoint, shares: Arc<RwLock<HashMap<String, SharedFolder>>>) {
    while let Some(incoming) = endpoint.accept().await {
        let shares = shares.clone();
        tokio::spawn(async move {
            match incoming.await {
                Ok(conn) => {
                    let remote = conn.remote_id();
                    eprintln!("[PeerStorage] Accepted connection from {remote}");
                    handle_connection(conn, shares).await;
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
    shares: Arc<RwLock<HashMap<String, SharedFolder>>>,
) {
    let remote = conn.remote_id();

    loop {
        match conn.accept_bi().await {
            Ok((send, mut recv)) => {
                let shares = shares.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_stream(send, &mut recv, &shares).await {
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
    shares: &RwLock<HashMap<String, SharedFolder>>,
) -> Result<(), PeerStorageError> {
    let request = protocol::read_request(recv)
        .await
        .map_err(|e| PeerStorageError::ProtocolError {
            reason: e.to_string(),
        })?;

    let response = match request {
        Request::List { path } => handle_list(shares, &path).await,
        Request::Stat { path } => handle_stat(shares, &path).await,
        Request::Read { path, range } => {
            return handle_read(&mut send, shares, &path, range).await;
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
// Request handlers
// ============================================================================

fn resolve_path(
    shares: &HashMap<String, SharedFolder>,
    request_path: &str,
) -> Result<PathBuf, Response> {
    let trimmed = request_path.trim_start_matches('/');
    let (share_id, sub_path) = trimmed.split_once('/').unwrap_or((trimmed, ""));

    let share = shares.get(share_id).ok_or_else(|| Response::Error {
        message: format!("Share not found: {share_id}"),
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
    shares: &RwLock<HashMap<String, SharedFolder>>,
    path: &str,
) -> Response {
    let shares = shares.read().await;

    if path.is_empty() || path == "/" {
        let entries: Vec<FileEntry> = shares
            .iter()
            .map(|(id, share)| FileEntry {
                name: format!("{id} — {}", share.name),
                size: 0,
                is_dir: true,
                modified: None,
            })
            .collect();
        return Response::List { entries };
    }

    let local_path = match resolve_path(&shares, path) {
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
    shares: &RwLock<HashMap<String, SharedFolder>>,
    path: &str,
) -> Response {
    let shares = shares.read().await;
    let local_path = match resolve_path(&shares, path) {
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
    shares: &RwLock<HashMap<String, SharedFolder>>,
    path: &str,
    range: Option<[u64; 2]>,
) -> Result<(), PeerStorageError> {
    let local_path = {
        let shares = shares.read().await;
        match resolve_path(&shares, path) {
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
