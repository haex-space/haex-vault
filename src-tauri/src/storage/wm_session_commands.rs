//! Tauri commands for the opt-in session restore (spec 022-session-restore,
//! [contracts/wm-session.md](../../../specs/022-session-restore/contracts/wm-session.md)).
//! Since spec 023 (FR-024) the setting applies to the whole vault: one value,
//! on or off; the saved session itself stays per device.
//!
//! Each `#[tauri::command]` resolves the active database and the current
//! device, then delegates to a plain `async fn` that the tests call directly.
//! No command takes a device UUID from the frontend: a caller passing a
//! foreign one could otherwise read or delete another device's session.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager, State};
use ts_rs::TS;
use uuid::Uuid;

use crate::error::{HaexError, Result};
use crate::identity::{installation_id_path, read_or_mint_installation_uuid};
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::known_devices;
use crate::storage::preferences::{self, PrefScope};
use crate::storage::query::Query;
use crate::storage::wm_session::{self, WmSessionError};
use crate::vault_gate::VaultDb;

/// Preference key of the setting "Sitzung wiederherstellen" (data-model.md).
pub const SESSION_RESTORE_KEY: &str = "wm.session_restore";

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

/// Whether the session is saved and restored, for every device of the vault.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct SessionRestoreState {
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct SessionRestoreSetArgs {
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct WmSessionLoad {
    pub restore: SessionRestoreState,
    /// The saved session as the frontend wrote it, validated there.
    #[ts(type = "unknown")]
    pub session: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct WmSessionSaveArgs {
    #[ts(type = "unknown")]
    pub session: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct WmSessionSaved {
    pub saved: bool,
}

// ---------------------------------------------------------------------------
// Device resolution
// ---------------------------------------------------------------------------

/// Resolves the current device's `vault_device_uuid` from the installation
/// id, the same pair `current_device_info` uses.
pub(crate) fn current_device_uuid<R: tauri::Runtime>(
    app: &AppHandle<R>,
    db: &VaultDb,
) -> Result<Uuid> {
    let installation_id_file =
        installation_id_path(&app.path().app_local_data_dir().map_err(|e| {
            HaexError::PathResolution {
                reason: format!("app_local_data_dir: {e}"),
            }
        })?);
    let installation_uuid =
        read_or_mint_installation_uuid(&installation_id_file).map_err(HaexError::from)?;
    db.read_blocking(|r| known_devices::get_vault_device_uuid(r, installation_uuid))?
        .ok_or_else(|| HaexError::InvalidInput {
            reason: "current device is not registered in known_devices".to_string(),
        })
}

// ---------------------------------------------------------------------------
// Core logic (testable without a live Tauri State/AppHandle)
// ---------------------------------------------------------------------------

fn session_error_to_haex(err: WmSessionError) -> HaexError {
    match err {
        WmSessionError::Crdt(e) => HaexError::from(e),
        WmSessionError::TooLarge { bytes } => HaexError::SessionTooLarge { bytes },
        other => HaexError::InvalidInput {
            reason: other.to_string(),
        },
    }
}

/// The vault value; unset or unreadable means off (spec 022 FR-001).
fn restore_enabled(q: &mut impl Query) -> haex_crdt::Result<bool> {
    let value = preferences::get(q, PrefScope::Vault, SESSION_RESTORE_KEY)?;
    Ok(preferences::parse_bool(value.as_deref()).unwrap_or(false))
}

async fn restore_get(db: VaultDb) -> Result<SessionRestoreState> {
    let enabled = db.read(|r| restore_enabled(r)).await?;
    Ok(SessionRestoreState { enabled })
}

/// Writes the vault value and, in the same transaction, deletes this
/// device's saved session when turned off (FR-005, FR-007): no save can slip
/// in between. Other devices delete theirs on their next load (FR-008).
async fn restore_set(
    db: VaultDb,
    device: Uuid,
    args: SessionRestoreSetArgs,
) -> Result<SessionRestoreState> {
    db.write(move |tx| {
        let value = if args.enabled { "true" } else { "false" };
        preferences::insert_or_update(tx, PrefScope::Vault, SESSION_RESTORE_KEY, value)?;
        if !args.enabled {
            wm_session::delete(tx, device)?;
        }
        Ok(())
    })
    .await?;
    Ok(SessionRestoreState {
        enabled: args.enabled,
    })
}

/// Returns the setting and, when it applies, the saved session. When it
/// does not apply, a leftover session is deleted first (FR-008, FR-012).
async fn session_load(db: VaultDb, device: Uuid) -> Result<WmSessionLoad> {
    let (enabled, session) = db
        .write(move |tx| {
            if !restore_enabled(tx)? {
                wm_session::delete(tx, device)?;
                return Ok((false, None));
            }
            let session = wm_session::load(tx, device).map_err(session_error_to_haex)?;
            Ok((true, session))
        })
        .await?;
    Ok(WmSessionLoad {
        restore: SessionRestoreState { enabled },
        session,
    })
}

/// Writes the session only while restore applies, so a late debounced save
/// after turning it off creates nothing.
async fn session_save(db: VaultDb, device: Uuid, session: Value) -> Result<WmSessionSaved> {
    let saved = db
        .write(move |tx| {
            if !restore_enabled(tx)? {
                return Ok(false);
            }
            wm_session::save(tx, device, &session).map_err(session_error_to_haex)?;
            Ok(true)
        })
        .await?;
    Ok(WmSessionSaved { saved })
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn wm_session_restore_get(state: State<'_, AppState>) -> Result<SessionRestoreState> {
    restore_get(active_database(&state)?).await
}

#[tauri::command]
pub async fn wm_session_restore_set(
    app: AppHandle,
    state: State<'_, AppState>,
    args: SessionRestoreSetArgs,
) -> Result<SessionRestoreState> {
    let db = active_database(&state)?;
    let device = current_device_uuid(&app, &db)?;
    restore_set(db, device, args).await
}

#[tauri::command]
pub async fn wm_session_load(app: AppHandle, state: State<'_, AppState>) -> Result<WmSessionLoad> {
    let db = active_database(&state)?;
    let device = current_device_uuid(&app, &db)?;
    session_load(db, device).await
}

#[tauri::command]
pub async fn wm_session_save(
    app: AppHandle,
    state: State<'_, AppState>,
    args: WmSessionSaveArgs,
) -> Result<WmSessionSaved> {
    let db = active_database(&state)?;
    let device = current_device_uuid(&app, &db)?;
    session_save(db, device, args.session).await
}

#[cfg(test)]
#[path = "wm_session_commands_tests.rs"]
mod tests;
