//! Authorization management for browser bridge clients
//!
//! Uses the haex_bridge_authorized_clients table managed via Drizzle migrations.
//! All SQL operations use CRDT-compatible execution via the core database functions.
//! The CRDT functions automatically handle tombstone filtering.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// An authorized client stored in the database
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, rename_all = "camelCase")]
#[serde(rename_all = "camelCase")]
pub struct AuthorizedClient {
    /// Row ID
    pub id: String,
    /// Unique client identifier (public key fingerprint)
    pub client_id: String,
    /// Human-readable client name
    pub client_name: String,
    /// Client's public key (base64)
    pub public_key: String,
    /// Extension ID this client can access
    pub extension_id: String,
    /// When the client was authorized (ISO 8601)
    pub authorized_at: Option<String>,
    /// Last time the client connected (ISO 8601)
    pub last_seen: Option<String>,
}

/// Pending authorization request waiting for user approval
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, rename_all = "camelCase")]
#[serde(rename_all = "camelCase")]
pub struct PendingAuthorization {
    /// Unique client identifier
    pub client_id: String,
    /// Human-readable client name
    pub client_name: String,
    /// Client's public key (base64)
    pub public_key: String,
    /// Requested extension ID
    pub extension_id: String,
}

// SQL queries as constants for use with CRDT database functions
// Note: No haex_tombstone filter needed - select_with_crdt handles this automatically

pub const SQL_IS_AUTHORIZED: &str =
    "SELECT COUNT(*) FROM haex_bridge_authorized_clients
     WHERE client_id = ?1 AND extension_id = ?2";

pub const SQL_IS_CLIENT_KNOWN: &str =
    "SELECT COUNT(*) FROM haex_bridge_authorized_clients
     WHERE client_id = ?1";

pub const SQL_GET_CLIENT_EXTENSION: &str =
    "SELECT extension_id FROM haex_bridge_authorized_clients
     WHERE client_id = ?1";

pub const SQL_GET_CLIENT: &str =
    "SELECT id, client_id, client_name, public_key, extension_id, authorized_at, last_seen
     FROM haex_bridge_authorized_clients
     WHERE client_id = ?1";

pub const SQL_GET_ALL_CLIENTS: &str =
    "SELECT id, client_id, client_name, public_key, extension_id, authorized_at, last_seen
     FROM haex_bridge_authorized_clients
     ORDER BY authorized_at DESC";

pub const SQL_INSERT_CLIENT: &str =
    "INSERT INTO haex_bridge_authorized_clients (id, client_id, client_name, public_key, extension_id)
     VALUES (?1, ?2, ?3, ?4, ?5)";

pub const SQL_UPDATE_LAST_SEEN: &str =
    "UPDATE haex_bridge_authorized_clients
     SET last_seen = datetime('now')
     WHERE client_id = ?1";

// Note: For CRDT, deletion is done via UPDATE to set haex_tombstone = 1
// This is handled by sql_execute_with_crdt when using DELETE syntax
pub const SQL_DELETE_CLIENT: &str =
    "DELETE FROM haex_bridge_authorized_clients
     WHERE client_id = ?1";

/// Helper to parse authorized client from query result row
pub fn parse_authorized_client(row: &[serde_json::Value]) -> Option<AuthorizedClient> {
    if row.len() < 7 {
        return None;
    }

    Some(AuthorizedClient {
        id: row[0].as_str()?.to_string(),
        client_id: row[1].as_str()?.to_string(),
        client_name: row[2].as_str()?.to_string(),
        public_key: row[3].as_str()?.to_string(),
        extension_id: row[4].as_str()?.to_string(),
        authorized_at: row[5].as_str().map(|s| s.to_string()),
        last_seen: row[6].as_str().map(|s| s.to_string()),
    })
}
