// src-tauri/src/crdt/cleanup.rs

use crate::crdt::trigger::DELETED_ROWS_TABLE;
use crate::table_names::TABLE_CRDT_CONFIGS;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use ts_rs::TS;
use uhlc::Timestamp;

/// Result of the delete-log cleanup operation.
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CleanupResult {
    /// Number of delete-log rows hard-deleted.
    pub tombstones_deleted: usize,
    /// Kept for backwards compatibility with the old tombstone API (always 1 now).
    pub applied_deleted: usize,
    /// Total entries deleted.
    pub total_deleted: usize,
}

/// RAII guard to ensure PRAGMA foreign_keys is re-enabled on drop.
pub(crate) struct ForeignKeyGuard<'a>(&'a Connection);

impl<'a> ForeignKeyGuard<'a> {
    pub(crate) fn disable(conn: &'a Connection) -> Result<Self, rusqlite::Error> {
        conn.execute("PRAGMA foreign_keys = OFF", [])?;
        Ok(Self(conn))
    }
}

impl Drop for ForeignKeyGuard<'_> {
    fn drop(&mut self) {
        let _ = self.0.execute("PRAGMA foreign_keys = ON", []);
    }
}

/// Cleans up old delete-log entries. Deletes rows from `haex_deleted_rows`
/// whose `haex_hlc` is older than `retention_days`.
///
/// `retention_days == 0` hard-deletes every delete-log entry.
pub fn cleanup_deleted_rows(
    conn: &Connection,
    retention_days: u32,
) -> Result<CleanupResult, rusqlite::Error> {
    eprintln!(
        "🧹 [cleanup_deleted_rows] Called with retention_days={}",
        retention_days
    );

    let _fk_guard = ForeignKeyGuard::disable(conn)?;

    let deleted = if retention_days == 0 {
        let delete_sql = format!("DELETE FROM \"{}\"", DELETED_ROWS_TABLE);
        conn.execute(&delete_sql, [])?
    } else {
        let query = format!(
            "SELECT value FROM {} WHERE key = ?1 AND type = 'hlc'",
            TABLE_CRDT_CONFIGS
        );
        let current_hlc_str: Option<String> = conn
            .query_row(&query, ["hlc_timestamp"], |row| row.get(0))
            .ok();

        let current_hlc_str = match current_hlc_str {
            Some(s) => s,
            None => {
                eprintln!("No HLC timestamp found in config, skipping cleanup");
                return Ok(CleanupResult {
                    tombstones_deleted: 0,
                    applied_deleted: 0,
                    total_deleted: 0,
                });
            }
        };

        let current_timestamp = Timestamp::from_str(&current_hlc_str).map_err(|e| {
            eprintln!("Failed to parse HLC timestamp '{current_hlc_str}': {e:?}");
            rusqlite::Error::InvalidQuery
        })?;

        let current_hlc_num = current_timestamp.get_time().as_u64();
        let retention_ns = retention_days as u64 * 24 * 60 * 60 * 1_000_000_000;
        let cutoff_hlc_num = current_hlc_num.saturating_sub(retention_ns) as i64;

        let delete_sql = format!(
            "DELETE FROM \"{}\"
             WHERE haex_hlc IS NOT NULL
             AND CAST(substr(haex_hlc, 1, instr(haex_hlc, '/') - 1) AS INTEGER) < ?1",
            DELETED_ROWS_TABLE
        );
        conn.execute(&delete_sql, [cutoff_hlc_num])?
    };

    if deleted > 0 {
        eprintln!("Cleaned up {deleted} entries from {DELETED_ROWS_TABLE}");
    }

    Ok(CleanupResult {
        tombstones_deleted: deleted,
        applied_deleted: 1,
        total_deleted: deleted,
    })
}

/// Gets statistics about CRDT tables.
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CrdtStats {
    /// Total number of rows across all CRDT-enabled tables.
    pub total_entries: i64,
    /// Always 0 with the delete-log architecture (kept for backwards compat).
    pub pending_upload: i64,
    /// Number of CRDT-enabled tables.
    pub pending_apply: i64,
    /// Alias of `total_entries` minus delete-log entries.
    pub applied: i64,
    /// Alias of `total_entries` (kept for backwards compat).
    pub insert_count: i64,
    /// Alias of `applied` (kept for backwards compat).
    pub update_count: i64,
    /// Number of rows currently in `haex_deleted_rows`.
    pub delete_count: i64,
}

pub fn get_crdt_stats(conn: &Connection) -> Result<CrdtStats, rusqlite::Error> {
    let mut total_entries: i64 = 0;
    let mut crdt_table_count: i64 = 0;

    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '%_no_sync'",
    )?;

    let table_names: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;

    for table_name in table_names {
        crdt_table_count += 1;

        let count: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM \"{}\"", table_name),
            [],
            |row| row.get(0),
        )?;
        total_entries += count;
    }

    let delete_count: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM \"{}\"", DELETED_ROWS_TABLE),
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let applied = total_entries.saturating_sub(delete_count);

    Ok(CrdtStats {
        total_entries,
        pending_upload: 0,
        pending_apply: crdt_table_count,
        applied,
        insert_count: total_entries,
        update_count: applied,
        delete_count,
    })
}
