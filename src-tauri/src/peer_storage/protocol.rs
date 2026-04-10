//! File access protocol over QUIC streams
//!
//! Simple request/response protocol for browsing, reading, and writing remote files.
//! Every request carries a UCAN token for per-request authorization.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// ALPN protocol identifier for peer storage
pub const ALPN: &[u8] = b"haex-peer/1";

/// Maximum request size (1 MB — covers Write header but not file data)
const MAX_REQUEST_SIZE: usize = 1024 * 1024;

/// Maximum metadata response size (10 MB — large directory listings)
const MAX_RESPONSE_META_SIZE: usize = 10 * 1024 * 1024;

// ============================================================================
// Request types
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Request {
    /// List directory contents
    List {
        path: String,
        ucan_token: String,
    },
    /// Get file/directory metadata
    Stat {
        path: String,
        ucan_token: String,
    },
    /// Read a file (with optional byte range)
    Read {
        path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        range: Option<[u64; 2]>,
        ucan_token: String,
    },
    /// Recursive file manifest for sync
    Manifest {
        path: String,
        ucan_token: String,
    },
    /// Write a file. File data follows on the stream after this header.
    Write {
        path: String,
        size: u64,
        ucan_token: String,
    },
    /// Delete a file
    Delete {
        path: String,
        to_trash: bool,
        ucan_token: String,
    },
    /// Create a directory (including parents)
    CreateDirectory {
        path: String,
        ucan_token: String,
    },
}

impl Request {
    /// Extract the UCAN token from any request variant.
    pub fn ucan_token(&self) -> &str {
        match self {
            Request::List { ucan_token, .. }
            | Request::Stat { ucan_token, .. }
            | Request::Read { ucan_token, .. }
            | Request::Manifest { ucan_token, .. }
            | Request::Write { ucan_token, .. }
            | Request::Delete { ucan_token, .. }
            | Request::CreateDirectory { ucan_token, .. } => ucan_token,
        }
    }

    /// Whether this request requires write capability.
    pub fn requires_write(&self) -> bool {
        matches!(
            self,
            Request::Write { .. } | Request::Delete { .. } | Request::CreateDirectory { .. }
        )
    }
}

// ============================================================================
// Response types
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    /// Directory listing
    List { entries: Vec<FileEntry> },
    /// File/directory metadata
    Stat { entry: FileEntry },
    /// File data header (actual bytes follow on the stream)
    ReadHeader { size: u64 },
    /// Recursive manifest of all files
    Manifest { entries: Vec<crate::file_sync::types::FileState> },
    /// Write completed successfully
    WriteOk,
    /// Delete completed successfully
    DeleteOk,
    /// Directory created successfully
    CreateDirectoryOk,
    /// Error response
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileEntry {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub modified: Option<u64>,
}

// ============================================================================
// Wire format helpers
// ============================================================================

/// Encode a request to bytes (length-prefixed JSON)
pub fn encode_request(req: &Request) -> Result<Vec<u8>, serde_json::Error> {
    let json = serde_json::to_vec(req)?;
    let len = (json.len() as u32).to_be_bytes();
    let mut buf = Vec::with_capacity(4 + json.len());
    buf.extend_from_slice(&len);
    buf.extend_from_slice(&json);
    Ok(buf)
}

/// Encode a response to bytes (length-prefixed JSON)
pub fn encode_response(resp: &Response) -> Result<Vec<u8>, serde_json::Error> {
    let json = serde_json::to_vec(resp)?;
    let len = (json.len() as u32).to_be_bytes();
    let mut buf = Vec::with_capacity(4 + json.len());
    buf.extend_from_slice(&len);
    buf.extend_from_slice(&json);
    Ok(buf)
}

/// Read a length-prefixed JSON message from a QUIC receive stream
pub async fn read_message<T: serde::de::DeserializeOwned>(
    recv: &mut iroh::endpoint::RecvStream,
    max_size: usize,
) -> Result<T, PeerProtocolError> {
    let mut len_buf = [0u8; 4];
    recv.read_exact(&mut len_buf)
        .await
        .map_err(|e| PeerProtocolError::Read(e.to_string()))?;
    let len = u32::from_be_bytes(len_buf) as usize;

    if len > max_size {
        return Err(PeerProtocolError::MessageTooLarge { size: len, max: max_size });
    }

    let mut buf = vec![0u8; len];
    recv.read_exact(&mut buf)
        .await
        .map_err(|e| PeerProtocolError::Read(e.to_string()))?;

    serde_json::from_slice(&buf).map_err(|e| PeerProtocolError::InvalidJson(e.to_string()))
}

/// Read an incoming request
pub async fn read_request(
    recv: &mut iroh::endpoint::RecvStream,
) -> Result<Request, PeerProtocolError> {
    read_message(recv, MAX_REQUEST_SIZE).await
}

/// Read an incoming response
pub async fn read_response(
    recv: &mut iroh::endpoint::RecvStream,
) -> Result<Response, PeerProtocolError> {
    read_message(recv, MAX_RESPONSE_META_SIZE).await
}

#[derive(Debug, thiserror::Error)]
pub enum PeerProtocolError {
    #[error("Failed to read from stream: {0}")]
    Read(String),
    #[error("Invalid JSON: {0}")]
    InvalidJson(String),
    #[error("Message too large: {size} bytes (max {max})")]
    MessageTooLarge { size: usize, max: usize },
}
