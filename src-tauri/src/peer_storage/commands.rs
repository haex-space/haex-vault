//! Tauri commands for peer storage

use std::path::PathBuf;
use tauri::State;

use crate::AppState;
use crate::peer_storage::error::PeerStorageError;
use crate::peer_storage::protocol::FileEntry;

/// Load shares for the current device from the database.
/// Returns a list of (id, name, local_path) tuples.
fn load_shares_from_db(
    state: &AppState,
    endpoint_id: &str,
) -> Result<Vec<(String, String, PathBuf)>, PeerStorageError> {
    let db_guard = state.db.0.lock().map_err(|e| PeerStorageError::Database {
        reason: format!("DB lock error: {e}"),
    })?;
    let conn = db_guard.as_ref().ok_or_else(|| PeerStorageError::Database {
        reason: "No database connection — vault not open".to_string(),
    })?;

    let mut stmt = conn
        .prepare(
            "SELECT id, name, local_path FROM haex_peer_shares \
             WHERE device_endpoint_id = ?1 AND IFNULL(haex_tombstone, 0) != 1",
        )
        .map_err(|e| PeerStorageError::Database {
            reason: format!("Failed to prepare share query: {e}"),
        })?;

    let shares = stmt
        .query_map([endpoint_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                PathBuf::from(row.get::<_, String>(2)?),
            ))
        })
        .map_err(|e| PeerStorageError::Database {
            reason: format!("Failed to query shares: {e}"),
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| PeerStorageError::Database {
            reason: format!("Failed to read share row: {e}"),
        })?;

    Ok(shares)
}

/// Start the peer storage endpoint and load shares for this device from DB
#[tauri::command]
pub async fn peer_storage_start(
    state: State<'_, AppState>,
) -> Result<String, PeerStorageError> {
    let mut endpoint = state.peer_storage.lock().await;
    let endpoint_id = endpoint.endpoint_id().to_string();

    // Load shares from DB before starting
    let shares = load_shares_from_db(&state, &endpoint_id)?;
    endpoint.clear_shares();
    for (id, name, local_path) in &shares {
        if local_path.exists() && local_path.is_dir() {
            endpoint.add_share(id.clone(), name.clone(), local_path.clone());
        } else {
            eprintln!(
                "[PeerStorage] Skipping share '{}': path does not exist: {}",
                name,
                local_path.display()
            );
        }
    }

    let node_id = endpoint.start().await?;
    eprintln!("[PeerStorage] Started with {} shares loaded from DB", shares.len());
    Ok(node_id.to_string())
}

/// Stop the peer storage endpoint
#[tauri::command]
pub async fn peer_storage_stop(
    state: State<'_, AppState>,
) -> Result<(), PeerStorageError> {
    let mut endpoint = state.peer_storage.lock().await;
    endpoint.stop().await
}

/// Get the current node ID and running status
#[tauri::command]
pub async fn peer_storage_status(
    state: State<'_, AppState>,
) -> Result<PeerStorageStatus, PeerStorageError> {
    let endpoint = state.peer_storage.lock().await;
    Ok(PeerStorageStatus {
        running: endpoint.is_running(),
        node_id: endpoint.endpoint_id().to_string(),
    })
}

/// Reload shares from DB into the running endpoint.
/// Called by the frontend after adding/removing shares via Drizzle.
#[tauri::command]
pub async fn peer_storage_reload_shares(
    state: State<'_, AppState>,
) -> Result<usize, PeerStorageError> {
    let mut endpoint = state.peer_storage.lock().await;
    let endpoint_id = endpoint.endpoint_id().to_string();

    let shares = load_shares_from_db(&state, &endpoint_id)?;
    endpoint.clear_shares();
    let mut loaded = 0;
    for (id, name, local_path) in &shares {
        if local_path.exists() && local_path.is_dir() {
            endpoint.add_share(id.clone(), name.clone(), local_path.clone());
            loaded += 1;
        } else {
            eprintln!(
                "[PeerStorage] Skipping share '{}': path does not exist: {}",
                name,
                local_path.display()
            );
        }
    }

    eprintln!("[PeerStorage] Reloaded {loaded}/{} shares from DB", shares.len());
    Ok(loaded)
}

/// Browse a remote peer's shared files
#[tauri::command(rename_all = "camelCase")]
pub async fn peer_storage_remote_list(
    state: State<'_, AppState>,
    node_id: String,
    path: String,
) -> Result<Vec<FileEntry>, PeerStorageError> {
    let remote_id: iroh::EndpointId = node_id
        .parse()
        .map_err(|e| PeerStorageError::ConnectionFailed {
            reason: format!("Invalid EndpointId: {e}"),
        })?;

    let endpoint = state.peer_storage.lock().await;
    endpoint.remote_list(remote_id, &path).await
}

/// Read a file from a remote peer
#[tauri::command(rename_all = "camelCase")]
pub async fn peer_storage_remote_read(
    state: State<'_, AppState>,
    node_id: String,
    path: String,
) -> Result<String, PeerStorageError> {
    let remote_id: iroh::EndpointId = node_id
        .parse()
        .map_err(|e| PeerStorageError::ConnectionFailed {
            reason: format!("Invalid EndpointId: {e}"),
        })?;

    let endpoint = state.peer_storage.lock().await;
    let (_size, data) = endpoint.remote_read(remote_id, &path, None).await?;

    // Return as base64 for now
    Ok(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &data,
    ))
}

// ============================================================================
// Response types
// ============================================================================

#[derive(Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PeerStorageStatus {
    pub running: bool,
    pub node_id: String,
}
