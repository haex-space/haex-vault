//! Autonomous sync loop for local spaces.
//!
//! Runs entirely in Rust: connects to leader, pushes dirty changes,
//! pulls remote changes, applies them to local DB, and emits Tauri events.

use std::collections::HashSet;
use std::time::Duration;

use tauri::Emitter;
use tokio::sync::watch;

use crate::crdt::commands::{apply_remote_changes_to_db, clear_dirty_table_inner, RemoteColumnChange};
use crate::crdt::scanner::{scan_all_dirty_tables_for_local_changes, LocalColumnChange};
use crate::database::DbConnection;

use super::error::DeliveryError;
use super::peer::PeerSession;

/// Default poll interval between sync cycles.
const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Maximum backoff duration for reconnection attempts.
const MAX_RECONNECT_BACKOFF: Duration = Duration::from_secs(60);

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
    // Establish initial connection
    let session = PeerSession::connect(
        &iroh_endpoint,
        &leader_endpoint_id,
        leader_relay_url.as_deref(),
        &our_did,
        &our_endpoint_id,
        Some("sync-loop"),
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
fn local_to_remote_change(
    local: &LocalColumnChange,
    batch_id: &str,
    seq: usize,
    total: usize,
) -> RemoteColumnChange {
    RemoteColumnChange {
        table_name: local.table_name.clone(),
        row_pks: local.row_pks.clone(),
        column_name: local.column_name.clone(),
        hlc_timestamp: local.hlc_timestamp.clone(),
        batch_id: batch_id.to_string(),
        batch_seq: seq,
        batch_total: total,
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

                    // Try to reconnect
                    match PeerSession::connect(
                        &iroh_endpoint,
                        &leader_endpoint_id,
                        leader_relay_url.as_deref(),
                        &our_did,
                        &our_endpoint_id,
                        Some("sync-loop"),
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
        eprintln!(
            "[SyncLoop] Pushing {} changes for space {}",
            changes.len(),
            space_id
        );

        // Collect affected table names and the max HLC before push
        let pushed_table_names: HashSet<String> = changes
            .iter()
            .map(|c| c.table_name.clone())
            .collect();

        let max_hlc = changes
            .iter()
            .map(|c| c.hlc_timestamp.as_str())
            .max()
            .unwrap_or("")
            .to_string();

        // Record the current timestamp for clearing dirty tables
        let push_timestamp = sqlite_datetime_now();

        let changes_json = serde_json::to_value(&changes).map_err(|e| {
            DeliveryError::ProtocolError {
                reason: format!("Failed to serialize changes: {}", e),
            }
        })?;

        session.push_changes(space_id, changes_json).await?;

        // Update last_push_hlc to the max HLC from pushed changes
        *last_push_hlc = Some(max_hlc);

        // Clear dirty tables for the tables we just pushed
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
                // Generate a batch ID for this pull
                let batch_id = uuid::Uuid::new_v4().to_string();
                let total = remote_locals.len();

                // Convert LocalColumnChange -> RemoteColumnChange
                let remote_changes: Vec<RemoteColumnChange> = remote_locals
                    .iter()
                    .enumerate()
                    .map(|(i, local)| local_to_remote_change(local, &batch_id, i + 1, total))
                    .collect();

                // Find the max HLC from pulled changes
                let max_pulled_hlc = remote_locals
                    .iter()
                    .map(|c| c.hlc_timestamp.as_str())
                    .max()
                    .unwrap_or("")
                    .to_string();

                // Collect affected table names for the event
                let affected_tables: Vec<String> = remote_locals
                    .iter()
                    .map(|c| c.table_name.clone())
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect();

                // Apply remote changes to local DB (no backend_info for local delivery)
                apply_remote_changes_to_db(db, remote_changes, None).map_err(|e| {
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
