// src-tauri/src/database/stats.rs

use crate::crdt::trigger::DELETED_ROWS_TABLE;
use crate::database::core::with_connection;
use crate::database::error::DatabaseError;
use crate::table_names::TABLE_CRDT_DIRTY_TABLES;
use crate::AppState;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use tauri::State;
use ts_rs::TS;

/// Statistics for a single table
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TableStats {
    /// Table name
    pub name: String,
    /// Total number of rows
    pub total_rows: i64,
    /// Number of active (non-tombstoned) rows
    pub active_rows: i64,
    /// Number of tombstoned (soft-deleted) rows
    pub tombstone_rows: i64,
}

/// Statistics grouped by extension or system
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionStats {
    /// Extension UUID from haex_extensions table, or null for system tables
    pub extension_id: Option<String>,
    /// Extension name (e.g., "haex-pass") or "System" for core tables
    pub name: String,
    /// Tables belonging to this extension
    pub tables: Vec<TableStats>,
    /// Total rows across all tables
    pub total_rows: i64,
    /// Total active rows across all tables
    pub active_rows: i64,
    /// Total tombstone rows across all tables
    pub tombstone_rows: i64,
}

/// Tombstone entry for display
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TombstoneEntry {
    /// Table name
    pub table_name: String,
    /// Primary key value(s) as JSON string
    pub primary_key: String,
    /// When the entry was deleted (HLC timestamp)
    pub deleted_at: String,
}

/// Pending sync entry
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct PendingSyncInfo {
    /// Table name with pending changes
    pub table_name: String,
    /// When the table was last modified
    pub last_modified: String,
    /// Number of rows that need to be synced (estimated)
    pub pending_rows: i64,
}

/// Comprehensive database information
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseInfo {
    /// Database file size in bytes
    pub file_size_bytes: u64,
    /// Database file size formatted (e.g., "2.5 MB")
    pub file_size_formatted: String,
    /// Statistics grouped by extension
    pub extensions: Vec<ExtensionStats>,
    /// Pending sync information
    pub pending_sync: Vec<PendingSyncInfo>,
    /// Total pending sync entries
    pub total_pending_sync: i64,
    /// Tombstone entries (limited to most recent)
    pub tombstones: Vec<TombstoneEntry>,
    /// Total tombstone count across all tables
    pub total_tombstones: i64,
    /// Total entries across all CRDT tables
    pub total_entries: i64,
    /// Total active entries
    pub total_active: i64,
}

/// Installed extension info from haex_extensions table
struct InstalledExtension {
    id: String,
    public_key: String,
    name: String,
}

/// Format bytes to human-readable string
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Get database file size
fn get_database_size(conn: &Connection) -> Result<u64, DatabaseError> {
    // Get the database path
    let path: String = conn
        .query_row("PRAGMA database_list", [], |row| row.get::<_, String>(2))
        .map_err(|e| DatabaseError::ExecutionError {
            sql: "PRAGMA database_list".to_string(),
            reason: e.to_string(),
            table: None,
        })?;

    // Get file size
    let metadata = fs::metadata(&path).map_err(|e| DatabaseError::IoError {
        path: path.clone(),
        reason: e.to_string(),
    })?;

    // Also check for WAL and SHM files
    let wal_size = fs::metadata(format!("{}-wal", path))
        .map(|m| m.len())
        .unwrap_or(0);
    let shm_size = fs::metadata(format!("{}-shm", path))
        .map(|m| m.len())
        .unwrap_or(0);

    Ok(metadata.len() + wal_size + shm_size)
}

/// Get all installed extensions from haex_extensions table
fn get_installed_extensions(conn: &Connection) -> Result<Vec<InstalledExtension>, DatabaseError> {
    let mut stmt = conn
        .prepare("SELECT id, public_key, name FROM haex_extensions")
        .map_err(|e| DatabaseError::ExecutionError {
            sql: "SELECT extensions".to_string(),
            reason: e.to_string(),
            table: None,
        })?;

    let extensions = stmt
        .query_map([], |row| {
            Ok(InstalledExtension {
                id: row.get(0)?,
                public_key: row.get(1)?,
                name: row.get(2)?,
            })
        })
        .map_err(|e| DatabaseError::ExecutionError {
            sql: "query extensions".to_string(),
            reason: e.to_string(),
            table: None,
        })?
        .filter_map(Result::ok)
        .collect();

    Ok(extensions)
}

/// Check if a table belongs to an extension
/// Extension tables have format: {public_key}__{extension_name}__{table}
fn find_extension_for_table<'a>(
    table_name: &str,
    extensions: &'a [InstalledExtension],
) -> Option<&'a InstalledExtension> {
    for ext in extensions {
        // Build the prefix: {public_key}__{name}__
        let prefix = format!("{}__{}__", ext.public_key, ext.name);
        if table_name.starts_with(&prefix) {
            return Some(ext);
        }
    }
    None
}

/// Get all CRDT tables with their statistics
fn get_table_statistics(conn: &Connection) -> Result<Vec<TableStats>, DatabaseError> {
    let mut stats = Vec::new();

    // Get all table names
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")
        .map_err(|e| DatabaseError::ExecutionError {
            sql: "SELECT tables".to_string(),
            reason: e.to_string(),
            table: None,
        })?;

    let table_names: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| DatabaseError::ExecutionError {
            sql: "query tables".to_string(),
            reason: e.to_string(),
            table: None,
        })?
        .filter_map(Result::ok)
        .collect();

    // Count delete-log entries per target table; tombstones are not in the main
    // tables anymore.
    let mut tombstone_counts: HashMap<String, i64> = HashMap::new();
    if let Ok(mut stmt) = conn.prepare(&format!(
        "SELECT table_name, COUNT(*) FROM \"{}\" GROUP BY table_name",
        DELETED_ROWS_TABLE
    )) {
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        }) {
            for row in rows.filter_map(Result::ok) {
                tombstone_counts.insert(row.0, row.1);
            }
        }
    }

    for table_name in table_names {
        let total_rows: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM \"{}\"", table_name),
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let tombstone_rows = *tombstone_counts.get(&table_name).unwrap_or(&0);
        stats.push(TableStats {
            name: table_name,
            total_rows,
            active_rows: total_rows,
            tombstone_rows,
        });
    }

    Ok(stats)
}

/// Get pending sync information from dirty tables
fn get_pending_sync(conn: &Connection) -> Result<Vec<PendingSyncInfo>, DatabaseError> {
    let mut pending = Vec::new();

    let mut stmt = conn
        .prepare(&format!("SELECT table_name, last_modified FROM {TABLE_CRDT_DIRTY_TABLES} ORDER BY last_modified DESC"))
        .map_err(|e| DatabaseError::ExecutionError {
            sql: "SELECT dirty_tables".to_string(),
            reason: e.to_string(),
            table: None,
        })?;

    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| DatabaseError::ExecutionError {
            sql: "query dirty_tables".to_string(),
            reason: e.to_string(),
            table: None,
        })?;

    for row in rows.filter_map(Result::ok) {
        let (table_name, last_modified) = row;

        // Estimate pending rows as the total row count in the dirty table.
        let pending_rows: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM \"{}\"", table_name),
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        pending.push(PendingSyncInfo {
            table_name,
            last_modified,
            pending_rows,
        });
    }

    Ok(pending)
}

/// Get the most recent delete-log entries.
fn get_tombstone_entries(
    conn: &Connection,
    _table_stats: &[TableStats],
    limit: usize,
) -> Result<Vec<TombstoneEntry>, DatabaseError> {
    let query = format!(
        "SELECT table_name, row_pks, haex_hlc FROM \"{}\" ORDER BY haex_hlc DESC LIMIT ?",
        DELETED_ROWS_TABLE
    );

    let mut stmt = match conn.prepare(&query) {
        Ok(s) => s,
        Err(_) => return Ok(Vec::new()),
    };

    let rows = stmt
        .query_map([limit as i64], |row| {
            Ok(TombstoneEntry {
                table_name: row.get(0)?,
                primary_key: row.get(1)?,
                deleted_at: row.get(2)?,
            })
        })
        .map_err(|e| DatabaseError::ExecutionError {
            sql: query.clone(),
            reason: e.to_string(),
            table: Some(DELETED_ROWS_TABLE.to_string()),
        })?;

    Ok(rows.filter_map(Result::ok).collect())
}

/// Gets comprehensive database information
#[tauri::command]
pub fn get_database_info(state: State<'_, AppState>) -> Result<DatabaseInfo, DatabaseError> {
    with_connection(&state.db, |conn| {
        // Get file size
        let file_size_bytes = get_database_size(conn)?;
        let file_size_formatted = format_bytes(file_size_bytes);

        // Get installed extensions
        let installed_extensions = get_installed_extensions(conn)?;

        // Get table statistics
        let table_stats = get_table_statistics(conn)?;

        // Group by extension
        let mut extension_map: HashMap<String, ExtensionStats> = HashMap::new();

        // Add system group first
        extension_map.insert(
            "system".to_string(),
            ExtensionStats {
                extension_id: None,
                name: "System".to_string(),
                tables: Vec::new(),
                total_rows: 0,
                active_rows: 0,
                tombstone_rows: 0,
            },
        );

        for table in &table_stats {
            // Find which extension this table belongs to
            let (key, ext_id, ext_name) =
                if let Some(ext) = find_extension_for_table(&table.name, &installed_extensions) {
                    (ext.id.clone(), Some(ext.id.clone()), ext.name.clone())
                } else {
                    ("system".to_string(), None, "System".to_string())
                };

            let ext_stats = extension_map.entry(key).or_insert_with(|| ExtensionStats {
                extension_id: ext_id,
                name: ext_name,
                tables: Vec::new(),
                total_rows: 0,
                active_rows: 0,
                tombstone_rows: 0,
            });

            ext_stats.tables.push(table.clone());
            ext_stats.total_rows += table.total_rows;
            ext_stats.active_rows += table.active_rows;
            ext_stats.tombstone_rows += table.tombstone_rows;
        }

        // Convert to sorted vec (system first, then alphabetically by name)
        let mut extensions: Vec<ExtensionStats> = extension_map.into_values().collect();
        extensions.sort_by(|a, b| {
            if a.extension_id.is_none() {
                std::cmp::Ordering::Less
            } else if b.extension_id.is_none() {
                std::cmp::Ordering::Greater
            } else {
                a.name.cmp(&b.name)
            }
        });

        // Get pending sync
        let pending_sync = get_pending_sync(conn)?;
        let total_pending_sync: i64 = pending_sync.iter().map(|p| p.pending_rows).sum();

        // Get tombstones (limit to 100)
        let tombstones = get_tombstone_entries(conn, &table_stats, 100)?;
        let total_tombstones: i64 = table_stats.iter().map(|t| t.tombstone_rows).sum();

        // Calculate totals
        let total_entries: i64 = table_stats.iter().map(|t| t.total_rows).sum();
        let total_active: i64 = table_stats.iter().map(|t| t.active_rows).sum();

        Ok(DatabaseInfo {
            file_size_bytes,
            file_size_formatted,
            extensions,
            pending_sync,
            total_pending_sync,
            tombstones,
            total_tombstones,
            total_entries,
            total_active,
        })
    })
}
