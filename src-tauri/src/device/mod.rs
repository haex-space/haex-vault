//! Device identity management
//!
//! Each device has a unique Ed25519 keypair that serves as its identity.
//! The public key (EndpointId) is used as a cryptographically strong device
//! identifier across the system: peer storage, space device registration,
//! CRDT device tracking, etc.
//!
//! The keypair is generated on first vault open per device and stored
//! encrypted in the app data directory (not in the vault DB, which is portable).

pub mod error;
pub mod key;

use tauri::{AppHandle, Manager, State};

use crate::AppState;
use crate::extension::database::executor::SqlExecutor;
use error::DeviceError;

/// Read a vault setting by key, or return None if not found.
/// Uses IFNULL(haex_tombstone, 0) != 1 to match the CRDT tombstone convention
/// (tombstone can be NULL, 0, or 1).
fn read_setting(conn: &rusqlite::Connection, key: &str) -> Result<Option<String>, DeviceError> {
    match conn.query_row(
        "SELECT value FROM haex_vault_settings WHERE key = ?1 AND IFNULL(haex_tombstone, 0) != 1",
        [key],
        |row| row.get(0),
    ) {
        Ok(val) => Ok(Some(val)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(DeviceError::Database {
            reason: format!("Failed to read setting '{key}': {e}"),
        }),
    }
}

/// Initialize the device key after vault open.
///
/// Reads `device_key_secret` and `vault_id` from vault settings (generating them
/// if missing for pre-existing vaults), loads or generates the Ed25519 device key
/// from the app data directory, and replaces the ephemeral key in the PeerEndpoint.
///
/// Returns the EndpointId (public key) for this device.
#[tauri::command]
pub async fn device_init_key(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, DeviceError> {
    // 1. Read or generate device_key_secret and vault_id from the vault database
    let (secret_hex, vault_uuid) = {
        let db_guard = state.db.0.lock().map_err(|e| DeviceError::Database {
            reason: format!("DB lock error: {e}"),
        })?;
        let conn = db_guard.as_ref().ok_or_else(|| DeviceError::Database {
            reason: "No database connection — vault not open".to_string(),
        })?;

        // vault_id must always exist (set at creation or synced from remote)
        let uuid = read_setting(conn, "vault_id")?.ok_or_else(|| DeviceError::Database {
            reason: "vault_id not found — vault may not be fully initialized".to_string(),
        })?;

        // device_key_secret may be missing in pre-existing vaults — generate if needed
        let secret = match read_setting(conn, "device_key_secret")? {
            Some(val) => val,
            None => {
                let mut secret_bytes = [0u8; 32];
                rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut secret_bytes);
                let new_secret = hex::encode(secret_bytes);

                let hlc_service = state.hlc.lock().map_err(|e| DeviceError::Database {
                    reason: format!("HLC lock error: {e}"),
                })?;

                let tx = conn.unchecked_transaction().map_err(|e| DeviceError::Database {
                    reason: format!("Failed to begin transaction: {e}"),
                })?;
                SqlExecutor::execute_internal_typed(
                    &tx,
                    &hlc_service,
                    "INSERT INTO haex_vault_settings (id, key, type, value) VALUES (?, 'device_key_secret', 'system', ?)",
                    rusqlite::params![uuid::Uuid::new_v4().to_string(), new_secret],
                ).map_err(|e| DeviceError::Database {
                    reason: format!("Failed to store device_key_secret: {e}"),
                })?;
                tx.commit().map_err(|e| DeviceError::Database {
                    reason: format!("Failed to commit device_key_secret: {e}"),
                })?;

                eprintln!("[Device] Generated device_key_secret for existing vault");
                new_secret
            }
        };

        (secret, uuid)
    };

    // 2. Decode the hex secret into 32 bytes
    let secret_bytes: [u8; 32] = hex::decode(&secret_hex)
        .map_err(|e| DeviceError::KeyError {
            reason: format!("Invalid device_key_secret hex: {e}"),
        })?
        .try_into()
        .map_err(|v: Vec<u8>| DeviceError::KeyError {
            reason: format!("device_key_secret wrong length: expected 32, got {}", v.len()),
        })?;

    // 3. Resolve app data directory
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| DeviceError::KeyError {
            reason: format!("Cannot resolve app data directory: {e}"),
        })?;

    // 4. Load or generate the Ed25519 device key
    let secret_key = key::load_or_generate(&app_data_dir, &vault_uuid, &secret_bytes)?;
    let endpoint_id = secret_key.public();

    // 5. Replace the ephemeral key in the PeerEndpoint
    {
        let mut endpoint = state.peer_storage.lock().await;
        endpoint.replace_key(secret_key);
    }

    eprintln!("[Device] Key initialized, EndpointId: {endpoint_id}");
    Ok(endpoint_id.to_string())
}
