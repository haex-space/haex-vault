// src-tauri/src/crdt/cleanup.rs

use crate::table_names::TABLE_CRDT_CHANGES;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Result of tombstone cleanup operation
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CleanupResult {
    /// Number of tombstone entries deleted
    pub tombstones_deleted: usize,
    /// Number of applied entries deleted (non-DELETE operations)
    pub applied_deleted: usize,
    /// Total entries deleted
    pub total_deleted: usize,
}

/// Cleans up old tombstone (DELETE) entries and applied changes
///
/// - DELETE entries older than `retention_days` are removed
/// - INSERT/UPDATE entries with sync_state='applied' can also be removed
///
/// # Arguments
/// * `conn` - Database connection
/// * `retention_days` - Number of days to keep DELETE tombstones
pub fn cleanup_tombstones(
    conn: &Connection,
    retention_days: u32,
) -> Result<CleanupResult, rusqlite::Error> {
    // Delete old tombstones (DELETE operations)
    let tombstones_sql = format!(
        "DELETE FROM {TABLE_CRDT_CHANGES}
         WHERE operation = 'DELETE'
         AND created_at < datetime('now', '-{retention_days} days')"
    );

    let tombstones_deleted = conn.execute(&tombstones_sql, [])?;

    // Delete applied INSERT/UPDATE entries (they've been synced and can be cleaned)
    let applied_sql = format!(
        "DELETE FROM {TABLE_CRDT_CHANGES}
         WHERE sync_state = 'applied'
         AND operation != 'DELETE'"
    );

    let applied_deleted = conn.execute(&applied_sql, [])?;

    Ok(CleanupResult {
        tombstones_deleted,
        applied_deleted,
        total_deleted: tombstones_deleted + applied_deleted,
    })
}

/// Gets statistics about the CRDT changes table
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CrdtStats {
    pub total_entries: usize,
    pub pending_upload: usize,
    pub pending_apply: usize,
    pub applied: usize,
    pub insert_count: usize,
    pub update_count: usize,
    pub delete_count: usize,
}

pub fn get_crdt_stats(conn: &Connection) -> Result<CrdtStats, rusqlite::Error> {
    let total_entries: usize = conn.query_row(
        &format!("SELECT COUNT(*) FROM {TABLE_CRDT_CHANGES}"),
        [],
        |row| row.get(0),
    )?;

    let pending_upload: usize = conn.query_row(
        &format!("SELECT COUNT(*) FROM {TABLE_CRDT_CHANGES} WHERE sync_state = 'pending_upload'"),
        [],
        |row| row.get(0),
    )?;

    let pending_apply: usize = conn.query_row(
        &format!("SELECT COUNT(*) FROM {TABLE_CRDT_CHANGES} WHERE sync_state = 'pending_apply'"),
        [],
        |row| row.get(0),
    )?;

    let applied: usize = conn.query_row(
        &format!("SELECT COUNT(*) FROM {TABLE_CRDT_CHANGES} WHERE sync_state = 'applied'"),
        [],
        |row| row.get(0),
    )?;

    let insert_count: usize = conn.query_row(
        &format!("SELECT COUNT(*) FROM {TABLE_CRDT_CHANGES} WHERE operation = 'INSERT'"),
        [],
        |row| row.get(0),
    )?;

    let update_count: usize = conn.query_row(
        &format!("SELECT COUNT(*) FROM {TABLE_CRDT_CHANGES} WHERE operation = 'UPDATE'"),
        [],
        |row| row.get(0),
    )?;

    let delete_count: usize = conn.query_row(
        &format!("SELECT COUNT(*) FROM {TABLE_CRDT_CHANGES} WHERE operation = 'DELETE'"),
        [],
        |row| row.get(0),
    )?;

    Ok(CrdtStats {
        total_entries,
        pending_upload,
        pending_apply,
        applied,
        insert_count,
        update_count,
        delete_count,
    })
}
