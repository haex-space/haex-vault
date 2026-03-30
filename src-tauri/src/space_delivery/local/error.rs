//! Error types for local space delivery.

use serde::Serialize;

#[derive(Debug, thiserror::Error, Serialize)]
pub enum DeliveryError {
    #[error("Not a leader for this space")]
    NotLeader,
    #[error("No leader found for space {space_id}")]
    NoLeader { space_id: String },
    #[error("Space not found: {space_id}")]
    SpaceNotFound { space_id: String },
    #[error("Access denied: {reason}")]
    AccessDenied { reason: String },
    #[error("Connection failed: {reason}")]
    ConnectionFailed { reason: String },
    #[error("Protocol error: {reason}")]
    ProtocolError { reason: String },
    #[error("Database error: {reason}")]
    Database { reason: String },
    #[error("MLS error: {reason}")]
    Mls { reason: String },
}
