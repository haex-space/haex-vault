//! Remote peer FS commands: list, read, write, mkdir.

use std::path::PathBuf;
use std::sync::Arc;
use tauri::ipc::Channel;
use tauri::{Manager, State};

use super::open_file::{move_to_public_downloads, verify_local_target_intact};
use crate::peer_storage::error::PeerStorageError;
use crate::peer_storage::protocol::FileEntry;
use crate::AppState;

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
    Error { error: String },
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
    let remote_id: iroh::EndpointId =
        node_id
            .parse()
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: format!("Invalid EndpointId: {e}"),
            })?;

    let parsed_relay = relay_url.and_then(|s| s.parse::<iroh::RelayUrl>().ok());

    let endpoint = state.peer_storage.read().await;
    endpoint
        .remote_list(remote_id, parsed_relay, &path, &ucan_token)
        .await
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
    expected_size: Option<u64>,
    expected_modified: Option<u64>,
    space_folder: Option<String>,
    space_id: Option<String>,
    ucan_token: String,
    on_event: Channel<TransferEvent>,
) -> Result<String, PeerStorageError> {
    let remote_id: iroh::EndpointId =
        node_id
            .parse()
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: format!("Invalid EndpointId: {e}"),
            })?;

    let parsed_relay = relay_url.and_then(|s| s.parse::<iroh::RelayUrl>().ok());

    // Pre-flight dedup: if we've recorded a prior successful download for
    // (endpoint_id, remote_path), and the peer's current FileEntry matches
    // what we cached AND the local target is still intact, skip the network
    // round-trip and resolve the transfer with the existing local path.
    //
    // Three independent checks must all pass — any miss drops the row and
    // falls through to a fresh download:
    //   1. size matches
    //   2. modified matches (NULL == NULL counted as a match — some peers
    //      don't expose mtime)
    //   3. local target still exists with the recorded size on disk
    //      (filesystem stat on desktop, MediaStore URI len on Android)
    //
    // Only kicks in when the caller hasn't passed an explicit `save_to`
    // (which is a deliberate "write to this exact path" override).
    if save_to.is_none() {
        if let Some(expected) = expected_size {
            if let Ok(Some(record)) =
                crate::peer_storage::downloads::find(&state.db, &node_id, &path)
            {
                let modified_match = match (record.modified, expected_modified) {
                    (Some(a), Some(b)) => a == b,
                    (None, None) => true,
                    _ => false,
                };
                if record.size == expected
                    && modified_match
                    && verify_local_target_intact(&app, &record.local_path, expected)
                {
                    let _ = on_event.send(TransferEvent::Complete {
                        local_path: record.local_path.clone(),
                        total_bytes: expected,
                    });
                    return Ok(record.local_path);
                }
                // Mismatch or local target gone — drop the dead row so the
                // next round doesn't re-trip the same stale lookup.
                let _ = crate::peer_storage::downloads::delete(&state.db, &node_id, &path);
            }
        }
    }

    // Sanitize the per-space subfolder once — same string flows into the
    // desktop filesystem path and the Android MediaStore relative_path so
    // dedup works identically across platforms.
    let space_subfolder = match (&space_folder, &space_id) {
        (Some(name), _) => crate::peer_storage::downloads::sanitize_folder_segment(
            name,
            space_id.as_deref().unwrap_or("default"),
        ),
        (None, Some(id)) => crate::peer_storage::downloads::sanitize_folder_segment(id, "default"),
        (None, None) => "default".to_string(),
    };

    // Determine the on-disk staging path. On desktop this is the final
    // location (Downloads/HaexVault/<space>/<file>). On Android it's the
    // app-private staging path — `move_to_public_downloads` later copies it
    // into MediaStore's public Downloads under the same relative layout.
    let output_path = if let Some(ref dest) = save_to {
        PathBuf::from(dest)
    } else {
        let downloads_dir = app
            .path()
            .download_dir()
            .or_else(|_| app.path().cache_dir())
            .map_err(|e| PeerStorageError::ProtocolError {
                reason: format!("Failed to get downloads dir: {e}"),
            })?;

        let target_dir = downloads_dir.join("HaexVault").join(&space_subfolder);

        std::fs::create_dir_all(&target_dir).map_err(|e| PeerStorageError::ProtocolError {
            reason: format!("Failed to create downloads dir: {e}"),
        })?;
        let file_name = std::path::Path::new(&path)
            .file_name()
            .unwrap_or(std::ffi::OsStr::new("download"))
            .to_string_lossy()
            .to_string();

        // Land at the canonical name — no `(1)`/`(2)` suffixing. The
        // HaexVault/<space>/ subfolder is our managed area, scoped to a
        // single peer's view of a single space, so "the file at this
        // (peer, remote_path)" has exactly one canonical local name. When
        // the registry already records that download we short-circuit
        // above; falling through to this point means we want a fresh copy,
        // which should replace the previous bytes rather than accumulate
        // numbered duplicates.
        target_dir.join(&file_name)
    };

    // Create cancel + pause controls for this transfer. Reject duplicates so
    // a colliding id can't orphan an in-flight download's token.
    let (cancel_token, pause_flag) = if let Some(ref tid) = transfer_id {
        let cancel = tokio_util::sync::CancellationToken::new();
        let pause = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut tokens = state.transfer_tokens.lock().await;
        if tokens.contains_key(tid) {
            return Err(PeerStorageError::ProtocolError {
                reason: format!("transferId {tid} already in flight"),
            });
        }
        tokens.insert(tid.clone(), (cancel.clone(), pause.clone()));
        (Some(cancel), Some(pause))
    } else {
        (None, None)
    };

    let output_path_str = output_path.to_string_lossy().to_string();
    let app_handle = app.clone();
    // Captures for the registry insert on successful completion. `node_id`
    // and `path` are the lookup key; the others let us short-circuit a
    // future re-download.
    let registry_node_id = node_id.clone();
    let registry_remote_path = path.clone();
    let registry_modified = expected_modified;
    let android_sub_path = format!("HaexVault/{space_subfolder}");

    // Reaper holds an independent clone of the channel and the transfer_id
    // so that if the download task itself panics (and therefore drops its
    // `on_event` without ever sending Error), the JoinError observed here
    // is surfaced to the frontend as a TransferEvent::Error. Without this,
    // a panic in the spawned future would close the channel silently and
    // the frontend transfer store would hang in "in-flight" forever.
    let on_event_reaper = on_event.clone();
    let transfer_id_for_reaper = transfer_id.clone();
    let app_handle_for_reaper = app.clone();

    // Spawn the download on a separate task. The IPC handler returns immediately
    // with the target path. Progress/completion/errors are streamed via the Channel.
    let download_handle = tokio::spawn(async move {
        let state = app_handle.state::<AppState>();

        // Progress callback with throttling: at most every 100ms to avoid
        // overwhelming the IPC bridge on mobile (each message crosses JNI/WebView).
        //
        // Multi-stream downloads call this from up to 4 parallel tasks, so we
        // hold the throttle timestamp and the last-emitted byte count under
        // one lock and clamp `received` to the running max. Without the clamp
        // a thread whose `cb()` runs after a larger `received` would emit a
        // smaller `bytes_received`, breaking the frontend's delta-based EMA.
        let on_event_progress = on_event.clone();
        let progress_cb: Arc<dyn Fn(u64, u64) + Send + Sync> = Arc::new({
            let state = std::sync::Mutex::new((
                std::time::Instant::now() - std::time::Duration::from_secs(1),
                0_u64, // last emitted bytes_received — monotonically clamped
            ));
            move |received: u64, total: u64| {
                let now = std::time::Instant::now();
                let mut guard = state.lock().unwrap_or_else(|e| e.into_inner());
                let monotonic = received.max(guard.1);
                let should_emit =
                    monotonic >= total || now.duration_since(guard.0).as_millis() >= 100;
                if should_emit {
                    guard.0 = now;
                    guard.1 = monotonic;
                    let _ = on_event_progress.send(TransferEvent::Progress {
                        bytes_received: monotonic,
                        total_bytes: total,
                    });
                }
            }
        });

        let result = crate::peer_storage::client::download_file_to_path(
            state.peer_storage.clone(),
            remote_id,
            parsed_relay,
            path.clone(),
            output_path.clone(),
            // File-browser flow has no manifest; the stat-probe response
            // supplies the chunked hash that governs verification.
            None,
            Some(progress_cb),
            cancel_token,
            pause_flag,
            ucan_token.clone(),
        )
        .await;

        // Clean up cancel token
        if let Some(tid) = &transfer_id {
            state.transfer_tokens.lock().await.remove(tid);
        }

        match result {
            Ok(stream_result) => {
                let final_path =
                    move_to_public_downloads(&app_handle, &output_path, Some(&android_sub_path));
                // Record the successful download so the next click on the
                // same (peer, path) can skip the network. If the insert
                // fails we log and keep going — a failed registry write
                // just means the user pays the cost of one more re-download
                // next time, not a transfer-failed outcome.
                if let Err(e) = crate::peer_storage::downloads::upsert(
                    &state.db,
                    &registry_node_id,
                    &registry_remote_path,
                    stream_result.bytes,
                    registry_modified,
                    &final_path,
                ) {
                    eprintln!("[peer_storage] Failed to record download in registry: {e}");
                }
                let _ = on_event.send(TransferEvent::Complete {
                    local_path: final_path,
                    total_bytes: stream_result.bytes,
                });
            }
            Err(e) => {
                let _ = on_event.send(TransferEvent::Error {
                    error: e.to_string(),
                });
            }
        }
    });

    // Reaper: await the download handle and surface a JoinError (panic /
    // runtime cancellation) as a TransferEvent::Error so the frontend sees
    // the failure instead of an orphaned transfer. Normal completion is a
    // no-op here because the download task already sent Complete/Error on
    // `on_event` before returning.
    tokio::spawn(async move {
        if let Err(join_err) = download_handle.await {
            eprintln!("[peer_storage] remote_read download task terminated abnormally: {join_err}");
            let _ = on_event_reaper.send(TransferEvent::Error {
                error: format!("download task crashed: {join_err}"),
            });
            // Also drop the transfer token registration so a subsequent
            // request with the same id is not blocked as "already in flight".
            if let Some(tid) = transfer_id_for_reaper {
                let state = app_handle_for_reaper.state::<AppState>();
                state.transfer_tokens.lock().await.remove(&tid);
            }
        }
    });

    Ok(output_path_str)
}

/// Upload a local file to a remote peer.
///
/// Mirrors [`peer_storage_remote_read`]: spawns the streaming write in a
/// background task and reports progress/completion/errors via the supplied
/// `on_event` channel. Returns immediately after the task is spawned.
///
/// If `transfer_id` is provided a [`CancellationToken`] is registered in
/// `AppState.transfer_tokens` so the existing `peer_storage_transfer_cancel`
/// command can abort the upload — same control surface as downloads.
#[tauri::command(rename_all = "camelCase")]
pub async fn peer_storage_remote_write(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    node_id: String,
    relay_url: Option<String>,
    path: String,
    source_path: String,
    transfer_id: Option<String>,
    ucan_token: String,
    on_event: Channel<TransferEvent>,
) -> Result<(), PeerStorageError> {
    let remote_id: iroh::EndpointId =
        node_id
            .parse()
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: format!("Invalid EndpointId: {e}"),
            })?;
    let parsed_relay = relay_url.and_then(|s| s.parse::<iroh::RelayUrl>().ok());

    // Register cancel token under the transfer id so the existing
    // peer_storage_transfer_cancel command can abort this upload. Reject
    // duplicates so a colliding id can't orphan an in-flight upload's token.
    let cancel_token = if let Some(ref tid) = transfer_id {
        let cancel = tokio_util::sync::CancellationToken::new();
        let pause = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut tokens = state.transfer_tokens.lock().await;
        if tokens.contains_key(tid) {
            return Err(PeerStorageError::ProtocolError {
                reason: format!("transferId {tid} already in flight"),
            });
        }
        tokens.insert(tid.clone(), (cancel.clone(), pause));
        Some(cancel)
    } else {
        None
    };

    let app_handle = app.clone();
    let source_path_buf = PathBuf::from(&source_path);
    let on_event_progress = on_event.clone();

    tokio::spawn(async move {
        let state = app_handle.state::<AppState>();

        // 100ms throttling on progress emits — same window the read path uses.
        let progress_cb: Option<Box<dyn Fn(u64, u64) + Send>> = Some({
            let last_emit = std::sync::Mutex::new(
                std::time::Instant::now() - std::time::Duration::from_secs(1),
            );
            Box::new(move |sent: u64, total: u64| {
                let now = std::time::Instant::now();
                let should_emit = {
                    let last = last_emit.lock().unwrap_or_else(|e| e.into_inner());
                    sent >= total || now.duration_since(*last).as_millis() >= 100
                };
                if should_emit {
                    *last_emit.lock().unwrap_or_else(|e| e.into_inner()) = now;
                    let _ = on_event_progress.send(TransferEvent::Progress {
                        bytes_received: sent,
                        total_bytes: total,
                    });
                }
            }) as Box<dyn Fn(u64, u64) + Send>
        });

        let options = crate::peer_storage::streaming::SendOptions {
            on_progress: progress_cb,
            cancel_token,
        };

        let result = {
            let endpoint = state.peer_storage.read().await;
            endpoint
                .remote_write_file(
                    remote_id,
                    parsed_relay,
                    &path,
                    &source_path_buf,
                    &ucan_token,
                    options,
                )
                .await
        };

        if let Some(tid) = &transfer_id {
            state.transfer_tokens.lock().await.remove(tid);
        }

        match result {
            Ok(bytes) => {
                let _ = on_event.send(TransferEvent::Complete {
                    local_path: source_path,
                    total_bytes: bytes,
                });
            }
            Err(e) => {
                let _ = on_event.send(TransferEvent::Error {
                    error: e.to_string(),
                });
            }
        }
    });

    Ok(())
}

/// Create a directory on a remote peer.
#[tauri::command(rename_all = "camelCase")]
pub async fn peer_storage_remote_create_directory(
    state: State<'_, AppState>,
    node_id: String,
    relay_url: Option<String>,
    path: String,
    ucan_token: String,
) -> Result<(), PeerStorageError> {
    let remote_id: iroh::EndpointId =
        node_id
            .parse()
            .map_err(|e| PeerStorageError::ConnectionFailed {
                reason: format!("Invalid EndpointId: {e}"),
            })?;
    let parsed_relay = relay_url.and_then(|s| s.parse::<iroh::RelayUrl>().ok());

    let endpoint = state.peer_storage.read().await;
    endpoint
        .remote_create_directory(remote_id, parsed_relay, &path, &ucan_token)
        .await
}
