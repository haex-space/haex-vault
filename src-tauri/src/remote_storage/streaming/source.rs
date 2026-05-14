//! Streaming source trait + shared types.
//!
//! Keep this trait minimal so the matrix of (target × operation) stays
//! tractable: every new target only has to answer "how big is this?" and
//! "give me bytes [start..=end]".

use async_trait::async_trait;
use thiserror::Error;

/// Inclusive byte range `[start..=end]`. Matches the semantics of the
/// HTTP `Range: bytes=N-M` header so adapters don't have to reinterpret
/// edges.
///
/// Fields are private to prevent construction of inverted ranges (`end <
/// start`) that would underflow when computing length.
#[derive(Debug, Clone, Copy)]
pub struct ByteRange {
    start: u64,
    end: u64,
}

impl ByteRange {
    /// Build an inclusive range, rejecting `end < start`.
    pub fn new(start: u64, end: u64) -> Result<Self, StreamingError> {
        if end < start {
            return Err(StreamingError::BadRequest(format!(
                "invalid range: end ({end}) < start ({start})"
            )));
        }
        Ok(Self { start, end })
    }

    pub fn start(&self) -> u64 {
        self.start
    }

    pub fn end(&self) -> u64 {
        self.end
    }
}

/// Errors that surface from streaming sources.
///
/// Distinct from `StorageError` because streaming has different failure
/// modes (range out of bounds, missing source) and we want the protocol
/// handler to map them to HTTP status codes without a translation layer.
#[derive(Debug, Error)]
pub enum StreamingError {
    #[error("source not found: {0}")]
    NotFound(String),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("backend error: {0}")]
    Backend(String),
}

/// A resolved location inside the `haex-stream://` URL space.
///
/// Path format (after the leading `/`):
///   - `s3/<backend_id>/<key…>`
///   - `local/<base64url(path)>`
///   - `p2p/<node_id>/<blob_hash>`
///
/// Adapters parse this from the URI path before constructing a concrete
/// source. New targets add a variant here + a match arm in the protocol
/// handler factory.
#[derive(Debug)]
pub enum StreamRoute {
    S3 { backend_id: String, key: String },
}

#[async_trait]
pub trait StreamingSource: Send + Sync {
    /// Total size in bytes. Cached upstream — called once per request.
    async fn size(&self) -> Result<u64, StreamingError>;

    /// Read bytes `[range.start..=range.end]`. Adapters must return exactly
    /// `range.end - range.start + 1` bytes on success.
    async fn read_range(&self, range: ByteRange) -> Result<Vec<u8>, StreamingError>;

    /// Optional content type. The protocol handler falls back to
    /// `application/octet-stream` if `None`.
    async fn content_type(&self) -> Option<String> {
        None
    }
}
