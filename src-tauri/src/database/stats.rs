// src-tauri/src/database/stats.rs

use crate::crdt::trigger::TOMBSTONE_COLUMN;
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
        .prepare("SELECT id, public_key, name FROM haex_extensions WHERE haex_tombstone = 0 OR haex_tombstone IS NULL")
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

    for table_name in table_names {
        // Check if this table has a haex_tombstone column (CRDT-enabled)
        let has_tombstone: bool = conn
            .prepare(&format!("PRAGMA table_info(\"{}\")", table_name))
            .map_err(|e| DatabaseError::ExecutionError {
                sql: format!("PRAGMA table_info({})", table_name),
                reason: e.to_string(),
                table: Some(table_name.clone()),
            })?
            .query_map([], |row| {
                let col_name: String = row.get(1)?;
                Ok(col_name == TOMBSTONE_COLUMN)
            })
            .map_err(|e| DatabaseError::ExecutionError {
                sql: "query columns".to_string(),
                reason: e.to_string(),
                table: Some(table_name.clone()),
            })?
            .filter_map(Result::ok)
            .any(|x| x);

        if !has_tombstone {
            continue;
        }

        // Count rows
        let total_rows: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM \"{}\"", table_name),
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let active_rows: i64 = conn
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM \"{}\" WHERE {} = 0 OR {} IS NULL",
                    table_name, TOMBSTONE_COLUMN, TOMBSTONE_COLUMN
                ),
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Count actual tombstones (haex_tombstone = 1), not just non-active
        let tombstone_rows: i64 = conn
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM \"{}\" WHERE {} = 1",
                    table_name, TOMBSTONE_COLUMN
                ),
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        stats.push(TableStats {
            name: table_name,
            total_rows,
            active_rows,
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

        // Estimate pending rows (rows modified since last sync)
        // This is an approximation - we count all non-tombstoned rows for simplicity
        let pending_rows: i64 = conn
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM \"{}\" WHERE {} = 0",
                    table_name, TOMBSTONE_COLUMN
                ),
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

/// Get tombstone entries (limited)
fn get_tombstone_entries(
    conn: &Connection,
    table_stats: &[TableStats],
    limit: usize,
) -> Result<Vec<TombstoneEntry>, DatabaseError> {
    let mut tombstones = Vec::new();

    for table in table_stats {
        if table.tombstone_rows == 0 {
            continue;
        }

        // Get primary key columns for this table
        let pk_columns: Vec<String> = conn
            .prepare(&format!("PRAGMA table_info(\"{}\")", table.name))
            .map_err(|e| DatabaseError::ExecutionError {
                sql: format!("PRAGMA table_info({})", table.name),
                reason: e.to_string(),
                table: Some(table.name.clone()),
            })?
            .query_map([], |row| {
                let name: String = row.get(1)?;
                let is_pk: i32 = row.get(5)?;
                Ok((name, is_pk > 0))
            })
            .map_err(|e| DatabaseError::ExecutionError {
                sql: "query pk columns".to_string(),
                reason: e.to_string(),
                table: Some(table.name.clone()),
            })?
            .filter_map(Result::ok)
            .filter(|(_, is_pk)| *is_pk)
            .map(|(name, _)| name)
            .collect();

        if pk_columns.is_empty() {
            continue;
        }

        // Get tombstoned entries
        let pk_select = pk_columns.join(", ");
        let query = format!(
            "SELECT {}, haex_timestamp FROM \"{}\" WHERE {} = 1 ORDER BY haex_timestamp DESC LIMIT ?",
            pk_select, table.name, TOMBSTONE_COLUMN
        );

        let remaining = limit.saturating_sub(tombstones.len());
        if remaining == 0 {
            break;
        }

        let mut stmt = conn
            .prepare(&query)
            .map_err(|e| DatabaseError::ExecutionError {
                sql: query.clone(),
                reason: e.to_string(),
                table: Some(table.name.clone()),
            })?;

        let rows = stmt
            .query_map([remaining as i64], |row| {
                // Build PK JSON
                let mut pk_values = serde_json::Map::new();
                for (i, col) in pk_columns.iter().enumerate() {
                    if let Ok(val) = row.get::<_, String>(i) {
                        pk_values.insert(col.clone(), serde_json::Value::String(val));
                    }
                }
                let timestamp: String = row.get(pk_columns.len())?;
                Ok((
                    serde_json::to_string(&pk_values).unwrap_or_default(),
                    timestamp,
                ))
            })
            .map_err(|e| DatabaseError::ExecutionError {
                sql: "query tombstones".to_string(),
                reason: e.to_string(),
                table: Some(table.name.clone()),
            })?;

        for row in rows.filter_map(Result::ok) {
            let (primary_key, deleted_at) = row;
            tombstones.push(TombstoneEntry {
                table_name: table.name.clone(),
                primary_key,
                deleted_at,
            });
        }
    }

    Ok(tombstones)
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
