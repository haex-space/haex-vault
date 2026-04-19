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
//! Write-side tag enforcement: on create/update commands (to be added in a
//! follow-up commit) the submitted item MUST carry at least one tag within
//! the extension's scope, otherwise the write is rejected.

use crate::database::core::{execute_with_crdt, select_with_crdt};
use crate::database::error::DatabaseError;
use crate::database::row::get_string;
use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::{PasswordsAction, PasswordsScope};
use crate::extension::utils::{emit_permission_prompt_if_needed, resolve_extension_id};
use crate::AppState;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use tauri::{AppHandle, State, WebviewWindow};
use ts_rs::TS;

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

    let perm_result =
        PermissionManager::check_passwords_permission(&state, &extension_id, PasswordsAction::Read)
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

fn non_empty(s: String) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn get_i64_opt(row: &[JsonValue], idx: usize) -> Option<i64> {
    row.get(idx).and_then(|v| v.as_i64())
}

fn get_autofill_aliases(row: &[JsonValue], idx: usize) -> Option<HashMap<String, Vec<String>>> {
    match row.get(idx) {
        Some(JsonValue::Null) | None => None,
        Some(JsonValue::String(s)) if !s.is_empty() => serde_json::from_str(s).ok(),
        _ => None,
    }
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

    let perm_result =
        PermissionManager::check_passwords_permission(&state, &extension_id, PasswordsAction::Read)
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

    let row = item_rows.first().ok_or_else(|| ExtensionError::ValidationError {
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

fn build_read_item_query(scope: &PasswordsScope, item_id: &str) -> (String, Vec<JsonValue>) {
    const COLS: &str = "id, title, username, password, note, icon, color, url, \
                        otp_secret, otp_digits, otp_period, otp_algorithm, \
                        autofill_aliases, expires_at, created_at, updated_at";

    match scope {
        PasswordsScope::All => (
            format!(
                "SELECT {} FROM haex_passwords_item_details WHERE id = ?1",
                COLS
            ),
            vec![JsonValue::String(item_id.to_string())],
        ),
        PasswordsScope::Tags(allowed_tags) => {
            let placeholders: Vec<String> =
                (2..=allowed_tags.len() + 1).map(|i| format!("?{}", i)).collect();
            let sql = format!(
                "SELECT {cols} FROM haex_passwords_item_details \
                 WHERE id = ?1 \
                   AND id IN ( \
                       SELECT DISTINCT scope_it.item_id \
                       FROM haex_passwords_item_tags scope_it \
                       INNER JOIN haex_passwords_tags scope_t \
                           ON scope_t.id = scope_it.tag_id \
                       WHERE scope_t.name IN ({placeholders}) \
                   )",
                cols = COLS,
                placeholders = placeholders.join(",")
            );
            let mut params = vec![JsonValue::String(item_id.to_string())];
            params.extend(allowed_tags.iter().map(|t| JsonValue::String(t.clone())));
            (sql, params)
        }
    }
}

fn read_item_tags(
    state: &State<'_, AppState>,
    item_id: &str,
) -> Result<Vec<String>, ExtensionError> {
    let sql = "SELECT t.name \
               FROM haex_passwords_item_tags it \
               INNER JOIN haex_passwords_tags t ON t.id = it.tag_id \
               WHERE it.item_id = ?1"
        .to_string();
    let rows = select_with_crdt(sql, vec![JsonValue::String(item_id.to_string())], &state.db)
        .map_err(|e| ExtensionError::Database {
            source: DatabaseError::DatabaseError {
                reason: e.to_string(),
            },
        })?;
    Ok(rows.iter().map(|r| get_string(r, 0)).collect())
}

fn read_item_key_values(
    state: &State<'_, AppState>,
    item_id: &str,
) -> Result<Vec<PasswordKeyValue>, ExtensionError> {
    let sql = "SELECT id, key, value FROM haex_passwords_item_key_values \
               WHERE item_id = ?1"
        .to_string();
    let rows = select_with_crdt(sql, vec![JsonValue::String(item_id.to_string())], &state.db)
        .map_err(|e| ExtensionError::Database {
            source: DatabaseError::DatabaseError {
                reason: e.to_string(),
            },
        })?;
    Ok(rows
        .iter()
        .map(|r| PasswordKeyValue {
            id: get_string(r, 0),
            key: non_empty(get_string(r, 1)),
            value: non_empty(get_string(r, 2)),
        })
        .collect())
}

/// Builds the list-query SQL and parameters for a given tag scope.
///
/// Strategy: a subquery identifies items with at least one in-scope tag;
/// the outer query then LEFT-JOINs the FULL tag list per item, so the UI
/// receives complete tag context (not just the matching tags).
fn build_list_query(scope: &PasswordsScope) -> (String, Vec<JsonValue>) {
    const COLS: &str = "i.id, i.title, i.username, i.url, i.icon, i.color, \
                        i.created_at, i.updated_at, \
                        GROUP_CONCAT(DISTINCT t.name) as tag_names";

    match scope {
        PasswordsScope::All => {
            let sql = format!(
                "SELECT {cols} \
                 FROM haex_passwords_item_details i \
                 LEFT JOIN haex_passwords_item_tags it ON it.item_id = i.id \
                 LEFT JOIN haex_passwords_tags t ON t.id = it.tag_id \
                 GROUP BY i.id",
                cols = COLS
            );
            (sql, vec![])
        }
        PasswordsScope::Tags(allowed_tags) => {
            let placeholders: Vec<String> =
                (1..=allowed_tags.len()).map(|i| format!("?{}", i)).collect();
            let sql = format!(
                "SELECT {cols} \
                 FROM haex_passwords_item_details i \
                 LEFT JOIN haex_passwords_item_tags it ON it.item_id = i.id \
                 LEFT JOIN haex_passwords_tags t ON t.id = it.tag_id \
                 WHERE i.id IN ( \
                     SELECT DISTINCT scope_it.item_id \
                     FROM haex_passwords_item_tags scope_it \
                     INNER JOIN haex_passwords_tags scope_t \
                         ON scope_t.id = scope_it.tag_id \
                     WHERE scope_t.name IN ({placeholders}) \
                 ) \
                 GROUP BY i.id",
                cols = COLS,
                placeholders = placeholders.join(",")
            );
            let params = allowed_tags
                .iter()
                .map(|t| JsonValue::String(t.clone()))
                .collect();
            (sql, params)
        }
    }
}

// =============================================================================
// Write side: store / update
// =============================================================================

/// Input for create & update operations.
///
/// `tags` is required and must contain at least one tag within the
/// extension's permission scope (variant B enforcement). Items outside the
/// extension's own scope cannot be created — the write is rejected.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PasswordStoreInput {
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
pub async fn extension_password_store(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    input: PasswordStoreInput,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<String, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    let perm_result = PermissionManager::check_passwords_permission(
        &state,
        &extension_id,
        PasswordsAction::ReadWrite,
    )
    .await;
    if let Err(ref e) = perm_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    let scope = perm_result?;

    validate_tags_in_scope(&input.tags, &scope)?;

    let item_id = uuid::Uuid::new_v4().to_string();
    let hlc = lock_hlc(&state)?;

    insert_item_row(&state, &hlc, &item_id, &input)?;
    upsert_and_link_tags(&state, &hlc, &item_id, &input.tags)?;
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
    input: PasswordStoreInput,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<(), ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    let perm_result = PermissionManager::check_passwords_permission(
        &state,
        &extension_id,
        PasswordsAction::ReadWrite,
    )
    .await;
    if let Err(ref e) = perm_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    let scope = perm_result?;

    ensure_item_in_scope(&state, &item_id, &scope)?;
    validate_tags_in_scope(&input.tags, &scope)?;

    let hlc = lock_hlc(&state)?;

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

// --- Internal helpers -------------------------------------------------------

fn validate_tags_in_scope(tags: &[String], scope: &PasswordsScope) -> Result<(), ExtensionError> {
    if tags.is_empty() {
        return Err(ExtensionError::ValidationError {
            reason: "At least one tag is required for a password entry".to_string(),
        });
    }
    match scope {
        PasswordsScope::All => Ok(()),
        PasswordsScope::Tags(allowed) => {
            if tags.iter().any(|t| allowed.contains(t)) {
                Ok(())
            } else {
                Err(ExtensionError::SecurityViolation {
                    reason: format!(
                        "At least one submitted tag must be within the extension's scope \
                         (allowed tags: {:?})",
                        allowed
                    ),
                })
            }
        }
    }
}

fn ensure_item_in_scope(
    state: &State<'_, AppState>,
    item_id: &str,
    scope: &PasswordsScope,
) -> Result<(), ExtensionError> {
    let (sql, params) = build_read_item_query(scope, item_id);
    // We only care about existence, not the row contents — reuse the read
    // query (which already combines id lookup + scope check in one WHERE).
    let rows = select_with_crdt(sql, params, &state.db).map_err(|e| ExtensionError::Database {
        source: DatabaseError::DatabaseError {
            reason: e.to_string(),
        },
    })?;
    if rows.is_empty() {
        return Err(ExtensionError::ValidationError {
            reason: format!("Password item {} not found", item_id),
        });
    }
    Ok(())
}

fn lock_hlc<'a>(
    state: &'a State<'_, AppState>,
) -> Result<std::sync::MutexGuard<'a, crate::crdt::hlc::HlcService>, ExtensionError> {
    state.hlc.lock().map_err(|_| ExtensionError::Database {
        source: DatabaseError::MutexPoisoned {
            reason: "HLC lock poisoned".to_string(),
        },
    })
}

fn insert_item_row(
    state: &State<'_, AppState>,
    hlc: &std::sync::MutexGuard<crate::crdt::hlc::HlcService>,
    item_id: &str,
    input: &PasswordStoreInput,
) -> Result<(), ExtensionError> {
    let sql = "INSERT INTO haex_passwords_item_details \
               (id, title, username, password, note, icon, color, url, \
                otp_secret, otp_digits, otp_period, otp_algorithm, \
                autofill_aliases, expires_at) \
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)"
        .to_string();
    let params = vec![
        JsonValue::String(item_id.to_string()),
        opt_str_param(&input.title),
        opt_str_param(&input.username),
        opt_str_param(&input.password),
        opt_str_param(&input.note),
        opt_str_param(&input.icon),
        opt_str_param(&input.color),
        opt_str_param(&input.url),
        opt_str_param(&input.otp_secret),
        opt_i64_param(input.otp_digits),
        opt_i64_param(input.otp_period),
        opt_str_param(&input.otp_algorithm),
        serialize_aliases(&input.autofill_aliases),
        opt_str_param(&input.expires_at),
    ];
    execute_with_crdt(sql, params, &state.db, hlc).map_err(|e| ExtensionError::Database {
        source: e,
    })?;
    Ok(())
}

fn update_item_row(
    state: &State<'_, AppState>,
    hlc: &std::sync::MutexGuard<crate::crdt::hlc::HlcService>,
    item_id: &str,
    input: &PasswordStoreInput,
) -> Result<(), ExtensionError> {
    let sql = "UPDATE haex_passwords_item_details SET \
               title = ?2, username = ?3, password = ?4, note = ?5, icon = ?6, \
               color = ?7, url = ?8, otp_secret = ?9, otp_digits = ?10, \
               otp_period = ?11, otp_algorithm = ?12, autofill_aliases = ?13, \
               expires_at = ?14, updated_at = CURRENT_TIMESTAMP \
               WHERE id = ?1"
        .to_string();
    let params = vec![
        JsonValue::String(item_id.to_string()),
        opt_str_param(&input.title),
        opt_str_param(&input.username),
        opt_str_param(&input.password),
        opt_str_param(&input.note),
        opt_str_param(&input.icon),
        opt_str_param(&input.color),
        opt_str_param(&input.url),
        opt_str_param(&input.otp_secret),
        opt_i64_param(input.otp_digits),
        opt_i64_param(input.otp_period),
        opt_str_param(&input.otp_algorithm),
        serialize_aliases(&input.autofill_aliases),
        opt_str_param(&input.expires_at),
    ];
    execute_with_crdt(sql, params, &state.db, hlc).map_err(|e| ExtensionError::Database {
        source: e,
    })?;
    Ok(())
}

/// For each tag name: look up its id, inserting a new tag row if missing.
/// Then link it to the item via `haex_passwords_item_tags`.
fn upsert_and_link_tags(
    state: &State<'_, AppState>,
    hlc: &std::sync::MutexGuard<crate::crdt::hlc::HlcService>,
    item_id: &str,
    tag_names: &[String],
) -> Result<(), ExtensionError> {
    for name in tag_names {
        let tag_id = upsert_tag(state, hlc, name)?;
        let link_id = uuid::Uuid::new_v4().to_string();
        let sql = "INSERT OR IGNORE INTO haex_passwords_item_tags (id, item_id, tag_id) \
                   VALUES (?1, ?2, ?3)"
            .to_string();
        let params = vec![
            JsonValue::String(link_id),
            JsonValue::String(item_id.to_string()),
            JsonValue::String(tag_id),
        ];
        execute_with_crdt(sql, params, &state.db, hlc).map_err(|e| {
            ExtensionError::Database { source: e }
        })?;
    }
    Ok(())
}

fn upsert_tag(
    state: &State<'_, AppState>,
    hlc: &std::sync::MutexGuard<crate::crdt::hlc::HlcService>,
    name: &str,
) -> Result<String, ExtensionError> {
    let rows = select_with_crdt(
        "SELECT id FROM haex_passwords_tags WHERE name = ?1".to_string(),
        vec![JsonValue::String(name.to_string())],
        &state.db,
    )
    .map_err(|e| ExtensionError::Database {
        source: DatabaseError::DatabaseError {
            reason: e.to_string(),
        },
    })?;
    if let Some(r) = rows.first() {
        return Ok(get_string(r, 0));
    }
    let new_id = uuid::Uuid::new_v4().to_string();
    execute_with_crdt(
        "INSERT INTO haex_passwords_tags (id, name) VALUES (?1, ?2)".to_string(),
        vec![
            JsonValue::String(new_id.clone()),
            JsonValue::String(name.to_string()),
        ],
        &state.db,
        hlc,
    )
    .map_err(|e| ExtensionError::Database { source: e })?;
    Ok(new_id)
}

fn delete_item_tag_links(
    state: &State<'_, AppState>,
    hlc: &std::sync::MutexGuard<crate::crdt::hlc::HlcService>,
    item_id: &str,
) -> Result<(), ExtensionError> {
    execute_with_crdt(
        "DELETE FROM haex_passwords_item_tags WHERE item_id = ?1".to_string(),
        vec![JsonValue::String(item_id.to_string())],
        &state.db,
        hlc,
    )
    .map_err(|e| ExtensionError::Database { source: e })?;
    Ok(())
}

fn insert_key_values(
    state: &State<'_, AppState>,
    hlc: &std::sync::MutexGuard<crate::crdt::hlc::HlcService>,
    item_id: &str,
    key_values: &[PasswordKeyValueInput],
) -> Result<(), ExtensionError> {
    for kv in key_values {
        let kv_id = uuid::Uuid::new_v4().to_string();
        execute_with_crdt(
            "INSERT INTO haex_passwords_item_key_values (id, item_id, key, value) \
             VALUES (?1, ?2, ?3, ?4)"
                .to_string(),
            vec![
                JsonValue::String(kv_id),
                JsonValue::String(item_id.to_string()),
                opt_str_param(&kv.key),
                opt_str_param(&kv.value),
            ],
            &state.db,
            hlc,
        )
        .map_err(|e| ExtensionError::Database { source: e })?;
    }
    Ok(())
}

fn delete_key_values(
    state: &State<'_, AppState>,
    hlc: &std::sync::MutexGuard<crate::crdt::hlc::HlcService>,
    item_id: &str,
) -> Result<(), ExtensionError> {
    execute_with_crdt(
        "DELETE FROM haex_passwords_item_key_values WHERE item_id = ?1".to_string(),
        vec![JsonValue::String(item_id.to_string())],
        &state.db,
        hlc,
    )
    .map_err(|e| ExtensionError::Database { source: e })?;
    Ok(())
}

fn opt_str_param(v: &Option<String>) -> JsonValue {
    match v {
        Some(s) => JsonValue::String(s.clone()),
        None => JsonValue::Null,
    }
}

fn opt_i64_param(v: Option<i64>) -> JsonValue {
    match v {
        Some(n) => JsonValue::Number(n.into()),
        None => JsonValue::Null,
    }
}

fn serialize_aliases(v: &Option<HashMap<String, Vec<String>>>) -> JsonValue {
    match v {
        Some(map) => {
            JsonValue::String(serde_json::to_string(map).unwrap_or_else(|_| "{}".to_string()))
        }
        None => JsonValue::Null,
    }
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
        &extension_id,
        PasswordsAction::ReadWrite,
    )
    .await;
    if let Err(ref e) = perm_result {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    let scope = perm_result?;

    ensure_item_in_scope(&state, &item_id, &scope)?;

    let hlc = lock_hlc(&state)?;
    execute_with_crdt(
        "DELETE FROM haex_passwords_item_details WHERE id = ?1".to_string(),
        vec![JsonValue::String(item_id)],
        &state.db,
        &hlc,
    )
    .map_err(|e| ExtensionError::Database { source: e })?;

    Ok(())
}
