//! Transfer control commands: cancel/pause/resume.

use tauri::State;

use crate::peer_storage::error::PeerStorageError;
use crate::AppState;

// ============================================================================
// Transfer control commands
// ============================================================================

/// Cancel an active file transfer
#[tauri::command(rename_all = "camelCase")]
pub async fn peer_storage_transfer_cancel(
    state: State<'_, AppState>,
    transfer_id: String,
) -> Result<(), PeerStorageError> {
    if let Some((cancel, _)) = state.transfer_tokens.lock().await.get(&transfer_id) {
        cancel.cancel();
    }
    Ok(())
}

/// Pause an active file transfer
#[tauri::command(rename_all = "camelCase")]
pub async fn peer_storage_transfer_pause(
    state: State<'_, AppState>,
    transfer_id: String,
) -> Result<(), PeerStorageError> {
    if let Some((_, pause)) = state.transfer_tokens.lock().await.get(&transfer_id) {
        pause.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}

/// Resume a paused file transfer
#[tauri::command(rename_all = "camelCase")]
pub async fn peer_storage_transfer_resume(
    state: State<'_, AppState>,
    transfer_id: String,
) -> Result<(), PeerStorageError> {
    if let Some((_, pause)) = state.transfer_tokens.lock().await.get(&transfer_id) {
        pause.store(false, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}
