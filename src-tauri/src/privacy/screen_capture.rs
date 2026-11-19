//! Screen capture protection (spec 043 FR-011a): while a vault is open, Android allows no
//! screenshots or recordings of holzi's window and shows an empty preview among the recent apps.
//! On by default; the person can switch it off for this device only (a device setting, like
//! developer mode in `extensions/dev.rs`), and the choice takes effect at once.

use tauri::{AppHandle, Runtime, State};
use tauri_plugin_haex_android::HaexAndroidExt;
use uuid::Uuid;

use crate::error::Result;
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::preferences::{self, PrefScope};
use crate::storage::query::Query;
use crate::storage::wm_session_commands::current_device_uuid;
use crate::vault_gate::VaultDb;

/// The device setting; a missing value counts as on.
pub const KEY: &str = "privacy.screenCaptureProtection";

/// Whether the protection is on for `device`.
pub fn protection(q: &mut impl Query, device: Uuid) -> Result<bool> {
    let value = preferences::get(q, PrefScope::Device(device), KEY)?;
    Ok(preferences::parse_bool(value.as_deref()).unwrap_or(true))
}

/// Switches the protection for `device`.
pub fn set_protection(
    tx: &mut haex_crdt::CrdtTransaction<'_>,
    device: Uuid,
    enabled: bool,
) -> Result<()> {
    preferences::insert_or_update(
        tx,
        PrefScope::Device(device),
        KEY,
        if enabled { "true" } else { "false" },
    )?;
    Ok(())
}

/// Hands the choice to the window, where the platform has the setting; a failure is logged (the
/// window then keeps what it had).
fn apply<R: Runtime>(app: &AppHandle<R>, enabled: bool) {
    if !crate::platform::capabilities().screen_capture {
        return;
    }
    if let Err(error) = app.haex_android().set_secure(enabled) {
        log::warn!("screen capture protection: {error}");
    }
}

/// Applies this device's choice for the vault just opened (create, open, import).
pub async fn apply_for_vault<R: Runtime>(app: &AppHandle<R>, db: &VaultDb) {
    if !crate::platform::capabilities().screen_capture {
        return;
    }
    let enabled = match current_device_uuid(app, db) {
        Ok(device) => db
            .read(move |q| protection(q, device).map_err(Into::into))
            .await
            .unwrap_or(true),
        // Without a known device the protection stays on, the safe side.
        Err(_) => true,
    };
    apply(app, enabled);
}

/// The protection on this device.
#[tauri::command]
pub async fn screen_capture_protection_get(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<bool> {
    let db = active_database(&state)?;
    let device = current_device_uuid(&app, &db)?;
    db.read(move |q| protection(q, device).map_err(Into::into))
        .await
}

/// Switches the protection on this device and applies it at once.
#[tauri::command]
pub async fn screen_capture_protection_set(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<bool> {
    let db = active_database(&state)?;
    let device = current_device_uuid(&app, &db)?;
    db.write(move |tx| set_protection(tx, device, enabled).map_err(Into::into))
        .await?;
    apply(&app, enabled);
    Ok(enabled)
}

#[cfg(test)]
#[path = "screen_capture_tests.rs"]
mod tests;
