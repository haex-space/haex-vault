// src-tauri/src/database/migrations/pending_tables.rs
// Pending tables helpers + Tauri commands.

use crate::database::core::with_connection;
use crate::database::error::DatabaseError;
use crate::table_names::TABLE_CRDT_PENDING_TABLES;
use crate::AppState;
use rusqlite::{params, Connection};
use tauri::State;

/// Returns pending table names that NOW exist locally (i.e. the extension was
/// installed after the skip was recorded). Tables that still do not exist are
/// excluded so they never trigger a cursor reset before they are ready.
///
/// Free helper for non-Tauri call sites (the sync-loop recovery step) that
/// operate on a `&Connection` directly.
pub fn get_recoverable_pending_tables_inner(
    conn: &Connection,
) -> Result<Vec<String>, DatabaseError> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT pt.table_name FROM {} pt \
             WHERE EXISTS ( \
                 SELECT 1 FROM sqlite_master \
                 WHERE type = 'table' AND name = pt.table_name \
             ) \
             ORDER BY pt.table_name",
            TABLE_CRDT_PENDING_TABLES
        ))
        .map_err(DatabaseError::from)?;

    let tables = stmt
        .query_map([], |row| row.get(0))
        .map_err(DatabaseError::from)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(DatabaseError::from)?;

    Ok(tables)
}

/// Deletes a pending-table marker from a bare connection.
///
/// Used after the server-path cursor reset + full re-pull successfully
/// completes for the table. Clearing a non-existent marker is a no-op `Ok`
/// (DELETE affecting zero rows is not an error).
pub fn clear_pending_table_inner(conn: &Connection, table_name: &str) -> Result<(), DatabaseError> {
    conn.execute(
        &format!(
            "DELETE FROM {} WHERE table_name = ?",
            TABLE_CRDT_PENDING_TABLES
        ),
        params![table_name],
    )
    .map_err(DatabaseError::from)?;

    Ok(())
}

/// Counts pending tables on a bare connection.
///
/// Cheap gate the recovery step calls before doing any recovery work.
pub fn pending_tables_count(conn: &Connection) -> Result<i64, DatabaseError> {
    let count = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM {}", TABLE_CRDT_PENDING_TABLES),
            [],
            |row| row.get(0),
        )
        .map_err(DatabaseError::from)?;

    Ok(count)
}

/// Returns pending table names that NOW exist locally (see
/// `get_recoverable_pending_tables_inner`).
///
/// The TypeScript pull orchestrator calls this before computing
/// `initialCursor`; a non-empty result triggers a full re-pull so the
/// server-path cursor is reset to cover the previously-skipped tables.
#[tauri::command]
pub fn get_recoverable_pending_tables(
    state: State<'_, AppState>,
) -> Result<Vec<String>, DatabaseError> {
    with_connection(&state.db, |conn| get_recoverable_pending_tables_inner(conn))
}

/// Clears a specific pending-table marker after the re-pull that was triggered
/// by the marker has completed successfully.
///
/// On pull failure the TypeScript layer must NOT call this command, so the
/// marker stays in place and the next pull cycle re-triggers the reset.
#[tauri::command]
pub fn clear_pending_table(
    state: State<'_, AppState>,
    table_name: String,
) -> Result<(), DatabaseError> {
    with_connection(&state.db, |conn| {
        clear_pending_table_inner(conn, &table_name)
    })
}
