//! Inbound pull phase: paginated by transaction-HLC group, with the
//! held-back-trailing-group buffer pipeline and per-group apply + cursor
//! advance.

use std::collections::HashSet;

use tauri::{Emitter, Manager};

use super::super::error::DeliveryError;
use super::super::peer::PeerSession;
use super::log_sync;
use crate::crdt::commands::{
    apply_remote_changes_to_db, group_by_transaction_hlc, RemoteColumnChange,
};
use crate::crdt::hlc::{compare_hlc_strings, hlc_max};
use crate::crdt::scanner::LocalColumnChange;
use crate::database::DbConnection;

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

/// Paginated pull phase: pull transaction-HLC pages from the leader, apply each
/// complete transaction group with failure isolation, and emit the UI-refresh
/// event once after all pages applied.
///
/// The serve side packs WHOLE HLC-groups into each page up to a byte budget
/// and reports `has_more`; a transaction is never split across a page. This
/// lets a transaction larger than the legacy 10 MB wire cap (e.g. a password
/// attachment blob) traverse the wire one page at a time.
///
/// Two cursors are in play:
///   - `page_after`: the IN-CYCLE pull cursor. Starts at the cumulative apply
///     cursor and advances to the MAX HLC of each page so the next page's
///     strictly-greater scan resumes correctly (HLC is unique per source
///     transaction → no skips/dups).
///   - `last_pull_timestamp`: the cumulative apply cursor (in-memory, carried
///     across cycles for this loop's lifetime — NOT persisted to disk; it
///     resets to `None` on restart, which is safe because re-apply is
///     idempotent LWW). Advanced ONLY by `apply_groups_advancing_cursor`
///     after a group actually commits. A page pulled but not yet applied
///     must NOT move it.
///
/// `split_complete_groups` holds back the trailing (max-HLC) group while more
/// pages are coming — with HLC-aligned pages that group is complete and
/// applies on the next page (or at `has_more = false`), via the carried-over
/// `buffer`.
///
/// The HLC lock is re-acquired per page, scoped to the apply: a
/// `std::sync::MutexGuard` is not `Send`, so it cannot be held across the
/// `pull_changes().await` at the top of the loop (the future is spawned via
/// `tokio::spawn`, which requires `Send`). Re-locking per page is cheap — the
/// apply is the dominant work — and `lock_or_fail` surfaces a banner-visible
/// failure on poison instead of silently applying without advancing the clock.
pub(super) async fn run_pull_phase(
    db: &DbConnection,
    session: &PeerSession,
    space_id: &str,
    app_handle: &tauri::AppHandle,
    last_pull_timestamp: &mut Option<String>,
) -> Result<(), DeliveryError> {
    let mut page_after: Option<String> = last_pull_timestamp.clone();
    let mut buffer: Vec<RemoteColumnChange> = Vec::new();
    let mut affected_tables: HashSet<String> = HashSet::new();

    loop {
        // Snapshot the cursor we are about to request with, so we can detect a
        // leader that claims `has_more` without making progress (see the
        // stall guard at the bottom of the loop).
        let prev_after = page_after.clone();

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

        // Stall guard: a correct leader paginating with `has_more = true` always
        // returns at least one new HLC-group, so `page_after` advances past
        // `prev_after`. If it did NOT advance — an empty page with `has_more`, or
        // a leader that ignores the cursor and replays the same page — continuing
        // would spin this loop forever issuing QUIC requests (the apply is an
        // idempotent no-op each time). Stop instead; the next cycle retries from
        // the cumulative apply cursor.
        if page_after == prev_after {
            log_sync(
                app_handle,
                "warn",
                &format!(
                    "pull: has_more=true but cursor did not advance (after={:?}, count={}); \
                     stopping to avoid an infinite pull loop",
                    page_after.as_deref(),
                    page_count
                ),
            );
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

    Ok(())
}

#[cfg(test)]
#[path = "../sync_loop_streaming_tests.rs"]
mod streaming_tests;
