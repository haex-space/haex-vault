//! Error types for LocalSend module

use thiserror::Error;

#[derive(Debug, Error)]
pub enum LocalSendError {
    #[error("Server already running")]
    ServerAlreadyRunning,

    #[error("Server not running")]
    ServerNotRunning,

    #[error("Discovery already running")]
    DiscoveryAlreadyRunning,

    #[error("Discovery not running")]
    DiscoveryNotRunning,

    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    #[error("Transfer rejected by receiver")]
    TransferRejected,

    #[error("Transfer cancelled")]
    TransferCancelled,

    #[error("Transfer failed: {0}")]
    TransferFailed(String),

    #[error("Invalid file path: {0}")]
    InvalidFilePath(String),

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("TLS error: {0}")]
    TlsError(String),

    #[error("Protocol error: {0}")]
    ProtocolError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("IO error: {0}")]
    IoError(String),

    #[error("Timeout")]
    Timeout,

    #[error("Invalid PIN")]
    InvalidPin,

    #[error("Internal error: {0}")]
    Internal(String),
}

impl serde::Serialize for LocalSendError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<std::io::Error> for LocalSendError {
    fn from(err: std::io::Error) -> Self {
        LocalSendError::IoError(err.to_string())
    }
}

impl From<serde_json::Error> for LocalSendError {
    fn from(err: serde_json::Error) -> Self {
        LocalSendError::SerializationError(err.to_string())
    }
}

impl From<reqwest::Error> for LocalSendError {
    fn from(err: reqwest::Error) -> Self {
        LocalSendError::NetworkError(err.to_string())
    }
}
