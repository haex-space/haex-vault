//! Error types for peer storage

#[derive(Debug, thiserror::Error)]
pub enum PeerStorageError {
    #[error("Endpoint not running")]
    EndpointNotRunning,

    #[error("Endpoint already running")]
    EndpointAlreadyRunning,

    #[error("Connection failed: {reason}")]
    ConnectionFailed { reason: String },

    #[error("Protocol error: {reason}")]
    ProtocolError { reason: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Path not shared: {path}")]
    PathNotShared { path: String },

    #[error("Access denied for peer {peer_id}")]
    AccessDenied { peer_id: String },

    #[error("Path traversal attempt: {path}")]
    PathTraversal { path: String },

    #[error("Database error: {reason}")]
    Database { reason: String },

    /// The caller supplied chunk-metadata (e.g. from a sync manifest) that
    /// disagrees with the stat-probe's authoritative server-side hashes.
    /// Surfaced as a distinct variant so the engine can report "the file
    /// changed underneath the manifest" without conflating it with transport
    /// or auth errors.
    #[error(
        "Manifest hash mismatch: manifest claims {manifest_file_hash}, server reports {actual_file_hash}"
    )]
    ManifestHashMismatch {
        manifest_file_hash: String,
        actual_file_hash: String,
    },

    /// A chunk delivered over the wire did not match the expected BLAKE3
    /// hash. Distinct from `ManifestHashMismatch` (which fires before any
    /// bytes are read) so the engine can react with per-range retry against
    /// a different peer instead of aborting the whole sync.
    #[error("Chunk hash mismatch at index {index}: expected {expected}, got {actual}")]
    ChunkHashMismatch {
        index: usize,
        expected: String,
        actual: String,
    },

    /// The transfer was aborted via its cancellation token (user pressed
    /// cancel). Distinct from a transport failure so the retry pool does NOT
    /// re-attempt it and the frontend can treat it as a deliberate action
    /// rather than an error. The message contains "cancelled" so existing
    /// substring checks keep working.
    #[error("Transfer cancelled")]
    Cancelled,
}

impl serde::Serialize for PeerStorageError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
