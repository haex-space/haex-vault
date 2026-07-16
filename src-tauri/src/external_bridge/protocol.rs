//! Protocol definitions for browser bridge communication

use crate::extension::core::manifest::ExtensionPermissions;
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
    /// Declared action names the client wants to call on this extension
    /// (e.g. `["getItems", "createItem"]`), or `["*"]` for all actions.
    /// Checked against `ResourceType::ExtensionApi` permission rows with
    /// target `"{extension_public_key}::{name}::{action}"`.
    #[serde(default)]
    pub actions: Vec<String>,
}

/// Declared core (haex-vault built-in) permissions a client wants at
/// handshake time. Reuses the extension manifest's `ExtensionPermissions`
/// shape — today only `passwords` is relevant for external clients.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, rename_all = "camelCase")]
#[serde(rename_all = "camelCase")]
pub struct ClientPermissions {
    pub core: ExtensionPermissions,
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
    /// Declared core permissions (protocol v2+). `None` is treated the same
    /// as an all-`Ask` empty declaration — every core action prompts on
    /// first use rather than being silently denied.
    #[serde(default)]
    pub permissions: Option<ClientPermissions>,
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

/// `true` iff the client declared at least one permission (core or
/// per-extension action) — i.e. the handshake carries a real manifest rather
/// than being silently omitted. An extension entry with an empty `actions`
/// list declares nothing for that extension; it does not by itself count.
pub fn has_permissions_declaration(client: &ClientInfo) -> bool {
    client.permissions.is_some()
        || client
            .requested_extensions
            .iter()
            .any(|e| !e.actions.is_empty())
}

/// Serializes a client's declared manifest (core permissions + per-extension
/// declared actions) into a canonical JSON string used for two purposes:
/// (1) persisted as `requested_permissions` on grant, (2) recomputed on every
/// handshake and compared against the persisted value to detect a manifest
/// change (which forces re-authorization — see `connection::handle_connection`).
pub fn canonical_requested_permissions(
    permissions: &Option<ClientPermissions>,
    requested_extensions: &[RequestedExtension],
) -> String {
    #[derive(Serialize)]
    struct CanonicalManifest<'a> {
        permissions: &'a Option<ClientPermissions>,
        requested_extensions: &'a [RequestedExtension],
    }
    let mut value = match serde_json::to_value(CanonicalManifest {
        permissions,
        requested_extensions,
    }) {
        Ok(value) => value,
        Err(_) => return String::new(),
    };
    sort_arrays_recursively(&mut value);
    value.to_string()
}

/// Sorts every JSON array in `value` (recursively, children first) by the
/// elements' serialized form. A client that emits its declaration arrays
/// (`requested_extensions`, per-extension `actions`, core permission entries)
/// in a different order across sessions still produces the same canonical
/// string — array order carries no meaning in the declaration, and an
/// order-only difference must not force re-authorization. Object key order is
/// already deterministic (struct field order via serde).
fn sort_arrays_recursively(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(items) => {
            for item in items.iter_mut() {
                sort_arrays_recursively(item);
            }
            items.sort_by_cached_key(|item| item.to_string());
        }
        serde_json::Value::Object(map) => {
            for (_, item) in map.iter_mut() {
                sort_arrays_recursively(item);
            }
        }
        _ => {}
    }
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

#[cfg(test)]
mod declaration_tests {
    use super::*;

    fn client(
        permissions: Option<ClientPermissions>,
        requested_extensions: Vec<RequestedExtension>,
    ) -> ClientInfo {
        ClientInfo {
            client_id: "c1".to_string(),
            client_name: "Test Client".to_string(),
            public_key: "pk".to_string(),
            requested_extensions,
            permissions,
        }
    }

    fn requested_extension(actions: Vec<&str>) -> RequestedExtension {
        RequestedExtension {
            name: "haex-notes".to_string(),
            extension_public_key: "pk1".to_string(),
            actions: actions.into_iter().map(String::from).collect(),
        }
    }

    #[test]
    fn no_permissions_and_no_actions_is_not_a_declaration() {
        let c = client(None, vec![requested_extension(vec![])]);
        assert!(!has_permissions_declaration(&c));
    }

    #[test]
    fn no_permissions_and_no_requested_extensions_is_not_a_declaration() {
        let c = client(None, vec![]);
        assert!(!has_permissions_declaration(&c));
    }

    #[test]
    fn core_permissions_present_counts_as_a_declaration() {
        let c = client(
            Some(ClientPermissions {
                core: Default::default(),
            }),
            vec![],
        );
        assert!(has_permissions_declaration(&c));
    }

    #[test]
    fn a_single_declared_action_counts_as_a_declaration() {
        let c = client(None, vec![requested_extension(vec!["getItems"])]);
        assert!(has_permissions_declaration(&c));
    }

    #[test]
    fn canonical_manifest_changes_when_actions_change() {
        let a = canonical_requested_permissions(&None, &[requested_extension(vec!["getItems"])]);
        let b = canonical_requested_permissions(
            &None,
            &[requested_extension(vec!["getItems", "createItem"])],
        );
        assert_ne!(a, b);
    }

    #[test]
    fn canonical_manifest_is_stable_for_identical_input() {
        let a = canonical_requested_permissions(&None, &[requested_extension(vec!["getItems"])]);
        let b = canonical_requested_permissions(&None, &[requested_extension(vec!["getItems"])]);
        assert_eq!(a, b);
    }

    #[test]
    fn canonical_manifest_is_invariant_to_array_order() {
        let other = RequestedExtension {
            name: "haex-pass".to_string(),
            extension_public_key: "pk2".to_string(),
            actions: vec!["a".to_string(), "b".to_string()],
        };
        let a = canonical_requested_permissions(
            &None,
            &[
                requested_extension(vec!["getItems", "createItem"]),
                other.clone(),
            ],
        );
        let b = canonical_requested_permissions(
            &None,
            &[other, requested_extension(vec!["createItem", "getItems"])],
        );
        assert_eq!(a, b);
    }
}

#[allow(dead_code)]
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
