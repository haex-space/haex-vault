//! Owner-vault pending-column recovery: re-pull and re-apply CRDT column values
//! that an earlier apply skipped because the local schema lacked the column.

use tauri::Manager;

use super::super::error::DeliveryError;
use super::super::peer::PeerSession;
use super::log_sync;
use super::pull::local_to_remote_change;
use crate::crdt::commands::{apply_remote_changes_to_db, RemoteColumnChange};
use crate::crdt::scanner::LocalColumnChange;
use crate::crdt::trigger::get_table_schema;
use crate::database::core::with_connection;
use crate::database::error::DatabaseError;
use crate::database::migrations::{
    clear_pending_column_row_inner, get_pending_column_rows_inner, pending_columns_count,
    PendingColumnRow,
};
use crate::database::DbConnection;

/// Owed row-aware markers we can recover THIS cycle: those whose column exists
/// in the local schema now. A pending entry may reference a column the local
/// migration has not re-added yet (the loop runs continuously between the skip
/// and the app update that migrates). If we recovered+cleared such an entry,
/// `apply_remote_changes_to_db` would just re-skip the still-missing column
/// and we'd have cleared the pending marker — the skipped values would then
/// NEVER be recovered (silent data loss). So columns the migration has not
/// re-added stay pending for a later cycle.
pub(super) fn recoverable_pending_columns(
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
pub(super) fn rows_present_in_changes(
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
pub(super) fn pending_rows_to_clear(
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
pub(super) async fn run_owner_pending_column_recovery(
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
    //
    // `space_id` is a Haex UUID (ASCII) today, but use char-aware truncation so
    // a future caller passing a multi-byte string can't panic on a UTF-8
    // boundary slice.
    log_sync(
        app_handle,
        "info",
        &format!(
            "owner pending-column recovery: space={} columns={} requested={} recovered={} left_pending={}",
            space_id.chars().take(8).collect::<String>(),
            request_pairs.len(),
            owed_rows.len(),
            cleared,
            owed_rows.len() - cleared,
        ),
    );

    Ok(())
}
