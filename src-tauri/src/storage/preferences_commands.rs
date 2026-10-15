//! Tauri command surface for `preferences`. See spec 002
//! `contracts/tauri-commands.md` for the wire shapes.
//!
//! `PrefScopeWire` is a wire-side discriminated union that maps to the
//! Rust `PrefScope` enum. Frontend sends
//! `{ kind: 'vault' } | { kind: 'device', uuid }`; the Rust side folds
//! it back into the storage enum.

use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::error::{HolziError, Result};
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::preferences::{self, PrefError, PrefScope};

/// Wire representation of `PrefScope`. Symmetric with the TypeScript
/// `PrefScope` type in `usePreferences.ts`.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PrefScopeWire {
    Vault,
    Device { uuid: Uuid },
}

impl TryFrom<PrefScopeWire> for PrefScope {
    type Error = HolziError;

    fn try_from(value: PrefScopeWire) -> std::result::Result<Self, Self::Error> {
        let scope = match value {
            PrefScopeWire::Vault => PrefScope::Vault,
            PrefScopeWire::Device { uuid } => PrefScope::Device(uuid),
        };
        preferences::validate_scope(scope).map_err(pref_error_to_holzi)?;
        Ok(scope)
    }
}

fn pref_error_to_holzi(err: PrefError) -> HolziError {
    HolziError::InvalidInput {
        reason: err.to_string(),
    }
}

fn validate_key(key: &str) -> Result<()> {
    preferences::validate_key(key).map_err(pref_error_to_holzi)
}

/// Spec 042: the workspace background, a WebP data URL scaled down in the frontend.
pub(crate) const BACKGROUND_KEY: &str = "appearance.background";
pub(crate) const BACKGROUND_PREFIX: &str = "data:image/webp;base64,";
/// Far above a 2560-px WebP (a few hundred KB) and below one sync page (`sync/change.rs`).
pub(crate) const BACKGROUND_MAX_BYTES: usize = 4 * 1024 * 1024;

/// A WebP data URL with nothing but base64 after the prefix, within the size limit.
fn is_background_value(value: &str) -> bool {
    value.len() <= BACKGROUND_MAX_BYTES
        && value.strip_prefix(BACKGROUND_PREFIX).is_some_and(|data| {
            !data.is_empty()
                && data
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='))
        })
}

/// Spec 044 (data-model.md): the file browser's device preferences.
fn is_valid_files_value(key: &str, value: &str) -> bool {
    match key {
        "files.view" => matches!(value, "list" | "grid"),
        "files.hidden" => matches!(value, "true" | "false"),
        // Spec 053: the icon set of the file browser, a vault-wide choice.
        crate::files::icons::SELECTION_KEY => crate::files::icons::is_valid_selection(value),
        "files.sort" => value.split_once(':').is_some_and(|(by, direction)| {
            matches!(by, "name" | "size" | "modified" | "type")
                && matches!(direction, "asc" | "desc")
        }),
        _ => true,
    }
}

/// Checks the value of a key that has a fixed set of choices or shape; every other key takes any
/// value. Spec 034: the clipboard clearing time of the password manager (0, 15, 30, 60 or 120
/// seconds). Spec 042: the workspace background.
pub(crate) fn validate_value(key: &str, value: &str) -> Result<()> {
    if key == crate::passwords::settings::CLIPBOARD_CLEAR_KEY
        && !crate::passwords::settings::is_valid_clear_seconds(value)
    {
        return Err(HolziError::InvalidInput {
            reason: format!("{key} must be one of 0, 15, 30, 60, 120"),
        });
    }
    if !is_valid_files_value(key, value) {
        return Err(HolziError::InvalidInput {
            reason: format!("{key} has no such value"),
        });
    }
    if key == BACKGROUND_KEY && !is_background_value(value) {
        return Err(HolziError::InvalidInput {
            reason: format!(
                "{key} must be a WebP data URL of at most {BACKGROUND_MAX_BYTES} bytes"
            ),
        });
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPrefArgs {
    pub scope: PrefScopeWire,
    pub key: String,
}

/// Reads a single preference. `None` covers both "row absent" and
/// "row present with SQL NULL value" (data-model.md).
#[tauri::command]
pub async fn get_pref(state: State<'_, AppState>, args: GetPrefArgs) -> Result<Option<String>> {
    validate_key(&args.key)?;
    let scope = PrefScope::try_from(args.scope)?;
    let key = args.key;
    let db = active_database(&state)?;
    db.read(move |r| preferences::get(r, scope, &key)).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetPrefArgs {
    pub scope: PrefScopeWire,
    pub key: String,
    pub value: String,
}

/// Inserts or updates a preference row.
#[tauri::command]
pub async fn set_pref(state: State<'_, AppState>, args: SetPrefArgs) -> Result<()> {
    validate_key(&args.key)?;
    validate_value(&args.key, &args.value)?;
    let scope = PrefScope::try_from(args.scope)?;
    let SetPrefArgs { key, value, .. } = args;
    let db = active_database(&state)?;
    db.write(move |tx| preferences::insert_or_update(tx, scope, &key, &value).map(|_| ()))
        .await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearPrefArgs {
    pub scope: PrefScopeWire,
    pub key: String,
}

/// Deletes a preference row. Idempotent — deleting an absent row is
/// not an error.
#[tauri::command]
pub async fn clear_pref(state: State<'_, AppState>, args: ClearPrefArgs) -> Result<()> {
    validate_key(&args.key)?;
    let scope = PrefScope::try_from(args.scope)?;
    let key = args.key;
    let db = active_database(&state)?;
    db.write(move |tx| preferences::delete(tx, scope, &key).map(|_| ()))
        .await
}
