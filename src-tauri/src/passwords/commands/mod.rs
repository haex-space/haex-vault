//! Tauri bridge commands for the Core Passwords API.
//!
//! Extensions access the password vault only through these commands — direct
//! access to the `haex_passwords_*` system tables is forbidden by policy.
//!
//! Permission model:
//!   - Declared via ExtensionPermission { resource: passwords, action, target }
//!   - `action`: Read | ReadWrite
//!   - `target`: Tag filter. "*" = all tags; otherwise an exact tag name.
//!     Multiple permissions are OR-ed (union of tags).
//!
//! Write-side tag enforcement: on create/update commands the submitted item
//! MUST carry at least one tag within the extension's scope, otherwise the
//! write is rejected (SecurityViolation). This prevents an extension from
//! tagging items out of its own reach.

use crate::database::core::{execute_with_crdt, select_with_crdt};
use crate::database::error::DatabaseError;
use crate::database::row::get_string;
use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::{PasswordsAction, PasswordsScope, Principal};
use crate::extension::utils::{emit_permission_prompt_if_needed, resolve_extension_id};
use crate::AppState;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use tauri::{AppHandle, State, WebviewWindow};
use ts_rs::TS;

pub mod helpers;
pub(crate) use helpers::*;

/// Lean view of a password item for lists.
///
/// Does NOT include secret fields (password, otpSecret, private_key, ...).
/// Full details require a separate `extension_password_read` call, which
/// allows the core to audit per-record reads.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PasswordItemSummary {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// List password items visible to the calling extension.
///
/// Returned items are already filtered by the permission's tag scope — an
/// extension granted `read target=calendar` sees only items with tag
/// "calendar", and never learns about the existence of others.
#[tauri::command]
pub async fn extension_password_list(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<Vec<PasswordItemSummary>, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    let perm_result = PermissionManager::check_passwords_permission(
        &state,
        &Principal::Extension(extension_id.clone()),
        PasswordsAction::Read,
    )
    .await;
    if let Err(ref e) = perm_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    let scope = perm_result?;

    let (sql, params) = build_list_query(&scope);

    let rows = select_with_crdt(sql, params, &state.db).map_err(|e| ExtensionError::Database {
        source: DatabaseError::DatabaseError {
            reason: e.to_string(),
        },
    })?;

    let summaries: Vec<PasswordItemSummary> = rows
        .iter()
        .map(|row| {
            let tags_str = get_string(row, 8);
            let tags = if tags_str.is_empty() {
                vec![]
            } else {
                tags_str.split(',').map(|s| s.to_string()).collect()
            };
            PasswordItemSummary {
                id: get_string(row, 0),
                title: non_empty(get_string(row, 1)),
                username: non_empty(get_string(row, 2)),
                url: non_empty(get_string(row, 3)),
                icon: non_empty(get_string(row, 4)),
                color: non_empty(get_string(row, 5)),
                created_at: non_empty(get_string(row, 6)),
                updated_at: non_empty(get_string(row, 7)),
                tags,
            }
        })
        .collect();

    Ok(summaries)
}

/// Full password item with relations, returned by `extension_password_read`.
///
/// Includes secret fields (password, otp_secret). Attachments and passkeys
/// have their own bridge commands and are NOT returned here to avoid pulling
/// large base64 blobs into every read.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PasswordItemFull {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub otp_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otp_digits: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otp_period: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otp_algorithm: Option<String>,

    /// Maps canonical field names to autofill aliases for browser matching.
    /// e.g. `{ "username": ["email", "login"], "password": ["pass"] }`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub autofill_aliases: Option<HashMap<String, Vec<String>>>,

    pub tags: Vec<String>,
    pub key_values: Vec<PasswordKeyValue>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PasswordKeyValue {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// Read a password item by id with full details.
///
/// The scope check is combined with the id lookup in a single WHERE clause —
/// if the item does not exist OR is outside the extension's tag scope, the
/// result is identical ("not found"). Existence of out-of-scope items is
/// never disclosed.
#[tauri::command]
pub async fn extension_password_read(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    item_id: String,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<PasswordItemFull, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    let perm_result = PermissionManager::check_passwords_permission(
        &state,
        &Principal::Extension(extension_id.clone()),
        PasswordsAction::Read,
    )
    .await;
    if let Err(ref e) = perm_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    let scope = perm_result?;

    let item_rows = {
        let (sql, params) = build_read_item_query(&scope, &item_id);
        select_with_crdt(sql, params, &state.db).map_err(|e| ExtensionError::Database {
            source: DatabaseError::DatabaseError {
                reason: e.to_string(),
            },
        })?
    };

    let row = item_rows
        .first()
        .ok_or_else(|| ExtensionError::ValidationError {
            reason: format!("Password item {} not found", item_id),
        })?;

    let tags = read_item_tags(&state, &item_id)?;
    let key_values = read_item_key_values(&state, &item_id)?;

    Ok(PasswordItemFull {
        id: get_string(row, 0),
        title: non_empty(get_string(row, 1)),
        username: non_empty(get_string(row, 2)),
        password: non_empty(get_string(row, 3)),
        note: non_empty(get_string(row, 4)),
        icon: non_empty(get_string(row, 5)),
        color: non_empty(get_string(row, 6)),
        url: non_empty(get_string(row, 7)),
        otp_secret: non_empty(get_string(row, 8)),
        otp_digits: get_i64_opt(row, 9),
        otp_period: get_i64_opt(row, 10),
        otp_algorithm: non_empty(get_string(row, 11)),
        autofill_aliases: get_autofill_aliases(row, 12),
        expires_at: non_empty(get_string(row, 13)),
        created_at: non_empty(get_string(row, 14)),
        updated_at: non_empty(get_string(row, 15)),
        tags,
        key_values,
    })
}

// =============================================================================
// Write side: create / update
// =============================================================================

/// Input for create & update operations.
///
/// `tags` is required and must contain at least one tag within the
/// extension's permission scope (variant B enforcement). Items outside the
/// extension's own scope cannot be created — the write is rejected.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PasswordInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otp_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otp_digits: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otp_period: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otp_algorithm: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub autofill_aliases: Option<HashMap<String, Vec<String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub tags: Vec<String>,
    #[serde(default)]
    pub key_values: Vec<PasswordKeyValueInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PasswordKeyValueInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// Create a new password item. Returns the generated item id.
#[tauri::command]
pub async fn extension_password_create(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    input: PasswordInput,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<String, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    let perm_result = PermissionManager::check_passwords_permission(
        &state,
        &Principal::Extension(extension_id.clone()),
        PasswordsAction::ReadWrite,
    )
    .await;
    if let Err(ref e) = perm_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    let scope = perm_result?;

    // Validate the caller's SUBMITTED tags first, so an out-of-scope tag is
    // still rejected exactly as before — the default-label injection below must
    // not mask a no-overlap submission. The one relaxation: a scoped (Tags)
    // extension may submit no tags, because the default label is injected next
    // and guarantees ≥1 in-scope tag. All scope keeps requiring ≥1 tag.
    if !(input.tags.is_empty() && matches!(scope, PasswordsScope::Tags { .. })) {
        validate_tags_in_scope(&input.tags, &scope)?;
    }

    // Inject the scope's default label (the explicitly resolved default) so
    // every entry a scoped extension creates lands within its own reach. For
    // All scope this is a passthrough.
    let tags = resolve_create_tags(&input.tags, &scope);

    let item_id = uuid::Uuid::new_v4().to_string();
    let hlc = lock_hlc(&state, "passwords::commands::extension_password_create")?;

    insert_item_row(&state, &hlc, &item_id, &input)?;
    upsert_and_link_tags(&state, &hlc, &item_id, &tags)?;
    insert_key_values(&state, &hlc, &item_id, &input.key_values)?;

    Ok(item_id)
}

/// Update an existing password item. Scope enforcement applies to both the
/// existing item (must be in scope) AND the new tag set (must keep ≥1 tag
/// in scope — extensions cannot "orphan" an item out of their own reach).
#[tauri::command]
pub async fn extension_password_update(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    item_id: String,
    input: PasswordInput,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<(), ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    let perm_result = PermissionManager::check_passwords_permission(
        &state,
        &Principal::Extension(extension_id.clone()),
        PasswordsAction::ReadWrite,
    )
    .await;
    if let Err(ref e) = perm_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    let scope = perm_result?;

    ensure_item_in_scope(&state, &item_id, &scope)?;
    validate_tags_in_scope(&input.tags, &scope)?;

    let hlc = lock_hlc(&state, "passwords::commands::extension_password_update")?;

    update_item_row(&state, &hlc, &item_id, &input)?;

    // Replace tag links and key-values wholesale. A CRDT-aware diff would be
    // more efficient but correctness comes first; optimize once profiling
    // shows it matters.
    delete_item_tag_links(&state, &hlc, &item_id)?;
    upsert_and_link_tags(&state, &hlc, &item_id, &input.tags)?;
    delete_key_values(&state, &hlc, &item_id)?;
    insert_key_values(&state, &hlc, &item_id, &input.key_values)?;

    Ok(())
}

// =============================================================================
// delete
// =============================================================================

/// Delete a password item by id.
///
/// The item must be in the extension's tag scope, otherwise the call fails
/// with "not found" — same indistinguishable-existence semantics as read.
/// Child rows (tags links, key-values, binaries, snapshots, passkeys) are
/// removed by the foreign-key cascades declared in the schema.
#[tauri::command]
pub async fn extension_password_delete(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    item_id: String,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<(), ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    let perm_result = PermissionManager::check_passwords_permission(
        &state,
        &Principal::Extension(extension_id.clone()),
        PasswordsAction::ReadWrite,
    )
    .await;
    if let Err(ref e) = perm_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    let scope = perm_result?;

    ensure_item_in_scope(&state, &item_id, &scope)?;

    let hlc = lock_hlc(&state, "passwords::commands::extension_password_delete")?;
    execute_with_crdt(
        "DELETE FROM haex_passwords_item_details WHERE id = ?1".to_string(),
        vec![JsonValue::String(item_id)],
        &state.db,
        &hlc,
    )
    .map_err(|e| ExtensionError::Database { source: e })?;

    Ok(())
}
