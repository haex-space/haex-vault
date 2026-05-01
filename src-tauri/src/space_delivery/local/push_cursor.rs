//! Per-space, per-device CRDT push cursor for local space delivery.
//!
//! Persisted in `haex_vault_settings` under
//! `local_sync_push_hlc:<space_id>` (one row per `device_id` via the
//! `(key, device_id)` unique index). The cursor is the max HLC string of
//! the last successfully pushed chunk; the next sync-loop session resumes
//! from there instead of re-scanning every space-scoped row from t=0.
//!
//! Without this cursor, every reconnect re-scans the whole local DB and
//! tries to push every row again — including rows that were just pulled
//! from the leader and would now be incorrectly attributed to us, hitting
//! the "need Write, have Read" capability check and looping forever.

use crate::database::constants::vault_settings_key::{
    LOCAL_SYNC_MLS_CURSOR_PREFIX, LOCAL_SYNC_PUSH_HLC_PREFIX,
};
use crate::database::core::with_connection;
use crate::database::error::DatabaseError;
use crate::database::DbConnection;

fn cursor_key(space_id: &str) -> String {
    format!("{LOCAL_SYNC_PUSH_HLC_PREFIX}{space_id}")
}

fn mls_cursor_key(space_id: &str) -> String {
    format!("{LOCAL_SYNC_MLS_CURSOR_PREFIX}{space_id}")
}

/// Read the persisted push cursor for `(space_id, device_id)`. Returns
/// `None` on first run, on missing row, or on any DB error — the caller
/// then scans from t=0, which is correct (just slower) on first run.
pub fn load_last_push_hlc(
    db: &DbConnection,
    space_id: &str,
    device_id: &str,
) -> Option<String> {
    let key = cursor_key(space_id);
    with_connection(db, |conn| {
        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM haex_vault_settings \
                 WHERE key = ?1 AND device_id = ?2",
                rusqlite::params![key, device_id],
                |row| row.get(0),
            )
            .ok()
            .flatten();
        Ok::<_, DatabaseError>(value)
    })
    .ok()
    .flatten()
}

/// Persist the push cursor for `(space_id, device_id)`. Idempotent via
/// `ON CONFLICT (key, device_id) DO UPDATE`. Errors are logged and
/// swallowed — a failed checkpoint just means the next session re-scans
/// from the previous cursor (or t=0), never a data-loss risk.
pub fn save_last_push_hlc(
    db: &DbConnection,
    space_id: &str,
    device_id: &str,
    hlc: &str,
) {
    let key = cursor_key(space_id);
    let row_id = uuid::Uuid::new_v4().to_string();

    let result: Result<(), DatabaseError> = with_connection(db, |conn| {
        conn.execute(
            "INSERT INTO haex_vault_settings (id, key, value, device_id) \
             VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(key, device_id) DO UPDATE SET value = excluded.value",
            rusqlite::params![row_id, key, hlc, device_id],
        )
        .map(|_| ())
        .map_err(DatabaseError::from)
    });

    if let Err(e) = result {
        eprintln!(
            "[SyncLoop] Failed to save push cursor for space={space_id} \
             device={device_id}: {e}"
        );
    }
}

/// Read the persisted MLS message cursor for `(space_id, device_id)`.
/// Returns `None` on first run or any DB error — the caller then fetches
/// from id=0, which is safe (just fetches historical messages that will
/// be skipped or processed from scratch).
pub fn load_last_mls_cursor(
    db: &DbConnection,
    space_id: &str,
    device_id: &str,
) -> Option<i64> {
    let key = mls_cursor_key(space_id);
    with_connection(db, |conn| {
        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM haex_vault_settings \
                 WHERE key = ?1 AND device_id = ?2",
                rusqlite::params![key, device_id],
                |row| row.get(0),
            )
            .ok()
            .flatten();
        Ok::<_, DatabaseError>(value)
    })
    .ok()
    .flatten()
    .and_then(|s| {
        s.parse::<i64>().map_err(|e| {
            eprintln!(
                "[SyncLoop] Failed to parse MLS cursor value '{s}' for \
                 space={space_id} device={device_id}: {e}"
            );
        })
        .ok()
    })
}

/// Persist the MLS message cursor for `(space_id, device_id)`.
pub fn save_last_mls_cursor(
    db: &DbConnection,
    space_id: &str,
    device_id: &str,
    message_id: i64,
) {
    let key = mls_cursor_key(space_id);
    let row_id = uuid::Uuid::new_v4().to_string();
    let value = message_id.to_string();

    let result: Result<(), DatabaseError> = with_connection(db, |conn| {
        conn.execute(
            "INSERT INTO haex_vault_settings (id, key, value, device_id) \
             VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(key, device_id) DO UPDATE SET value = excluded.value",
            rusqlite::params![row_id, key, value, device_id],
        )
        .map(|_| ())
        .map_err(DatabaseError::from)
    });

    if let Err(e) = result {
        eprintln!(
            "[SyncLoop] Failed to save MLS cursor for space={space_id} \
             device={device_id}: {e}"
        );
    }
}
