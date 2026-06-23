// src-tauri/src/database/migrations/pending_columns.rs
// Pending columns helpers + Tauri commands.

use super::{PendingColumn, PendingColumnRow};
use crate::database::core::with_connection;
use crate::database::error::DatabaseError;
use crate::table_names::TABLE_CRDT_PENDING_COLUMNS;
use crate::AppState;
use rusqlite::{params, Connection};
use tauri::State;

/// Reads the distinct pending (table, column) pairs from a bare connection.
///
/// The marker table is now row-granular, but this TS/HTTP-facing helper
/// collapses the rows to one entry per (table, column) so the HTTP recovery
/// loop still iterates each column once. Free helper so non-Tauri call sites
/// (the owner-sync request handler and the sync-loop recovery step) can operate
/// on a `&Connection` directly.
pub fn get_pending_columns_inner(conn: &Connection) -> Result<Vec<PendingColumn>, DatabaseError> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT DISTINCT table_name, column_name FROM {} ORDER BY table_name, column_name",
            TABLE_CRDT_PENDING_COLUMNS
        ))
        .map_err(DatabaseError::from)?;

    let columns = stmt
        .query_map([], |row| {
            Ok(PendingColumn {
                table_name: row.get(0)?,
                column_name: row.get(1)?,
            })
        })
        .map_err(DatabaseError::from)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(DatabaseError::from)?;

    Ok(columns)
}

/// Row-aware read for the P2P recovery path: one entry per owed
/// (table, column, row_pks).
pub fn get_pending_column_rows_inner(
    conn: &Connection,
) -> Result<Vec<PendingColumnRow>, DatabaseError> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT table_name, column_name, row_pks FROM {} ORDER BY table_name, column_name, row_pks",
            TABLE_CRDT_PENDING_COLUMNS
        ))
        .map_err(DatabaseError::from)?;
    let rows = stmt
        .query_map([], |row| {
            Ok(PendingColumnRow {
                table_name: row.get(0)?,
                column_name: row.get(1)?,
                row_pks: row.get(2)?,
            })
        })
        .map_err(DatabaseError::from)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(DatabaseError::from)?;
    Ok(rows)
}

/// Deletes EVERY owed row of a pending column from a bare connection.
///
/// The clear-all-for-column variant used by the authoritative HTTP recovery
/// path: once the HTTP dump has served a whole column, every row owed for it is
/// cleared. Free helper for non-Tauri call sites. Clearing a non-existent
/// column is a no-op `Ok` (DELETE affecting zero rows is not an error).
pub fn clear_pending_column_inner(
    conn: &Connection,
    table_name: &str,
    column_name: &str,
) -> Result<(), DatabaseError> {
    conn.execute(
        &format!(
            "DELETE FROM {} WHERE table_name = ? AND column_name = ?",
            TABLE_CRDT_PENDING_COLUMNS
        ),
        params![table_name, column_name],
    )
    .map_err(DatabaseError::from)?;

    Ok(())
}

/// Clears ONE owed row of a pending column (P2P recovery clears only rows the
/// dump actually served). Clearing a non-existent triple is a no-op `Ok`.
pub fn clear_pending_column_row_inner(
    conn: &Connection,
    table_name: &str,
    column_name: &str,
    row_pks: &str,
) -> Result<(), DatabaseError> {
    conn.execute(
        &format!(
            "DELETE FROM {} WHERE table_name = ? AND column_name = ? AND row_pks = ?",
            TABLE_CRDT_PENDING_COLUMNS
        ),
        params![table_name, column_name, row_pks],
    )
    .map_err(DatabaseError::from)?;

    Ok(())
}

/// Counts pending columns on a bare connection.
///
/// Cheap gate the sync-loop recovery step calls every cycle before doing any
/// recovery work.
pub fn pending_columns_count(conn: &Connection) -> Result<i64, DatabaseError> {
    let count = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM {}", TABLE_CRDT_PENDING_COLUMNS),
            [],
            |row| row.get(0),
        )
        .map_err(DatabaseError::from)?;

    Ok(count)
}

/// Gets all pending columns that were skipped during sync
///
/// These are columns that existed on remote devices but not locally
/// (due to schema version differences). After the app is updated and
/// migrations add these columns, we need to pull all data for them.
#[tauri::command]
pub fn get_pending_columns(
    state: State<'_, AppState>,
) -> Result<Vec<PendingColumn>, DatabaseError> {
    with_connection(&state.db, |conn| get_pending_columns_inner(conn))
}

/// Clears a specific pending column after its data has been successfully pulled
#[tauri::command]
pub fn clear_pending_column(
    state: State<'_, AppState>,
    table_name: String,
    column_name: String,
) -> Result<(), DatabaseError> {
    with_connection(&state.db, |conn| {
        clear_pending_column_inner(conn, &table_name, &column_name)
    })
}
