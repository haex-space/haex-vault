//! The messages on the wire (spec 049, `contracts/wire-protocol.md`): the shapes haex-vault 2.x
//! (Legacy) sends and haex-pass-browser reads, as JSON text frames tagged by `type`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The protocol version holzi speaks; a handshake below it is refused (FR-049). Version 3 adds the
/// proof of the client's key (FR-048).
pub const PROTOCOL_VERSION: u32 = 3;

/// The target pair of the password manager (FR-029); anything else is not supported.
pub const CORE_EXTENSION_PUBLIC_KEY: &str = "__core__";
pub const CORE_EXTENSION_NAME: &str = "core";

/// A frame from the client.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Incoming {
    Handshake(Handshake),
    Proof(ProofEnvelope),
    Request(RequestEnvelope),
    Ping,
    Pong,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Handshake {
    pub version: u32,
    pub client: ClientInfo,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientInfo {
    pub client_id: String,
    #[serde(default)]
    pub client_name: Option<String>,
    pub public_key: String,
    #[serde(default)]
    pub requested_extensions: Option<Value>,
    #[serde(default)]
    pub permissions: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestEnvelope {
    pub action: String,
    pub message: String,
    pub iv: String,
    pub public_key: String,
    #[serde(default)]
    pub extension_public_key: Option<String>,
    #[serde(default)]
    pub extension_name: Option<String>,
}

/// The client's answer to the challenge, sealed to the bridge key like a request (FR-048).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProofEnvelope {
    pub public_key: String,
    pub iv: String,
    pub message: String,
}

impl RequestEnvelope {
    /// Whether the request names the password manager (FR-029).
    pub fn targets_core(&self) -> bool {
        self.extension_public_key.as_deref() == Some(CORE_EXTENSION_PUBLIC_KEY)
            && self.extension_name.as_deref() == Some(CORE_EXTENSION_NAME)
    }
}

/// A frame to the client.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Outgoing {
    HandshakeResponse(HandshakeResponse),
    Challenge(ChallengeFrame),
    Response(ResponseEnvelope),
    AuthorizationUpdate { authorized: bool },
    Pong,
    Error(ErrorFrame),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandshakeResponse {
    pub version: u32,
    pub server_public_key: String,
    pub authorized: bool,
    pub pending_approval: bool,
}

/// The challenge of protocol 3 (FR-048): random bytes sealed to the client's key like an answer,
/// with the bridge key the proof is sealed to.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChallengeFrame {
    pub server_public_key: String,
    pub public_key: String,
    pub iv: String,
    pub message: String,
}

/// The answer envelope; `clientId` is empty and the extension fields are `null`, as in the
/// template.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseEnvelope {
    pub action: String,
    pub message: String,
    pub iv: String,
    pub client_id: String,
    pub public_key: String,
    pub extension_public_key: Option<String>,
    pub extension_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ErrorFrame {
    pub code: ErrorCode,
    pub message: String,
}

impl ErrorFrame {
    pub fn new(code: ErrorCode) -> Self {
        Self {
            code,
            message: code.message().to_owned(),
        }
    }
}

/// Every code the bridge sends, on frames and in answers. The text is fixed: no service error,
/// address or content ever reaches a client (FR-039, research R16).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    ParseError,
    ProtocolUpdateRequired,
    PermissionsDeclarationRequired,
    ProofFailed,
    Unauthorized,
    DecryptionError,
    InvalidRequest,
    NotFound,
    PermissionDenied,
    PermissionPromptTimeout,
    OriginMismatch,
    PasskeyExcluded,
    UnsupportedAlgorithm,
    UserCancelled,
    NotSupported,
    ServerError,
}

impl ErrorCode {
    pub fn message(self) -> &'static str {
        match self {
            Self::ParseError => "Could not read the message",
            Self::ProtocolUpdateRequired => {
                "Protocol version 3 is required; update haex-pass to 1.4.0 or newer"
            }
            Self::PermissionsDeclarationRequired => {
                "A declaration of password permissions is required"
            }
            Self::ProofFailed => "The client did not prove that it holds its key",
            Self::Unauthorized => "Client not authorized",
            Self::DecryptionError => "Could not decrypt the request",
            Self::InvalidRequest => "Invalid request",
            Self::NotFound => "Not found",
            Self::PermissionDenied => "Permission denied",
            Self::PermissionPromptTimeout => "Permission prompt timed out",
            Self::OriginMismatch => "Origin mismatch",
            Self::PasskeyExcluded => "Passkey already registered",
            Self::UnsupportedAlgorithm => "Unsupported algorithm",
            // The extension compares the text itself (`webauthn-inject.ts`).
            Self::UserCancelled => "USER_CANCELLED",
            Self::NotSupported => "Not supported",
            Self::ServerError => "Internal error",
        }
    }
}

/// The plaintext of an answer: `{requestId, success, data}` or `{requestId, success, error,
/// errorCode}`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reply {
    pub request_id: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<ErrorCode>,
}

impl Reply {
    pub fn ok(request_id: String, data: Value) -> Self {
        Self {
            request_id,
            success: true,
            data: Some(data),
            error: None,
            error_code: None,
        }
    }

    pub fn fail(request_id: String, code: ErrorCode) -> Self {
        Self {
            request_id,
            success: false,
            data: None,
            error: Some(code.message()),
            error_code: Some(code),
        }
    }
}

/// Reads a frame; anything that is not one of the five known frames is a `PARSE_ERROR`.
pub fn parse(text: &str) -> Result<Incoming, ErrorCode> {
    serde_json::from_str(text).map_err(|_| ErrorCode::ParseError)
}

/// Writes a frame as JSON text.
pub fn render(frame: &Outgoing) -> String {
    // Serializing these plain structs cannot fail: no maps with non-string keys, no custom
    // serializers.
    serde_json::to_string(frame).unwrap_or_default()
}
