//! Autonomous sync loop for local spaces.
//!
//! Runs entirely in Rust: connects to leader, pushes dirty changes,
//! pulls remote changes, applies them to local DB, and emits Tauri events.

use std::collections::HashSet;
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use tauri::{Emitter, Manager};
use tokio::sync::watch;

use crate::crdt::commands::{apply_remote_changes_to_db, clear_dirty_table_inner, RemoteColumnChange};
use crate::crdt::hlc::hlc_max;
use crate::crdt::scanner::{scan_all_dirty_tables_for_local_changes, LocalColumnChange};
use crate::database::DbConnection;
use super::error::DeliveryError;
use super::peer::PeerSession;

/// Default poll interval between sync cycles.
const POLL_INTERVAL: Duration = Duration::from_secs(5);


/// Maximum backoff duration for reconnection attempts.
const MAX_RECONNECT_BACKOFF: Duration = Duration::from_secs(60);

/// Soft cap for changes per QUIC push request. Mirrors the HTTP path's
/// `PUSH_CHUNK_SOFT_LIMIT` — see `src/stores/sync/orchestrator/push.ts`.
/// A single transaction-HLC group larger than this is still sent in one
/// request rather than split.
const PUSH_CHUNK_SOFT_LIMIT: usize = 2000;

/// Splits an HLC-sorted slice of local changes into HLC-aligned chunks.
///
/// Contract matches the TypeScript `chunkChangesByHlc`:
/// - Input must be sorted by hlc_timestamp ascending.
/// - An HLC group is never split between chunks.
/// - A group larger than `soft_limit` becomes its own oversized chunk.
fn chunk_changes_by_hlc(
    changes: &[LocalColumnChange],
    soft_limit: usize,
) -> Vec<&[LocalColumnChange]> {
    if changes.is_empty() {
        return Vec::new();
    }

    let mut chunks: Vec<&[LocalColumnChange]> = Vec::new();
    let mut chunk_start = 0usize;
    let mut group_start = 0usize;
    let mut chunk_len = 0usize;

    for i in 1..=changes.len() {
        let boundary = i == changes.len()
            || changes[i].hlc_timestamp != changes[i - 1].hlc_timestamp;
        if !boundary {
            continue;
        }

        let group_size = i - group_start;
        // Would appending the completed group exceed the limit? If so, emit
        // the current chunk first. A group bigger than `soft_limit` still
        // goes into one chunk — HLC atomicity trumps chunk size.
        if chunk_len > 0 && chunk_len + group_size > soft_limit {
            chunks.push(&changes[chunk_start..group_start]);
            chunk_start = group_start;
            chunk_len = 0;
        }
        chunk_len += group_size;
        group_start = i;
    }

    if chunk_len > 0 {
        chunks.push(&changes[chunk_start..]);
    }
    chunks
}

/// Handle to a running sync loop. Call `stop()` to terminate.
pub struct SyncLoopHandle {
    stop_sender: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}

impl SyncLoopHandle {
    /// Signal the sync loop to stop.
    pub fn stop(&self) {
        let _ = self.stop_sender.send(true);
    }

    /// Check if the sync loop task has finished.
    pub fn is_finished(&self) -> bool {
        self.task.is_finished()
    }
}

/// Start the sync loop as a peer connecting to a leader.
///
/// The loop will:
/// 1. Connect to the leader via `PeerSession`
/// 2. Scan dirty tables for outbound changes
/// 3. Push changes to the leader
/// 4. Pull changes from the leader
/// 5. Apply them to the local DB
/// 6. Emit Tauri events for frontend UI refresh
/// 7. Repeat with a poll interval, stoppable via the returned handle
pub async fn start_peer_sync_loop(
    db: DbConnection,
    iroh_endpoint: iroh::Endpoint,
    leader_endpoint_id: String,
    leader_relay_url: Option<String>,
    space_id: String,
    our_did: String,
    our_endpoint_id: String,
    device_id: String,
    app_handle: tauri::AppHandle,
) -> Result<SyncLoopHandle, DeliveryError> {
    // Establish initial connection. UCAN is loaded from the DB inside
    // `PeerSession::connect`, so reconnect-after-expiry gets a fresh token
    // without any state plumbing up here.
    let session = PeerSession::connect(
        &iroh_endpoint,
        &leader_endpoint_id,
        leader_relay_url.as_deref(),
        &space_id,
        &our_did,
        &our_endpoint_id,
        Some("sync-loop"),
        &db,
    )
    .await?;

    let (stop_tx, stop_rx) = watch::channel(false);

    let task = tokio::spawn(run_sync_loop(
        db,
        iroh_endpoint,
        session,
        leader_endpoint_id,
        leader_relay_url,
        space_id,
        our_did,
        our_endpoint_id,
        device_id,
        app_handle,
        stop_rx,
    ));

    Ok(SyncLoopHandle {
        stop_sender: stop_tx,
        task,
    })
}

/// Convert a `LocalColumnChange` to a `RemoteColumnChange` for the apply function.
pub fn local_to_remote_change(local: &LocalColumnChange) -> RemoteColumnChange {
    RemoteColumnChange {
        table_name: local.table_name.clone(),
        row_pks: local.row_pks.clone(),
        column_name: local.column_name.clone(),
        hlc_timestamp: local.hlc_timestamp.clone(),
        decrypted_value: local.value.clone(),
    }
}

/// The main sync loop. Runs until the stop signal is received.
async fn run_sync_loop(
    db: DbConnection,
    iroh_endpoint: iroh::Endpoint,
    mut session: PeerSession,
    leader_endpoint_id: String,
    leader_relay_url: Option<String>,
    space_id: String,
    our_did: String,
    our_endpoint_id: String,
    device_id: String,
    app_handle: tauri::AppHandle,
    mut stop_rx: watch::Receiver<bool>,
) {
    let mut last_push_hlc: Option<String> = None;
    let mut last_pull_timestamp: Option<String> = None;
    let mut last_mls_message_id: Option<i64> = None;
    let mut key_packages_refilled = false;

    eprintln!("[SyncLoop] Started for space {}", space_id);

    loop {
        // Check if stop was requested
        if *stop_rx.borrow() {
            eprintln!("[SyncLoop] Stop signal received, exiting");
            break;
        }

        match run_sync_cycle(
            &db,
            &session,
            &space_id,
            &device_id,
            &app_handle,
            &mut last_push_hlc,
            &mut last_pull_timestamp,
            &mut last_mls_message_id,
            &mut key_packages_refilled,
        )
        .await
        {
            Ok(()) => {
                // Cycle completed successfully, wait for next cycle or stop signal
                tokio::select! {
                    _ = tokio::time::sleep(POLL_INTERVAL) => {},
                    _ = stop_rx.changed() => {
                        eprintln!("[SyncLoop] Stop signal received during sleep, exiting");
                        break;
                    },
                }
            }
            Err(e) => {
                eprintln!("[SyncLoop] Sync cycle failed: {}", e);

                // Attempt reconnection with exponential backoff
                let mut backoff = Duration::from_secs(5);
                loop {
                    if *stop_rx.borrow() {
                        eprintln!("[SyncLoop] Stop signal received during reconnect, exiting");
                        session.close();
                        return;
                    }

                    eprintln!(
                        "[SyncLoop] Reconnecting in {}s...",
                        backoff.as_secs()
                    );

                    // Emit error event for frontend
                    let _ = app_handle.emit(
                        "local-sync-error",
                        serde_json::json!({
                            "spaceId": space_id,
                            "error": e.to_string(),
                            "reconnecting": true,
                        }),
                    );

                    // Wait for backoff duration or stop signal
                    tokio::select! {
                        _ = tokio::time::sleep(backoff) => {},
                        _ = stop_rx.changed() => {
                            eprintln!("[SyncLoop] Stop signal received during backoff, exiting");
                            session.close();
                            return;
                        },
                    }

                    // Try to reconnect — pulls the current UCAN from the DB,
                    // so a token renewed during the outage takes effect here.
                    match PeerSession::connect(
                        &iroh_endpoint,
                        &leader_endpoint_id,
                        leader_relay_url.as_deref(),
                        &space_id,
                        &our_did,
                        &our_endpoint_id,
                        Some("sync-loop"),
                        &db,
                    )
                    .await
                    {
                        Ok(new_session) => {
                            eprintln!("[SyncLoop] Reconnected successfully");
                            session = new_session;
                            break;
                        }
                        Err(reconnect_err) => {
                            eprintln!("[SyncLoop] Reconnection failed: {}", reconnect_err);
                            backoff = (backoff * 2).min(MAX_RECONNECT_BACKOFF);
                        }
                    }
                }
            }
        }
    }

    session.close();
    eprintln!("[SyncLoop] Stopped for space {}", space_id);
}

/// Execute a single push+pull sync cycle.
async fn run_sync_cycle(
    db: &DbConnection,
    session: &PeerSession,
    space_id: &str,
    device_id: &str,
    app_handle: &tauri::AppHandle,
    last_push_hlc: &mut Option<String>,
    last_pull_timestamp: &mut Option<String>,
    last_mls_message_id: &mut Option<i64>,
    key_packages_refilled: &mut bool,
) -> Result<(), DeliveryError> {
    // 1. PUSH: Scan dirty tables for local changes
    let changes = scan_all_dirty_tables_for_local_changes(
        db,
        last_push_hlc.as_deref(),
        device_id,
    )
    .map_err(|e| DeliveryError::Database {
        reason: format!("Failed to scan dirty tables: {}", e),
    })?;

    if !changes.is_empty() {
        // Chunk at HLC boundaries so a transaction-HLC group is never split
        // across QUIC requests. The scanner already returns changes sorted
        // by hlc_timestamp globally, so a single linear pass is enough.
        let chunks = chunk_changes_by_hlc(&changes, PUSH_CHUNK_SOFT_LIMIT);

        eprintln!(
            "[SyncLoop] Pushing {} changes in {} HLC-aligned chunk(s) for space {}",
            changes.len(),
            chunks.len(),
            space_id
        );

        // Collect affected table names and the max HLC across all chunks —
        // used for dirty-table cleanup after every chunk succeeded.
        let pushed_table_names: HashSet<String> = changes
            .iter()
            .map(|c| c.table_name.clone())
            .collect();

        // Record the timestamp for clearing dirty tables.
        let push_timestamp = sqlite_datetime_now();

        for (idx, chunk) in chunks.iter().enumerate() {
            let chunk_max_hlc = hlc_max(chunk.iter().map(|c| c.hlc_timestamp.as_str()))
                .unwrap_or("")
                .to_string();

            let chunk_json = serde_json::to_value(chunk).map_err(|e| {
                DeliveryError::ProtocolError {
                    reason: format!("Failed to serialize chunk {}: {}", idx, e),
                }
            })?;

            session.push_changes(space_id, chunk_json).await?;

            // Checkpoint after each successful chunk so a later failure does
            // not re-push completed groups. The scanner will pick up whatever
            // remains on the next cycle.
            *last_push_hlc = Some(chunk_max_hlc);
        }

        // Clear dirty tables only after the whole batch succeeded. Any mid-
        // loop failure leaves them dirty so the scanner re-emits the groups.
        for table_name in &pushed_table_names {
            if let Err(e) = clear_dirty_table_inner(db, table_name, Some(&push_timestamp)) {
                eprintln!(
                    "[SyncLoop] Warning: failed to clear dirty table '{}': {}",
                    table_name, e
                );
            }
        }
    }

    // 2. PULL: Get changes from leader
    let remote_changes_json = session
        .pull_changes(space_id, last_pull_timestamp.as_deref())
        .await?;

    if let Some(changes_array) = remote_changes_json.as_array() {
        if !changes_array.is_empty() {
            eprintln!(
                "[SyncLoop] Pulled {} changes for space {}",
                changes_array.len(),
                space_id
            );

            // Deserialize into LocalColumnChange format (same JSON shape)
            let remote_locals: Vec<LocalColumnChange> =
                serde_json::from_value(remote_changes_json.clone()).map_err(|e| {
                    DeliveryError::ProtocolError {
                        reason: format!("Failed to deserialize pulled changes: {}", e),
                    }
                })?;

            if !remote_locals.is_empty() {
                // Convert LocalColumnChange -> RemoteColumnChange (HLC is the grouping key)
                let remote_changes: Vec<RemoteColumnChange> = remote_locals
                    .iter()
                    .map(local_to_remote_change)
                    .collect();

                // Find the max HLC from pulled changes
                let max_pulled_hlc = hlc_max(remote_locals.iter().map(|c| c.hlc_timestamp.as_str()))
                    .unwrap_or("")
                    .to_string();

                // Collect affected table names for the event
                let affected_tables: Vec<String> = remote_locals
                    .iter()
                    .map(|c| c.table_name.clone())
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect();

                // Apply remote changes to local DB (no backend_info for local delivery).
                // HLC clock is advanced internally by apply_remote_changes_to_db.
                let hlc_service = {
                    let state: tauri::State<'_, crate::AppState> = app_handle.state();
                    state.hlc.lock().ok().map(|guard| guard.clone())
                };
                apply_remote_changes_to_db(db, remote_changes, None, hlc_service.as_ref())
                    .map_err(|e| {
                        DeliveryError::Database {
                            reason: format!("Failed to apply remote changes: {}", e),
                        }
                    })?;

                // Update last_pull_timestamp
                if !max_pulled_hlc.is_empty() {
                    *last_pull_timestamp = Some(max_pulled_hlc);
                }

                // Emit Tauri event for frontend UI refresh
                let _ = app_handle.emit(
                    "local-sync-completed",
                    serde_json::json!({
                        "spaceId": space_id,
                        "tables": affected_tables,
                    }),
                );
            }
        }
    }

    // 3. MLS: Fetch commits from leader, process, and ACK
    if let Err(e) = fetch_and_process_mls_messages(db, session, space_id, last_mls_message_id, app_handle).await {
        eprintln!("[SyncLoop] MLS message processing failed: {e}");
        // Non-fatal: CRDT sync still worked, MLS will retry next cycle
    }

    // 4. KeyPackage refill: run once per session (ClaimInvite already uploads 10)
    if !*key_packages_refilled {
        match refill_key_packages_if_needed(db, session, space_id).await {
            Ok(()) => *key_packages_refilled = true,
            Err(e) => eprintln!("[SyncLoop] KeyPackage refill failed (will retry next cycle): {e}"),
        }
    }

    Ok(())
}

/// Fetch MLS messages from leader, process them locally, and send ACKs.
async fn fetch_and_process_mls_messages(
    db: &DbConnection,
    session: &PeerSession,
    space_id: &str,
    last_mls_message_id: &mut Option<i64>,
    app_handle: &tauri::AppHandle,
) -> Result<(), DeliveryError> {
    let messages = session
        .fetch_mls_messages(space_id, *last_mls_message_id)
        .await?;

    if messages.is_empty() {
        return Ok(());
    }

    eprintln!(
        "[SyncLoop] Processing {} MLS message(s) for space {}",
        messages.len(),
        space_id
    );

    let mut acked_ids = Vec::new();

    for msg in &messages {
        let blob = match BASE64.decode(&msg.message) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("[SyncLoop] Failed to decode MLS message {}: {e}", msg.id);
                continue;
            }
        };

        match crate::mls::blocking::process_message(db.0.clone(), space_id.to_string(), blob).await {
            Ok(_) => {
                acked_ids.push(msg.id);
                *last_mls_message_id = Some(msg.id);
                eprintln!(
                    "[SyncLoop] Processed MLS {} message (id={})",
                    msg.message_type, msg.id
                );
            }
            Err(e) => {
                eprintln!(
                    "[SyncLoop] Failed to process MLS message {}: {e}",
                    msg.id
                );

                // Detect epoch gap — attempt rejoin via External Commit
                if e.contains("epoch") || e.contains("Welcome") || e.contains("group") {
                    eprintln!("[SyncLoop] Possible epoch gap detected, attempting rejoin for space {space_id}");
                    match attempt_rejoin(db, session, space_id, app_handle).await {
                        Ok(()) => {
                            eprintln!("[SyncLoop] Rejoin successful, will retry messages next cycle");
                            // Reset cursor so next cycle re-fetches from leader
                            *last_mls_message_id = None;
                        }
                        Err(rejoin_err) => {
                            eprintln!("[SyncLoop] Rejoin failed: {rejoin_err}");
                        }
                    }
                }

                break;
            }
        }
    }

    // ACK successfully processed messages
    if !acked_ids.is_empty() {
        let count = acked_ids.len();
        session.ack_commits(space_id, acked_ids).await?;

        // Emit event for frontend (e.g., epoch key re-derivation)
        let _ = app_handle.emit(
            "local-mls-commit-processed",
            serde_json::json!({
                "spaceId": space_id,
                "processedCount": count,
            }),
        );
    }

    Ok(())
}

/// Attempt to rejoin an MLS group via External Commit after detecting an epoch gap.
async fn attempt_rejoin(
    db: &DbConnection,
    session: &PeerSession,
    space_id: &str,
    app_handle: &tauri::AppHandle,
) -> Result<(), DeliveryError> {
    // TODO: Pass a real UCAN token here. For now, the leader validates the peer's
    // identity via the Announce DID. A proper UCAN should be fetched from the local DB.
    let ucan_token = String::new();

    // 1. Request GroupInfo from leader
    let group_info_b64 = session.request_rejoin(space_id, &ucan_token).await?;

    let group_info_bytes = BASE64.decode(&group_info_b64).map_err(|e| {
        DeliveryError::ProtocolError {
            reason: format!("Failed to decode GroupInfo: {e}"),
        }
    })?;

    // 2. Create External Commit
    let (commit_bytes, epoch_key) = crate::mls::blocking::join_by_external_commit(
        db.0.clone(),
        space_id.to_string(),
        group_info_bytes,
    )
    .await
    .map_err(|e| DeliveryError::ProtocolError {
        reason: format!("External commit failed: {e}"),
    })?;

    let commit_b64 = BASE64.encode(&commit_bytes);

    // 3. Submit the External Commit to the leader for distribution
    session
        .submit_external_commit(space_id, &commit_b64, &ucan_token)
        .await?;

    // 4. Emit event so frontend can update the epoch key
    let _ = app_handle.emit(
        "local-mls-rejoin-completed",
        serde_json::json!({
            "spaceId": space_id,
            "newEpoch": epoch_key.epoch,
        }),
    );

    eprintln!(
        "[SyncLoop] Rejoin completed for space {space_id}, new epoch: {}",
        epoch_key.epoch
    );

    Ok(())
}

/// Query the leader for key package status and upload more if requested.
async fn refill_key_packages_if_needed(
    db: &DbConnection,
    session: &PeerSession,
    space_id: &str,
) -> Result<(), DeliveryError> {
    let (available, needed) = session.query_key_package_status(space_id).await?;

    if needed == 0 {
        return Ok(());
    }

    eprintln!(
        "[SyncLoop] KeyPackage refill: {available} on leader, {needed} more requested"
    );

    let packages = crate::mls::blocking::generate_key_packages(db.0.clone(), needed)
        .await
        .map_err(|e| DeliveryError::ProtocolError {
            reason: format!("Failed to generate key packages: {e}"),
        })?;

    let packages_b64: Vec<String> = packages
        .iter()
        .map(|p| BASE64.encode(p))
        .collect();

    session.upload_key_packages(space_id, packages_b64).await?;

    eprintln!(
        "[SyncLoop] Uploaded {needed} key packages for space {space_id}"
    );

    Ok(())
}

/// Returns the current UTC time in SQLite `datetime('now')` format: `YYYY-MM-DD HH:MM:SS`.
///
/// This matches the format used by CRDT dirty table triggers so that the
/// `last_modified <= ?` comparison works correctly.
fn sqlite_datetime_now() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
    )
}
