use tauri::State;

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
    with_mls_manager(&state, |mgr| mgr.export_epoch_key(&space_id))
}

#[tauri::command]
pub fn mls_get_epoch_key(state: State<'_, AppState>, space_id: String, epoch: u64) -> Result<MlsEpochKey, String> {
    with_mls_manager(&state, |mgr| mgr.get_epoch_key(&space_id, epoch))
}
