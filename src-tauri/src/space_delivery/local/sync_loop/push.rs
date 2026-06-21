//! Outbound push phase: scan dirty CRDT changes, chunk at HLC boundaries, and
//! push chunk-by-chunk to the leader.

use std::collections::HashSet;

use super::super::error::DeliveryError;
use super::super::peer::PeerSession;
use super::super::push_cursor::save_last_push_hlc;
use super::membership::filter_foreign_membership_rows;
use super::{sqlite_datetime_now, SyncMode};
use crate::crdt::commands::clear_dirty_table_inner;
use crate::crdt::hlc::hlc_max;
use crate::crdt::scanner::{
    scan_all_crdt_tables_for_owner, scan_membership_tables_for_local_changes,
    scan_space_scoped_tables_for_local_changes, LocalColumnChange,
};
use crate::database::core::with_connection;
use crate::database::error::DatabaseError;
use crate::database::DbConnection;

/// Soft cap for changes per QUIC push request. Mirrors the HTTP path's
/// `PUSH_CHUNK_SOFT_LIMIT` — see `src/stores/sync/orchestrator/push.ts`.
/// A single transaction-HLC group larger than this is still sent in one
/// request rather than split.
const PUSH_CHUNK_SOFT_LIMIT: usize = 2000;

/// Decide which scanner produces the push batch for `mode` and return its
/// changes. Extracted from [`run_push_phase`] so the mode→scanner decision is
/// unit-testable without an `AppHandle` or QUIC session (a `DbConnection` over
/// an in-memory `rusqlite::Connection` is enough).
///
/// The `SpaceScoped` branch calls the existing space-scoped scanners verbatim,
/// so its output is byte-identical to the previous inline logic. The
/// `OwnerVault` branch runs the full-vault scanner over the caller-supplied
/// table list.
pub(super) fn collect_push_changes(
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

/// Push local space-scoped changes to the leader.
///
/// Scans only rows belonging to `space_id` (via the space-scoped whitelist
/// scanner), chunks them at HLC-group boundaries, and pushes chunk-by-chunk.
/// On a per-chunk failure the remaining chunks are skipped and the partial
/// progress is checkpointed in `last_push_hlc` so the next cycle resumes
/// without re-sending what the leader already accepted.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_push_phase(
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
