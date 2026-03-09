//! Tauri commands for extension shared space row assignments.
//!
//! These commands allow extensions to assign/unassign their table rows
//! to shared spaces for selective sync.

use crate::database::core::with_connection;
use crate::database::error::DatabaseError;
use crate::extension::error::ExtensionError;
use crate::extension::utils::{get_extension_table_prefix, resolve_extension_id};
use crate::table_names::TABLE_SHARED_SPACE_SYNC;
use crate::AppState;

use serde::{Deserialize, Serialize};
use tauri::{State, WebviewWindow};

/// A single row assignment to a shared space.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceAssignment {
    pub table_name: String,
    pub row_pks: String,
    pub space_id: String,
}

/// Result of a space assignment query.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceAssignmentRow {
    pub table_name: String,
    pub row_pks: String,
    pub space_id: String,
}

/// Validates that all table names in the assignments start with the extension's prefix.
fn validate_table_prefixes(
    assignments: &[SpaceAssignment],
    prefix: &str,
) -> Result<(), ExtensionError> {
    for assignment in assignments {
        if !assignment.table_name.starts_with(prefix) {
            return Err(ExtensionError::SecurityViolation {
                reason: format!(
                    "Table '{}' does not belong to this extension (expected prefix '{}')",
                    assignment.table_name, prefix
                ),
            });
        }
    }
    Ok(())
}

/// Validates that a single table name starts with the extension's prefix.
fn validate_single_table_prefix(
    table_name: &str,
    prefix: &str,
) -> Result<(), ExtensionError> {
    if !table_name.starts_with(prefix) {
        return Err(ExtensionError::SecurityViolation {
            reason: format!(
                "Table '{}' does not belong to this extension (expected prefix '{}')",
                table_name, prefix
            ),
        });
    }
    Ok(())
}

/// Bulk assign rows to shared spaces (INSERT OR IGNORE).
///
/// Extensions can only assign rows from their own tables (validated via prefix).
#[tauri::command]
pub async fn extension_space_assign(
    window: WebviewWindow,
    state: State<'_, AppState>,
    assignments: Vec<SpaceAssignment>,
    // Optional parameters for iframe mode
    public_key: Option<String>,
    name: Option<String>,
) -> Result<u64, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    let extension = state
        .extension_manager
        .get_extension(&extension_id)
        .ok_or_else(|| ExtensionError::ValidationError {
            reason: format!("Extension with ID {} not found", extension_id),
        })?;

    let prefix = get_extension_table_prefix(
        &extension.manifest.public_key,
        &extension.manifest.name,
    );

    validate_table_prefixes(&assignments, &prefix)?;

    if assignments.is_empty() {
        return Ok(0);
    }

    let total_inserted = with_connection(&state.db, |conn| {
        let mut inserted: u64 = 0;
        for assignment in &assignments {
            let sql = format!(
                "INSERT OR IGNORE INTO {} (table_name, row_pks, space_id) VALUES (?1, ?2, ?3)",
                TABLE_SHARED_SPACE_SYNC
            );
            let rows = conn
                .execute(
                    &sql,
                    rusqlite::params![
                        assignment.table_name,
                        assignment.row_pks,
                        assignment.space_id,
                    ],
                )
                .map_err(DatabaseError::from)?;
            inserted += rows as u64;
        }
        Ok(inserted)
    })?;

    Ok(total_inserted)
}

/// Bulk unassign rows from shared spaces (DELETE).
///
/// Extensions can only unassign rows from their own tables (validated via prefix).
#[tauri::command]
pub async fn extension_space_unassign(
    window: WebviewWindow,
    state: State<'_, AppState>,
    assignments: Vec<SpaceAssignment>,
    // Optional parameters for iframe mode
    public_key: Option<String>,
    name: Option<String>,
) -> Result<u64, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    let extension = state
        .extension_manager
        .get_extension(&extension_id)
        .ok_or_else(|| ExtensionError::ValidationError {
            reason: format!("Extension with ID {} not found", extension_id),
        })?;

    let prefix = get_extension_table_prefix(
        &extension.manifest.public_key,
        &extension.manifest.name,
    );

    validate_table_prefixes(&assignments, &prefix)?;

    if assignments.is_empty() {
        return Ok(0);
    }

    let total_deleted = with_connection(&state.db, |conn| {
        let mut deleted: u64 = 0;
        for assignment in &assignments {
            let sql = format!(
                "DELETE FROM {} WHERE table_name = ?1 AND row_pks = ?2 AND space_id = ?3",
                TABLE_SHARED_SPACE_SYNC
            );
            let rows = conn
                .execute(
                    &sql,
                    rusqlite::params![
                        assignment.table_name,
                        assignment.row_pks,
                        assignment.space_id,
                    ],
                )
                .map_err(DatabaseError::from)?;
            deleted += rows as u64;
        }
        Ok(deleted)
    })?;

    Ok(total_deleted)
}

/// Get space assignments for an extension's table, optionally filtered by row PKs.
///
/// Extensions can only query assignments for their own tables (validated via prefix).
#[tauri::command]
pub async fn extension_space_get_assignments(
    window: WebviewWindow,
    state: State<'_, AppState>,
    table_name: String,
    row_pks: Option<Vec<String>>,
    // Optional parameters for iframe mode
    public_key: Option<String>,
    name: Option<String>,
) -> Result<Vec<SpaceAssignmentRow>, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    let extension = state
        .extension_manager
        .get_extension(&extension_id)
        .ok_or_else(|| ExtensionError::ValidationError {
            reason: format!("Extension with ID {} not found", extension_id),
        })?;

    let prefix = get_extension_table_prefix(
        &extension.manifest.public_key,
        &extension.manifest.name,
    );

    validate_single_table_prefix(&table_name, &prefix)?;

    let rows = with_connection(&state.db, |conn| {
        match &row_pks {
            Some(pks) if !pks.is_empty() => {
                // Build a query with IN clause using positional parameters
                let placeholders: Vec<String> =
                    (2..=pks.len() + 1).map(|i| format!("?{}", i)).collect();
                let sql = format!(
                    "SELECT table_name, row_pks, space_id FROM {} WHERE table_name = ?1 AND row_pks IN ({})",
                    TABLE_SHARED_SPACE_SYNC,
                    placeholders.join(", ")
                );

                let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
                params.push(Box::new(table_name.clone()));
                for pk in pks {
                    params.push(Box::new(pk.clone()));
                }

                let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                    params.iter().map(|p| p.as_ref()).collect();

                let mut stmt = conn.prepare(&sql).map_err(DatabaseError::from)?;
                let result = stmt
                    .query_map(param_refs.as_slice(), |row| {
                        Ok(SpaceAssignmentRow {
                            table_name: row.get(0)?,
                            row_pks: row.get(1)?,
                            space_id: row.get(2)?,
                        })
                    })
                    .map_err(DatabaseError::from)?;

                result
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(DatabaseError::from)
            }
            _ => {
                // No filter, return all assignments for this table
                let sql = format!(
                    "SELECT table_name, row_pks, space_id FROM {} WHERE table_name = ?1",
                    TABLE_SHARED_SPACE_SYNC
                );

                let mut stmt = conn.prepare(&sql).map_err(DatabaseError::from)?;
                let result = stmt
                    .query_map([&table_name], |row| {
                        Ok(SpaceAssignmentRow {
                            table_name: row.get(0)?,
                            row_pks: row.get(1)?,
                            space_id: row.get(2)?,
                        })
                    })
                    .map_err(DatabaseError::from)?;

                result
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(DatabaseError::from)
            }
        }
    })?;

    Ok(rows)
}
