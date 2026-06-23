use crate::crdt::trigger::{get_table_schema as get_table_schema_internal, ColumnInfo};
use crate::database::core::with_connection;
use crate::database::error::DatabaseError;
use crate::table_names::TABLE_CRDT_DIRTY_TABLES;
use crate::AppState;
use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DirtyTable {
    pub table_name: String,
    pub last_modified: String,
}

/// Gets table schema information (columns and their properties)
#[tauri::command]
pub fn get_table_schema(
    table_name: String,
    state: State<'_, AppState>,
) -> Result<Vec<ColumnInfo>, DatabaseError> {
    with_connection(&state.db, |conn| {
        Ok(get_table_schema_internal(conn, &table_name).map_err(DatabaseError::from)?)
    })
}

/// Gets all dirty tables that need to be synced
#[tauri::command]
pub fn get_dirty_tables(state: State<'_, AppState>) -> Result<Vec<DirtyTable>, DatabaseError> {
    with_connection(&state.db, |conn| {
        let mut stmt = conn
            .prepare(&format!("SELECT table_name, last_modified FROM {TABLE_CRDT_DIRTY_TABLES} ORDER BY last_modified ASC"))
            .map_err(DatabaseError::from)?;

        let rows = stmt
            .query_map([], |row| {
                Ok(DirtyTable {
                    table_name: row.get(0)?,
                    last_modified: row.get(1)?,
                })
            })
            .map_err(DatabaseError::from)?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(DatabaseError::from)
    })
}

/// Inner logic for clearing a dirty table, callable from Rust without Tauri state.
pub fn clear_dirty_table_inner(
    db: &crate::database::DbConnection,
    table_name: &str,
    before_timestamp: Option<&str>,
) -> Result<(), DatabaseError> {
    with_connection(db, |conn| {
        match before_timestamp {
            Some(ts) => {
                conn.execute(
                    &format!(
                        "DELETE FROM {TABLE_CRDT_DIRTY_TABLES} WHERE table_name = ?1 AND last_modified <= ?2"
                    ),
                    [table_name, ts],
                )
                .map_err(DatabaseError::from)?;
            }
            None => {
                conn.execute(
                    &format!("DELETE FROM {TABLE_CRDT_DIRTY_TABLES} WHERE table_name = ?1"),
                    [table_name],
                )
                .map_err(DatabaseError::from)?;
            }
        }

        Ok(())
    })
}

/// Clears a specific table from the dirty tables tracker.
/// If before_timestamp is provided, only clears entries with last_modified <= that timestamp.
/// This prevents clearing entries that were added AFTER the sync scan started.
#[tauri::command]
pub fn clear_dirty_table(
    table_name: String,
    before_timestamp: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), DatabaseError> {
    clear_dirty_table_inner(&state.db, &table_name, before_timestamp.as_deref())
}

/// Clears all dirty tables
#[tauri::command]
pub fn clear_all_dirty_tables(state: State<'_, AppState>) -> Result<(), DatabaseError> {
    with_connection(&state.db, |conn| {
        conn.execute(&format!("DELETE FROM {TABLE_CRDT_DIRTY_TABLES}"), [])
            .map_err(DatabaseError::from)?;

        Ok(())
    })
}

/// Gets all CRDT-enabled tables (tables with a `haex_hlc` column).
#[tauri::command]
pub fn get_all_crdt_tables(state: State<'_, AppState>) -> Result<Vec<String>, DatabaseError> {
    use crate::database::init::discover_crdt_tables;

    with_connection(&state.db, |conn| discover_crdt_tables(conn))
}

/// Ensures all CRDT tables have proper triggers set up.
/// This should be called after applying synced extension migrations to make sure
/// newly created extension tables have their dirty-table triggers.
/// Returns the number of tables that had triggers created.
#[tauri::command]
pub fn ensure_extension_triggers(state: State<'_, AppState>) -> Result<usize, DatabaseError> {
    use crate::database::init::ensure_triggers_for_all_tables;

    with_connection(&state.db, |conn| ensure_triggers_for_all_tables(conn))
}
