//! Protocol definitions for browser bridge communication

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Extension requested by an external client
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, rename_all = "camelCase")]
#[serde(rename_all = "camelCase")]
pub struct RequestedExtension {
    /// Extension name (e.g., "haex-pass")
    pub name: String,
    /// Extension's public key (hex string from manifest)
    /// Named differently from ClientInfo.public_key to avoid confusion
    pub extension_public_key: String,
}

/// Information about a connected client
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, rename_all = "camelCase")]
#[serde(rename_all = "camelCase")]
pub struct ClientInfo {
    /// Unique client identifier (public key fingerprint)
    pub client_id: String,
    /// Human-readable client name (e.g., "haex-pass Browser Extension")
    pub client_name: String,
    /// Client's public key for encryption (base64)
    pub public_key: String,
    /// Extensions the client wants to access
    /// If provided, matching extensions will be pre-selected in the authorization dialog
    #[serde(default)]
    pub requested_extensions: Vec<RequestedExtension>,
}

/// Request from browser extension to haex-vault
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeRequest {
    /// Request ID for correlation
    pub id: String,
    /// Target extension ID (e.g., "haex-pass")
    pub extension_id: String,
    /// Action to perform
    pub action: String,
    /// Action payload (extension-specific)
    #[serde(default)]
    pub payload: serde_json::Value,
}

/// Response from haex-vault to browser extension
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeResponse {
    /// Request ID for correlation
    pub id: String,
    /// Whether the request was successful
    pub success: bool,
    /// Response data (if successful)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    /// Error message (if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// Re-export EncryptedEnvelope from crypto module
pub use super::crypto::EncryptedEnvelope;

/// Initial handshake message from client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandshakeRequest {
    /// Protocol version
    pub version: u32,
    /// Client information
    pub client: ClientInfo,
}

/// Handshake response from server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandshakeResponse {
    /// Protocol version
    pub version: u32,
    /// Server's public key (base64)
    pub server_public_key: String,
    /// Whether client is authorized
    pub authorized: bool,
    /// If not authorized, authorization is pending user approval
    pub pending_approval: bool,
}

/// Protocol message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ProtocolMessage {
    /// Initial handshake
    Handshake(HandshakeRequest),
    /// Handshake response
    HandshakeResponse(HandshakeResponse),
    /// Encrypted request (after handshake)
    Request(EncryptedEnvelope),
    /// Encrypted response
    Response(EncryptedEnvelope),
    /// Authorization status update
    AuthorizationUpdate { authorized: bool },
    /// Ping/keepalive
    Ping,
    /// Pong response
    Pong,
    /// Error message
    Error { code: String, message: String },
}

impl BridgeResponse {
    pub fn success(id: String, data: serde_json::Value) -> Self {
        Self {
            id,
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(id: String, message: String) -> Self {
        Self {
            id,
            success: false,
            data: None,
            error: Some(message),
        }
    }
}
