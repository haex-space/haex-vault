use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value as JsonValue;

use crate::database::DbConnection;

use super::error::SyncEngineError;

/// Get the current Unix timestamp in seconds.
fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ---------------------------------------------------------------------------
// Sync-state DB types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SyncStateEntry {
    pub id: String,
    pub rule_id: String,
    pub relative_path: String,
    pub file_size: u64,
    pub modified_at: u64,
    pub synced_at: String,
    pub deleted: bool,
    /// SHA-256 of the file content as advertised by the sender at the time
    /// this row was last upserted. Lets the next manifest comparison reuse
    /// the sender's hash instead of re-hashing locally — without it, the
    /// receiver's mtime drift after `tokio::fs::copy` would force the diff
    /// engine to fall back to the size+mtime heuristic and re-fire transfers.
    pub hash: Option<String>,
    /// Opaque object key on the cloud backend (Phase 4, Round C). Present
    /// only for encrypted-cloud sync rules; `None` for peer / local sync
    /// and for cloud rules that have not yet run through the encrypting
    /// provider's write path or a `bootstrap_object_key_cache` pass.
    pub object_key: Option<String>,
}

// ---------------------------------------------------------------------------
// Sync-state DB operations
// ---------------------------------------------------------------------------

/// Load all sync state entries for a rule.
pub fn load_sync_state(
    db: &DbConnection,
    rule_id: &str,
) -> Result<Vec<SyncStateEntry>, SyncEngineError> {
    let sql = "SELECT id, rule_id, relative_path, file_size, modified_at, synced_at, deleted, hash, object_key FROM haex_sync_state_no_sync WHERE rule_id = ?1".to_string();
    let params = vec![JsonValue::String(rule_id.to_string())];

    let rows = crate::database::core::select(sql, params, db)
        .map_err(|e| SyncEngineError::Database(e.to_string()))?;

    let entries = rows
        .iter()
        .map(|row| SyncStateEntry {
            id: row
                .first()
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            rule_id: row
                .get(1)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            relative_path: row
                .get(2)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            file_size: row.get(3).and_then(|v| v.as_u64()).unwrap_or(0),
            modified_at: row.get(4).and_then(|v| v.as_u64()).unwrap_or(0),
            synced_at: row
                .get(5)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            deleted: row
                .get(6)
                .and_then(|v| v.as_i64())
                .map(|v| v != 0)
                .unwrap_or(false),
            hash: row.get(7).and_then(|v| v.as_str()).map(|s| s.to_string()),
            object_key: row.get(8).and_then(|v| v.as_str()).map(|s| s.to_string()),
        })
        .collect();

    Ok(entries)
}

/// Insert or update a sync state entry after a file is synced.
///
/// Uses INSERT OR REPLACE on the unique `(rule_id, relative_path)` index.
/// `hash` is the sender's SHA-256 — pass `None` only when the source did not
/// provide one (legacy peer or hashing disabled).
///
/// **`object_key` preservation (Phase 4, Round D).** `object_key` is
/// populated by the encrypting cloud provider decorator when a file is
/// first written or by `bootstrap_object_key_cache` when a fresh device
/// recovers the local `relative_path -> object_key` map from bucket
/// sidecars. `INSERT OR REPLACE` deletes and reinserts the whole row on
/// conflict, so an ordinary upsert here would silently null out
/// `object_key` and force the next bootstrap to re-download every sidecar.
/// The subquery in the VALUES list reads the pre-conflict row's
/// `object_key` (SQLite evaluates VALUES *before* firing the
/// delete-then-insert of REPLACE), so callers that do not know the
/// `object_key` — every non-encrypting caller — leave the cached value
/// untouched.
pub fn upsert_sync_state(
    db: &DbConnection,
    rule_id: &str,
    relative_path: &str,
    file_size: u64,
    modified_at: u64,
    hash: Option<&str>,
) -> Result<(), SyncEngineError> {
    let now = unix_now().to_string();
    let id = uuid::Uuid::new_v4().to_string();

    let sql = "INSERT OR REPLACE INTO haex_sync_state_no_sync (id, rule_id, relative_path, file_size, modified_at, synced_at, deleted, hash, object_key) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, (SELECT object_key FROM haex_sync_state_no_sync WHERE rule_id = ?2 AND relative_path = ?3))".to_string();
    let params = vec![
        JsonValue::String(id),
        JsonValue::String(rule_id.to_string()),
        JsonValue::String(relative_path.to_string()),
        JsonValue::Number(serde_json::Number::from(file_size)),
        JsonValue::Number(serde_json::Number::from(modified_at)),
        JsonValue::String(now),
        match hash {
            Some(h) => JsonValue::String(h.to_string()),
            None => JsonValue::Null,
        },
    ];

    crate::database::core::execute(sql, params, db)
        .map_err(|e| SyncEngineError::Database(e.to_string()))?;

    Ok(())
}

/// Mark a file as deleted in sync state.
pub fn mark_deleted(
    db: &DbConnection,
    rule_id: &str,
    relative_path: &str,
) -> Result<(), SyncEngineError> {
    let now = unix_now().to_string();

    let sql = "UPDATE haex_sync_state_no_sync SET deleted = 1, synced_at = ?1 WHERE rule_id = ?2 AND relative_path = ?3".to_string();
    let params = vec![
        JsonValue::String(now),
        JsonValue::String(rule_id.to_string()),
        JsonValue::String(relative_path.to_string()),
    ];

    crate::database::core::execute(sql, params, db)
        .map_err(|e| SyncEngineError::Database(e.to_string()))?;

    Ok(())
}

/// Clear all sync state for a rule (when the rule is deleted).
pub fn clear_sync_state(db: &DbConnection, rule_id: &str) -> Result<(), SyncEngineError> {
    let sql = "DELETE FROM haex_sync_state_no_sync WHERE rule_id = ?1".to_string();
    let params = vec![JsonValue::String(rule_id.to_string())];

    crate::database::core::execute(sql, params, db)
        .map_err(|e| SyncEngineError::Database(e.to_string()))?;

    Ok(())
}
