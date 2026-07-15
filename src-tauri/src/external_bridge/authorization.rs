//! Authorization management for external clients
//!
//! Uses the haex_external_authorized_clients and haex_external_blocked_clients tables
//! managed via Drizzle migrations.
//! All SQL operations use CRDT-compatible execution via the core database functions.
//! The CRDT functions automatically handle tombstone filtering.

use crate::table_names::{
    // Authorized clients table and columns
    COL_EXTERNAL_AUTHORIZED_CLIENTS_AUTHORIZED_AT,
    COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_ID,
    COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_NAME,
    COL_EXTERNAL_AUTHORIZED_CLIENTS_EXTENSION_ID,
    COL_EXTERNAL_AUTHORIZED_CLIENTS_ID,
    COL_EXTERNAL_AUTHORIZED_CLIENTS_LAST_SEEN,
    COL_EXTERNAL_AUTHORIZED_CLIENTS_PUBLIC_KEY,
    COL_EXTERNAL_AUTHORIZED_CLIENTS_REQUESTED_PERMISSIONS,
    // Blocked clients table and columns
    COL_EXTERNAL_BLOCKED_CLIENTS_BLOCKED_AT,
    COL_EXTERNAL_BLOCKED_CLIENTS_CLIENT_ID,
    COL_EXTERNAL_BLOCKED_CLIENTS_CLIENT_NAME,
    COL_EXTERNAL_BLOCKED_CLIENTS_ID,
    COL_EXTERNAL_BLOCKED_CLIENTS_PUBLIC_KEY,
    // Extensions table
    TABLE_EXTENSIONS,
    TABLE_EXTERNAL_AUTHORIZED_CLIENTS,
    TABLE_EXTERNAL_BLOCKED_CLIENTS,
};
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
    /// Canonical JSON of the client's declared manifest at authorization time
    /// (`ClientInfo.permissions` + `requestedExtensions[].actions`). Compared
    /// against the live handshake declaration to detect manifest changes.
    pub requested_permissions: String,
}

/// A blocked client stored in the database
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, rename_all = "camelCase")]
#[serde(rename_all = "camelCase")]
pub struct BlockedClient {
    /// Row ID
    pub id: String,
    /// Unique client identifier (public key fingerprint)
    pub client_id: String,
    /// Human-readable client name
    pub client_name: String,
    /// Client's public key (base64)
    pub public_key: String,
    /// When the client was blocked (ISO 8601)
    pub blocked_at: Option<String>,
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
    /// Extensions the client wants to access
    /// These should be pre-selected in the authorization dialog (matched by name + extensionPublicKey)
    #[serde(default)]
    pub requested_extensions: Vec<super::protocol::RequestedExtension>,
    /// Declared core permissions from the handshake (protocol v2+), shown in
    /// the authorization dialog alongside `requested_extensions`.
    #[serde(default)]
    pub permissions: Option<super::protocol::ClientPermissions>,
}

// ============================================================================
// SQL queries for authorized clients
// ============================================================================

lazy_static::lazy_static! {
    pub static ref SQL_IS_AUTHORIZED: String = format!(
        "SELECT COUNT(*) FROM {TABLE_EXTERNAL_AUTHORIZED_CLIENTS}
         WHERE {COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_ID} = ?1 AND {COL_EXTERNAL_AUTHORIZED_CLIENTS_EXTENSION_ID} = ?2"
    );

    pub static ref SQL_IS_CLIENT_KNOWN: String = format!(
        "SELECT COUNT(*) FROM {TABLE_EXTERNAL_AUTHORIZED_CLIENTS}
         WHERE {COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_ID} = ?1"
    );

    pub static ref SQL_GET_CLIENT_EXTENSION: String = format!(
        "SELECT {COL_EXTERNAL_AUTHORIZED_CLIENTS_EXTENSION_ID} FROM {TABLE_EXTERNAL_AUTHORIZED_CLIENTS}
         WHERE {COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_ID} = ?1"
    );

    pub static ref SQL_GET_CLIENT: String = format!(
        "SELECT {COL_EXTERNAL_AUTHORIZED_CLIENTS_ID}, {COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_ID}, \
         {COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_NAME}, {COL_EXTERNAL_AUTHORIZED_CLIENTS_PUBLIC_KEY}, \
         {COL_EXTERNAL_AUTHORIZED_CLIENTS_EXTENSION_ID}, {COL_EXTERNAL_AUTHORIZED_CLIENTS_AUTHORIZED_AT}, \
         {COL_EXTERNAL_AUTHORIZED_CLIENTS_LAST_SEEN}, {COL_EXTERNAL_AUTHORIZED_CLIENTS_REQUESTED_PERMISSIONS}
         FROM {TABLE_EXTERNAL_AUTHORIZED_CLIENTS}
         WHERE {COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_ID} = ?1"
    );

    pub static ref SQL_GET_ALL_CLIENTS: String = format!(
        "SELECT {COL_EXTERNAL_AUTHORIZED_CLIENTS_ID}, {COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_ID}, \
         {COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_NAME}, {COL_EXTERNAL_AUTHORIZED_CLIENTS_PUBLIC_KEY}, \
         {COL_EXTERNAL_AUTHORIZED_CLIENTS_EXTENSION_ID}, {COL_EXTERNAL_AUTHORIZED_CLIENTS_AUTHORIZED_AT}, \
         {COL_EXTERNAL_AUTHORIZED_CLIENTS_LAST_SEEN}, {COL_EXTERNAL_AUTHORIZED_CLIENTS_REQUESTED_PERMISSIONS}
         FROM {TABLE_EXTERNAL_AUTHORIZED_CLIENTS}
         ORDER BY {COL_EXTERNAL_AUTHORIZED_CLIENTS_AUTHORIZED_AT} DESC"
    );

    pub static ref SQL_INSERT_CLIENT: String = format!(
        "INSERT INTO {TABLE_EXTERNAL_AUTHORIZED_CLIENTS} \
         ({COL_EXTERNAL_AUTHORIZED_CLIENTS_ID}, {COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_ID}, \
          {COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_NAME}, {COL_EXTERNAL_AUTHORIZED_CLIENTS_PUBLIC_KEY}, \
          {COL_EXTERNAL_AUTHORIZED_CLIENTS_EXTENSION_ID}, {COL_EXTERNAL_AUTHORIZED_CLIENTS_REQUESTED_PERMISSIONS})
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
    );

    // Upsert support for re-grants: a client that re-authorizes (e.g. after a
    // manifest change) already has a (client_id, extension_id) row — a plain
    // INSERT would violate the unique index, so the grant path looks the row
    // up and UPDATEs it instead.
    pub static ref SQL_GET_CLIENT_EXTENSION_ROW_ID: String = format!(
        "SELECT {COL_EXTERNAL_AUTHORIZED_CLIENTS_ID} FROM {TABLE_EXTERNAL_AUTHORIZED_CLIENTS} \
         WHERE {COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_ID} = ?1 \
         AND {COL_EXTERNAL_AUTHORIZED_CLIENTS_EXTENSION_ID} = ?2 LIMIT 1"
    );

    pub static ref SQL_UPDATE_CLIENT_GRANT: String = format!(
        "UPDATE {TABLE_EXTERNAL_AUTHORIZED_CLIENTS} \
         SET {COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_NAME} = ?2, \
             {COL_EXTERNAL_AUTHORIZED_CLIENTS_PUBLIC_KEY} = ?3 \
         WHERE {COL_EXTERNAL_AUTHORIZED_CLIENTS_ID} = ?1"
    );

    // Keeps the stored manifest identical across ALL of a client's rows
    // (`get_stored_requested_permissions` reads an arbitrary one) — rows
    // granted for other extensions in an earlier session would otherwise
    // retain an outdated declaration and flap between authorized and
    // re-authorization-required depending on which row the lookup hits.
    pub static ref SQL_UPDATE_CLIENT_REQUESTED_PERMISSIONS: String = format!(
        "UPDATE {TABLE_EXTERNAL_AUTHORIZED_CLIENTS} \
         SET {COL_EXTERNAL_AUTHORIZED_CLIENTS_REQUESTED_PERMISSIONS} = ?1 \
         WHERE {COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_ID} = ?2"
    );

    pub static ref SQL_UPDATE_LAST_SEEN: String = format!(
        "UPDATE {TABLE_EXTERNAL_AUTHORIZED_CLIENTS}
         SET {COL_EXTERNAL_AUTHORIZED_CLIENTS_LAST_SEEN} = datetime('now')
         WHERE {COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_ID} = ?1"
    );

    // DELETE goes through sql_execute_with_crdt; the BEFORE-DELETE trigger logs
    // the row into haex_deleted_rows so remotes learn about it.
    pub static ref SQL_DELETE_CLIENT: String = format!(
        "DELETE FROM {TABLE_EXTERNAL_AUTHORIZED_CLIENTS}
         WHERE {COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_ID} = ?1"
    );

    // ============================================================================
    // SQL queries for blocked clients
    // ============================================================================

    pub static ref SQL_IS_BLOCKED: String = format!(
        "SELECT COUNT(*) FROM {TABLE_EXTERNAL_BLOCKED_CLIENTS}
         WHERE {COL_EXTERNAL_BLOCKED_CLIENTS_CLIENT_ID} = ?1"
    );

    pub static ref SQL_GET_BLOCKED_CLIENT: String = format!(
        "SELECT {COL_EXTERNAL_BLOCKED_CLIENTS_ID}, {COL_EXTERNAL_BLOCKED_CLIENTS_CLIENT_ID}, \
         {COL_EXTERNAL_BLOCKED_CLIENTS_CLIENT_NAME}, {COL_EXTERNAL_BLOCKED_CLIENTS_PUBLIC_KEY}, \
         {COL_EXTERNAL_BLOCKED_CLIENTS_BLOCKED_AT}
         FROM {TABLE_EXTERNAL_BLOCKED_CLIENTS}
         WHERE {COL_EXTERNAL_BLOCKED_CLIENTS_CLIENT_ID} = ?1"
    );

    pub static ref SQL_GET_ALL_BLOCKED_CLIENTS: String = format!(
        "SELECT {COL_EXTERNAL_BLOCKED_CLIENTS_ID}, {COL_EXTERNAL_BLOCKED_CLIENTS_CLIENT_ID}, \
         {COL_EXTERNAL_BLOCKED_CLIENTS_CLIENT_NAME}, {COL_EXTERNAL_BLOCKED_CLIENTS_PUBLIC_KEY}, \
         {COL_EXTERNAL_BLOCKED_CLIENTS_BLOCKED_AT}
         FROM {TABLE_EXTERNAL_BLOCKED_CLIENTS}
         ORDER BY {COL_EXTERNAL_BLOCKED_CLIENTS_BLOCKED_AT} DESC"
    );

    pub static ref SQL_INSERT_BLOCKED_CLIENT: String = format!(
        "INSERT INTO {TABLE_EXTERNAL_BLOCKED_CLIENTS} \
         ({COL_EXTERNAL_BLOCKED_CLIENTS_ID}, {COL_EXTERNAL_BLOCKED_CLIENTS_CLIENT_ID}, \
          {COL_EXTERNAL_BLOCKED_CLIENTS_CLIENT_NAME}, {COL_EXTERNAL_BLOCKED_CLIENTS_PUBLIC_KEY})
         VALUES (?1, ?2, ?3, ?4)"
    );

    pub static ref SQL_DELETE_BLOCKED_CLIENT: String = format!(
        "DELETE FROM {TABLE_EXTERNAL_BLOCKED_CLIENTS}
         WHERE {COL_EXTERNAL_BLOCKED_CLIENTS_CLIENT_ID} = ?1"
    );

    // ============================================================================
    // SQL queries for extension lookup and authorization validation
    // ============================================================================

    /// Check if a client is authorized for a specific extension (by extension public_key + name)
    /// Uses JOIN to lookup extension by public_key and name
    pub static ref SQL_IS_CLIENT_AUTHORIZED_FOR_EXTENSION: String = format!(
        "SELECT COUNT(*) FROM {TABLE_EXTERNAL_AUTHORIZED_CLIENTS} ac
         JOIN {TABLE_EXTENSIONS} e ON ac.{COL_EXTERNAL_AUTHORIZED_CLIENTS_EXTENSION_ID} = e.id
         WHERE ac.{COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_ID} = ?1
         AND e.public_key = ?2
         AND e.name = ?3"
    );

    /// Get extension ID by public_key and name
    pub static ref SQL_GET_EXTENSION_ID_BY_PUBLIC_KEY_AND_NAME: String = format!(
        "SELECT id FROM {TABLE_EXTENSIONS}
         WHERE public_key = ?1 AND name = ?2"
    );
}

/// Helper to parse authorized client from query result row
pub fn parse_authorized_client(row: &[serde_json::Value]) -> Option<AuthorizedClient> {
    if row.len() < 8 {
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
        requested_permissions: row[7].as_str().unwrap_or_default().to_string(),
    })
}

/// Helper to parse blocked client from query result row
pub fn parse_blocked_client(row: &[serde_json::Value]) -> Option<BlockedClient> {
    if row.len() < 5 {
        return None;
    }

    Some(BlockedClient {
        id: row[0].as_str()?.to_string(),
        client_id: row[1].as_str()?.to_string(),
        client_name: row[2].as_str()?.to_string(),
        public_key: row[3].as_str()?.to_string(),
        blocked_at: row[4].as_str().map(|s| s.to_string()),
    })
}
