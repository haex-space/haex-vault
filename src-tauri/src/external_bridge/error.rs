//! Error types for browser bridge

use thiserror::Error;
use tokio::sync::mpsc::error::SendError;
use tokio_tungstenite::tungstenite::Message;

#[derive(Error, Debug)]
pub enum BridgeError {
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Channel send error: {0}")]
    ChannelSend(#[from] SendError<Message>),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Client not authorized: {0}")]
    Unauthorized(String),

    #[error("Extension not found: {0}")]
    ExtensionNotFound(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Server not running")]
    NotRunning,

    #[error("Server already running")]
    AlreadyRunning,

    #[error("Authorization denied")]
    AuthorizationDenied,

    #[error("Request timeout")]
    Timeout,

    #[error("Crypto error: {0}")]
    Crypto(String),
}
