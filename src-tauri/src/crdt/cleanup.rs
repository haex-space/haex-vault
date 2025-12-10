// src-tauri/src/crdt/cleanup.rs

use crate::crdt::trigger::TOMBSTONE_COLUMN;
use crate::table_names::TABLE_CRDT_CONFIGS;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use ts_rs::TS;
use uhlc::Timestamp;

/// Result of tombstone cleanup operation
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CleanupResult {
    /// Number of tombstone entries hard-deleted across all tables
    pub tombstones_deleted: usize,
    /// Number of tables processed (kept for backwards compatibility)
    pub applied_deleted: usize,
    /// Total entries deleted
    pub total_deleted: usize,
}

/// Cleans up old tombstones (hard-deletes rows with haex_tombstone = 1)
///
/// Converts soft-deletes older than `retention_days` into hard deletes
/// to prevent unbounded table growth.
///
/// This function dynamically discovers all CRDT-enabled tables by:
/// 1. Querying sqlite_master for all tables
/// 2. Checking each table for a haex_tombstone column
/// 3. Hard-deleting old tombstoned rows from those tables
///
/// The age check works by comparing HLC timestamps.
/// HLC timestamp format (uhlc 0.8.2): "time/node_id_hex" (e.g. "7575643027736195360/1010101010101010101010101010101")
/// The time component is a 64-bit NTP64 timestamp in nanoseconds.
///
/// # Arguments
/// * `conn` - Database connection
/// * `retention_days` - Number of days to keep soft-deleted tombstones
pub fn cleanup_tombstones(
    conn: &Connection,
    retention_days: u32,
) -> Result<CleanupResult, rusqlite::Error> {
    eprintln!(
        "🧹 [cleanup_tombstones] Called with retention_days={}",
        retention_days
    );

    let mut total_deleted = 0;
    let mut tables_processed = 0;

    // Get current HLC timestamp from config
    let query = format!(
        "SELECT value FROM {} WHERE key = ?1 AND type = 'hlc'",
        TABLE_CRDT_CONFIGS
    );
    let current_hlc_str: Option<String> = conn
        .query_row(&query, ["hlc_timestamp"], |row| row.get(0))
        .ok();

    // Calculate cutoff timestamp (only needed if retention_days > 0)
    let cutoff_hlc_num = if retention_days > 0 {
        // If no HLC timestamp exists yet, skip cleanup (can't calculate cutoff)
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

        // Parse current HLC timestamp using uhlc's FromStr
        let current_timestamp = Timestamp::from_str(&current_hlc_str).map_err(|e| {
            eprintln!("Failed to parse HLC timestamp '{current_hlc_str}': {e:?}");
            rusqlite::Error::InvalidQuery
        })?;

        // Extract the time component as u64 (NTP64 nanoseconds)
        let current_hlc_num = current_timestamp.get_time().as_u64();

        // Calculate cutoff: subtract retention_days worth of nanoseconds
        let retention_ns = retention_days as u64 * 24 * 60 * 60 * 1_000_000_000;
        current_hlc_num.saturating_sub(retention_ns)
    } else {
        // Force delete mode: cutoff not needed, will delete all tombstones
        0
    };

    // Get all table names from database
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
    )?;

    let table_names: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;

    for table_name in table_names {
        // Check if this table has a haex_tombstone column
        // This automatically filters out non-CRDT tables like haex_crdt_configs
        let has_tombstone_column: bool = conn
            .prepare(&format!("PRAGMA table_info(\"{}\")", table_name))?
            .query_map([], |row| {
                let col_name: String = row.get(1)?;
                Ok(col_name == TOMBSTONE_COLUMN)
            })?
            .filter_map(Result::ok)
            .any(|x| x);

        if !has_tombstone_column {
            continue;
        }

        // Hard-delete tombstoned rows
        // If retention_days is 0, delete ALL tombstones (force delete)
        // Otherwise, only delete tombstones older than the retention period
        let deleted_count = if retention_days == 0 {
            // Force delete: remove all tombstones regardless of age
            let delete_sql = format!(
                "DELETE FROM \"{}\" WHERE {} = 1",
                table_name, TOMBSTONE_COLUMN
            );
            eprintln!("🧹 [cleanup_tombstones] FORCE DELETE: {}", delete_sql);
            conn.execute(&delete_sql, [])?
        } else {
            // Normal cleanup: only delete tombstones older than cutoff
            // Extract the NTP64 time component (before '/') and compare with cutoff
            let delete_sql = format!(
                "DELETE FROM \"{}\"
                 WHERE {TOMBSTONE_COLUMN} = 1
                 AND CAST(substr(haex_timestamp, 1, instr(haex_timestamp, '/') - 1) AS INTEGER) < ?1",
                table_name
            );
            conn.execute(&delete_sql, [cutoff_hlc_num])?
        };

        if deleted_count > 0 {
            eprintln!("Cleaned up {deleted_count} tombstones from {table_name}");
        }

        total_deleted += deleted_count;
        tables_processed += 1;
    }

    Ok(CleanupResult {
        tombstones_deleted: total_deleted,
        applied_deleted: tables_processed, // Reuse field for tables_processed for backwards compatibility
        total_deleted,
    })
}

/// Gets statistics about CRDT tables
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CrdtStats {
    /// Total number of rows across all CRDT tables
    pub total_entries: usize,
    /// Number of tables with dirty changes (kept for backwards compatibility)
    pub pending_upload: usize,
    /// Number of CRDT-enabled tables (kept for backwards compatibility)
    pub pending_apply: usize,
    /// Number of non-tombstoned entries
    pub applied: usize,
    /// Total count across all tables (kept for backwards compatibility)
    pub insert_count: usize,
    /// Number of non-tombstoned entries (kept for backwards compatibility)
    pub update_count: usize,
    /// Number of tombstoned (soft-deleted) entries
    pub delete_count: usize,
}

pub fn get_crdt_stats(conn: &Connection) -> Result<CrdtStats, rusqlite::Error> {
    let mut total_entries = 0;
    let mut non_tombstoned = 0;
    let mut tombstoned = 0;
    let mut crdt_table_count = 0;

    // Get all table names from database
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
    )?;

    let table_names: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;

    for table_name in table_names {
        // Check if this table has a haex_tombstone column
        // This automatically filters out non-CRDT tables like haex_crdt_configs
        let has_tombstone_column: bool = conn
            .prepare(&format!("PRAGMA table_info(\"{}\")", table_name))?
            .query_map([], |row| {
                let col_name: String = row.get(1)?;
                Ok(col_name == TOMBSTONE_COLUMN)
            })?
            .filter_map(Result::ok)
            .any(|x| x);

        if !has_tombstone_column {
            continue;
        }

        crdt_table_count += 1;

        // Count total entries
        let count: usize = conn.query_row(
            &format!("SELECT COUNT(*) FROM \"{}\"", table_name),
            [],
            |row| row.get(0),
        )?;
        total_entries += count;

        // Count non-tombstoned entries
        let active_count: usize = conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM \"{}\" WHERE {} = 0",
                table_name, TOMBSTONE_COLUMN
            ),
            [],
            |row| row.get(0),
        )?;
        non_tombstoned += active_count;

        // Count tombstoned entries
        let tombstone_count: usize = conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM \"{}\" WHERE {} = 1",
                table_name, TOMBSTONE_COLUMN
            ),
            [],
            |row| row.get(0),
        )?;
        tombstoned += tombstone_count;
    }

    Ok(CrdtStats {
        total_entries,
        pending_upload: 0, // Not applicable with new architecture
        pending_apply: crdt_table_count,
        applied: non_tombstoned,
        insert_count: total_entries,
        update_count: non_tombstoned,
        delete_count: tombstoned,
    })
}
