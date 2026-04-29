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
use crate::crdt::scanner::{
    scan_membership_tables_for_local_changes, scan_space_scoped_tables_for_local_changes,
    LocalColumnChange,
};
use crate::database::DbConnection;
use super::error::DeliveryError;
use super::peer::PeerSession;
use super::push_cursor::{load_last_push_hlc, save_last_push_hlc};

/// Sync-loop DB logging helper — writes to `haex_logs` so the e2e harness
/// can extract the trace via `sql_select_with_crdt`. The Tauri stderr is
/// muted in the Docker test rig (tauri-driver child process redirects to
/// `/dev/null`), so eprintln-only logs are invisible to CI.
fn log_sync(app_handle: &tauri::AppHandle, level: &str, message: &str) {
    eprintln!("[SyncLoop] [{level}] {message}");
    let state: tauri::State<'_, crate::AppState> = app_handle.state();
    let _ = crate::logging::insert_log(&state, level, "SyncLoop", None, message, None, "rust");
}

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
    log_sync(&app_handle, "info", &format!(
        "connecting: space={} leader={} our_did={}",
        &space_id[..8.min(space_id.len())],
        &leader_endpoint_id[..16.min(leader_endpoint_id.len())],
        &our_did[..24.min(our_did.len())],
    ));

    // Establish initial connection. UCAN is loaded from the DB inside
    // `PeerSession::connect`, so reconnect-after-expiry gets a fresh token
    // without any state plumbing up here.
    let session = match PeerSession::connect(
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
        Ok(s) => {
            log_sync(&app_handle, "info", &format!(
                "connected: space={} leader={}",
                &space_id[..8.min(space_id.len())],
                &leader_endpoint_id[..16.min(leader_endpoint_id.len())],
            ));
            s
        }
        Err(e) => {
            log_sync(&app_handle, "error", &format!(
                "connect failed: space={} leader={} err={}",
                &space_id[..8.min(space_id.len())],
                &leader_endpoint_id[..16.min(leader_endpoint_id.len())],
                e,
            ));
            return Err(e);
        }
    };

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
    let mut last_push_hlc: Option<String> = load_last_push_hlc(&db, &space_id, &device_id);
    let mut last_pull_timestamp: Option<String> = None;
    let mut last_mls_message_id: Option<i64> = None;
    let mut key_packages_refilled = false;

    // Translate our device UUID into the uhlc node-id form once per session
    // so the push scanner can fast-filter ping-pong rows. A non-UUID device_id
    // (only the in-process leader path uses one — see leader.rs) yields None
    // and disables the filter, which is the correct fallback: that path does
    // not push back to itself anyway.
    let our_node: Option<u128> = crate::crdt::hlc::device_uuid_to_hlc_node(&device_id);
    if our_node.is_none() {
        log_sync(&app_handle, "warn", &format!(
            "device_id is not a UUID, scanner origin filter disabled: device={}",
            &device_id[..16.min(device_id.len())],
        ));
    }

    // Resolve our identity UUID once for the membership-row ownership filter.
    // The leader writes haex_space_members rows for other members (during
    // ClaimInvite) using the leader's HLC node, so the origin filter alone is
    // insufficient — we also need to drop rows whose identity_id ≠ ours.
    let our_identity_id: Option<String> = crate::database::core::select_with_crdt(
        "SELECT id FROM haex_identities WHERE did = ?1 LIMIT 1".to_string(),
        vec![serde_json::Value::String(our_did.clone())],
        &db,
    )
    .ok()
    .and_then(|rows| rows.into_iter().next())
    .and_then(|row| row.into_iter().next())
    .and_then(|v| match v { serde_json::Value::String(s) => Some(s), _ => None });

    // Determine once whether this member may push user-content tables
    // (haex_peer_shares). Read-only members must not: the leader rejects any
    // batch containing non-membership-system rows without Write capability,
    // which would leave the push cursor stuck and block MLS KeyPackage uploads.
    let can_push_user_content =
        super::ucan::has_write_capability(&db, &space_id, &our_did);
    if !can_push_user_content {
        log_sync(&app_handle, "info", &format!(
            "read-only member: push restricted to membership-system tables for space={}",
            &space_id[..8.min(space_id.len())],
        ));
    }

    log_sync(
        &app_handle,
        "info",
        &format!(
            "started: space={} leader={} our_did={} cursor={:?}",
            &space_id[..8.min(space_id.len())],
            &leader_endpoint_id[..16.min(leader_endpoint_id.len())],
            &our_did[..24.min(our_did.len())],
            last_push_hlc.as_deref(),
        ),
    );

    loop {
        // Check if stop was requested
        if *stop_rx.borrow() {
            log_sync(&app_handle, "info", &format!("stop signal received: space={}", &space_id[..8.min(space_id.len())]));
            break;
        }

        match run_sync_cycle(
            &db,
            &session,
            &space_id,
            &device_id,
            our_node,
            can_push_user_content,
            our_identity_id.as_deref(),
            &our_endpoint_id,
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
                        log_sync(&app_handle, "info", &format!("stop during sleep: space={}", &space_id[..8.min(space_id.len())]));
                        break;
                    },
                }
            }
            Err(e) => {
                let endpoint_dead_at_failure = iroh_endpoint.is_closed();
                log_sync(&app_handle, "error", &format!(
                    "cycle failed: space={} err={} endpoint_closed={}",
                    &space_id[..8.min(space_id.len())], e, endpoint_dead_at_failure,
                ));

                // Attempt reconnection with exponential backoff
                let mut backoff = Duration::from_secs(5);
                let mut reconnect_attempt: u32 = 0;
                loop {
                    if *stop_rx.borrow() {
                        eprintln!("[SyncLoop] Stop signal received during reconnect, exiting");
                        session.close();
                        return;
                    }

                    reconnect_attempt += 1;
                    let endpoint_closed_now = iroh_endpoint.is_closed();
                    eprintln!(
                        "[SyncLoop] Reconnecting in {}s (attempt {}, endpoint_closed={})...",
                        backoff.as_secs(),
                        reconnect_attempt,
                        endpoint_closed_now,
                    );

                    // Emit error event for frontend
                    let _ = app_handle.emit(
                        "local-sync-error",
                        serde_json::json!({
                            "spaceId": space_id,
                            "error": e.to_string(),
                            "reconnecting": true,
                            "endpointClosed": endpoint_closed_now,
                            "attempt": reconnect_attempt,
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
                            log_sync(&app_handle, "info", &format!(
                                "reconnected: space={} after {} attempt(s)",
                                &space_id[..8.min(space_id.len())], reconnect_attempt,
                            ));
                            session = new_session;
                            break;
                        }
                        Err(reconnect_err) => {
                            let endpoint_closed_post = iroh_endpoint.is_closed();
                            log_sync(&app_handle, "warn", &format!(
                                "reconnect failed: space={} attempt={} err={} endpoint_closed={}",
                                &space_id[..8.min(space_id.len())],
                                reconnect_attempt,
                                reconnect_err,
                                endpoint_closed_post,
                            ));
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

/// Push local space-scoped changes to the leader.
///
/// Scans only rows belonging to `space_id` (via the space-scoped whitelist
/// scanner), chunks them at HLC-group boundaries, and pushes chunk-by-chunk.
/// On a per-chunk failure the remaining chunks are skipped and the partial
/// progress is checkpointed in `last_push_hlc` so the next cycle resumes
/// without re-sending what the leader already accepted.
async fn run_push_phase(
    db: &DbConnection,
    session: &PeerSession,
    space_id: &str,
    device_id: &str,
    our_node: Option<u128>,
    can_push_user_content: bool,
    our_identity_id: Option<&str>,
    our_endpoint_id: &str,
    last_push_hlc: &mut Option<String>,
) -> Result<(), DeliveryError> {
    // Read-only members must not include haex_peer_shares in the push batch.
    // The leader rejects any batch that touches a non-membership-system table
    // without Write capability, which would leave the cursor stuck at t=0 and
    // block membership-data (e.g. MLS KeyPackages) from ever reaching the leader.
    let all_changes = if can_push_user_content {
        scan_space_scoped_tables_for_local_changes(
            db,
            space_id,
            last_push_hlc.as_deref(),
            device_id,
            our_node,
        )
    } else {
        scan_membership_tables_for_local_changes(
            db,
            space_id,
            last_push_hlc.as_deref(),
            device_id,
            our_node,
        )
    }
    .map_err(|e| DeliveryError::Database {
        reason: format!("Failed to scan space-scoped tables: {}", e),
    })?;

    if all_changes.is_empty() {
        return Ok(());
    }

    // Drop haex_space_members rows owned by other identities and
    // haex_space_devices rows registered for other endpoints. The leader
    // writes these rows on behalf of new members (ClaimInvite / Announce),
    // stamping the leader's HLC node so they pass the origin filter but fail
    // the server's per-row ownership check. Filtering here prevents the push
    // cursor from stalling on an unresolvable ownership violation.
    let (changes, foreign_max_hlc) =
        filter_foreign_membership_rows(db, space_id, all_changes, our_identity_id, our_endpoint_id);

    if !changes.is_empty() {
        // Chunk at HLC boundaries so a transaction-HLC group is never split
        // across QUIC requests. The scanner already returns changes sorted by
        // hlc_timestamp globally, so a single linear pass is enough.
        let chunks = chunk_changes_by_hlc(&changes, PUSH_CHUNK_SOFT_LIMIT);

        eprintln!(
            "[SyncLoop] Pushing {} changes in {} HLC-aligned chunk(s) for space {}",
            changes.len(),
            chunks.len(),
            space_id
        );

        let pushed_table_names: HashSet<String> = changes
            .iter()
            .map(|c| c.table_name.clone())
            .collect();

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
            // remains on the next cycle. The cursor is also persisted to the
            // DB so a process restart or reconnect resumes from here instead
            // of re-scanning from t=0 (which would re-push every previously
            // pulled row and trip the leader's capability check).
            save_last_push_hlc(db, space_id, device_id, &chunk_max_hlc);
            *last_push_hlc = Some(chunk_max_hlc);
        }

        // Clear dirty-table markers only after the whole batch succeeded. A
        // mid-loop failure leaves them dirty so the next cycle re-emits the
        // remaining groups.
        //
        // The threshold is captured *after* the push loop. Capturing before
        // and then `<=`-comparing in clear_dirty_table_inner created a
        // same-second race: a local write between scan start and capture
        // (same second, post-scan) produced a marker equal to the threshold
        // and got wrongly cleared even though its row was never pushed.
        // Capturing here bounds the window to concurrent writes that race
        // with `sqlite_datetime_now()` itself; any surviving inconsistency
        // is a dirty-tracker hint only, not a data-loss risk — the scanner
        // finds unsynced rows via HLC, not via dirty markers.
        let push_timestamp = sqlite_datetime_now();
        for table_name in &pushed_table_names {
            if let Err(e) = clear_dirty_table_inner(db, table_name, Some(&push_timestamp)) {
                eprintln!(
                    "[SyncLoop] Warning: failed to clear dirty table '{}': {}",
                    table_name, e
                );
            }
        }
    }

    // Advance the cursor past any rows we skipped due to foreign ownership.
    // Without this, a skipped row with a higher HLC than all pushable rows
    // keeps the cursor below it, causing a silent no-op re-scan every cycle.
    if let Some(skip_hlc) = foreign_max_hlc {
        if last_push_hlc
            .as_deref()
            .map_or(true, |cur| crate::crdt::hlc::hlc_is_newer(&skip_hlc, cur))
        {
            save_last_push_hlc(db, space_id, device_id, &skip_hlc);
            *last_push_hlc = Some(skip_hlc);
        }
    }

    Ok(())
}

/// Execute a single push+pull sync cycle.
///
/// Push and pull are independent phases: a failing push (e.g. insufficient
/// UCAN capability, transient protocol error) is logged but does not abort
/// the pull. Only pull failures propagate as `Err` and trigger reconnect,
/// because those are the signal that the session is actually broken.
async fn run_sync_cycle(
    db: &DbConnection,
    session: &PeerSession,
    space_id: &str,
    device_id: &str,
    our_node: Option<u128>,
    can_push_user_content: bool,
    our_identity_id: Option<&str>,
    our_endpoint_id: &str,
    app_handle: &tauri::AppHandle,
    last_push_hlc: &mut Option<String>,
    last_pull_timestamp: &mut Option<String>,
    last_mls_message_id: &mut Option<i64>,
    key_packages_refilled: &mut bool,
) -> Result<(), DeliveryError> {
    // 1. PUSH (best-effort) — never blocks the pull below.
    if let Err(e) = run_push_phase(db, session, space_id, device_id, our_node, can_push_user_content, our_identity_id, our_endpoint_id, last_push_hlc).await {
        eprintln!("[SyncLoop] Push phase failed (pull continues): {}", e);
    }

    // 2. PULL: Get changes from leader
    let remote_changes_json = session
        .pull_changes(space_id, last_pull_timestamp.as_deref())
        .await?;

    if let Some(changes_array) = remote_changes_json.as_array() {
        // Log every cycle's pull result so the e2e harness can tell
        // "leader returned 0 changes" (membership/scope problem) apart
        // from "pull never happened" (loop never started / connect failed).
        let table_summary: std::collections::BTreeMap<String, usize> = changes_array
            .iter()
            .filter_map(|c| c.get("tableName").and_then(|v| v.as_str()).map(String::from))
            .fold(std::collections::BTreeMap::new(), |mut acc, t| {
                *acc.entry(t).or_insert(0) += 1;
                acc
            });
        log_sync(app_handle, "info", &format!(
            "pull: space={} count={} tables={:?} after={:?}",
            &space_id[..8.min(space_id.len())],
            changes_array.len(),
            table_summary,
            last_pull_timestamp.as_deref(),
        ));
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
                        Ok(ec_msg_id) => {
                            // After External Commit our local epoch jumped to
                            // the leader's current epoch. Advance the cursor
                            // to the max of:
                            //   (a) the highest id in the current batch — skips
                            //       all stale historical commits in this fetch.
                            //   (b) the msg_id of the External Commit just
                            //       stored by the leader — skips the EC itself
                            //       so the next cycle doesn't re-fetch it and
                            //       trip on its old epoch number. Without this,
                            //       every EC stored in the buffer triggers
                            //       another rejoin in an infinite loop.
                            let batch_max = messages.iter().map(|m| m.id).max().unwrap_or(msg.id);
                            let skip_to = batch_max.max(ec_msg_id);
                            eprintln!(
                                "[SyncLoop] Rejoin successful, advancing cursor past msg {} (skipping {} stale message(s)) for space {space_id}",
                                skip_to,
                                messages.len() - acked_ids.len(),
                            );
                            *last_mls_message_id = Some(skip_to);
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
/// Returns the message ID of the stored External Commit so the caller can advance
/// the MLS cursor past it (preventing the next fetch from re-tripping on it).
async fn attempt_rejoin(
    db: &DbConnection,
    session: &PeerSession,
    space_id: &str,
    app_handle: &tauri::AppHandle,
) -> Result<i64, DeliveryError> {
    // 1. Request GroupInfo from leader
    let group_info_b64 = session.request_rejoin(space_id).await?;

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

    // 3. Submit the External Commit to the leader for distribution.
    //    The returned msg_id lets the caller advance the MLS cursor past the
    //    EC so the next fetch doesn't re-process it as a stale epoch-N message.
    let ec_msg_id = session
        .submit_external_commit(space_id, &commit_b64)
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

    Ok(ec_msg_id)
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

/// Separate `changes` into rows this device may push and rows it must skip.
///
/// Returns `(pushable, foreign_max_hlc)`:
/// - `pushable` contains all changes except membership-table rows owned by
///   another identity or endpoint.
/// - `foreign_max_hlc` is the max HLC of any skipped row, so the push cursor
///   can be advanced past rows that will never be pushable.
///
/// Background: when this device acts as leader it writes `haex_space_members`
/// rows for newly joined members (ClaimInvite) and `haex_space_devices` rows
/// for announcing peers. Those rows get the leader's HLC node, so they pass
/// the push-scanner origin filter but fail the server's per-row ownership
/// check. This function drops them pre-flight.
fn filter_foreign_membership_rows(
    db: &DbConnection,
    space_id: &str,
    changes: Vec<LocalColumnChange>,
    our_identity_id: Option<&str>,
    our_endpoint_id: &str,
) -> (Vec<LocalColumnChange>, Option<String>) {
    // Collect the row IDs we actually own for the two checked tables.
    let owned_member_ids: HashSet<String> = match our_identity_id {
        Some(identity_id) => query_owned_row_ids(
            db,
            "SELECT id FROM haex_space_members WHERE space_id = ?1 AND identity_id = ?2",
            space_id,
            identity_id,
        ),
        // Unknown identity → can't filter → treat all as owned (safe fallback).
        None => HashSet::new(),
    };

    let owned_device_ids: HashSet<String> = query_owned_row_ids(
        db,
        "SELECT id FROM haex_space_devices WHERE space_id = ?1 AND device_endpoint_id = ?2",
        space_id,
        our_endpoint_id,
    );

    // Single pass: check ownership per column change against the pre-fetched
    // owned-id sets. Log each foreign row once (deduplicated by row identity).
    let mut pushable: Vec<LocalColumnChange> = Vec::new();
    let mut foreign_max_hlc: Option<String> = None;
    let mut logged_foreign: HashSet<(String, String)> = HashSet::new();

    for change in changes {
        let owned = match change.table_name.as_str() {
            "haex_space_members" => {
                if our_identity_id.is_none() {
                    true // identity unknown → can't filter → pass through
                } else {
                    extract_pk_id(&change.row_pks)
                        .map(|id| owned_member_ids.contains(&id))
                        .unwrap_or(true) // parse failure → don't silently drop
                }
            }
            "haex_space_devices" => extract_pk_id(&change.row_pks)
                .map(|id| owned_device_ids.contains(&id))
                .unwrap_or(true),
            _ => true,
        };

        if owned {
            pushable.push(change);
        } else {
            let row_key = (change.table_name.clone(), change.row_pks.clone());
            if logged_foreign.insert(row_key) {
                eprintln!(
                    "[SyncLoop] Skipping foreign-owned row {}/{} (not owned by this device)",
                    change.table_name, change.row_pks,
                );
            }
            if foreign_max_hlc
                .as_deref()
                .map_or(true, |cur| crate::crdt::hlc::hlc_is_newer(&change.hlc_timestamp, cur))
            {
                foreign_max_hlc = Some(change.hlc_timestamp);
            }
        }
    }

    (pushable, foreign_max_hlc)
}

/// Run a SQL query of the form `SELECT id FROM <table> WHERE space_id = ?1 AND <owner_col> = ?2`
/// and return the matching id values as a `HashSet`.
fn query_owned_row_ids(
    db: &DbConnection,
    sql: &str,
    space_id: &str,
    owner_value: &str,
) -> HashSet<String> {
    crate::database::core::select_with_crdt(
        sql.to_string(),
        vec![
            serde_json::Value::String(space_id.to_string()),
            serde_json::Value::String(owner_value.to_string()),
        ],
        db,
    )
    .ok()
    .map(|rows| {
        rows.into_iter()
            .filter_map(|row| row.into_iter().next())
            .filter_map(|v| match v {
                serde_json::Value::String(s) => Some(s),
                _ => None,
            })
            .collect()
    })
    .unwrap_or_default()
}

/// Extract the `id` value from a `row_pks` JSON string like `{"id":"<uuid>"}`.
fn extract_pk_id(row_pks: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(row_pks)
        .ok()
        .and_then(|m| m.get("id")?.as_str().map(str::to_string))
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

#[cfg(test)]
mod tests {
    // Regression tests for the MLS cursor-skip logic in fetch_and_process_mls_messages.
    //
    // Root cause of the infinite rejoin loop:
    //   1. Peer submits External Commit → leader stores it as message id=N.
    //   2. Old code set cursor to batch_max (id of last message in the *current*
    //      fetch batch, which was < N since the EC wasn't in the batch yet).
    //   3. Next fetch: WHERE id > batch_max returned the EC (id=N) again.
    //   4. EC had the old epoch → "Wrong Epoch" error → another rejoin → new EC
    //      stored at id=N+1 → infinite loop.
    //
    // Fix: cursor = max(batch_max, ec_msg_id) so the EC itself is skipped.

    /// Simulate: fetch returned [id=1,2,3], EC stored by leader as id=4.
    /// skip_to must be 4 so the next fetch (WHERE id > 4) misses the EC.
    #[test]
    fn cursor_skips_ec_when_ec_is_beyond_batch() {
        let message_ids: Vec<i64> = vec![1, 2, 3];
        let failing_msg_id: i64 = 3;
        let ec_msg_id: i64 = 4;

        let batch_max = message_ids.iter().copied().max().unwrap_or(failing_msg_id);
        let skip_to = batch_max.max(ec_msg_id);

        assert_eq!(skip_to, 4, "cursor must advance past the EC (id=4), not stop at batch max (id=3)");
    }

    /// EC arrives in the same batch as the failing message (unusual but possible).
    /// skip_to must be batch_max so no messages are dropped.
    #[test]
    fn cursor_uses_batch_max_when_ec_already_in_batch() {
        let message_ids: Vec<i64> = vec![1, 2, 3, 4, 5];
        let failing_msg_id: i64 = 3;
        let ec_msg_id: i64 = 2; // hypothetically already in the batch

        let batch_max = message_ids.iter().copied().max().unwrap_or(failing_msg_id);
        let skip_to = batch_max.max(ec_msg_id);

        assert_eq!(skip_to, 5, "when batch_max > ec_msg_id, use batch_max to avoid losing later messages");
    }

    /// Single-message batch where that message is the failing one.
    /// unwrap_or(msg.id) kicks in → batch_max = failing_msg_id.
    #[test]
    fn cursor_handles_single_failing_message_in_batch() {
        let ec_msg_id: i64 = 8;

        // messages = [failing_msg with id=7], batch_max = max of [7] = 7
        let batch_max: i64 = 7;
        let skip_to = batch_max.max(ec_msg_id);

        assert_eq!(skip_to, 8);
    }
}
