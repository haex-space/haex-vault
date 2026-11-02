//! `current_device_info`, `update_device_alias` and `list_vault_devices` Tauri commands.
//!
//! See spec 002 [`contracts/tauri-commands.md`](../../../specs/002-onboarding-model-prefs/contracts/tauri-commands.md).
//! The device info command powers the onboarding-wizard's alias prefill,
//! the workspace-landing header, and the onboarded-middleware's
//! decision ("alias == null → route to /onboarding"). Alias-rename is
//! the settings-screen entry point and the wizard's final commit step.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

use crate::error::{HolziError, Result};
use crate::hardware::hostname;
use crate::identity::{installation_id_path, read_or_mint_installation_uuid, VAULT_SCOPE_UUID};
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::known_devices;
use crate::storage::query::Query;
use crate::sync::device_view::{self, VaultDevice};
use crate::sync::registry::SyncRegistry;
use crate::vault_gate::VaultDb;

/// Frontend view of the active device's identity + OS hostname.
///
/// `alias` is `None` before the onboarding wizard's final commit (that
/// is what triggers the onboarded-middleware redirect). `hostname` is
/// the raw `Option<String>` from `sysinfo`; the frontend substitutes
/// its localised fallback when it comes back `None`
/// (`onboarding.alias.defaultPlaceholder`). Both are camelCased on the
/// wire per the module-wide serde convention.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfoPayload {
    pub installation_uuid: Uuid,
    pub vault_device_uuid: Uuid,
    pub alias: Option<String>,
    pub hostname: Option<String>,
}

/// Returns the current device's identity for the active vault.
///
/// Requires an open vault — the payload joins the app-local
/// installation-id file with the vault-tracked `known_devices` row.
/// Errors as `NoActiveInstance` when no vault is open.
#[tauri::command]
pub async fn current_device_info(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<DeviceInfoPayload> {
    let installation_id_file =
        installation_id_path(&app.path().app_local_data_dir().map_err(|e| {
            HolziError::PathResolution {
                reason: format!("app_local_data_dir: {e}"),
            }
        })?);
    let installation_uuid =
        read_or_mint_installation_uuid(&installation_id_file).map_err(HolziError::from)?;

    let db = active_database(&state)?;
    let installation_for_query = installation_uuid;
    let row: Option<(String, Option<String>)> = db
        .read(move |r| {
            r.query_row(
                "SELECT vault_device_uuid, alias FROM known_devices \
                 WHERE installation_uuid = ?1",
                haex_crdt::rusqlite::params![installation_for_query.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
        })
        .await?;

    let (vault_device_uuid, alias) = row.ok_or_else(|| HolziError::CrdtInit {
        reason: format!("no known_devices row for installation {installation_uuid}"),
    })?;
    let vault_device_uuid =
        Uuid::parse_str(&vault_device_uuid).map_err(|e| HolziError::CrdtInit {
            reason: format!("stored vault_device_uuid parse: {e}"),
        })?;
    if vault_device_uuid == VAULT_SCOPE_UUID {
        return Err(HolziError::CrdtInit {
            reason: "installation UUID resolved to the vault-scope sentinel".into(),
        });
    }

    Ok(DeviceInfoPayload {
        installation_uuid,
        vault_device_uuid,
        alias,
        hostname: hostname::suggested_alias(),
    })
}

/// Resolves this installation's `known_devices.vault_device_uuid` for the
/// active vault — the UUID device-scoped preferences and other per-device
/// state key on. A lighter-weight sibling of [`current_device_info`] for
/// backend-internal callers (spec 010's `voice::resolve_local_adapter`)
/// that need "this device"'s identity without a frontend-supplied UUID and
/// don't need the alias/hostname `current_device_info` also returns.
pub async fn resolve_vault_device_uuid(app: &AppHandle, db: &VaultDb) -> Result<Uuid> {
    let installation_id_file =
        installation_id_path(&app.path().app_local_data_dir().map_err(|e| {
            HolziError::PathResolution {
                reason: format!("app_local_data_dir: {e}"),
            }
        })?);
    let installation_uuid =
        read_or_mint_installation_uuid(&installation_id_file).map_err(HolziError::from)?;

    let raw: Option<String> = db
        .read(move |r| {
            r.query_row(
                "SELECT vault_device_uuid FROM known_devices WHERE installation_uuid = ?1",
                haex_crdt::rusqlite::params![installation_uuid.to_string()],
                |row| row.get(0),
            )
        })
        .await?;

    let vault_device_uuid = raw.ok_or_else(|| HolziError::CrdtInit {
        reason: format!("no known_devices row for installation {installation_uuid}"),
    })?;
    let vault_device_uuid =
        Uuid::parse_str(&vault_device_uuid).map_err(|e| HolziError::CrdtInit {
            reason: format!("stored vault_device_uuid parse: {e}"),
        })?;
    if vault_device_uuid == VAULT_SCOPE_UUID {
        return Err(HolziError::CrdtInit {
            reason: "installation UUID resolved to the vault-scope sentinel".into(),
        });
    }
    Ok(vault_device_uuid)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDeviceAliasArgs {
    pub alias: String,
}

/// Sets or renames the alias on the active device's `known_devices` row.
///
/// Whitespace-trimmed; a resulting empty string is rejected as
/// `InvalidInput` so the onboarding wizard's "required" contract holds
/// through the backend as well.
#[tauri::command]
pub async fn update_device_alias(
    app: AppHandle,
    state: State<'_, AppState>,
    args: UpdateDeviceAliasArgs,
) -> Result<()> {
    let trimmed = args.alias.trim().to_string();
    if trimmed.is_empty() {
        return Err(HolziError::InvalidInput {
            reason: "alias is empty or whitespace-only".into(),
        });
    }

    let installation_id_file =
        installation_id_path(&app.path().app_local_data_dir().map_err(|e| {
            HolziError::PathResolution {
                reason: format!("app_local_data_dir: {e}"),
            }
        })?);
    let installation_uuid =
        read_or_mint_installation_uuid(&installation_id_file).map_err(HolziError::from)?;

    let db = active_database(&state)?;
    let alias_owned = trimmed;
    db.write(move |tx| {
        known_devices::update_alias(tx, installation_uuid, &alias_owned)?;
        Ok(())
    })
    .await
}

/// Lists the vault's devices with role, online state, last time online and any problem (spec
/// 024, FR-033; spec 023 FR-022). Requires an open vault (`NoActiveInstance` otherwise, like
/// [`current_device_info`]). Without a running sync service nobody counts as online.
#[tauri::command]
pub async fn list_vault_devices(
    app: AppHandle,
    state: State<'_, AppState>,
    registry: State<'_, std::sync::Arc<SyncRegistry>>,
) -> Result<Vec<VaultDevice>> {
    let installation_id_file =
        installation_id_path(&app.path().app_local_data_dir().map_err(|e| {
            HolziError::PathResolution {
                reason: format!("app_local_data_dir: {e}"),
            }
        })?);
    let installation_uuid =
        read_or_mint_installation_uuid(&installation_id_file).map_err(HolziError::from)?;
    let connected: std::collections::HashSet<[u8; 32]> = registry
        .get()
        .map(|runtime| runtime.node.connected().into_iter().collect())
        .unwrap_or_default();

    let db = active_database(&state)?;
    db.read(move |r| device_view::load(r, installation_uuid, &connected))
        .await
}
