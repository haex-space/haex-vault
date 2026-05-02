//! Tauri commands for peer storage

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Manager, State};
use tauri::ipc::Channel;

use crate::AppState;
use crate::database::DbConnection;
use crate::peer_storage::endpoint::is_content_uri;
use crate::peer_storage::error::PeerStorageError;
use crate::peer_storage::protocol::FileEntry;

// ============================================================================
// Channel message types
// ============================================================================

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", tag = "event")]
pub enum TransferEvent {
    #[serde(rename_all = "camelCase")]
    Progress {
        bytes_received: u64,
        total_bytes: u64,
    },
    #[serde(rename_all = "camelCase")]
    Complete {
        local_path: String,
        total_bytes: u64,
    },
    #[serde(rename_all = "camelCase")]
    Error {
        error: String,
    },
}

// ============================================================================
// DB helpers
// ============================================================================

/// Load shares for the current device from the database.
/// Returns a list of (id, name, local_path, space_id) tuples.
fn load_shares_from_db(
    state: &AppState,
    endpoint_id: &str,
) -> Result<Vec<(String, String, String, String)>, PeerStorageError> {
    let sql = "SELECT id, name, local_path, space_id FROM haex_peer_shares WHERE device_endpoint_id = ?1".to_string();
    let params = vec![serde_json::Value::String(endpoint_id.to_string())];

    let rows = crate::database::core::select_with_crdt(sql, params, &state.db)
        .map_err(|e| PeerStorageError::Database { reason: e.to_string() })?;

    let shares = rows.iter().map(|row| {
        let id = row.get(0).and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let name = row.get(1).and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let path = row.get(2).and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let space_id = row.get(3).and_then(|v| v.as_str()).unwrap_or_default().to_string();
        (id, name, path, space_id)
    }).collect();

    Ok(shares)
}

/// Load allowed peers from haex_space_devices.
/// Returns a map: remote EndpointId (string) -> set of space_ids they may access.
/// Excludes our own endpoint ID.
fn load_allowed_peers_from_db(
    state: &AppState,
    own_endpoint_id: &str,
) -> Result<HashMap<String, HashSet<String>>, PeerStorageError> {
    let sql = "SELECT device_endpoint_id, space_id FROM haex_space_devices WHERE device_endpoint_id != ?1".to_string();
    let params = vec![serde_json::Value::String(own_endpoint_id.to_string())];

    let rows = crate::database::core::select_with_crdt(sql, params, &state.db)
        .map_err(|e| PeerStorageError::Database { reason: e.to_string() })?;

    let mut allowed: HashMap<String, HashSet<String>> = HashMap::new();
    for row in &rows {
        let endpoint_id = row.get(0).and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let space_id = row.get(1).and_then(|v| v.as_str()).unwrap_or_default().to_string();
        allowed.entry(endpoint_id).or_default().insert(space_id);
    }

    Ok(allowed)
}

/// Reload only the allowed-peers map from haex_space_devices into the running endpoint.
///
/// Cheaper than reload_state_from_db (skips share path validation). Called from the
/// space-delivery leader after it receives a SyncPush that touches haex_space_devices,
/// so the new peer is authorized before Response::Ok is returned.
pub(crate) async fn reload_allowed_peers(
    state: &AppState,
    endpoint: &crate::peer_storage::endpoint::PeerEndpoint,
) -> Result<(), PeerStorageError> {
    let endpoint_id = endpoint.endpoint_id().to_string();
    let allowed_peers = load_allowed_peers_from_db(state, &endpoint_id)?;
    let peer_count: usize = allowed_peers.values().map(|s| s.len()).sum();
    endpoint.set_allowed_peers(allowed_peers).await;
    eprintln!("[PeerStorage] Updated allowed peers: {peer_count} peers across spaces");
    Ok(())
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
        if is_content_uri(local_path) {
            // Android Content URI — cannot validate with std::fs, always load.
            // The android_fs plugin handles validation when actually serving files.
            endpoint.add_share(id.clone(), name.clone(), local_path.clone(), space_id.clone()).await;
            loaded += 1;
        } else {
            let path = PathBuf::from(local_path);
            if path.exists() && path.is_dir() {
                endpoint.add_share(id.clone(), name.clone(), local_path.clone(), space_id.clone()).await;
                loaded += 1;
            } else {
                eprintln!(
                    "[PeerStorage] Skipping share '{}': path does not exist: {}",
                    name, local_path
                );
            }
        }
    }

    endpoint.set_allowed_peers(allowed_peers).await;

    eprintln!("[PeerStorage] Loaded {loaded}/{} shares from DB", shares.len());
    Ok(loaded)
}

// ============================================================================
// Endpoint lifecycle commands
// ============================================================================

/// Start the peer storage endpoint and load shares for this device from DB
#[tauri::command(rename_all = "camelCase")]
pub async fn peer_storage_start(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    relay_url: Option<String>,
) -> Result<PeerStorageStartInfo, PeerStorageError> {
    let mut endpoint = state.peer_storage.write().await;

    // Store AppHandle so the accept loop can use android_fs for Content URI shares
    endpoint.set_app_handle(app.clone()).await;

    // Load shares and allowed peers from DB before starting
    reload_state_from_db(&state, &*endpoint).await?;

    let node_id = endpoint.start(relay_url).await?;

    // Register the unified multi-space handler so this device can accept
    // PushInvite/ClaimInvite from peers and route leader requests by space_id.
    // With an empty leader map it only handles PushInvite.
    {
        let has_handler = endpoint.state.read().await.delivery_handler.is_some();
        if !has_handler {
            let db_conn = DbConnection(state.db.0.clone());
            let hlc_clone = state.hlc.lock().map_err(|_| PeerStorageError::EndpointNotRunning)?.clone();
            let handler = std::sync::Arc::new(
                crate::space_delivery::local::multi_leader::MultiSpaceLeaderHandler {
                    leaders: state.leader_state.clone(),
                    db: db_conn,
                    hlc: std::sync::Arc::new(std::sync::Mutex::new(hlc_clone)),
                    app_handle: app.clone(),
                },
            );
            endpoint.set_delivery_handler(handler).await;
        }
    }

    // Clone the iroh endpoint handle before dropping the write lock so the
    // relay wait below does not block concurrent read operations (e.g. local_delivery_connect).
    let iroh_ep = endpoint.endpoint_ref().cloned();
    drop(endpoint);

    // Wait briefly for relay connection so we can advertise our relay URL to peers
    let relay_url = if let Some(ep) = iroh_ep {
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
    let mut endpoint = state.peer_storage.write().await;
    endpoint.stop().await
}

/// Get the current node ID and running status
#[tauri::command]
pub async fn peer_storage_status(
    state: State<'_, AppState>,
) -> Result<PeerStorageStatus, PeerStorageError> {
    let endpoint = state.peer_storage.read().await;
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
    let endpoint = state.peer_storage.read().await;
    reload_state_from_db(&state, &*endpoint).await
}

// ============================================================================
// Remote peer operations
// ============================================================================

/// Browse a remote peer's shared files
#[tauri::command(rename_all = "camelCase")]
pub async fn peer_storage_remote_list(
    state: State<'_, AppState>,
    node_id: String,
    relay_url: Option<String>,
    path: String,
    ucan_token: String,
) -> Result<Vec<FileEntry>, PeerStorageError> {
    let remote_id: iroh::EndpointId = node_id
        .parse()
        .map_err(|e| PeerStorageError::ConnectionFailed {
            reason: format!("Invalid EndpointId: {e}"),
        })?;

    let parsed_relay = relay_url.and_then(|s| s.parse::<iroh::RelayUrl>().ok());

    let endpoint = state.peer_storage.read().await;
    endpoint.remote_list(remote_id, parsed_relay, &path, &ucan_token).await
}

/// Download a file from a remote peer directly to disk.
///
/// Uses Tauri's Channel API to stream progress, completion, and error events
/// back to the frontend. The command returns the target path immediately;
/// the actual download runs async and reports status via the channel.
#[tauri::command(rename_all = "camelCase")]
pub async fn peer_storage_remote_read(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    node_id: String,
    relay_url: Option<String>,
    path: String,
    transfer_id: Option<String>,
    save_to: Option<String>,
    ucan_token: String,
    on_event: Channel<TransferEvent>,
) -> Result<String, PeerStorageError> {
    let remote_id: iroh::EndpointId = node_id
        .parse()
        .map_err(|e| PeerStorageError::ConnectionFailed {
            reason: format!("Invalid EndpointId: {e}"),
        })?;

    let parsed_relay = relay_url.and_then(|s| s.parse::<iroh::RelayUrl>().ok());

    // Determine output path: explicit save_to or system Downloads folder
    let output_path = if let Some(ref dest) = save_to {
        PathBuf::from(dest)
    } else {
        let downloads_dir = app.path().download_dir()
            .or_else(|_| app.path().cache_dir())
            .map_err(|e| PeerStorageError::ProtocolError {
                reason: format!("Failed to get downloads dir: {e}"),
            })?;
        std::fs::create_dir_all(&downloads_dir).map_err(|e| PeerStorageError::ProtocolError {
            reason: format!("Failed to create downloads dir: {e}"),
        })?;
        let file_name = std::path::Path::new(&path)
            .file_name()
            .unwrap_or(std::ffi::OsStr::new("download"))
            .to_string_lossy()
            .to_string();
        deduplicate_path(&downloads_dir, &file_name)
    };

    // Create cancel + pause controls for this transfer
    let (cancel_token, pause_flag) = if let Some(ref tid) = transfer_id {
        let cancel = tokio_util::sync::CancellationToken::new();
        let pause = Arc::new(std::sync::atomic::AtomicBool::new(false));
        state.transfer_tokens.lock().await.insert(tid.clone(), (cancel.clone(), pause.clone()));
        (Some(cancel), Some(pause))
    } else {
        (None, None)
    };

    let output_path_str = output_path.to_string_lossy().to_string();
    let app_handle = app.clone();

    // Spawn the download on a separate task. The IPC handler returns immediately
    // with the target path. Progress/completion/errors are streamed via the Channel.
    tokio::spawn(async move {
        let state = app_handle.state::<AppState>();

        // Progress callback with throttling: at most every 100ms to avoid
        // overwhelming the IPC bridge on mobile (each message crosses JNI/WebView).
        let on_event_progress = on_event.clone();
        let progress_cb: Option<Box<dyn Fn(u64, u64) + Send>> = Some({
            let last_emit = std::sync::Mutex::new(std::time::Instant::now() - std::time::Duration::from_secs(1));
            Box::new(move |received: u64, total: u64| {
                let now = std::time::Instant::now();
                let should_emit = {
                    let last = last_emit.lock().unwrap_or_else(|e| e.into_inner());
                    received >= total || now.duration_since(*last).as_millis() >= 100
                };
                if should_emit {
                    *last_emit.lock().unwrap_or_else(|e| e.into_inner()) = now;
                    let _ = on_event_progress.send(TransferEvent::Progress {
                        bytes_received: received,
                        total_bytes: total,
                    });
                }
            }) as Box<dyn Fn(u64, u64) + Send>
        });

        // Hold the lock only for stream open (bounded by connection timeout ~3s).
        // The actual file I/O runs without any lock so peer_storage_start/stop are
        // not blocked for the duration of the download.
        let streams = {
            let endpoint = state.peer_storage.read().await;
            endpoint.open_stream(remote_id, parsed_relay).await
        };
        let (mut send, mut recv) = match streams {
            Ok(s) => s,
            Err(e) => {
                let _ = on_event.send(TransferEvent::Error { error: e.to_string() });
                if let Some(tid) = &transfer_id {
                    state.transfer_tokens.lock().await.remove(tid);
                }
                return;
            }
        };
        let result = crate::peer_storage::endpoint::PeerEndpoint::read_open_streams_to_file(
            &mut send, &mut recv, &path, &output_path,
            None, progress_cb, cancel_token, pause_flag, &ucan_token,
        ).await;

        // Clean up cancel token
        if let Some(tid) = &transfer_id {
            state.transfer_tokens.lock().await.remove(tid);
        }

        match result {
            Ok(total_bytes) => {
                let final_path = move_to_public_downloads(&app_handle, &output_path);
                let _ = on_event.send(TransferEvent::Complete {
                    local_path: final_path,
                    total_bytes,
                });
            }
            Err(e) => {
                let _ = on_event.send(TransferEvent::Error {
                    error: e.to_string(),
                });
            }
        }
    });

    Ok(output_path_str)
}

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

// ============================================================================
// Open file with system app (cross-platform)
// ============================================================================

/// Open a file with the system's default app.
/// On Android, uses android_fs FileOpener (Intent-based).
/// On Desktop, uses tauri-plugin-opener.
pub fn open_file_with_system(
    #[allow(unused_variables)] app: &tauri::AppHandle,
    path: &str,
) -> Result<(), PeerStorageError> {
    #[cfg(target_os = "android")]
    {
        use tauri_plugin_android_fs::{AndroidFsExt, FileUri};

        let api = app.android_fs();
        let uri = if path.starts_with('{') {
            FileUri::from_json_str(path).map_err(|e| PeerStorageError::ProtocolError {
                reason: format!("Invalid Content URI: {e:?}"),
            })?
        } else {
            FileUri::from_path(path)
        };
        api.file_opener().open_file(&uri).map_err(|e| PeerStorageError::ProtocolError {
            reason: format!("Failed to open file: {e:?}"),
        })?;
    }
    #[cfg(not(target_os = "android"))]
    {
        use tauri_plugin_opener::OpenerExt;
        app.opener().open_path(path, None::<String>).map_err(|e| PeerStorageError::ProtocolError {
            reason: format!("Failed to open file: {e}"),
        })?;
    }
    Ok(())
}

/// Tauri command wrapper for open_file_with_system.
#[tauri::command(rename_all = "camelCase")]
pub async fn open_file_system(
    app: tauri::AppHandle,
    path: String,
) -> Result<(), PeerStorageError> {
    open_file_with_system(&app, &path)
}

// ============================================================================
// Helpers
// ============================================================================

/// On Android, copy a downloaded file from the app-private directory to the
/// public Downloads folder via MediaStore so it becomes visible in the system
/// file manager. Returns the FileUri JSON string of the public file on Android,
/// or the original path string on other platforms.
fn move_to_public_downloads(
    #[allow(unused_variables)] app_handle: &tauri::AppHandle,
    output_path: &std::path::Path,
) -> String {
    #[cfg(target_os = "android")]
    {
        use tauri_plugin_android_fs::{AndroidFsExt, PublicGeneralPurposeDir};

        let file_name = output_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let result: Result<String, String> = (|| {
            let api = app_handle.android_fs();
            let ps = api.public_storage();

            // Create empty file in public Downloads (MediaStore)
            let dest_uri = ps.create_new_file(
                None,
                PublicGeneralPurposeDir::Download,
                &file_name,
                None,
            ).map_err(|e| format!("create_new_file: {e:?}"))?;

            // Stream-copy from app-private temp file to public Downloads
            let mut src = std::fs::File::open(output_path)
                .map_err(|e| format!("open src: {e}"))?;
            let mut dest = api.open_file_writable(&dest_uri)
                .map_err(|e| format!("open dest: {e:?}"))?;
            std::io::copy(&mut src, &mut dest)
                .map_err(|e| format!("copy: {e}"))?;
            drop(dest);

            // Clean up temp file
            let _ = std::fs::remove_file(output_path);

            Ok(dest_uri.to_json_string().map_err(|e| format!("to_json: {e:?}"))?)
        })();

        match result {
            Ok(uri_json) => uri_json,
            Err(e) => {
                eprintln!("[peer_storage] Failed to move to public Downloads: {e}");
                // Fallback: return original path
                output_path.to_string_lossy().to_string()
            }
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        output_path.to_string_lossy().to_string()
    }
}

/// Find a non-colliding file path: photo.jpg → photo (1).jpg → photo (2).jpg → …
fn deduplicate_path(dir: &std::path::Path, file_name: &str) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }

    let stem = std::path::Path::new(file_name)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let ext = std::path::Path::new(file_name)
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();

    for i in 1..10_000 {
        let numbered = dir.join(format!("{stem} ({i}){ext}"));
        if !numbered.exists() {
            return numbered;
        }
    }

    // Fallback: use UUID suffix
    dir.join(format!("{stem}_{}{ext}", uuid::Uuid::new_v4()))
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
