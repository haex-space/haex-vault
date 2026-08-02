//! Tauri commands for extension shared space management.
//!
//! These commands allow extensions to:
//! - Assign/unassign their table rows to shared spaces for selective sync
//! - List shared spaces from the local database

use super::queries::{
    SQL_DELETE_SHARED_SPACE_SYNC, SQL_INSERT_SHARED_SPACE_SYNC,
    SQL_SELECT_SPACE_MEMBERS_WITH_IDENTITY, SQL_SHARED_SPACE_SYNC_SELECT_COLS,
};
use crate::critical::CriticalFailureCode;
use crate::database::core;
use crate::database::error::DatabaseError;
use crate::database::row::{get_bool, get_string};
use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::{Principal, SpaceAction};
use crate::extension::utils::{get_extension_table_prefix, prompt_on_err, resolve_extension_id};
use crate::AppState;

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State, WebviewWindow};

/// A single row assignment to a shared space.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceAssignment {
    pub table_name: String,
    pub row_pks: String,
    pub space_id: String,
    /// Optional category identifier for logically related assignments (e.g. calendar ID)
    pub category: Option<String>,
    /// Optional type name for display (e.g. "Calendar", "Password Folder")
    #[serde(rename = "type")]
    pub type_name: Option<String>,
    /// Optional instance label for display (e.g. "Personal", "Team Q1")
    pub type_label: Option<String>,
}

/// Result of a space assignment query.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceAssignmentRow {
    pub id: String,
    pub table_name: String,
    pub row_pks: String,
    pub space_id: String,
    pub extension_public_key: Option<String>,
    pub extension_name: Option<String>,
    pub category: Option<String>,
    #[serde(rename = "type")]
    pub type_name: Option<String>,
    pub type_label: Option<String>,
    pub created_at: Option<String>,
}

/// Verify the vault owns at least one active local identity that is an active
/// member of `space_id`. Called from `extension_space_assign` /
/// `extension_space_unassign` BEFORE any write to `haex_shared_space_sync`
/// so non-members cannot register (or un-register) rows into a space.
///
/// "Active local identity" means a row in `haex_identities` with
/// `private_key IS NOT NULL` (the local user), joined via `identity_id` to
/// a `haex_space_members` row for `space_id`.
///
/// There is no `status` column on `haex_space_members` today — row existence
/// IS the membership. The word "active" in this fn name refers to
/// `haex_identities.private_key IS NOT NULL` (the local identity's private
/// key is present), NOT to a membership status flag.
///
/// Fails closed on DB errors: any rusqlite error is propagated as
/// `ExtensionError::Database` (never silently treated as "not a member =
/// accept"). See memory `design-decision-positive-register-gate`.
pub(super) fn require_active_local_member(
    conn: &rusqlite::Connection,
    space_id: &str,
) -> Result<(), ExtensionError> {
    let is_member: bool = conn
        .query_row(
            "SELECT 1 FROM haex_space_members m \
             JOIN haex_identities i ON i.id = m.identity_id \
             WHERE m.space_id = ?1 AND i.private_key IS NOT NULL \
             LIMIT 1",
            rusqlite::params![space_id],
            |_| Ok(true),
        )
        .optional()
        .map_err(|e| ExtensionError::Database {
            source: DatabaseError::DatabaseError {
                reason: e.to_string(),
            },
        })?
        .unwrap_or(false);
    if !is_member {
        return Err(ExtensionError::SecurityViolation {
            reason: format!("caller is not an active local member of space_id={space_id}"),
        });
    }
    Ok(())
}

/// Verifies the active local identity is a member of every space referenced
/// in `assignments`. All-or-nothing: the first non-member rejects the whole
/// batch. Callers pass their assignment list; dedup happens here.
///
/// `with_connection` returns `Result<T, DatabaseError>`; we thread the
/// per-call `ExtensionError` through as the inner `Result` so
/// `SecurityViolation` reaches the caller without being flattened to a
/// stringly-typed `DatabaseError`. The `??` unwraps: outer `?` = DB / lock
/// error, inner `?` = `ExtensionError` from `require_active_local_member`.
///
/// Fail-closed on DB errors — a DB failure surfaces as
/// [`ExtensionError::Database`], not silently as "member".
pub(super) fn require_active_local_member_for_all(
    db: &crate::database::DbConnection,
    assignments: &[SpaceAssignment],
) -> Result<(), ExtensionError> {
    let unique_space_ids: std::collections::BTreeSet<&str> =
        assignments.iter().map(|a| a.space_id.as_str()).collect();
    core::with_connection(
        db,
        |conn| -> Result<Result<(), ExtensionError>, DatabaseError> {
            for space_id in &unique_space_ids {
                if let Err(e) = require_active_local_member(conn, space_id) {
                    return Ok(Err(e));
                }
            }
            Ok(Ok(()))
        },
    )
    .map_err(|e| ExtensionError::Database { source: e })??;
    Ok(())
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
fn validate_single_table_prefix(table_name: &str, prefix: &str) -> Result<(), ExtensionError> {
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

    let perm_result = PermissionManager::check_spaces_permission(
        &state,
        &Principal::Extension(extension_id.clone()),
        SpaceAction::ReadWrite,
    )
    .await;
    prompt_on_err(&app_handle, perm_result)?;

    let extension = state
        .extension_manager
        .get_extension(&extension_id)
        .ok_or_else(|| ExtensionError::ValidationError {
            reason: format!("Extension with ID {} not found", extension_id),
        })?;

    let prefix =
        get_extension_table_prefix(&extension.manifest.public_key, &extension.manifest.name);

    validate_table_prefixes(&assignments, &prefix)?;

    if assignments.is_empty() {
        return Ok(0);
    }

    // Space-membership gate: only vaults with an active local member of the
    // target space may (un-)register rows. See helper for rationale.
    require_active_local_member_for_all(&state.db, &assignments)?;

    let hlc_guard = state.lock_or_fail(
        &state.hlc,
        CriticalFailureCode::HlcMutexPoisoned,
        "extension::spaces::commands::extension_space_assign",
        serde_json::json!({}),
    )?;
    // NB: this function is `pub async fn` but the loop below is fully
    // synchronous — `hlc_guard` (a `MutexGuard<HlcService>`) is `!Send`,
    // so a future `.await` added inside the for-body would silently break
    // the `Send` bound required by the Tauri runtime. Keep the guard
    // scope strictly synchronous.

    // Use the authenticated extension identity (resolved above via
    // `get_extension`), NOT the caller-provided `public_key` / `name`
    // parameters — otherwise a compromised webview could spoof another
    // extension's identity in the shared-space-sync routing table.
    let ext_public_key = extension.manifest.public_key.clone();
    let ext_name = extension.manifest.name.clone();

    let mut total_inserted: u64 = 0;
    for assignment in &assignments {
        let id = uuid::Uuid::new_v4().to_string();

        core::execute_with_crdt(
            SQL_INSERT_SHARED_SPACE_SYNC.clone(),
            vec![
                serde_json::Value::String(id),
                serde_json::Value::String(assignment.table_name.clone()),
                serde_json::Value::String(assignment.row_pks.clone()),
                serde_json::Value::String(assignment.space_id.clone()),
                serde_json::Value::String(ext_public_key.clone()),
                serde_json::Value::String(ext_name.clone()),
                assignment
                    .category
                    .as_ref()
                    .map_or(serde_json::Value::Null, |v| {
                        serde_json::Value::String(v.clone())
                    }),
                assignment
                    .type_name
                    .as_ref()
                    .map_or(serde_json::Value::Null, |v| {
                        serde_json::Value::String(v.clone())
                    }),
                assignment
                    .type_label
                    .as_ref()
                    .map_or(serde_json::Value::Null, |v| {
                        serde_json::Value::String(v.clone())
                    }),
            ],
            &state.db,
            &hlc_guard,
            &state.column_sig_key_cache,
        )
        .map_err(|e| ExtensionError::Database { source: e })?;
        total_inserted += 1;
    }

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

    let perm_result = PermissionManager::check_spaces_permission(
        &state,
        &Principal::Extension(extension_id.clone()),
        SpaceAction::ReadWrite,
    )
    .await;
    prompt_on_err(&app_handle, perm_result)?;

    let extension = state
        .extension_manager
        .get_extension(&extension_id)
        .ok_or_else(|| ExtensionError::ValidationError {
            reason: format!("Extension with ID {} not found", extension_id),
        })?;

    let prefix =
        get_extension_table_prefix(&extension.manifest.public_key, &extension.manifest.name);

    validate_table_prefixes(&assignments, &prefix)?;

    if assignments.is_empty() {
        return Ok(0);
    }

    // Symmetric with `extension_space_assign`: only active local members of
    // the target space may un-register rows. See helper for rationale.
    require_active_local_member_for_all(&state.db, &assignments)?;

    let hlc_guard = state.lock_or_fail(
        &state.hlc,
        CriticalFailureCode::HlcMutexPoisoned,
        "extension::spaces::commands::extension_space_unassign",
        serde_json::json!({}),
    )?;
    // NB: `hlc_guard` is a `MutexGuard<HlcService>` (`!Send`). The loop
    // body below is synchronous — adding an `.await` inside would break
    // the `Send` bound required by the Tauri runtime.

    let mut total_deleted: u64 = 0;
    for assignment in &assignments {
        core::execute_with_crdt(
            SQL_DELETE_SHARED_SPACE_SYNC.clone(),
            vec![
                serde_json::Value::String(assignment.table_name.clone()),
                serde_json::Value::String(assignment.row_pks.clone()),
                serde_json::Value::String(assignment.space_id.clone()),
            ],
            &state.db,
            &hlc_guard,
            &state.column_sig_key_cache,
        )
        .map_err(|e| ExtensionError::Database { source: e })?;
        total_deleted += 1;
    }

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

    let perm_result = PermissionManager::check_spaces_permission(
        &state,
        &Principal::Extension(extension_id.clone()),
        SpaceAction::Read,
    )
    .await;
    prompt_on_err(&app_handle, perm_result)?;

    let extension = state
        .extension_manager
        .get_extension(&extension_id)
        .ok_or_else(|| ExtensionError::ValidationError {
            reason: format!("Extension with ID {} not found", extension_id),
        })?;

    let prefix =
        get_extension_table_prefix(&extension.manifest.public_key, &extension.manifest.name);

    validate_single_table_prefix(&table_name, &prefix)?;

    let (sql, params) = match &row_pks {
        Some(pks) if !pks.is_empty() => {
            let placeholders: Vec<String> =
                (2..=pks.len() + 1).map(|i| format!("?{}", i)).collect();
            let sql = format!(
                "{} WHERE table_name = ?1 AND row_pks IN ({})",
                *SQL_SHARED_SPACE_SYNC_SELECT_COLS,
                placeholders.join(", ")
            );
            let mut params = vec![serde_json::Value::String(table_name.clone())];
            for pk in pks {
                params.push(serde_json::Value::String(pk.clone()));
            }
            (sql, params)
        }
        _ => {
            let sql = format!(
                "{} WHERE table_name = ?1",
                *SQL_SHARED_SPACE_SYNC_SELECT_COLS
            );
            (sql, vec![serde_json::Value::String(table_name.clone())])
        }
    };

    let raw_rows = core::select_with_crdt(sql, params, &state.db)
        .map_err(|e| ExtensionError::Database { source: e })?;

    let rows: Vec<SpaceAssignmentRow> = raw_rows
        .iter()
        .map(|row| SpaceAssignmentRow {
            id: get_string(row, 0),
            table_name: get_string(row, 1),
            row_pks: get_string(row, 2),
            space_id: get_string(row, 3),
            extension_public_key: Some(get_string(row, 4)).filter(|s| !s.is_empty()),
            extension_name: Some(get_string(row, 5)).filter(|s| !s.is_empty()),
            category: Some(get_string(row, 6)).filter(|s| !s.is_empty()),
            type_name: Some(get_string(row, 7)).filter(|s| !s.is_empty()),
            type_label: Some(get_string(row, 8)).filter(|s| !s.is_empty()),
            created_at: Some(get_string(row, 9)).filter(|s| !s.is_empty()),
        })
        .collect();

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
    pub origin_url: String,
    pub created_at: String,
    pub capabilities: Vec<String>,
}

/// List all spaces from the local database.
///
/// Returns both local and remote spaces — no server fetch needed.
/// Includes the current user's capabilities per space (from UCAN tokens).
#[tauri::command]
pub async fn extension_space_list(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<Vec<DecryptedSpace>, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    let perm_result = PermissionManager::check_spaces_permission(
        &state,
        &Principal::Extension(extension_id.clone()),
        SpaceAction::Read,
    )
    .await;
    prompt_on_err(&app_handle, perm_result)?;

    let rows = core::select_with_crdt(
        "SELECT s.id, s.name, s.origin_url, s.created_at, \
                GROUP_CONCAT(DISTINCT t.capability) as capabilities \
         FROM haex_spaces s \
         LEFT JOIN haex_ucan_tokens t ON t.space_id = s.id \
           AND (t.audience_did IN (SELECT did FROM haex_identities WHERE private_key IS NOT NULL) \
                OR t.issuer_did IN (SELECT did FROM haex_identities WHERE private_key IS NOT NULL)) \
         GROUP BY s.id"
            .to_string(),
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
        .map(|row| {
            let caps_str = get_string(row, 4);
            let capabilities = if caps_str.is_empty() {
                vec![]
            } else {
                caps_str.split(',').map(|s| s.to_string()).collect()
            };
            DecryptedSpace {
                id: get_string(row, 0),
                name: get_string(row, 1),
                origin_url: get_string(row, 2),
                created_at: get_string(row, 3),
                capabilities,
            }
        })
        .collect();

    Ok(spaces)
}

/// A member of a shared space.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceMemberOut {
    pub did: String,
    /// `haex_identities.name`
    pub label: String,
    #[serde(rename = "isSelf")]
    pub is_self: bool,
}

/// List the members of a shared space with their display name and whether
/// each member is a local identity (`private_key IS NOT NULL`).
#[tauri::command]
pub async fn extension_space_get_members(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    space_id: String,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<Vec<SpaceMemberOut>, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    let perm_result = PermissionManager::check_spaces_permission(
        &state,
        &Principal::Extension(extension_id.clone()),
        SpaceAction::Read,
    )
    .await;
    prompt_on_err(&app_handle, perm_result)?;

    let rows = core::select_with_crdt(
        SQL_SELECT_SPACE_MEMBERS_WITH_IDENTITY.clone(),
        vec![serde_json::Value::String(space_id)],
        &state.db,
    )
    .map_err(|e| ExtensionError::Database { source: e })?;

    let members: Vec<SpaceMemberOut> = rows
        .iter()
        .map(|row| SpaceMemberOut {
            did: get_string(row, 0),
            label: get_string(row, 1),
            is_self: get_bool(row, 2),
        })
        .collect();

    Ok(members)
}
