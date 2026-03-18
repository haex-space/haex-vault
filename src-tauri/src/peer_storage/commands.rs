//! Tauri commands for peer storage

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tauri::State;

use crate::AppState;
use crate::peer_storage::error::PeerStorageError;
use crate::peer_storage::protocol::FileEntry;

/// Load shares for the current device from the database.
/// Returns a list of (id, name, local_path, space_id) tuples.
fn load_shares_from_db(
    state: &AppState,
    endpoint_id: &str,
) -> Result<Vec<(String, String, PathBuf, String)>, PeerStorageError> {
    let db_guard = state.db.0.lock().map_err(|e| PeerStorageError::Database {
        reason: format!("DB lock error: {e}"),
    })?;
    let conn = db_guard.as_ref().ok_or_else(|| PeerStorageError::Database {
        reason: "No database connection — vault not open".to_string(),
    })?;

    let mut stmt = conn
        .prepare(
            "SELECT id, name, local_path, space_id FROM haex_peer_shares \
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
                row.get::<_, String>(3)?,
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

/// Load allowed peers from haex_space_devices.
/// Returns a map: remote EndpointId (string) -> set of space_ids they may access.
/// Excludes our own endpoint ID.
fn load_allowed_peers_from_db(
    state: &AppState,
    own_endpoint_id: &str,
) -> Result<HashMap<String, HashSet<String>>, PeerStorageError> {
    let db_guard = state.db.0.lock().map_err(|e| PeerStorageError::Database {
        reason: format!("DB lock error: {e}"),
    })?;
    let conn = db_guard.as_ref().ok_or_else(|| PeerStorageError::Database {
        reason: "No database connection — vault not open".to_string(),
    })?;

    let mut stmt = conn
        .prepare(
            "SELECT device_endpoint_id, space_id FROM haex_space_devices \
             WHERE device_endpoint_id != ?1",
        )
        .map_err(|e| PeerStorageError::Database {
            reason: format!("Failed to prepare allowed peers query: {e}"),
        })?;

    let mut allowed: HashMap<String, HashSet<String>> = HashMap::new();

    let rows = stmt
        .query_map([own_endpoint_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
            ))
        })
        .map_err(|e| PeerStorageError::Database {
            reason: format!("Failed to query space devices: {e}"),
        })?;

    for row in rows {
        let (endpoint_id, space_id) = row.map_err(|e| PeerStorageError::Database {
            reason: format!("Failed to read space device row: {e}"),
        })?;
        allowed.entry(endpoint_id).or_default().insert(space_id);
    }

    Ok(allowed)
}

/// Reload shares and allowed peers into the endpoint from DB.
async fn reload_state_from_db(
    state: &AppState,
    endpoint: &crate::peer_storage::endpoint::PeerEndpoint,
) -> Result<usize, PeerStorageError> {
    let endpoint_id = endpoint.endpoint_id().to_string();

    let shares = load_shares_from_db(state, &endpoint_id)?;
    let allowed_peers = load_allowed_peers_from_db(state, &endpoint_id)?;

    endpoint.clear_shares().await;
    let mut loaded = 0;
    for (id, name, local_path, space_id) in &shares {
        if local_path.exists() && local_path.is_dir() {
            endpoint.add_share(id.clone(), name.clone(), local_path.clone(), space_id.clone()).await;
            loaded += 1;
        } else {
            eprintln!(
                "[PeerStorage] Skipping share '{}': path does not exist: {}",
                name,
                local_path.display()
            );
        }
    }

    endpoint.set_allowed_peers(allowed_peers).await;

    eprintln!("[PeerStorage] Loaded {loaded}/{} shares from DB", shares.len());
    Ok(loaded)
}

/// Start the peer storage endpoint and load shares for this device from DB
#[tauri::command]
pub async fn peer_storage_start(
    state: State<'_, AppState>,
) -> Result<PeerStorageStartInfo, PeerStorageError> {
    let endpoint = state.peer_storage.lock().await;

    // Load shares and allowed peers from DB before starting
    reload_state_from_db(&state, &endpoint).await?;

    drop(endpoint);

    let mut endpoint = state.peer_storage.lock().await;
    let node_id = endpoint.start().await?;

    // Wait briefly for relay connection so we can advertise our relay URL to peers
    let relay_url = if let Some(ep) = endpoint.endpoint_ref() {
        match tokio::time::timeout(std::time::Duration::from_secs(5), ep.online()).await {
            Ok(()) => ep.addr().relay_urls().next().cloned().map(|u| u.to_string()),
            Err(_) => None,
        }
    } else {
        None
    };

    Ok(PeerStorageStartInfo {
        node_id: node_id.to_string(),
        relay_url,
    })
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

/// Reload shares and allowed peers from DB into the running endpoint.
/// Called by the frontend after adding/removing shares or space devices via Drizzle.
#[tauri::command]
pub async fn peer_storage_reload_shares(
    state: State<'_, AppState>,
) -> Result<usize, PeerStorageError> {
    let endpoint = state.peer_storage.lock().await;
    reload_state_from_db(&state, &endpoint).await
}

/// Browse a remote peer's shared files
#[tauri::command(rename_all = "camelCase")]
pub async fn peer_storage_remote_list(
    state: State<'_, AppState>,
    node_id: String,
    relay_url: Option<String>,
    path: String,
) -> Result<Vec<FileEntry>, PeerStorageError> {
    let remote_id: iroh::EndpointId = node_id
        .parse()
        .map_err(|e| PeerStorageError::ConnectionFailed {
            reason: format!("Invalid EndpointId: {e}"),
        })?;

    let parsed_relay = relay_url.and_then(|s| s.parse::<iroh::RelayUrl>().ok());

    let endpoint = state.peer_storage.lock().await;
    endpoint.remote_list(remote_id, parsed_relay, &path).await
}

/// Read a file from a remote peer
#[tauri::command(rename_all = "camelCase")]
pub async fn peer_storage_remote_read(
    state: State<'_, AppState>,
    node_id: String,
    relay_url: Option<String>,
    path: String,
) -> Result<String, PeerStorageError> {
    let remote_id: iroh::EndpointId = node_id
        .parse()
        .map_err(|e| PeerStorageError::ConnectionFailed {
            reason: format!("Invalid EndpointId: {e}"),
        })?;

    let parsed_relay = relay_url.and_then(|s| s.parse::<iroh::RelayUrl>().ok());

    let endpoint = state.peer_storage.lock().await;
    let (_size, data) = endpoint.remote_read(remote_id, parsed_relay, &path, None).await?;

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
pub struct PeerStorageStartInfo {
    pub node_id: String,
    pub relay_url: Option<String>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PeerStorageStatus {
    pub running: bool,
    pub node_id: String,
}
