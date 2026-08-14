use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde_json::Value as JsonValue;
use tauri::State;

use crate::critical::CriticalFailureCode;
use crate::database::core::{execute_with_crdt, select_with_crdt};
use crate::mls::manager::MlsManager;
use crate::mls::types::{
    MlsCommitBundle, MlsEpochKey, MlsExternalCommitResult, MlsGroupInfo, MlsIdentityInfo,
    MlsKeyPackageWithPop, MlsProcessedMessage,
};
use crate::AppState;

fn with_mls_manager<T>(
    state: &State<'_, AppState>,
    f: impl FnOnce(&MlsManager) -> Result<T, String>,
) -> Result<T, String> {
    let manager = MlsManager::new(state.db.0.clone());
    f(&manager)
}

#[tauri::command]
pub fn mls_init_tables(state: State<'_, AppState>) -> Result<(), String> {
    with_mls_manager(&state, |mgr| mgr.init_tables())
}

#[tauri::command]
pub fn mls_init_identity(
    state: State<'_, AppState>,
    did: String,
) -> Result<MlsIdentityInfo, String> {
    with_mls_manager(&state, |mgr| mgr.init_identity(&did))
}

#[tauri::command]
pub fn mls_find_member_index(
    state: State<'_, AppState>,
    space_id: String,
    member_did: String,
) -> Result<Option<u32>, String> {
    with_mls_manager(&state, |mgr| {
        mgr.find_member_index_by_did(&space_id, &member_did)
    })
}

#[tauri::command]
pub fn mls_create_group(
    state: State<'_, AppState>,
    space_id: String,
) -> Result<MlsGroupInfo, String> {
    with_mls_manager(&state, |mgr| mgr.create_group(&space_id))
}

#[tauri::command]
pub fn mls_add_member(
    state: State<'_, AppState>,
    space_id: String,
    key_package: Vec<u8>,
    expected_did: String,
    pop: Vec<u8>,
) -> Result<MlsCommitBundle, String> {
    with_mls_manager(&state, |mgr| {
        mgr.add_member(&space_id, &key_package, &expected_did, &pop)
    })
}

#[tauri::command]
pub fn mls_remove_member(
    state: State<'_, AppState>,
    space_id: String,
    member_index: u32,
) -> Result<MlsCommitBundle, String> {
    with_mls_manager(&state, |mgr| mgr.remove_member(&space_id, member_index))
}

#[tauri::command]
pub fn mls_encrypt(
    state: State<'_, AppState>,
    space_id: String,
    plaintext: Vec<u8>,
) -> Result<Vec<u8>, String> {
    with_mls_manager(&state, |mgr| mgr.encrypt(&space_id, &plaintext))
}

// `mls_decrypt` / `mls_process_message` are used for the online-space
// (server-mediated) message path, which carries no committer-capability
// proof on the wire (out of scope for plan §5.8, which only extends the
// local/P2P `Request::MlsSendMessage` envelope). Both always pass `None`
// for the new params — a Remove arriving here only merges via the
// target-already-gone exemption in `authorize_committer_capability`.
#[tauri::command]
pub fn mls_decrypt(
    state: State<'_, AppState>,
    space_id: String,
    ciphertext: Vec<u8>,
) -> Result<Vec<u8>, String> {
    with_mls_manager(&state, |mgr| mgr.decrypt(&space_id, &ciphertext, None, None))
}

#[tauri::command]
pub fn mls_process_message(
    state: State<'_, AppState>,
    space_id: String,
    message: Vec<u8>,
) -> Result<MlsProcessedMessage, String> {
    with_mls_manager(&state, |mgr| {
        let payload = mgr.process_message(&space_id, &message, None, None)?;
        let content_type = if payload.is_empty() {
            "commit"
        } else {
            "application"
        };
        Ok(MlsProcessedMessage {
            content_type: content_type.to_string(),
            payload,
        })
    })
}

#[tauri::command]
pub fn mls_get_key_packages(
    state: State<'_, AppState>,
    count: u32,
) -> Result<Vec<MlsKeyPackageWithPop>, String> {
    let own_did = with_mls_manager(&state, |mgr| mgr.get_own_did())?;
    let identity = crate::space_delivery::local::quic_retry::load_signing_identity_for_did(
        &state.db, &own_did,
    )
    .map_err(|e| format!("Failed to load identity for proof-of-possession: {e}"))?;
    with_mls_manager(&state, |mgr| {
        mgr.generate_key_packages(count, &identity.signing_key)
    })
    .map(|pairs| {
        pairs
            .into_iter()
            .map(|(key_package, pop)| MlsKeyPackageWithPop { key_package, pop })
            .collect()
    })
}

#[tauri::command]
pub fn mls_has_group(state: State<'_, AppState>, space_id: String) -> Result<bool, String> {
    with_mls_manager(&state, |mgr| Ok(mgr.has_group(&space_id)))
}

#[tauri::command]
pub fn mls_export_epoch_key(
    state: State<'_, AppState>,
    space_id: String,
) -> Result<MlsEpochKey, String> {
    // 1. Derive key from MLS group
    let epoch_key = with_mls_manager(&state, |mgr| mgr.derive_epoch_key(&space_id))?;

    // 2. Persist via CRDT (synced to all user devices)
    let hlc = state
        .lock_or_fail(
            &state.hlc,
            CriticalFailureCode::HlcMutexPoisoned,
            "mls::commands::mls_export_epoch_key",
            serde_json::json!({}),
        )
        .map_err(|e| e.to_string())?;
    let id = uuid::Uuid::new_v4().to_string();
    let key_b64 = BASE64.encode(&epoch_key.key);

    // Update in place when the epoch already exists. This avoids the old
    // delete-then-insert sequence, where a signing failure on the INSERT
    // permanently removed the previously valid key.
    let updated = execute_with_crdt(
        "UPDATE haex_mls_sync_keys SET key_data = ?1 \
         WHERE space_id = ?2 AND epoch = ?3 \
         RETURNING id"
            .to_string(),
        vec![
            JsonValue::String(key_b64.clone()),
            JsonValue::String(space_id.clone()),
            JsonValue::Number((epoch_key.epoch as i64).into()),
        ],
        &state.db,
        &hlc,
        &state.column_sig_key_cache,
    )
    .map_err(|e| format!("Failed to update sync key: {e}"))?;

    if updated.is_empty() {
        execute_with_crdt(
            "INSERT INTO haex_mls_sync_keys (id, space_id, epoch, key_data) \
             VALUES (?1, ?2, ?3, ?4)"
                .to_string(),
            vec![
                JsonValue::String(id),
                JsonValue::String(space_id),
                JsonValue::Number((epoch_key.epoch as i64).into()),
                JsonValue::String(key_b64),
            ],
            &state.db,
            &hlc,
            &state.column_sig_key_cache,
        )
        .map_err(|e| format!("Failed to store sync key: {e}"))?;
    }

    Ok(epoch_key)
}

#[tauri::command]
pub fn mls_get_group_info(state: State<'_, AppState>, space_id: String) -> Result<Vec<u8>, String> {
    with_mls_manager(&state, |mgr| mgr.get_group_info(&space_id))
}

#[tauri::command]
pub fn mls_join_by_external_commit(
    state: State<'_, AppState>,
    space_id: String,
    group_info: Vec<u8>,
) -> Result<MlsExternalCommitResult, String> {
    with_mls_manager(&state, |mgr| {
        let (commit, epoch_key) = mgr.join_by_external_commit(&space_id, &group_info)?;
        Ok(MlsExternalCommitResult { commit, epoch_key })
    })
}

#[tauri::command]
pub fn mls_get_epoch_key(
    state: State<'_, AppState>,
    space_id: String,
    epoch: u64,
) -> Result<MlsEpochKey, String> {
    let rows = select_with_crdt(
        format!("SELECT key_data FROM haex_mls_sync_keys WHERE space_id = ?1 AND epoch = ?2"),
        vec![
            JsonValue::String(space_id.clone()),
            JsonValue::Number((epoch as i64).into()),
        ],
        &state.db,
    )
    .map_err(|e| format!("Failed to query sync key: {e}"))?;

    let row = rows
        .first()
        .ok_or_else(|| format!("No sync key found for space {space_id} epoch {epoch}"))?;
    let key_b64 = row
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Invalid key data".to_string())?;
    let key = BASE64
        .decode(key_b64)
        .map_err(|e| format!("Failed to decode sync key: {e}"))?;

    Ok(MlsEpochKey { epoch, key })
}
