//! Error types for local space delivery.

use serde::Serialize;

#[derive(Debug, thiserror::Error, Serialize)]
pub enum DeliveryError {
    #[error("Access denied: {reason}")]
    AccessDenied { reason: String },
    #[error("Connection failed: {reason}")]
    ConnectionFailed { reason: String },
    #[error("Protocol error: {reason}")]
    ProtocolError { reason: String },
    #[error("Database error: {reason}")]
    Database { reason: String },
}
