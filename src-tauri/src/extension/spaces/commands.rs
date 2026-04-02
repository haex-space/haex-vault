//! Tauri commands for extension shared space management.
//!
//! These commands allow extensions to:
//! - Assign/unassign their table rows to shared spaces for selective sync
//! - List shared spaces from the local database

use crate::database::core::{self, with_connection};
use crate::database::error::DatabaseError;
use crate::database::row::get_string;
use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::SpaceAction;
use crate::extension::utils::{
    emit_permission_prompt_if_needed, get_extension_table_prefix, resolve_extension_id,
};
use crate::table_names::TABLE_SHARED_SPACE_SYNC;
use crate::AppState;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State, WebviewWindow};

/// A single row assignment to a shared space.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceAssignment {
    pub table_name: String,
    pub row_pks: String,
    pub space_id: String,
    /// Optional group identifier for logically related assignments (e.g. calendar ID)
    pub group_id: Option<String>,
    /// Optional type label for display (e.g. "Calendar", "Password Folder")
    #[serde(rename = "type")]
    pub type_name: Option<String>,
    /// Optional display label (e.g. "Personal", "Team Q1")
    pub label: Option<String>,
}

/// Result of a space assignment query.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceAssignmentRow {
    pub id: String,
    pub table_name: String,
    pub row_pks: String,
    pub space_id: String,
    pub extension_id: Option<String>,
    pub group_id: Option<String>,
    #[serde(rename = "type")]
    pub type_name: Option<String>,
    pub label: Option<String>,
    pub created_at: Option<String>,
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
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    assignments: Vec<SpaceAssignment>,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<u64, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    let perm_result =
        PermissionManager::check_spaces_permission(&state, &extension_id, SpaceAction::ReadWrite)
            .await;
    if let Err(ref e) = perm_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    perm_result?;

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
            let id = uuid::Uuid::new_v4().to_string();
            let sql = format!(
                "INSERT OR IGNORE INTO {} (id, table_name, row_pks, space_id, extension_id, group_id, type, label) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                TABLE_SHARED_SPACE_SYNC
            );
            let rows = conn
                .execute(
                    &sql,
                    rusqlite::params![
                        id,
                        assignment.table_name,
                        assignment.row_pks,
                        assignment.space_id,
                        extension_id,
                        assignment.group_id,
                        assignment.type_name,
                        assignment.label,
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
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    assignments: Vec<SpaceAssignment>,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<u64, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    let perm_result =
        PermissionManager::check_spaces_permission(&state, &extension_id, SpaceAction::ReadWrite)
            .await;
    if let Err(ref e) = perm_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    perm_result?;

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
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    table_name: String,
    row_pks: Option<Vec<String>>,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<Vec<SpaceAssignmentRow>, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    let perm_result =
        PermissionManager::check_spaces_permission(&state, &extension_id, SpaceAction::Read)
            .await;
    if let Err(ref e) = perm_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    perm_result?;

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
        let select_cols = format!(
            "SELECT id, table_name, row_pks, space_id, extension_id, group_id, type, label, created_at FROM {}",
            TABLE_SHARED_SPACE_SYNC
        );

        let map_row = |row: &rusqlite::Row| -> rusqlite::Result<SpaceAssignmentRow> {
            Ok(SpaceAssignmentRow {
                id: row.get(0)?,
                table_name: row.get(1)?,
                row_pks: row.get(2)?,
                space_id: row.get(3)?,
                extension_id: row.get(4)?,
                group_id: row.get(5)?,
                type_name: row.get(6)?,
                label: row.get(7)?,
                created_at: row.get(8)?,
            })
        };

        match &row_pks {
            Some(pks) if !pks.is_empty() => {
                let placeholders: Vec<String> =
                    (2..=pks.len() + 1).map(|i| format!("?{}", i)).collect();
                let sql = format!(
                    "{} WHERE table_name = ?1 AND row_pks IN ({})",
                    select_cols,
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
                    .query_map(param_refs.as_slice(), map_row)
                    .map_err(DatabaseError::from)?;

                result
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(DatabaseError::from)
            }
            _ => {
                let sql = format!("{} WHERE table_name = ?1", select_cols);

                let mut stmt = conn.prepare(&sql).map_err(DatabaseError::from)?;
                let result = stmt
                    .query_map([&table_name], map_row)
                    .map_err(DatabaseError::from)?;

                result
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(DatabaseError::from)
            }
        }
    })?;

    Ok(rows)
}

// ============================================================================
// Auth Token Management
// ============================================================================

/// Store the frontend Supabase JWT in AppState for use by space commands.
#[tauri::command]
pub async fn set_auth_token(
    state: State<'_, AppState>,
    token: Option<String>,
) -> Result<(), String> {
    *state
        .auth_token
        .lock()
        .map_err(|e| format!("Failed to lock auth_token: {}", e))? = token;
    Ok(())
}

// ============================================================================
// Space Management Commands
// ============================================================================

/// A shared space with its decrypted name.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecryptedSpace {
    pub id: String,
    pub name: String,
    pub role: String,
    pub origin_url: String,
    pub created_at: String,
}

/// List all spaces from the local database.
///
/// Returns both local and remote spaces — no server fetch needed.
#[tauri::command]
pub async fn extension_space_list(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<Vec<DecryptedSpace>, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    let perm_result =
        PermissionManager::check_spaces_permission(&state, &extension_id, SpaceAction::Read)
            .await;
    if let Err(ref e) = perm_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    perm_result?;

    let rows = core::select_with_crdt(
        "SELECT id, name, origin_url, role, created_at FROM haex_spaces".to_string(),
        vec![],
        &state.db,
    )
    .map_err(|e| ExtensionError::Database {
        source: DatabaseError::DatabaseError {
            reason: e.to_string(),
        },
    })?;

    let spaces: Vec<DecryptedSpace> = rows
        .iter()
        .map(|row| DecryptedSpace {
            id: get_string(row, 0),
            name: get_string(row, 1),
            origin_url: get_string(row, 2),
            role: get_string(row, 3),
            created_at: get_string(row, 4),
        })
        .collect();

    Ok(spaces)
}


