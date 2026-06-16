//! Tauri commands for peer storage
//!
//! ## Mutex poisoning in `last_emit` throttle locks
//!
//! The per-download progress callbacks (around lines 520-650 / 640-770) use
//! `Mutex<Instant>` locks with `unwrap_or_else(|e| e.into_inner())`. These
//! are throttling timestamps — a poison means at worst one extra progress
//! event slips through before throttling resumes. No data is at risk and no
//! CRDT path is involved, so a critical-failure banner would be misleading.
//! The HLC lock at the top of `peer_storage_start` DOES use `lock_or_fail`.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Manager, State};
use tauri::ipc::Channel;

use crate::critical::CriticalFailureCode;
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
    let sql = "SELECT id, name, local_path, space_id FROM haex_peer_shares WHERE endpoint_id = ?1".to_string();
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

/// Load the device's own (DID, signing key) for the quic_did_auth handshake.
/// Joins `haex_devices` (filtered to this endpoint's row) against
/// `haex_identities` to fetch the PKCS8-base64 private key for the identity
/// pinned in `owner_did`.
fn load_own_identity_for_device(
    state: &AppState,
    own_endpoint_id: &str,
) -> Result<crate::peer_storage::endpoint::OwnIdentity, PeerStorageError> {
    let sql = "SELECT i.did, i.private_key \
               FROM haex_devices d \
               JOIN haex_identities i ON i.did = d.owner_did \
               WHERE d.endpoint_id = ?1 \
               LIMIT 1"
        .to_string();
    let params = vec![serde_json::Value::String(own_endpoint_id.to_string())];

    let rows = crate::database::core::select_with_crdt(sql, params, &state.db)
        .map_err(|e| PeerStorageError::Database { reason: e.to_string() })?;

    let row = rows.first().ok_or_else(|| PeerStorageError::Database {
        reason: format!("no haex_devices row for endpoint_id {own_endpoint_id}"),
    })?;

    let did = row
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| PeerStorageError::Database {
            reason: "missing did column".into(),
        })?
        .to_string();

    let private_key_b64 = row
        .get(1)
        .and_then(|v| v.as_str())
        .ok_or_else(|| PeerStorageError::Database {
            reason: format!("identity row for {did} has no private_key — cannot sign DID-auth"),
        })?
        .to_string();

    let signing_key = crate::ucan::signing_key_from_pkcs8_base64(&private_key_b64)
        .map_err(|e| PeerStorageError::Database {
            reason: format!("decoding identity private_key for {did}: {e}"),
        })?;

    // Refuse to load a (did, private_key) pair whose halves don't match:
    // a drifted row would otherwise authenticate as `did` to peers but be
    // unable to sign for it, surfacing only later as silent handshake
    // failures. Encode the public key derived from the private key as a
    // did:key and compare against the row's stored DID — they must be byte
    // identical.
    let derived_did = crate::ucan::did_key_from_public_key(&signing_key.verifying_key());
    if derived_did != did {
        return Err(PeerStorageError::Database {
            reason: format!(
                "identity drift for endpoint_id={own_endpoint_id}: row says did={did} but private_key encodes did={derived_did}"
            ),
        });
    }

    Ok(crate::peer_storage::endpoint::OwnIdentity { did, signing_key })
}

/// Load the expected `(endpoint_id -> owner_did)` map for every peer we
/// could legitimately accept connections from. The query joins
/// `haex_space_devices` (cross-vault, UCAN-attributed) against
/// `haex_devices` (vault-private, populated by the
/// `haex_space_devices_ensure_refs` trigger from `authored_by_did`). The
/// result is the DB-side ground truth that the quic_did_auth handshake's
/// crypto-verified DID is cross-checked against in `handle_connection`.
///
/// Excludes our own endpoint and skips rows whose `owner_did` is NULL —
/// the latter only happens transiently during a partial sync and we'd
/// rather reject the peer than risk admitting an unverifiable one.
fn load_peer_owner_dids(
    state: &AppState,
    own_endpoint_id: &str,
) -> Result<HashMap<String, String>, PeerStorageError> {
    let sql = "SELECT DISTINCT sd.endpoint_id, d.owner_did \
               FROM haex_space_devices sd \
               JOIN haex_devices d ON d.id = sd.device_id \
               WHERE sd.endpoint_id != ?1 \
                 AND d.owner_did IS NOT NULL"
        .to_string();
    let params = vec![serde_json::Value::String(own_endpoint_id.to_string())];

    let rows = crate::database::core::select_with_crdt(sql, params, &state.db)
        .map_err(|e| PeerStorageError::Database { reason: e.to_string() })?;

    // Two passes: first gather every distinct (endpoint_id, owner_did)
    // pair, then accept only endpoint_ids that map to exactly one DID.
    // A single-pass loop that removed on conflict would silently let a
    // later row reinstate a conflicted endpoint, making acceptance depend
    // on SQL row order.
    use std::collections::HashSet as StdHashSet;
    let mut candidates: HashMap<String, StdHashSet<String>> = HashMap::new();
    for row in &rows {
        let endpoint_id = row.first().and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let owner_did = row.get(1).and_then(|v| v.as_str()).unwrap_or_default().to_string();
        if endpoint_id.is_empty() || owner_did.is_empty() {
            continue;
        }
        candidates.entry(endpoint_id).or_default().insert(owner_did);
    }

    let mut map: HashMap<String, String> = HashMap::new();
    for (endpoint_id, dids) in candidates {
        if dids.len() == 1 {
            map.insert(
                endpoint_id,
                dids.into_iter()
                    .next()
                    .expect("invariant: dids.len() == 1 checked above"),
            );
        } else {
            // Multiple distinct owner_dids for the same endpoint_id: cannot
            // pick a side safely. Drop permanently so handle_connection
            // rejects rather than admit an arbitrary one.
            let did_list: Vec<String> = dids.into_iter().collect();
            eprintln!(
                "[PeerStorage] Ambiguous owner_did for endpoint_id {endpoint_id}: \
                 {did_list:?} — dropping from peer_owner_dids map"
            );
        }
    }
    Ok(map)
}

/// Load allowed peers from haex_space_devices.
/// Returns a map: remote EndpointId (string) -> set of space_ids they may access.
/// Excludes our own endpoint ID.
fn load_allowed_peers_from_db(
    state: &AppState,
    own_endpoint_id: &str,
) -> Result<HashMap<String, HashSet<String>>, PeerStorageError> {
    let sql = "SELECT endpoint_id, space_id FROM haex_space_devices WHERE endpoint_id != ?1".to_string();
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
    // Keep the DID cross-check map in lock-step: a peer that just appeared
    // in allowed_peers but not yet in peer_owner_dids would be rejected by
    // handle_connection. Same SQL pass updates both.
    let owner_dids = load_peer_owner_dids(state, &endpoint_id)?;
    endpoint.set_peer_owner_dids(owner_dids).await;
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
    let peer_owner_dids = load_peer_owner_dids(state, &endpoint_id)?;

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
    endpoint.set_peer_owner_dids(peer_owner_dids).await;

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

    // Bail out before mutating endpoint state. `set_own_identity` below
    // asserts when the endpoint is already running, so without this guard
    // a second `peer_storage_start` call would panic instead of returning
    // the recoverable `EndpointAlreadyRunning` error that `start` raises.
    if endpoint.is_running() {
        return Err(PeerStorageError::EndpointAlreadyRunning);
    }

    // Store AppHandle so the accept loop can use android_fs for Content URI shares
    endpoint.set_app_handle(app.clone()).await;

    // Load the device's identity so the quic_did_auth handshake can prove
    // our DID to peers and reject inbound peers that present mismatched
    // UCANs. Must run before `start` — the accept loop is spawned inside
    // `start` and reads the identity slot from then on.
    let own_endpoint_id = endpoint.endpoint_id().to_string();
    let own_identity = load_own_identity_for_device(&state, &own_endpoint_id)?;
    endpoint.set_own_identity(own_identity.clone());

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
            let hlc_clone = state
                .lock_or_fail(
                    &state.hlc,
                    CriticalFailureCode::HlcMutexPoisoned,
                    "peer_storage::commands::peer_storage_start",
                    serde_json::json!({}),
                )?
                .clone();
            let handler = std::sync::Arc::new(
                crate::space_delivery::local::multi_leader::MultiSpaceLeaderHandler {
                    leaders: state.leader_state.clone(),
                    db: db_conn,
                    hlc: std::sync::Arc::new(std::sync::Mutex::new(hlc_clone)),
                    app_handle: app.clone(),
                    own_endpoint_id: own_endpoint_id.clone(),
                    own_identity: std::sync::Arc::new(std::sync::Mutex::new(None)),
                    endpoint_dids: std::sync::Arc::new(
                        tokio::sync::RwLock::new(std::collections::HashMap::new()),
                    ),
                },
            );
            // Mirror the same device identity onto the delivery handler so
            // its server-initiated quic_did_auth handshake can run on every
            // incoming connection. Same identity material as peer_storage —
            // the slot exists per-handler so peer_storage and space_delivery
            // can be configured independently if that ever becomes useful.
            handler.set_own_identity(own_identity.clone());
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

/// Report whether the active QUIC connection to a peer runs over a direct
/// LAN/WAN path or via the relay. Used to diagnose throughput problems —
/// relay-routed connections cap at ~1 MB/s per stream, which looks like a
/// code-tuning issue but is actually a NAT/hole-punch failure.
///
/// Returns `None` if there is no live cached connection. The caller should
/// first establish one (e.g. via `peer_storage_remote_list`) before asking.
#[tauri::command(rename_all = "camelCase")]
pub async fn peer_storage_diagnose_connection(
    state: State<'_, AppState>,
    node_id: String,
) -> Result<Option<crate::peer_storage::endpoint::ConnectionDiagnostics>, PeerStorageError> {
    let remote_id: iroh::EndpointId = node_id
        .parse()
        .map_err(|e| PeerStorageError::ConnectionFailed {
            reason: format!("Invalid EndpointId: {e}"),
        })?;

    let endpoint = state.peer_storage.read().await;
    Ok(endpoint.diagnose_connection(remote_id))
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
    expected_size: Option<u64>,
    expected_modified: Option<u64>,
    space_folder: Option<String>,
    space_id: Option<String>,
    ucan_token: String,
    on_event: Channel<TransferEvent>,
) -> Result<String, PeerStorageError> {
    let remote_id: iroh::EndpointId = node_id
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
        (None, Some(id)) => {
            crate::peer_storage::downloads::sanitize_folder_segment(id, "default")
        }
        (None, None) => "default".to_string(),
    };

    // Determine the on-disk staging path. On desktop this is the final
    // location (Downloads/HaexVault/<space>/<file>). On Android it's the
    // app-private staging path — `move_to_public_downloads` later copies it
    // into MediaStore's public Downloads under the same relative layout.
    let output_path = if let Some(ref dest) = save_to {
        PathBuf::from(dest)
    } else {
        let downloads_dir = app.path().download_dir()
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

    // Spawn the download on a separate task. The IPC handler returns immediately
    // with the target path. Progress/completion/errors are streamed via the Channel.
    tokio::spawn(async move {
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
                let final_path = move_to_public_downloads(
                    &app_handle,
                    &output_path,
                    Some(&android_sub_path),
                );
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
    let remote_id: iroh::EndpointId = node_id
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
    let remote_id: iroh::EndpointId = node_id
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
/// file manager. The `sub_path` parameter places the file under a relative
/// directory inside Downloads (e.g. `HaexVault/My Space`) — MediaStore creates
/// the directory chain on demand. Returns the FileUri JSON string of the
/// public file on Android, or the original path string on other platforms.
fn move_to_public_downloads(
    #[allow(unused_variables)] app_handle: &tauri::AppHandle,
    output_path: &std::path::Path,
    #[allow(unused_variables)] sub_path: Option<&str>,
) -> String {
    #[cfg(target_os = "android")]
    {
        use tauri_plugin_android_fs::{AndroidFsExt, PublicGeneralPurposeDir};

        let file_name = output_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        // MediaStore takes a single relative_path that includes the file name.
        // Build `HaexVault/<space>/<file>` here so the dir chain materialises.
        let relative_path = match sub_path {
            Some(s) if !s.is_empty() => format!("{s}/{file_name}"),
            _ => file_name.clone(),
        };

        let result: Result<String, String> = (|| {
            let api = app_handle.android_fs();
            let ps = api.public_storage();

            let dest_uri = ps.create_new_file(
                None,
                PublicGeneralPurposeDir::Download,
                &relative_path,
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

/// Verify that a previously-recorded local path still references a file
/// with the expected size. Returns `true` only on an exact match — any I/O
/// error, missing target, or size mismatch is treated as a cache miss and
/// triggers a fresh download.
///
/// On desktop, `local_path` is a filesystem path. On Android, it is a
/// JSON-encoded `FileUri` pointing at a MediaStore entry; we call
/// `android_fs.get_len` which returns an error if the user has deleted
/// the file via the system file manager.
fn verify_local_target_intact(
    #[allow(unused_variables)] app_handle: &tauri::AppHandle,
    local_path: &str,
    expected_size: u64,
) -> bool {
    #[cfg(target_os = "android")]
    {
        if local_path.starts_with('{') {
            use tauri_plugin_android_fs::{AndroidFsExt, FileUri};
            let Ok(uri) = FileUri::from_json_str(local_path) else {
                return false;
            };
            return app_handle
                .android_fs()
                .get_len(&uri)
                .map(|len| len == expected_size)
                .unwrap_or(false);
        }
        // Fall through to filesystem check for non-URI paths (legacy rows).
    }

    match std::fs::metadata(local_path) {
        Ok(m) => m.is_file() && m.len() == expected_size,
        Err(_) => false,
    }
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
