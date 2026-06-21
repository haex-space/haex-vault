//! Autonomous sync loop for local spaces.
//!
//! Runs entirely in Rust: connects to leader, pushes dirty changes,
//! pulls remote changes, applies them to local DB, and emits Tauri events.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use tauri::{Emitter, Manager};
use tokio::sync::{watch, Notify};

use super::error::DeliveryError;
use super::peer::PeerSession;
use super::push_cursor::{
    load_last_mls_cursor, load_last_push_hlc, save_last_mls_cursor, save_last_push_hlc,
};
use crate::crdt::commands::{
    apply_remote_changes_to_db, clear_dirty_table_inner, group_by_transaction_hlc,
    RemoteColumnChange,
};
use crate::crdt::hlc::{compare_hlc_strings, hlc_max};
use crate::crdt::scanner::{
    scan_all_crdt_tables_for_owner, scan_membership_tables_for_local_changes,
    scan_space_scoped_tables_for_local_changes, LocalColumnChange,
};
use crate::crdt::trigger::get_table_schema;
use crate::database::core::with_connection;
use crate::database::error::DatabaseError;
use crate::database::migrations::{
    clear_pending_column_row_inner, get_pending_column_rows_inner, pending_columns_count,
    PendingColumnRow,
};
use crate::database::DbConnection;

/// Selects what the push phase scans and which phases run in a sync cycle.
///
/// The default, [`SyncMode::SpaceScoped`], is the existing shared-space sync:
/// only the space-scoped whitelist (filtered by `space_id`) is pushed and the
/// MLS phases run. [`SyncMode::OwnerVault`] is serverless sync of the owner's
/// OWN vault across the owner's OWN devices — it pushes the FULL CRDT table
/// set (no `space_id` filter) and skips the MLS phases.
#[derive(Clone)]
pub enum SyncMode {
    /// Existing behaviour: space-scoped whitelist push + MLS phases.
    SpaceScoped,
    /// Owner-mesh behaviour: full-vault push over the caller-resolved table
    /// list, no membership filtering, no MLS phases. The caller MUST resolve
    /// the table list (the loop never derives it).
    OwnerVault { tables: Vec<String> },
}

/// Decide which scanner produces the push batch for `mode` and return its
/// changes. Extracted from [`run_push_phase`] so the mode→scanner decision is
/// unit-testable without an `AppHandle` or QUIC session (a `DbConnection` over
/// an in-memory `rusqlite::Connection` is enough).
///
/// The `SpaceScoped` branch calls the existing space-scoped scanners verbatim,
/// so its output is byte-identical to the previous inline logic. The
/// `OwnerVault` branch runs the full-vault scanner over the caller-supplied
/// table list.
fn collect_push_changes(
    mode: &SyncMode,
    db: &DbConnection,
    space_id: &str,
    after_hlc: Option<&str>,
    device_id: &str,
    our_node: Option<u128>,
    can_push_user_content: bool,
) -> Result<Vec<LocalColumnChange>, DatabaseError> {
    match mode {
        // EXISTING behaviour, unchanged. Read-only members must not include
        // haex_peer_shares (the leader rejects a batch touching a
        // non-membership-system table without Write capability), so they scan
        // only the membership-system subset.
        SyncMode::SpaceScoped => {
            if can_push_user_content {
                scan_space_scoped_tables_for_local_changes(
                    db, space_id, after_hlc, device_id, our_node,
                )
            } else {
                scan_membership_tables_for_local_changes(
                    db, space_id, after_hlc, device_id, our_node,
                )
            }
        }
        // Reachable only after same-owner DID-auth — the security boundary that
        // proves the remote peer holds the SAME vault-owner DID lives in the
        // serving/connect gate (added in later tasks), not here. With that gate
        // in place this full-vault scan is sound; without it, it MUST NOT run.
        SyncMode::OwnerVault { tables } => with_connection(db, |conn| {
            scan_all_crdt_tables_for_owner(conn, tables, after_hlc, device_id, our_node)
        }),
    }
}

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
        let boundary =
            i == changes.len() || changes[i].hlc_timestamp != changes[i - 1].hlc_timestamp;
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
    wakeup: Arc<Notify>,
    task: tokio::task::JoinHandle<()>,
}

impl SyncLoopHandle {
    /// Signal the sync loop to stop.
    pub fn stop(&self) {
        let _ = self.stop_sender.send(true);
    }

    /// Cut the current `POLL_INTERVAL` sleep short so the next sync cycle
    /// starts immediately. Multiple calls before the loop wakes up coalesce
    /// into a single wake (Notify::notify_one semantics) — the cycle itself
    /// is the rate limit, not this signal.
    ///
    /// Calling this while a cycle is already running is a no-op for the
    /// current cycle and a wake for the next sleep.
    pub fn wakeup(&self) {
        self.wakeup.notify_one();
    }

    /// Check if the sync loop task has finished.
    pub fn is_finished(&self) -> bool {
        self.task.is_finished()
    }
}

/// Open a `PeerSession` appropriate for `mode`.
///
/// [`SyncMode::SpaceScoped`] uses [`PeerSession::connect`] (DID-auth + UCAN +
/// Announce) — the existing shared-space path, unchanged. [`SyncMode::OwnerVault`]
/// uses [`PeerSession::connect_owner`] (DID-auth only, NO UCAN, NO Announce):
/// the owner's own devices have no UCAN for themselves, and the security gate
/// is the same-owner DID-auth handshake which `connect_owner` still runs.
#[allow(clippy::too_many_arguments)]
async fn connect_for_mode(
    mode: &SyncMode,
    iroh_endpoint: &iroh::Endpoint,
    leader_endpoint_id: &str,
    leader_relay_url: Option<&str>,
    space_id: &str,
    our_did: &str,
    our_signing_key: &ed25519_dalek::SigningKey,
    our_endpoint_id: &str,
    db: &DbConnection,
) -> Result<PeerSession, DeliveryError> {
    match mode {
        SyncMode::SpaceScoped => {
            PeerSession::connect(
                iroh_endpoint,
                leader_endpoint_id,
                leader_relay_url,
                space_id,
                our_did,
                our_signing_key,
                our_endpoint_id,
                Some("sync-loop"),
                db,
            )
            .await
        }
        SyncMode::OwnerVault { .. } => {
            PeerSession::connect_owner(
                iroh_endpoint,
                leader_endpoint_id,
                leader_relay_url,
                our_did,
                our_signing_key,
                our_endpoint_id,
            )
            .await
        }
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
    mode: SyncMode,
    leader_endpoint_id: String,
    leader_relay_url: Option<String>,
    space_id: String,
    our_did: String,
    our_endpoint_id: String,
    device_id: String,
    app_handle: tauri::AppHandle,
) -> Result<SyncLoopHandle, DeliveryError> {
    log_sync(
        &app_handle,
        "info",
        &format!(
            "connecting: space={} leader={} our_did={}",
            &space_id[..8.min(space_id.len())],
            &leader_endpoint_id[..16.min(leader_endpoint_id.len())],
            &our_did[..24.min(our_did.len())],
        ),
    );

    // Load the identity's signing key once for the lifetime of this loop.
    // Every (re)connect drives the server-initiated quic_did_auth handshake,
    // and the loop reconnects internally on transient failure — keeping the
    // key in-process avoids repeated DB hits + private-key derivations.
    let our_identity = super::quic_retry::load_signing_identity_for_did(&db, &our_did)?;

    // Establish initial connection. UCAN is loaded from the DB inside
    // `PeerSession::connect`, so reconnect-after-expiry gets a fresh token
    // without any state plumbing up here.
    // Bounded retry around the initial connect. A single transient relay/
    // handshake hiccup — common in the relay-only docker-split CI network —
    // would otherwise hard-fail the whole sync loop, leaving the leader's
    // `connected_peers` empty until some external trigger reconnects, which
    // surfaces as the 110s "Vault B device row never synced" cross-vault
    // flake. The internal reconnect loop only runs *after* this first connect
    // succeeds, so the initial attempt needs its own retry. `connect` carries
    // no timeout of its own, so each attempt is bounded here too. Only
    // `ConnectionFailed` is retried; `AccessDenied`/`ProtocolError` are
    // deterministic and fail fast.
    let session = match super::quic_retry::retry_transient(
        "sync-loop initial connect",
        || async {
            match tokio::time::timeout(
                Duration::from_secs(10),
                connect_for_mode(
                    &mode,
                    &iroh_endpoint,
                    &leader_endpoint_id,
                    leader_relay_url.as_deref(),
                    &space_id,
                    &our_did,
                    &our_identity.signing_key,
                    &our_endpoint_id,
                    &db,
                ),
            )
            .await
            {
                Ok(result) => result,
                Err(_) => Err(DeliveryError::ConnectionFailed {
                    reason: "initial connect timed out after 10s".to_string(),
                }),
            }
        },
        |e| matches!(e, DeliveryError::ConnectionFailed { .. }),
    )
    .await
    {
        Ok(s) => {
            log_sync(
                &app_handle,
                "info",
                &format!(
                    "connected: space={} leader={}",
                    &space_id[..8.min(space_id.len())],
                    &leader_endpoint_id[..16.min(leader_endpoint_id.len())],
                ),
            );
            s
        }
        Err(e) => {
            log_sync(
                &app_handle,
                "error",
                &format!(
                    "connect failed after retries: space={} leader={} err={}",
                    &space_id[..8.min(space_id.len())],
                    &leader_endpoint_id[..16.min(leader_endpoint_id.len())],
                    e,
                ),
            );
            return Err(e);
        }
    };

    let (stop_tx, stop_rx) = watch::channel(false);
    let wakeup = Arc::new(Notify::new());

    let task = tokio::spawn(run_sync_loop(
        db,
        iroh_endpoint,
        session,
        mode,
        leader_endpoint_id,
        leader_relay_url,
        space_id,
        our_did,
        our_identity.signing_key,
        our_endpoint_id,
        device_id,
        app_handle,
        stop_rx,
        wakeup.clone(),
    ));

    Ok(SyncLoopHandle {
        stop_sender: stop_tx,
        wakeup,
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

/// Owed row-aware markers we can recover THIS cycle: those whose column exists
/// in the local schema now. (Same per-column existence filter as before, now
/// carried at row granularity.) A pending entry may reference a column the
/// local migration has not re-added yet (the loop runs continuously between the
/// skip and the app update that migrates). If we recovered+cleared such an
/// entry, `apply_remote_changes_to_db` would just re-skip the still-missing
/// column and we'd have cleared the pending marker — the skipped values would
/// then NEVER be recovered (silent data loss). So columns the migration has not
/// re-added stay pending for a later cycle.
fn recoverable_pending_columns(
    conn: &rusqlite::Connection,
) -> Result<Vec<PendingColumnRow>, DatabaseError> {
    let pending = get_pending_column_rows_inner(conn)?;
    let mut recoverable = Vec::new();
    for row in pending {
        // A missing table (PRAGMA returns an empty schema) or an unsafe table
        // name (Err) both yield "column not present" → not recoverable. We
        // intentionally swallow the schema error here: the row simply stays
        // pending and is retried on a later cycle once the table exists.
        let column_exists = get_table_schema(conn, &row.table_name)
            .map(|cols| cols.iter().any(|c| c.name == row.column_name))
            .unwrap_or(false);
        if column_exists {
            recoverable.push(row);
        }
    }
    Ok(recoverable)
}

/// The `(table, column, row_pks)` triples the pulled dump actually carried a
/// value for. Recovery clears a marker ONLY for these.
///
/// An owed row absent from the dump is ambiguous: the serving owner device may
/// be row-incomplete (it never received that row from a third device) or lack
/// the column entirely (normal version skew across the owner's own devices).
/// Clearing the marker on such an absent row would drop it while the skipped
/// value still lives only behind this device's incremental pull cursor — lost
/// forever. Leaving the row pending instead lets a later cycle retry against
/// whichever peer the loop connects to next.
fn rows_present_in_changes(
    changes: &[LocalColumnChange],
) -> std::collections::HashSet<(String, String, String)> {
    changes
        .iter()
        .map(|c| {
            (
                c.table_name.clone(),
                c.column_name.clone(),
                c.row_pks.clone(),
            )
        })
        .collect()
}

/// Of the owed markers we tried to recover, the ones safe to clear: present in
/// the dump. Owed rows the (non-authoritative) peer did not serve stay pending
/// — clearing them would be silent data loss.
fn pending_rows_to_clear(
    owed: &[PendingColumnRow],
    present: &std::collections::HashSet<(String, String, String)>,
) -> Vec<PendingColumnRow> {
    owed.iter()
        .filter(|r| {
            present.contains(&(
                r.table_name.clone(),
                r.column_name.clone(),
                r.row_pks.clone(),
            ))
        })
        .cloned()
        .collect()
}

/// Owner-vault pending-column recovery. Best-effort; the caller logs errors and
/// continues the cycle. Clears a pending entry ONLY after its value applied.
///
/// NOT unit-tested: this is `AppHandle`-bound (it reaches `app_handle.state`,
/// `lock_or_fail`, and a live `PeerSession` over QUIC), none of which exist as
/// cargo-test fixtures. Behavioural coverage is the later e2e (Pfad A). The pure
/// data-loss-guarding logic it depends on lives in `recoverable_pending_columns`,
/// `rows_present_in_changes`, and `pending_rows_to_clear`, which ARE unit-tested.
///
/// # Row granularity
///
/// The pending marker in `haex_crdt_pending_columns` is ROW-granular
/// (`table_name`, `column_name`, `row_pks`). A P2P peer is NOT authoritative: it
/// can be row-incomplete. In a mesh of 3+ owner devices a peer may serve some
/// rows of a column but lack others (e.g. a row originating on a third device it
/// never received). Recovery therefore clears a marker ONLY for the
/// `(table, column, row_pks)` triples the dump actually carried (see
/// `rows_present_in_changes` / `pending_rows_to_clear`); owed rows the peer did
/// not serve stay pending and are retried against whichever peer the loop reaches
/// next. Pulls are still issued per distinct `(table, column)` so a column with
/// many owed rows is requested once, not once per row.
async fn run_owner_pending_column_recovery(
    db: &DbConnection,
    session: &PeerSession,
    space_id: &str,
    app_handle: &tauri::AppHandle,
) -> Result<(), DeliveryError> {
    // 1. Cheap gate (indexed COUNT) — runs every cycle.
    let count =
        with_connection(db, |c| pending_columns_count(c)).map_err(|e| DeliveryError::Database {
            reason: format!("Failed to count pending columns: {e}"),
        })?;
    if count == 0 {
        return Ok(());
    }

    // 2. Only recover owed rows whose column the local migration has already
    //    re-added; the rest stay pending (clearing them now would be silent data
    //    loss).
    let owed_rows = with_connection(db, |c| recoverable_pending_columns(c)).map_err(|e| {
        DeliveryError::Database {
            reason: format!("Failed to read recoverable pending columns: {e}"),
        }
    })?;
    if owed_rows.is_empty() {
        return Ok(());
    }

    // 3. Pull from a serving owner device. The pull is per distinct
    //    `(table, column)` — a column with many owed rows is requested once, not
    //    once per row. A serving-side Error (e.g. oversize) surfaces here,
    //    propagates to the caller's log, and leaves the pending markers in place
    //    so the next cycle retries.
    let request_pairs: Vec<(String, String)> = {
        let mut seen = std::collections::HashSet::new();
        owed_rows
            .iter()
            .filter(|r| seen.insert((r.table_name.clone(), r.column_name.clone())))
            .map(|r| (r.table_name.clone(), r.column_name.clone()))
            .collect()
    };
    let json = session.pull_columns(space_id, &request_pairs).await?;

    let local_changes: Vec<LocalColumnChange> =
        serde_json::from_value(json).map_err(|e| DeliveryError::ProtocolError {
            reason: format!("Failed to deserialize pulled columns: {e}"),
        })?;

    // 4. Apply the pulled values (if any). An empty/partial result is NOT proof a
    //    row is fully recovered: the serving owner device may itself be
    //    row-incomplete or lack the column (normal version skew). We therefore
    //    clear a marker only for the rows the dump actually carried a value for
    //    (step 5); owed rows with no returned value stay pending and are retried
    //    next cycle against whichever peer the loop connects to. (A row genuinely
    //    unavailable on every reachable device re-pulls each cycle until found —
    //    bounded by the cheap COUNT gate and MAX_RESPONSE_SIZE; per-column backoff
    //    is a follow-up.)
    if !local_changes.is_empty() {
        let remote_changes: Vec<RemoteColumnChange> =
            local_changes.iter().map(local_to_remote_change).collect();

        // Mirror the pull block's HLC lock + apply: lock_or_fail surfaces a
        // banner-visible failure on poison rather than silently applying
        // without advancing the local clock.
        let state: tauri::State<'_, crate::AppState> = app_handle.state();
        let hlc_service = state.lock_or_fail(
            &state.hlc,
            crate::critical::CriticalFailureCode::HlcMutexPoisoned,
            "space_delivery::local::sync_loop::run_owner_pending_column_recovery",
            serde_json::json!({}),
        )?;
        apply_remote_changes_to_db(db, remote_changes, None, Some(&*hlc_service)).map_err(|e| {
            DeliveryError::Database {
                reason: format!("Failed to apply recovered columns: {e}"),
            }
        })?;
    }

    // 5. Clear ONLY the rows the dump actually carried a value for, and only after
    //    the apply above succeeded. Clearing an absent row would be silent data
    //    loss (see step 4 + `rows_present_in_changes` / `pending_rows_to_clear`).
    let present = rows_present_in_changes(&local_changes);
    let to_clear = pending_rows_to_clear(&owed_rows, &present);
    let cleared = to_clear.len();
    with_connection(db, |conn| {
        for r in &to_clear {
            clear_pending_column_row_inner(conn, &r.table_name, &r.column_name, &r.row_pks)?;
        }
        Ok::<(), DatabaseError>(())
    })
    .map_err(|e| DeliveryError::Database {
        reason: format!("Failed to clear pending columns: {e}"),
    })?;

    // 6. Observable trace for the e2e harness: how many distinct columns were
    //    pulled and how many owed ROWS were recovered+cleared vs left pending for
    //    a later cycle. The invariant requested == recovered + left_pending is
    //    row-aware (requested = owed rows we attempted this cycle).
    log_sync(
        app_handle,
        "info",
        &format!(
            "owner pending-column recovery: space={} columns={} requested={} recovered={} left_pending={}",
            &space_id[..8.min(space_id.len())],
            request_pairs.len(),
            owed_rows.len(),
            cleared,
            owed_rows.len() - cleared,
        ),
    );

    Ok(())
}

/// The main sync loop. Runs until the stop signal is received.
async fn run_sync_loop(
    db: DbConnection,
    iroh_endpoint: iroh::Endpoint,
    mut session: PeerSession,
    mode: SyncMode,
    leader_endpoint_id: String,
    leader_relay_url: Option<String>,
    space_id: String,
    our_did: String,
    our_signing_key: ed25519_dalek::SigningKey,
    our_endpoint_id: String,
    device_id: String,
    app_handle: tauri::AppHandle,
    mut stop_rx: watch::Receiver<bool>,
    wakeup: Arc<Notify>,
) {
    let mut last_push_hlc: Option<String> = load_last_push_hlc(&db, &space_id, &device_id);
    let mut last_pull_timestamp: Option<String> = None;
    let mut last_mls_message_id: Option<i64> = load_last_mls_cursor(&db, &space_id, &device_id);
    let mut key_packages_refilled = false;

    // Translate our device UUID into the uhlc node-id form once per session
    // so the push scanner can fast-filter ping-pong rows. A non-UUID device_id
    // (only the in-process leader path uses one — see leader.rs) yields None
    // and disables the filter, which is the correct fallback: that path does
    // not push back to itself anyway.
    let our_node: Option<u128> = crate::crdt::hlc::device_uuid_to_hlc_node(&device_id);
    if our_node.is_none() {
        log_sync(
            &app_handle,
            "warn",
            &format!(
                "device_id is not a UUID, scanner origin filter disabled: device={}",
                &device_id[..16.min(device_id.len())],
            ),
        );
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
    .and_then(|v| match v {
        serde_json::Value::String(s) => Some(s),
        _ => None,
    });

    // Determine once whether this member may push user-content tables
    // (haex_peer_shares). Read-only members must not: the leader rejects any
    // batch containing non-membership-system rows without Write capability,
    // which would leave the push cursor stuck and block MLS KeyPackage uploads.
    let can_push_user_content = super::ucan::has_write_capability(&db, &space_id, &our_did);
    if !can_push_user_content {
        log_sync(
            &app_handle,
            "info",
            &format!(
                "read-only member: push restricted to membership-system tables for space={}",
                &space_id[..8.min(space_id.len())],
            ),
        );
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
            log_sync(
                &app_handle,
                "info",
                &format!(
                    "stop signal received: space={}",
                    &space_id[..8.min(space_id.len())]
                ),
            );
            break;
        }

        match run_sync_cycle(
            &db,
            &session,
            &mode,
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
                // Cycle completed successfully, wait for next cycle, an
                // external wake-up (force_sync), or a stop signal.
                tokio::select! {
                    _ = tokio::time::sleep(POLL_INTERVAL) => {},
                    _ = wakeup.notified() => {},
                    _ = stop_rx.changed() => {
                        log_sync(&app_handle, "info", &format!("stop during sleep: space={}", &space_id[..8.min(space_id.len())]));
                        break;
                    },
                }
            }
            Err(e) => {
                let endpoint_dead_at_failure = iroh_endpoint.is_closed();
                log_sync(
                    &app_handle,
                    "error",
                    &format!(
                        "cycle failed: space={} err={} endpoint_closed={}",
                        &space_id[..8.min(space_id.len())],
                        e,
                        endpoint_dead_at_failure,
                    ),
                );

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

                    // Emit error event for frontend (main window only).
                    // Tauri v2 emit() broadcasts to every webview — extensions
                    // must not learn about p2p sync state for other spaces.
                    let _ = app_handle.emit_to(
                        "main",
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

                    // Try to reconnect — in space mode this pulls the current
                    // UCAN from the DB so a token renewed during the outage
                    // takes effect here; in owner mode reconnect re-runs only
                    // the DID-auth handshake (no UCAN).
                    match connect_for_mode(
                        &mode,
                        &iroh_endpoint,
                        &leader_endpoint_id,
                        leader_relay_url.as_deref(),
                        &space_id,
                        &our_did,
                        &our_signing_key,
                        &our_endpoint_id,
                        &db,
                    )
                    .await
                    {
                        Ok(new_session) => {
                            log_sync(
                                &app_handle,
                                "info",
                                &format!(
                                    "reconnected: space={} after {} attempt(s)",
                                    &space_id[..8.min(space_id.len())],
                                    reconnect_attempt,
                                ),
                            );
                            session = new_session;
                            break;
                        }
                        Err(reconnect_err) => {
                            let endpoint_closed_post = iroh_endpoint.is_closed();
                            log_sync(
                                &app_handle,
                                "warn",
                                &format!(
                                "reconnect failed: space={} attempt={} err={} endpoint_closed={}",
                                &space_id[..8.min(space_id.len())],
                                reconnect_attempt,
                                reconnect_err,
                                endpoint_closed_post,
                            ),
                            );
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
    mode: &SyncMode,
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
    // In owner-vault mode the full caller-resolved table set is scanned with no
    // space filter (the can_push_user_content gate does not apply — there is no
    // leader capability check on the owner's own mesh).
    let all_changes = collect_push_changes(
        mode,
        db,
        space_id,
        last_push_hlc.as_deref(),
        device_id,
        our_node,
        can_push_user_content,
    )
    .map_err(|e| DeliveryError::Database {
        reason: format!("Failed to scan CRDT tables: {}", e),
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
    //
    // Owner-vault mode skips this filter: there is no leader writing rows on
    // others' behalf in the owner's own device mesh, so every scanned row is
    // legitimately the owner's to push.
    let (changes, foreign_max_hlc) = match mode {
        SyncMode::SpaceScoped => filter_foreign_membership_rows(
            db,
            space_id,
            all_changes,
            our_identity_id,
            our_endpoint_id,
        ),
        SyncMode::OwnerVault { .. } => (all_changes, None),
    };

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

        let pushed_table_names: HashSet<String> =
            changes.iter().map(|c| c.table_name.clone()).collect();

        for (idx, chunk) in chunks.iter().enumerate() {
            let chunk_max_hlc = hlc_max(chunk.iter().map(|c| c.hlc_timestamp.as_str()))
                .unwrap_or("")
                .to_string();

            let chunk_json =
                serde_json::to_value(chunk).map_err(|e| DeliveryError::ProtocolError {
                    reason: format!("Failed to serialize chunk {}: {}", idx, e),
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

/// Split a pulled batch into the changes that are safe to apply now versus the
/// trailing transaction to hold back until a later page confirms it.
///
/// HLC == one source transaction (the HLC SQL UDF is transaction-scoped), so
/// all changes sharing an `hlc_timestamp` belong to the same source
/// transaction and must apply atomically — never split across a page boundary.
///
/// When `has_more == true` the transport is paginating and the highest-HLC
/// group in this page may be only partially delivered, so it is held back:
/// `to_apply` gets every change strictly below the max HLC, `hold_back` gets
/// every change at the max HLC. When `has_more == false` the page is complete
/// and everything applies (`hold_back` is empty).
///
/// Pure and deterministic: no I/O, comparison is numeric via
/// [`compare_hlc_strings`].
fn split_complete_groups(
    changes: Vec<RemoteColumnChange>,
    has_more: bool,
) -> (Vec<RemoteColumnChange>, Vec<RemoteColumnChange>) {
    if changes.is_empty() {
        return (Vec::new(), Vec::new());
    }
    if !has_more {
        return (changes, Vec::new());
    }
    let max_hlc = hlc_max(changes.iter().map(|c| c.hlc_timestamp.as_str()))
        .unwrap_or("")
        .to_string();
    changes.into_iter().partition(|change| {
        compare_hlc_strings(&change.hlc_timestamp, &max_hlc) == std::cmp::Ordering::Less
    })
}

/// Apply already-grouped changes (ascending transaction-HLC order) one DB
/// transaction per group, advancing `cursor` to each group's HLC after it
/// applies successfully.
///
/// Failure isolation: on the FIRST group whose `apply` returns `Err`, stops and
/// returns that error. `cursor` is left at the last successfully-applied group's
/// HLC (groups are ascending), so the next cycle resumes from there and the
/// deferred groups are re-pulled. Later groups are NOT attempted.
///
/// Pure control flow over an injected `apply` closure — the real caller passes a
/// closure wrapping [`apply_remote_changes_to_db`]; tests inject a recording /
/// failing closure so the isolation + cursor behaviour is unit-testable without
/// a live QUIC session or database. The group HLC is passed to the closure so it
/// can log which group failed.
fn apply_groups_advancing_cursor<F>(
    groups: Vec<(String, Vec<RemoteColumnChange>)>,
    cursor: &mut Option<String>,
    mut apply: F,
) -> Result<(), DeliveryError>
where
    F: FnMut(&str, Vec<RemoteColumnChange>) -> Result<(), DeliveryError>,
{
    for (group_hlc, group_changes) in groups {
        apply(&group_hlc, group_changes)?;
        // Advance the cursor only after the group committed. Empty HLCs (never
        // produced for real sync data) are skipped, matching the prior guard.
        if !group_hlc.is_empty() {
            *cursor = Some(group_hlc);
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
    mode: &SyncMode,
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
    if let Err(e) = run_push_phase(
        db,
        session,
        mode,
        space_id,
        device_id,
        our_node,
        can_push_user_content,
        our_identity_id,
        our_endpoint_id,
        last_push_hlc,
    )
    .await
    {
        eprintln!("[SyncLoop] Push phase failed (pull continues): {}", e);
    }

    // 2. PULL: paginated by transaction-HLC group.
    //
    // The serve side packs WHOLE HLC-groups into each page up to a byte budget
    // and reports `has_more`; a transaction is never split across a page. This
    // lets a transaction larger than the legacy 10 MB wire cap (e.g. a password
    // attachment blob) traverse the wire one page at a time.
    //
    // Two cursors are in play:
    //   - `page_after`: the IN-CYCLE pull cursor. Starts at the persisted apply
    //     cursor and advances to the MAX HLC of each page so the next page's
    //     strictly-greater scan resumes correctly (HLC is unique per source
    //     transaction → no skips/dups).
    //   - `last_pull_timestamp`: the PERSISTED apply cursor, advanced ONLY by
    //     `apply_groups_advancing_cursor` after a group actually commits. A page
    //     pulled but not yet applied must NOT move it.
    //
    // `split_complete_groups` holds back the trailing (max-HLC) group while more
    // pages are coming — with HLC-aligned pages that group is complete and
    // applies on the next page (or at `has_more = false`), via the carried-over
    // `buffer`.
    //
    // The HLC lock is re-acquired per page, scoped to the apply: a
    // `std::sync::MutexGuard` is not `Send`, so it cannot be held across the
    // `pull_changes().await` at the top of the loop (the future is spawned via
    // `tokio::spawn`, which requires `Send`). Re-locking per page is cheap — the
    // apply is the dominant work — and `lock_or_fail` surfaces a banner-visible
    // failure on poison instead of silently applying without advancing the clock.
    let mut page_after: Option<String> = last_pull_timestamp.clone();
    let mut buffer: Vec<RemoteColumnChange> = Vec::new();
    let mut affected_tables: HashSet<String> = HashSet::new();

    loop {
        let (page_json, has_more) = session
            .pull_changes(space_id, page_after.as_deref())
            .await?;

        let page_count = page_json.as_array().map(|a| a.len()).unwrap_or(0);
        // Per-page pull summary so the e2e harness can tell "leader returned 0
        // changes" (membership/scope problem) apart from "pull never happened"
        // (loop never started / connect failed), and observe pagination.
        let table_summary: std::collections::BTreeMap<String, usize> = page_json
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| {
                        c.get("tableName")
                            .and_then(|v| v.as_str())
                            .map(String::from)
                    })
                    .fold(std::collections::BTreeMap::new(), |mut acc, t| {
                        *acc.entry(t).or_insert(0) += 1;
                        acc
                    })
            })
            .unwrap_or_default();
        log_sync(
            app_handle,
            "info",
            &format!(
                "pull: space={} count={} has_more={} tables={:?} after={:?}",
                &space_id[..8.min(space_id.len())],
                page_count,
                has_more,
                table_summary,
                page_after.as_deref(),
            ),
        );

        if page_count > 0 {
            eprintln!(
                "[SyncLoop] Pulled {} changes (has_more={}) for space {}",
                page_count, has_more, space_id
            );

            // Deserialize this page into LocalColumnChange (same JSON shape).
            let remote_locals: Vec<LocalColumnChange> =
                serde_json::from_value(page_json).map_err(|e| DeliveryError::ProtocolError {
                    reason: format!("Failed to deserialize pulled changes: {}", e),
                })?;

            for c in &remote_locals {
                affected_tables.insert(c.table_name.clone());
            }

            // Advance the in-cycle pull cursor to this page's MAX HLC so the
            // next page resumes strictly after it (distinct from the persisted
            // apply cursor advanced inside `apply_groups_advancing_cursor`).
            if let Some(page_max_hlc) =
                hlc_max(remote_locals.iter().map(|c| c.hlc_timestamp.as_str()))
            {
                page_after = Some(page_max_hlc.to_string());
            }

            // Carry over any held-back trailing group from the prior page, then
            // add this page's changes.
            buffer.extend(remote_locals.iter().map(local_to_remote_change));
        }

        // Hold back the trailing (max-HLC) group while more pages are coming;
        // with HLC-aligned pages it is complete and applies next page (or now,
        // when has_more=false).
        let (to_apply, hold_back) = split_complete_groups(std::mem::take(&mut buffer), has_more);

        // Apply per transaction-HLC group with failure isolation; the PERSISTED
        // cursor advances per committed group. The error is propagated AFTER the
        // cursor has advanced for the groups that DID succeed, matching the
        // cycle's `?`-propagation convention. The HLC guard is scoped to this
        // block so it drops before the next page's `pull_changes().await` (the
        // guard is not `Send`).
        {
            let state: tauri::State<'_, crate::AppState> = app_handle.state();
            let hlc_service = state.lock_or_fail(
                &state.hlc,
                crate::critical::CriticalFailureCode::HlcMutexPoisoned,
                "space_delivery::local::sync_loop::run_sync_cycle::apply_remote",
                serde_json::json!({}),
            )?;

            apply_groups_advancing_cursor(
                group_by_transaction_hlc(to_apply),
                last_pull_timestamp,
                |group_hlc, group_changes| {
                    apply_remote_changes_to_db(db, group_changes, None, Some(&*hlc_service))
                        .map_err(|e| {
                            // log_sync (not eprintln) so the e2e harness can
                            // observe a per-group apply failure on the same
                            // structured channel as the pull outcomes above.
                            log_sync(
                                app_handle,
                                "warn",
                                &format!(
                                    "apply: transaction-HLC group {} failed: {} \
                                     (cursor at last applied; later groups deferred)",
                                    group_hlc, e
                                ),
                            );
                            DeliveryError::Database {
                                reason: format!("Failed to apply remote changes: {}", e),
                            }
                        })
                },
            )?;
        }

        buffer = hold_back;

        if !has_more {
            break;
        }
    }

    // Emit the UI-refresh event once after all pages applied (main window only).
    if !affected_tables.is_empty() {
        let _ = app_handle.emit_to(
            "main",
            "local-sync-completed",
            serde_json::json!({
                "spaceId": space_id,
                "tables": affected_tables.into_iter().collect::<Vec<_>>(),
            }),
        );
    }

    // 2b. OWNER-VAULT pending-column recovery (best-effort, owner mode only).
    if let SyncMode::OwnerVault { .. } = mode {
        if let Err(e) = run_owner_pending_column_recovery(db, session, space_id, app_handle).await {
            eprintln!("[SyncLoop] Owner pending-column recovery failed (cycle continues): {e}");
        }
    }

    // 3 + 4. MLS phases run only in space-scoped mode. The owner's own device
    // mesh has no MLS group: there is no leader distributing commits and no
    // KeyPackage exchange, so both phases are skipped. The CRDT push+pull above
    // run in BOTH modes.
    if matches!(mode, SyncMode::SpaceScoped) {
        // 3. MLS: Fetch commits from leader, process, and ACK
        if let Err(e) = fetch_and_process_mls_messages(
            db,
            session,
            space_id,
            device_id,
            last_mls_message_id,
            app_handle,
        )
        .await
        {
            eprintln!("[SyncLoop] MLS message processing failed: {e}");
            // Non-fatal: CRDT sync still worked, MLS will retry next cycle
        }

        // 4. KeyPackage refill: run once per session (ClaimInvite already uploads 10)
        if !*key_packages_refilled {
            match refill_key_packages_if_needed(db, session, space_id).await {
                Ok(()) => *key_packages_refilled = true,
                Err(e) => {
                    eprintln!("[SyncLoop] KeyPackage refill failed (will retry next cycle): {e}")
                }
            }
        }
    }

    Ok(())
}

/// Fetch MLS messages from leader, process them locally, and send ACKs.
async fn fetch_and_process_mls_messages(
    db: &DbConnection,
    session: &PeerSession,
    space_id: &str,
    device_id: &str,
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

        match crate::mls::blocking::process_message(db.0.clone(), space_id.to_string(), blob).await
        {
            Ok(_) => {
                acked_ids.push(msg.id);
                *last_mls_message_id = Some(msg.id);
                save_last_mls_cursor(db, space_id, device_id, msg.id);
                eprintln!(
                    "[SyncLoop] Processed MLS {} message (id={})",
                    msg.message_type, msg.id
                );
            }
            Err(e) => {
                eprintln!("[SyncLoop] Failed to process MLS message {}: {e}", msg.id);

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
                            save_last_mls_cursor(db, space_id, device_id, skip_to);
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

        // Emit event for frontend (main window only).
        let _ = app_handle.emit_to(
            "main",
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

    let group_info_bytes =
        BASE64
            .decode(&group_info_b64)
            .map_err(|e| DeliveryError::ProtocolError {
                reason: format!("Failed to decode GroupInfo: {e}"),
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

    // 4. Emit event so frontend can update the epoch key (main window only).
    let _ = app_handle.emit_to(
        "main",
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

    eprintln!("[SyncLoop] KeyPackage refill: {available} on leader, {needed} more requested");

    let packages = crate::mls::blocking::generate_key_packages(db.0.clone(), needed)
        .await
        .map_err(|e| DeliveryError::ProtocolError {
            reason: format!("Failed to generate key packages: {e}"),
        })?;

    let packages_b64: Vec<String> = packages.iter().map(|p| BASE64.encode(p)).collect();

    session.upload_key_packages(space_id, packages_b64).await?;

    eprintln!("[SyncLoop] Uploaded {needed} key packages for space {space_id}");

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
        "SELECT id FROM haex_space_devices WHERE space_id = ?1 AND endpoint_id = ?2",
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
            if foreign_max_hlc.as_deref().map_or(true, |cur| {
                crate::crdt::hlc::hlc_is_newer(&change.hlc_timestamp, cur)
            }) {
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
#[path = "sync_loop_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "sync_loop_streaming_tests.rs"]
mod streaming_tests;
