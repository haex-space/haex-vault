//! Authorization management for external clients
//!
//! Uses the haex_external_authorized_clients and haex_external_blocked_clients tables
//! managed via Drizzle migrations.
//! All SQL operations use CRDT-compatible execution via the core database functions.
//! The CRDT functions automatically handle tombstone filtering.

use crate::table_names::{
    // Authorized clients table and columns
    COL_EXTERNAL_AUTHORIZED_CLIENTS_AUTHORIZED_AT, COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_ID,
    COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_NAME, COL_EXTERNAL_AUTHORIZED_CLIENTS_EXTENSION_ID,
    COL_EXTERNAL_AUTHORIZED_CLIENTS_ID, COL_EXTERNAL_AUTHORIZED_CLIENTS_LAST_SEEN,
    COL_EXTERNAL_AUTHORIZED_CLIENTS_PUBLIC_KEY, TABLE_EXTERNAL_AUTHORIZED_CLIENTS,
    // Blocked clients table and columns
    COL_EXTERNAL_BLOCKED_CLIENTS_BLOCKED_AT, COL_EXTERNAL_BLOCKED_CLIENTS_CLIENT_ID,
    COL_EXTERNAL_BLOCKED_CLIENTS_CLIENT_NAME, COL_EXTERNAL_BLOCKED_CLIENTS_ID,
    COL_EXTERNAL_BLOCKED_CLIENTS_PUBLIC_KEY, TABLE_EXTERNAL_BLOCKED_CLIENTS,
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
    /// Requested extension ID
    pub extension_id: String,
}

// ============================================================================
// SQL queries for authorized clients
// Note: No haex_tombstone filter needed - select_with_crdt handles this automatically
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
         {COL_EXTERNAL_AUTHORIZED_CLIENTS_LAST_SEEN}
         FROM {TABLE_EXTERNAL_AUTHORIZED_CLIENTS}
         WHERE {COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_ID} = ?1"
    );

    pub static ref SQL_GET_ALL_CLIENTS: String = format!(
        "SELECT {COL_EXTERNAL_AUTHORIZED_CLIENTS_ID}, {COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_ID}, \
         {COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_NAME}, {COL_EXTERNAL_AUTHORIZED_CLIENTS_PUBLIC_KEY}, \
         {COL_EXTERNAL_AUTHORIZED_CLIENTS_EXTENSION_ID}, {COL_EXTERNAL_AUTHORIZED_CLIENTS_AUTHORIZED_AT}, \
         {COL_EXTERNAL_AUTHORIZED_CLIENTS_LAST_SEEN}
         FROM {TABLE_EXTERNAL_AUTHORIZED_CLIENTS}
         ORDER BY {COL_EXTERNAL_AUTHORIZED_CLIENTS_AUTHORIZED_AT} DESC"
    );

    pub static ref SQL_INSERT_CLIENT: String = format!(
        "INSERT INTO {TABLE_EXTERNAL_AUTHORIZED_CLIENTS} \
         ({COL_EXTERNAL_AUTHORIZED_CLIENTS_ID}, {COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_ID}, \
          {COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_NAME}, {COL_EXTERNAL_AUTHORIZED_CLIENTS_PUBLIC_KEY}, \
          {COL_EXTERNAL_AUTHORIZED_CLIENTS_EXTENSION_ID})
         VALUES (?1, ?2, ?3, ?4, ?5)"
    );

    pub static ref SQL_UPDATE_LAST_SEEN: String = format!(
        "UPDATE {TABLE_EXTERNAL_AUTHORIZED_CLIENTS}
         SET {COL_EXTERNAL_AUTHORIZED_CLIENTS_LAST_SEEN} = datetime('now')
         WHERE {COL_EXTERNAL_AUTHORIZED_CLIENTS_CLIENT_ID} = ?1"
    );

    // Note: For CRDT, deletion is done via UPDATE to set haex_tombstone = 1
    // This is handled by sql_execute_with_crdt when using DELETE syntax
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
}

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
