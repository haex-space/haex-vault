//! Device identity management.
//!
//! Identity model (see `.claude/plans/2026-05-22-haex-devices-refactor.md`):
//!
//! - `<app_data>/device_id`            plaintext UUID, identifies the physical device
//!                                     across vaults. No crypto material.
//! - `haex_devices.id`                 random UUID per vault row, opaque FK target.
//!                                     Hides the stable device-id from the sync server.
//! - `haex_devices.device_id`          mirrors the file UUID inside the encrypted vault.
//! - `haex_devices.endpoint_id`        iroh ed25519 public key, distinct per
//!                                     (device × vault) — prevents cross-vault correlation.
//! - `haex_devices.secret_key`         iroh ed25519 secret key, hex. SQLCipher protects
//!                                     it at rest; no filesystem key file anymore.

pub mod error;

use std::fs;
use std::path::PathBuf;

use iroh::SecretKey;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tauri::{AppHandle, Manager, State};
use ts_rs::TS;

use crate::AppState;
use crate::database::core;
use error::DeviceError;

const DEVICE_ID_FILE: &str = "device_id";

/// Resolve the path to `<app_data>/device_id`.
fn device_id_file_path(app_handle: &AppHandle) -> Result<PathBuf, DeviceError> {
    let dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| DeviceError::KeyError {
            reason: format!("Cannot resolve app data directory: {e}"),
        })?;
    Ok(dir.join(DEVICE_ID_FILE))
}

/// Load `<app_data>/device_id`, or generate and persist a random UUID if missing.
/// This file is plaintext and contains no crypto material — only a stable
/// identifier so the user can recognize this physical device across vaults.
fn load_or_generate_device_id_file(app_handle: &AppHandle) -> Result<String, DeviceError> {
    let path = device_id_file_path(app_handle)?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    if path.exists() {
        let raw = fs::read_to_string(&path)?;
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
        eprintln!(
            "[Device] device_id file at {} was empty, regenerating",
            path.display()
        );
    }

    let new_id = uuid::Uuid::new_v4().to_string();
    fs::write(&path, &new_id)?;
    Ok(new_id)
}

#[derive(Debug, Serialize, Deserialize, Clone, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct KnownDevice {
    pub id: String,
    pub device_id: String,
    pub endpoint_id: String,
    pub name: String,
    pub platform: String,
    pub avatar: Option<String>,
    pub avatar_options: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DeviceResolution {
    /// The OS-level device id (= contents of `<app_data>/device_id`).
    pub device_id: String,
    /// `Some(id)` when a `haex_devices` row with this `device_id` exists and
    /// can be reused silently.
    pub matched_id: Option<String>,
    /// `Some(endpoint_id)` when matched — convenience for callers that want
    /// to start the iroh endpoint immediately.
    pub matched_endpoint_id: Option<String>,
    /// All `haex_devices` rows in the open vault. Used by the reconciliation
    /// dialog so the user can either pick a known device row to claim or
    /// register a fresh row.
    pub known_devices: Vec<KnownDevice>,
}

/// Read every row from `haex_devices` and convert it to [`KnownDevice`].
/// Goes through `select_with_crdt` so tombstoned rows are filtered out.
fn list_known_devices(state: &State<'_, AppState>) -> Result<Vec<KnownDevice>, DeviceError> {
    let rows = core::select_with_crdt(
        "SELECT id, device_id, endpoint_id, name, platform, avatar, avatar_options, created_at \
         FROM haex_devices \
         ORDER BY created_at ASC"
            .to_string(),
        vec![],
        &state.db,
    )
    .map_err(|e| DeviceError::Database {
        reason: format!("select haex_devices: {e}"),
    })?;

    fn as_string(v: &JsonValue) -> Option<String> {
        v.as_str().map(|s| s.to_string())
    }

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(KnownDevice {
            id: as_string(&row[0]).unwrap_or_default(),
            device_id: as_string(&row[1]).unwrap_or_default(),
            endpoint_id: as_string(&row[2]).unwrap_or_default(),
            name: as_string(&row[3]).unwrap_or_default(),
            platform: as_string(&row[4]).unwrap_or_default(),
            avatar: as_string(&row[5]),
            avatar_options: as_string(&row[6]),
            created_at: as_string(&row[7]),
        });
    }
    Ok(out)
}

/// Resolve the open vault against `<app_data>/device_id`.
///
/// - Reads (or creates) the device id file.
/// - Looks for a matching row in `haex_devices` and reports it via
///   `matched_id` / `matched_endpoint_id` when found.
/// - Always returns the list of known devices so the frontend can render the
///   reconciliation dialog when no match exists.
#[tauri::command]
pub async fn device_resolve_for_vault(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<DeviceResolution, DeviceError> {
    let device_id = load_or_generate_device_id_file(&app_handle)?;

    let matched_rows = core::select_with_crdt(
        "SELECT id, endpoint_id FROM haex_devices WHERE device_id = ? LIMIT 1".to_string(),
        vec![JsonValue::String(device_id.clone())],
        &state.db,
    )
    .map_err(|e| DeviceError::Database {
        reason: format!("SELECT match for device_id: {e}"),
    })?;

    let (matched_id, matched_endpoint_id) = matched_rows
        .into_iter()
        .next()
        .map(|row| {
            (
                row[0].as_str().map(|s| s.to_string()),
                row[1].as_str().map(|s| s.to_string()),
            )
        })
        .unwrap_or((None, None));

    let known_devices = list_known_devices(&state)?;

    Ok(DeviceResolution {
        device_id,
        matched_id,
        matched_endpoint_id,
        known_devices,
    })
}

/// Generate a fresh ed25519 keypair for iroh, returning `(secret_hex, public_str)`.
fn generate_keypair() -> (String, String) {
    let mut bytes = [0u8; 32];
    rand::fill(&mut bytes);
    let secret = SecretKey::from_bytes(&bytes);
    let public = secret.public();
    (hex::encode(bytes), public.to_string())
}

#[derive(Debug, Serialize, Deserialize, Clone, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCreated {
    pub id: String,
    pub device_id: String,
    pub endpoint_id: String,
}

/// Register a brand-new `haex_devices` row for the current physical device.
/// Generates a fresh ed25519 keypair (so a fresh iroh EndpointId, distinct
/// from any other vault).
#[tauri::command(rename_all = "camelCase")]
pub async fn device_create_for_vault(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    name: String,
    platform: String,
    avatar: Option<String>,
    avatar_options: Option<String>,
) -> Result<DeviceCreated, DeviceError> {
    let device_id = load_or_generate_device_id_file(&app_handle)?;
    let (secret_hex, endpoint_id) = generate_keypair();
    let row_id = uuid::Uuid::new_v4().to_string();

    let hlc = state.hlc.lock().map_err(|_| DeviceError::Database {
        reason: "HLC lock poisoned".to_string(),
    })?;

    core::execute_with_crdt(
        "INSERT INTO haex_devices \
         (id, device_id, endpoint_id, secret_key, name, platform, avatar, avatar_options) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
            .to_string(),
        vec![
            JsonValue::String(row_id.clone()),
            JsonValue::String(device_id.clone()),
            JsonValue::String(endpoint_id.clone()),
            JsonValue::String(secret_hex),
            JsonValue::String(name),
            JsonValue::String(platform),
            avatar.map_or(JsonValue::Null, JsonValue::String),
            avatar_options.map_or(JsonValue::Null, JsonValue::String),
        ],
        &state.db,
        &hlc,
    )
    .map_err(|e| DeviceError::Database {
        reason: format!("INSERT haex_devices: {e}"),
    })?;

    Ok(DeviceCreated {
        id: row_id,
        device_id,
        endpoint_id,
    })
}

/// Reclaim an existing `haex_devices` row for the current physical device.
/// Used when the user picks "this is my old device" in the reconciliation
/// dialog: we generate a fresh keypair (because we cannot recover the lost
/// one) and rewrite the existing row.
///
/// The caller is expected to have warned the user when the previous
/// `endpoint_id` is still online — this command does not perform the check.
#[tauri::command(rename_all = "camelCase")]
pub async fn device_reclaim_existing(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    existing_id: String,
    name: Option<String>,
    platform: Option<String>,
    avatar: Option<String>,
    avatar_options: Option<String>,
) -> Result<DeviceCreated, DeviceError> {
    let device_id = load_or_generate_device_id_file(&app_handle)?;
    let (secret_hex, endpoint_id) = generate_keypair();

    let hlc = state.hlc.lock().map_err(|_| DeviceError::Database {
        reason: "HLC lock poisoned".to_string(),
    })?;

    // COALESCE keeps the existing column value whenever the caller passes
    // `None`. Avatars can be cleared explicitly with an empty string if needed.
    core::execute_with_crdt(
        "UPDATE haex_devices SET \
           device_id = ?, \
           endpoint_id = ?, \
           secret_key = ?, \
           name = COALESCE(?, name), \
           platform = COALESCE(?, platform), \
           avatar = COALESCE(?, avatar), \
           avatar_options = COALESCE(?, avatar_options) \
         WHERE id = ?"
            .to_string(),
        vec![
            JsonValue::String(device_id.clone()),
            JsonValue::String(endpoint_id.clone()),
            JsonValue::String(secret_hex),
            name.map_or(JsonValue::Null, JsonValue::String),
            platform.map_or(JsonValue::Null, JsonValue::String),
            avatar.map_or(JsonValue::Null, JsonValue::String),
            avatar_options.map_or(JsonValue::Null, JsonValue::String),
            JsonValue::String(existing_id.clone()),
        ],
        &state.db,
        &hlc,
    )
    .map_err(|e| DeviceError::Database {
        reason: format!("UPDATE haex_devices: {e}"),
    })?;

    Ok(DeviceCreated {
        id: existing_id,
        device_id,
        endpoint_id,
    })
}

/// Load the ed25519 secret key of the given `haex_devices` row into the
/// process-wide [`PeerEndpoint`]. Must be called before `peer_storage_start`.
#[tauri::command(rename_all = "camelCase")]
pub async fn endpoint_load_for_device(
    state: State<'_, AppState>,
    device_row_id: String,
) -> Result<String, DeviceError> {
    let secret_rows = core::select_with_crdt(
        "SELECT secret_key FROM haex_devices WHERE id = ? LIMIT 1".to_string(),
        vec![JsonValue::String(device_row_id.clone())],
        &state.db,
    )
    .map_err(|e| DeviceError::Database {
        reason: format!("SELECT secret_key: {e}"),
    })?;
    let secret_hex = secret_rows
        .into_iter()
        .next()
        .and_then(|row| row.into_iter().next())
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .ok_or_else(|| DeviceError::Database {
            reason: format!("no haex_devices row with id {device_row_id}"),
        })?;

    let secret_bytes: [u8; 32] = hex::decode(&secret_hex)
        .map_err(|e| DeviceError::KeyError {
            reason: format!("Invalid secret_key hex: {e}"),
        })?
        .try_into()
        .map_err(|v: Vec<u8>| DeviceError::KeyError {
            reason: format!("secret_key wrong length: expected 32, got {}", v.len()),
        })?;
    let secret_key = SecretKey::from_bytes(&secret_bytes);
    let endpoint_id = secret_key.public().to_string();

    {
        let mut endpoint = state.peer_storage.write().await;
        if endpoint.is_running() {
            let _ = endpoint.stop().await;
        }
        endpoint.replace_key(secret_key);
    }

    eprintln!("[Device] Endpoint key loaded for device row {device_row_id}, EndpointId: {endpoint_id}");
    Ok(endpoint_id)
}
