//! The OWNER-ONLY unscoped scans.
//!
//! Kept apart from `scan.rs` because these apply **no** space filter:
//! their output is the owner's full vault and therefore a cross-space-leak
//! hazard. Each function's own docs carry the DID-auth precondition.

use super::{scan_table_for_local_changes_scoped, LocalColumnChange};
use crate::database::error::DatabaseError;
use rusqlite::Connection;

/// **OWNER-ONLY, UNSCOPED BY DESIGN.** Scans every table named in
/// `table_names` for local CRDT changes with **no `space_id` filter**, then
/// returns the concatenated changes in a single global HLC-ascending order.
///
/// This exists solely for serverless P2P sync of the owner's own vault across
/// the owner's own devices: that path replicates the *full* CRDT table set
/// (all `haex_*` tables carrying a `haex_hlc_no_trigger` column, including vault-private
/// and extension tables), not just the space-scoped whitelist.
///
/// # Security
///
/// Because it applies **no** space filter, its output is the entire vault and
/// is therefore a cross-space-leak hazard. It MUST only be invoked from the
/// branch that has already proven, via DID-auth, that the remote peer is the
/// *same owner* on another of the owner's own devices. The full-vault scope
/// produced here must never reach a non-owner peer. For peer-to-peer sync of a
/// *shared space* use [`scan_space_scoped_tables_for_local_changes`] instead —
/// a previous general unscoped scanner was removed precisely because it leaked
/// cross-space rows.
///
/// The caller supplies the exact `table_names` to scan; this function never
/// derives the list itself, so scope stays in the caller's hands and the
/// behaviour is "scan exactly the tables the caller passes" — nothing more.
///
/// `origin_node` (when `Some`) restricts the result to rows whose HLC was
/// originally written by this node — see the doc on
/// [`scan_table_for_local_changes_scoped`] for the rationale.
pub(crate) fn scan_all_crdt_tables_for_owner(
    conn: &Connection,
    table_names: &[String],
    after_hlc: Option<&str>,
    device_id: &str,
    origin_node: Option<u128>,
) -> Result<Vec<LocalColumnChange>, DatabaseError> {
    let mut all_changes: Vec<LocalColumnChange> = Vec::new();
    for table_name in table_names {
        let changes = scan_table_for_local_changes_scoped(
            conn,
            table_name,
            after_hlc,
            device_id,
            None, // NO space filter — owner gets the full vault by design.
            origin_node,
        )?;
        all_changes.extend(changes);
    }

    // Global sort by transaction-HLC ascending so downstream chunking can
    // respect HLC-group boundaries without further grouping logic.
    all_changes.sort_by(|a, b| haex_crdt::compare_hlc_strings(&a.hlc_timestamp, &b.hlc_timestamp));

    Ok(all_changes)
}

/// **OWNER-ONLY, UNSCOPED BY DESIGN.** Dumps every row's current value for a
/// single `(table_name, column_name)` pair, with **no `space_id` filter, no
/// HLC threshold, and no origin-node filter**. This is the single-column
/// analogue of [`scan_all_crdt_tables_for_owner`].
///
/// It exists solely to RECOVER a column that a device skipped during apply
/// because it was missing the column locally (schema skew). After a migration
/// re-adds the column, the recovering device pulls the column's complete state
/// from another of the owner's own devices over P2P.
///
/// Two deliberate `None`s, both required for correct recovery:
///
/// * `after_hlc = None` — **FULL DUMP, no HLC threshold.** The recovering
///   device never held this column, so it has no meaningful cursor; it must
///   receive every row's current value regardless of how "old" the row's HLC
///   is. Threading an HLC threshold here would silently drop rows that were
///   last written before some arbitrary cursor — exactly the values recovery
///   needs.
/// * `origin_node_filter = None` — **NO ping-pong/origin filter.** The
///   recovering device wants the COMPLETE column state across all rows,
///   including rows authored by other devices — not just rows this serving
///   device wrote. This is the deliberate opposite of the push path's origin
///   filtering: there, filtering stops re-pushing peer-authored rows; here,
///   peer-authored rows are precisely what must be returned.
///
/// # Security
///
/// Because it applies **no** space filter, its output is the UNSCOPED
/// full-vault dump for the requested column and is therefore a
/// cross-space-leak hazard. It MUST only be invoked from a branch that has
/// already proven, via DID-auth, that the remote peer is the *same owner* on
/// another of the owner's own devices. The dump produced here must never reach
/// a non-owner peer. For peer-to-peer sync of a *shared space* use
/// [`scan_space_scoped_tables_for_local_changes`] instead.
///
/// # Caller notes
///
/// * `device_id` is stamped onto every returned `LocalColumnChange.device_id`
///   as the **serving** device — it is NOT the row's author (peer-authored rows
///   are returned with this serving device's id). Do not read provenance from
///   it.
/// * Results are **unordered** (raw scan order); unlike
///   [`scan_all_crdt_tables_for_owner`] this does not sort by HLC. A consumer
///   that needs HLC order must sort itself.
/// * An empty result is **ambiguous**: it means either the column legitimately
///   has no rows, or the requested `(table, column)` is wrong/excluded. Validate
///   the pair before treating an empty dump as "recovery complete".
pub fn scan_single_column_for_owner(
    conn: &Connection,
    table_name: &str,
    column_name: &str,
    device_id: &str,
) -> Result<Vec<LocalColumnChange>, DatabaseError> {
    // FULL DUMP (after_hlc = None): recovery has no cursor for a column it
    // never held. NO space filter (space_id_filter = None): owner gets the
    // full vault by design. NO origin filter (origin_node_filter = None):
    // recovery needs the complete column, including rows authored by other
    // devices.
    let changes =
        scan_table_for_local_changes_scoped(conn, table_name, None, device_id, None, None)?;

    Ok(changes
        .into_iter()
        .filter(|c| c.column_name == column_name)
        .collect())
}

// `scan_all_crdt_tables_for_local_changes` used to scan every CRDT table
// without a space filter. That function powered the old peer SyncPull and
// was the root of a cross-space data leak — a peer asking for space X
// would receive rows from every space the leader was in. It has been
// removed. Use `scan_space_scoped_tables_for_local_changes` for peer sync.
