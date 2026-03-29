use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde_json::Value as JsonValue;
use tauri::State;

use crate::database::core::{execute_with_crdt, select_with_crdt};
use crate::mls::manager::MlsManager;
use crate::mls::types::{MlsCommitBundle, MlsEpochKey, MlsGroupInfo, MlsIdentityInfo, MlsProcessedMessage};
use crate::AppState;

fn with_mls_manager<T>(state: &State<'_, AppState>, f: impl FnOnce(&MlsManager) -> Result<T, String>) -> Result<T, String> {
    let manager = MlsManager::new(state.db.0.clone());
    f(&manager)
}

#[tauri::command]
pub fn mls_init_tables(state: State<'_, AppState>) -> Result<(), String> {
    with_mls_manager(&state, |mgr| mgr.init_tables())
}

#[tauri::command]
pub fn mls_init_identity(state: State<'_, AppState>) -> Result<MlsIdentityInfo, String> {
    with_mls_manager(&state, |mgr| mgr.init_identity())
}

#[tauri::command]
pub fn mls_create_group(state: State<'_, AppState>, space_id: String) -> Result<MlsGroupInfo, String> {
    with_mls_manager(&state, |mgr| mgr.create_group(&space_id))
}

#[tauri::command]
pub fn mls_add_member(state: State<'_, AppState>, space_id: String, key_package: Vec<u8>) -> Result<MlsCommitBundle, String> {
    with_mls_manager(&state, |mgr| mgr.add_member(&space_id, &key_package))
}

#[tauri::command]
pub fn mls_remove_member(state: State<'_, AppState>, space_id: String, member_index: u32) -> Result<MlsCommitBundle, String> {
    with_mls_manager(&state, |mgr| mgr.remove_member(&space_id, member_index))
}

#[tauri::command]
pub fn mls_encrypt(state: State<'_, AppState>, space_id: String, plaintext: Vec<u8>) -> Result<Vec<u8>, String> {
    with_mls_manager(&state, |mgr| mgr.encrypt(&space_id, &plaintext))
}

#[tauri::command]
pub fn mls_decrypt(state: State<'_, AppState>, space_id: String, ciphertext: Vec<u8>) -> Result<Vec<u8>, String> {
    with_mls_manager(&state, |mgr| mgr.decrypt(&space_id, &ciphertext))
}

#[tauri::command]
pub fn mls_process_message(state: State<'_, AppState>, space_id: String, message: Vec<u8>) -> Result<MlsProcessedMessage, String> {
    with_mls_manager(&state, |mgr| {
        let payload = mgr.process_message(&space_id, &message)?;
        let content_type = if payload.is_empty() { "commit" } else { "application" };
        Ok(MlsProcessedMessage {
            content_type: content_type.to_string(),
            payload,
        })
    })
}

#[tauri::command]
pub fn mls_get_key_packages(state: State<'_, AppState>, count: u32) -> Result<Vec<Vec<u8>>, String> {
    with_mls_manager(&state, |mgr| mgr.generate_key_packages(count))
}

#[tauri::command]
pub fn mls_export_epoch_key(state: State<'_, AppState>, space_id: String) -> Result<MlsEpochKey, String> {
    // 1. Derive key from MLS group
    let epoch_key = with_mls_manager(&state, |mgr| mgr.derive_epoch_key(&space_id))?;

    // 2. Persist via CRDT (synced to all user devices)
    let hlc = state.hlc.lock().map_err(|e| format!("Failed to lock HLC: {e}"))?;
    let id = uuid::Uuid::new_v4().to_string();
    let key_b64 = BASE64.encode(&epoch_key.key);

    // Delete existing entry for this space+epoch, then insert
    execute_with_crdt(
        format!("DELETE FROM haex_mls_sync_keys WHERE space_id = ?1 AND epoch = ?2"),
        vec![JsonValue::String(space_id.clone()), JsonValue::Number((epoch_key.epoch as i64).into())],
        &state.db,
        &hlc,
    ).map_err(|e| format!("Failed to delete old sync key: {e}"))?;

    execute_with_crdt(
        format!("INSERT INTO haex_mls_sync_keys (id, space_id, epoch, key_data) VALUES (?1, ?2, ?3, ?4)"),
        vec![
            JsonValue::String(id),
            JsonValue::String(space_id),
            JsonValue::Number((epoch_key.epoch as i64).into()),
            JsonValue::String(key_b64),
        ],
        &state.db,
        &hlc,
    ).map_err(|e| format!("Failed to store sync key: {e}"))?;

    Ok(epoch_key)
}

#[tauri::command]
pub fn mls_get_epoch_key(state: State<'_, AppState>, space_id: String, epoch: u64) -> Result<MlsEpochKey, String> {
    let rows = select_with_crdt(
        format!("SELECT key_data FROM haex_mls_sync_keys WHERE space_id = ?1 AND epoch = ?2"),
        vec![JsonValue::String(space_id.clone()), JsonValue::Number((epoch as i64).into())],
        &state.db,
    ).map_err(|e| format!("Failed to query sync key: {e}"))?;

    let row = rows.first()
        .ok_or_else(|| format!("No sync key found for space {space_id} epoch {epoch}"))?;
    let key_b64 = row.first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Invalid key data".to_string())?;
    let key = BASE64.decode(key_b64)
        .map_err(|e| format!("Failed to decode sync key: {e}"))?;

    Ok(MlsEpochKey { epoch, key })
}
