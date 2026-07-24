//! Internal helpers shared by the password Tauri commands.
//!
//! Most helpers are private to this module; the two `pub(crate)` ones
//! (`resolve_create_tags`, `validate_tags_in_scope`) are also consumed by
//! `crate::passwords::tests` and re-exported from the parent.

use super::{PasswordInput, PasswordKeyValue, PasswordKeyValueInput};
use crate::critical::CriticalFailureCode;
use crate::database::core::{execute_with_crdt, select_with_crdt};
use crate::database::error::DatabaseError;
use crate::database::row::get_string;
use crate::extension::error::ExtensionError;
use crate::extension::permissions::types::PasswordsScope;
use crate::AppState;

use serde_json::Value as JsonValue;
use std::collections::HashMap;
use tauri::State;

pub(super) fn non_empty(s: String) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

pub(super) fn get_i64_opt(row: &[JsonValue], idx: usize) -> Option<i64> {
    row.get(idx).and_then(|v| v.as_i64())
}

pub(super) fn get_autofill_aliases(
    row: &[JsonValue],
    idx: usize,
) -> Option<HashMap<String, Vec<String>>> {
    match row.get(idx) {
        Some(JsonValue::Null) | None => None,
        Some(JsonValue::String(s)) if !s.is_empty() => serde_json::from_str(s).ok(),
        _ => None,
    }
}

pub(super) fn build_read_item_query(
    scope: &PasswordsScope,
    item_id: &str,
) -> (String, Vec<JsonValue>) {
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
        PasswordsScope::Tags {
            tags: allowed_tags, ..
        } => {
            let placeholders: Vec<String> = (2..=allowed_tags.len() + 1)
                .map(|i| format!("?{}", i))
                .collect();
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

pub(super) fn read_item_tags(
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

pub(super) fn read_item_key_values(
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
pub(super) fn build_list_query(scope: &PasswordsScope) -> (String, Vec<JsonValue>) {
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
        PasswordsScope::Tags {
            tags: allowed_tags, ..
        } => {
            let placeholders: Vec<String> = (1..=allowed_tags.len())
                .map(|i| format!("?{}", i))
                .collect();
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

// --- Internal helpers -------------------------------------------------------

/// Computes the final tag set written for a newly created entry.
///
/// Every newly created entry receives the scope's *default label* (the
/// explicitly resolved default — see [`PasswordsScope::default_label`]) so it
/// always lands within the extension's own reach. The default is ALWAYS applied
/// — even when the caller passed other allowed tags, the entry ends up carrying
/// both. The default is never duplicated if the caller already passed it.
///
/// For `PasswordsScope::All` (unscoped) there is no default label, so the
/// caller's tags pass through unchanged. A `Tags` scope with no resolved
/// default (multi-label read-only) also injects nothing — but the create path
/// only reaches here for write scopes, where a default is guaranteed (or the
/// grant was already rejected at check time).
///
/// This helper only *injects*; it never drops caller tags. Out-of-scope tags
/// are rejected separately by [`validate_tags_in_scope`] before this runs.
pub(crate) fn resolve_create_tags(submitted: &[String], scope: &PasswordsScope) -> Vec<String> {
    let mut tags = submitted.to_vec();
    if let Some(default) = scope.default_label() {
        if !tags.iter().any(|t| t == default) {
            tags.push(default.to_string());
        }
    }
    tags
}

pub(crate) fn validate_tags_in_scope(
    tags: &[String],
    scope: &PasswordsScope,
) -> Result<(), ExtensionError> {
    if tags.is_empty() {
        return Err(ExtensionError::ValidationError {
            reason: "At least one tag is required for a password entry".to_string(),
        });
    }
    match scope {
        PasswordsScope::All => Ok(()),
        PasswordsScope::Tags { tags: allowed, .. } => {
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

pub(super) fn ensure_item_in_scope(
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

/// Per-callsite location string lets the banner UPSERT-dedup attribute the
/// poison to a specific public command (`extension_password_create` vs
/// `_update` vs `_delete`) instead of collapsing all three into the same
/// row. Pass a unique `&'static str` from each caller.
pub(super) fn lock_hlc<'a>(
    state: &'a State<'_, AppState>,
    location: &'static str,
) -> Result<std::sync::MutexGuard<'a, crate::crdt::hlc::HlcService>, ExtensionError> {
    state
        .lock_or_fail(
            &state.hlc,
            CriticalFailureCode::HlcMutexPoisoned,
            location,
            serde_json::json!({}),
        )
        .map_err(ExtensionError::from)
}

pub(super) fn insert_item_row(
    state: &State<'_, AppState>,
    hlc: &std::sync::MutexGuard<crate::crdt::hlc::HlcService>,
    item_id: &str,
    input: &PasswordInput,
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
    execute_with_crdt(sql, params, &state.db, hlc, &state.column_sig_key_cache)
        .map_err(|e| ExtensionError::Database { source: e })?;
    Ok(())
}

pub(super) fn update_item_row(
    state: &State<'_, AppState>,
    hlc: &std::sync::MutexGuard<crate::crdt::hlc::HlcService>,
    item_id: &str,
    input: &PasswordInput,
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
    execute_with_crdt(sql, params, &state.db, hlc, &state.column_sig_key_cache)
        .map_err(|e| ExtensionError::Database { source: e })?;
    Ok(())
}

/// For each tag name: look up its id, inserting a new tag row if missing.
/// Then link it to the item via `haex_passwords_item_tags`.
pub(super) fn upsert_and_link_tags(
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
        execute_with_crdt(sql, params, &state.db, hlc, &state.column_sig_key_cache)
            .map_err(|e| ExtensionError::Database { source: e })?;
    }
    Ok(())
}

pub(super) fn upsert_tag(
    state: &State<'_, AppState>,
    hlc: &std::sync::MutexGuard<crate::crdt::hlc::HlcService>,
    name: &str,
) -> Result<String, ExtensionError> {
    let new_id = uuid::Uuid::new_v4().to_string();
    execute_with_crdt(
        "INSERT OR IGNORE INTO haex_passwords_tags (id, name) VALUES (?1, ?2)".to_string(),
        vec![
            JsonValue::String(new_id),
            JsonValue::String(name.to_string()),
        ],
        &state.db,
        hlc,
        &state.column_sig_key_cache,
    )
    .map_err(|e| ExtensionError::Database { source: e })?;
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
    rows.first()
        .map(|r| get_string(r, 0))
        .ok_or_else(|| ExtensionError::Database {
            source: DatabaseError::DatabaseError {
                reason: format!("tag '{name}' not found after upsert"),
            },
        })
}

pub(super) fn delete_item_tag_links(
    state: &State<'_, AppState>,
    hlc: &std::sync::MutexGuard<crate::crdt::hlc::HlcService>,
    item_id: &str,
) -> Result<(), ExtensionError> {
    execute_with_crdt(
        "DELETE FROM haex_passwords_item_tags WHERE item_id = ?1".to_string(),
        vec![JsonValue::String(item_id.to_string())],
        &state.db,
        hlc,
        &state.column_sig_key_cache,
    )
    .map_err(|e| ExtensionError::Database { source: e })?;
    Ok(())
}

pub(super) fn insert_key_values(
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
            &state.column_sig_key_cache,
        )
        .map_err(|e| ExtensionError::Database { source: e })?;
    }
    Ok(())
}

pub(super) fn delete_key_values(
    state: &State<'_, AppState>,
    hlc: &std::sync::MutexGuard<crate::crdt::hlc::HlcService>,
    item_id: &str,
) -> Result<(), ExtensionError> {
    execute_with_crdt(
        "DELETE FROM haex_passwords_item_key_values WHERE item_id = ?1".to_string(),
        vec![JsonValue::String(item_id.to_string())],
        &state.db,
        hlc,
        &state.column_sig_key_cache,
    )
    .map_err(|e| ExtensionError::Database { source: e })?;
    Ok(())
}

pub(super) fn opt_str_param(v: &Option<String>) -> JsonValue {
    match v {
        Some(s) => JsonValue::String(s.clone()),
        None => JsonValue::Null,
    }
}

pub(super) fn opt_i64_param(v: Option<i64>) -> JsonValue {
    match v {
        Some(n) => JsonValue::Number(n.into()),
        None => JsonValue::Null,
    }
}

pub(super) fn serialize_aliases(v: &Option<HashMap<String, Vec<String>>>) -> JsonValue {
    match v {
        Some(map) => {
            JsonValue::String(serde_json::to_string(map).unwrap_or_else(|_| "{}".to_string()))
        }
        None => JsonValue::Null,
    }
}
