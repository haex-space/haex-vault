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
}

impl serde::Serialize for PeerStorageError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
