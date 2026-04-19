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

use crate::database::core::select_with_crdt;
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
