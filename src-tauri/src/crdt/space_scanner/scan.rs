//! The space-scoped scans: one table scoped by its own `space_id` column,
//! the registry-driven pass for extension-owned content tables, and the two
//! whitelist entry points the sync transport calls.
//!
//! Split out of `mod.rs` to keep both files inside the repo's file-size
//! cap. The OWNER-ONLY unscoped scans live in `owner.rs`.

use super::{
    to_local_change, LocalColumnChange, MEMBERSHIP_SYSTEM_TABLES, SPACE_SCOPED_CRDT_TABLES,
};
use crate::database::core::with_connection;
use crate::database::error::DatabaseError;
use crate::database::DbConnection;
use haex_crdt::{get_table_schema, ScanFilters};
use rusqlite::Connection;
use std::collections::{BTreeMap, HashSet};

/// Like `haex_crdt::scan_table_for_local_changes` but with two additional
/// predicates:
///
/// * `space_id_filter` — restricts the scan to rows where `space_id = ?`. Used
///   by the space-scoped sync path to prevent leaking rows from other spaces.
///   It doubles as the sig-space: if a row survived the WHERE clause it is
///   *this* space's row, so any sig forwarded must also be indexed by *this*
///   space.
/// * `origin_node_filter` — when `Some`, the scanner emits a column change only
///   if its HLC's node-id matches the given `u128`. This stops "ping-pong"
///   re-pushes: rows freshly pulled from a peer carry that peer's HLC node-id
///   and would otherwise be re-scanned and pushed back on the next cycle.
///
/// A `space_id_filter` on a table that has no `space_id` column yields zero
/// rows rather than the whole table — `ScanFilters::column_eq` is fail-closed
/// on an absent filter column, so a misconfigured filter cannot leak a
/// vault-private table.
pub fn scan_table_for_local_changes_scoped(
    conn: &Connection,
    table_name: &str,
    after_hlc: Option<&str>,
    device_id: &str,
    space_id_filter: Option<&str>,
    origin_node_filter: Option<u128>,
) -> Result<Vec<LocalColumnChange>, DatabaseError> {
    let changes = haex_crdt::scan_table_for_local_changes(
        conn,
        table_name,
        after_hlc,
        device_id,
        ScanFilters {
            origin_node: origin_node_filter,
            row_pks: None,
            column_eq: space_id_filter.map(|space_id| ("space_id", space_id)),
        },
    )?;

    Ok(changes
        .into_iter()
        .map(|change| to_local_change(change, space_id_filter))
        .collect())
}

/// Scans an extension-owned table for rows whose canonical PK JSON is on the
/// `row_pks_set` allow-list — i.e. rows the caller has already resolved as
/// belonging to `space_id` via `haex_shared_space_sync`.
///
/// Unlike [`scan_table_for_local_changes_scoped`], this does NOT filter by a
/// `space_id` column on the target table: extension/content tables typically
/// do not carry one, and the row-to-space mapping lives entirely in the
/// registry. Per-column signatures are still extracted for `space_id` from
/// `haex_column_sigs_no_trigger` (W1 → W2 contract: the receiver's registered-content
/// gate rejects rows without a matching per-space sig).
///
/// # Error policy: warn-and-skip on the register, propagate on the scan
///
/// `table_name` comes from `haex_shared_space_sync`, which is itself
/// replicated content and therefore peer-influenced — treat it as untrusted.
/// A malformed register entry (unsafe identifier, dropped/renamed target
/// table, or a table with no primary key) must not abort the whole space
/// push: pass 1 (control-plane) already produced changes, and other register
/// entries for the same space must still be scanned. Those three cases are
/// therefore decided by the cheap pre-checks below and skipped with a
/// `warn!`.
///
/// Everything the scan itself reports is a genuine mid-scan failure and
/// **propagates**. Do not widen the pre-check into a catch-and-warn around
/// the scan call: that would turn a real I/O failure into "no changes",
/// silently stalling a space's push instead of surfacing it.
///
/// # Parameters
///
/// * `row_pks_set` — canonical PK JSON strings (`{"id":"row-1"}`, or
///   `{"col_b":"y","col_a":"x"}` for a composite PK declared `(col_b,
///   col_a)`). Encoding must match what `haex_crdt`'s scanner produces via
///   the explicit string builder iterating `pk_columns` in
///   **schema-declaration order** (the order `get_table_schema` returns
///   them). This matches the writer wire form: TS extensions emit
///   `JSON.stringify({pk1: ..., pk2: ...})` on object literals built in
///   schema-PK-declaration order, and `extension_space_assign` stores
///   `row_pks` verbatim. See `tableScanner.ts:446-449` for the TS reader's
///   `json_object` construction, which also preserves schema order. Every
///   register writer must therefore emit schema-declaration-order PK JSON,
///   or composite-PK rows with non-alphabetical schema order will silently
///   be skipped by the crate's `ScanFilters::row_pks` membership check.
/// * `after_hlc` — exclusive HLC lower bound, same semantics as
///   [`scan_table_for_local_changes_scoped`].
/// * `origin_node` — ping-pong filter, same semantics as
///   [`scan_table_for_local_changes_scoped`].
fn scan_registered_table_rows_for_space(
    conn: &Connection,
    table_name: &str,
    space_id: &str,
    row_pks_set: &[String],
    after_hlc: Option<&str>,
    device_id: &str,
    origin_node: Option<u128>,
) -> Result<Vec<LocalColumnChange>, DatabaseError> {
    if row_pks_set.is_empty() {
        return Ok(Vec::new());
    }

    // Pre-checks for the skip decision — see the error policy above. Cheap
    // (one PRAGMA), and deliberately separate from the scan so the scan's own
    // errors stay propagating.
    let schema = match get_table_schema(conn, table_name) {
        Ok(s) if !s.is_empty() => s,
        Ok(_) => return Ok(Vec::new()),
        Err(e) => {
            tracing::warn!(
                target: "crdt::space_scanner",
                table = %table_name,
                space_id,
                error = %e,
                "registry references unreadable table; skipping"
            );
            return Ok(Vec::new());
        }
    };

    if !schema.iter().any(|c| c.is_pk) {
        tracing::warn!(
            target: "crdt::space_scanner",
            table = %table_name,
            space_id,
            "registry references table without a primary key; skipping"
        );
        return Ok(Vec::new());
    }

    // Only the HLC threshold is pushed down to SQL — the PK allow-list is
    // applied in-memory by the crate's `row_pks` filter. Registry sizes are
    // expected to be dozens–thousands per space, so an extra scan of the
    // target table is cheaper than building a variadic PK-tuple WHERE clause.
    let wanted: HashSet<String> = row_pks_set.iter().cloned().collect();

    let changes = haex_crdt::scan_table_for_local_changes(
        conn,
        table_name,
        after_hlc,
        device_id,
        ScanFilters {
            origin_node,
            row_pks: Some(&wanted),
            column_eq: None,
        },
    )?;

    Ok(changes
        .into_iter()
        .map(|change| to_local_change(change, Some(space_id)))
        .collect())
}
/// Scans the whitelist of space-scoped CRDT tables for rows belonging to
/// `space_id`. This is the authoritative scanner for peer-to-peer SyncPull:
/// the caller guarantees that only these tables and only these rows cross
/// the wire, so peers cannot pull data from spaces they are not members of.
///
/// `origin_node` (when `Some`) restricts the result to rows whose HLC was
/// originally written by this node — see the doc on
/// [`scan_table_for_local_changes_scoped`] for the rationale.
///
/// Tables outside [`SPACE_SCOPED_CRDT_TABLES`] are never scanned.
pub fn scan_space_scoped_tables_for_local_changes(
    db: &DbConnection,
    space_id: &str,
    after_hlc: Option<&str>,
    device_id: &str,
    origin_node: Option<u128>,
) -> Result<Vec<LocalColumnChange>, DatabaseError> {
    with_connection(db, |conn| {
        let mut all_changes: Vec<LocalColumnChange> = Vec::new();

        // Pass 1: static whitelist of control-plane tables scoped by their
        // own `space_id` column.
        for table_name in SPACE_SCOPED_CRDT_TABLES {
            let changes = scan_table_for_local_changes_scoped(
                conn,
                table_name,
                after_hlc,
                device_id,
                Some(space_id),
                origin_node,
            )?;
            all_changes.extend(changes);
        }

        // Pass 2 (Task 5, W3): registry-driven scan of extension/content
        // tables. `haex_shared_space_sync` maps `(table_name, row_pks)` to
        // `space_id` for tables that do not carry a `space_id` column of
        // their own. The register and the whitelist are additive — a
        // whitelisted table already emitted its rows in pass 1, so any
        // register row referencing a whitelisted table is filtered out here
        // as defence-in-depth against a malicious/malformed registry entry.
        let registered: Vec<(String, String)> = {
            let mut stmt = conn
                .prepare(
                    "SELECT DISTINCT table_name, row_pks \
                     FROM haex_shared_space_sync \
                     WHERE space_id = ?1",
                )
                .map_err(DatabaseError::from)?;
            let mapped = stmt
                .query_map(rusqlite::params![space_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(DatabaseError::from)?;
            let mut out: Vec<(String, String)> = Vec::new();
            for r in mapped {
                out.push(r.map_err(DatabaseError::from)?);
            }
            out
        };

        // Group PKs by table so we make one scan call per table.
        let mut by_table: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (t, pks) in registered {
            if SPACE_SCOPED_CRDT_TABLES.contains(&t.as_str()) {
                // Guard: never route control-plane through the registry
                // path. A registry row lying about a whitelisted table would
                // otherwise re-emit the row without the `space_id`-column
                // scope check pass 1 performs.
                tracing::warn!(
                    target: "crdt::space_scanner",
                    table = %t,
                    space_id,
                    "registry references control-plane table; skipping (should not happen — investigate register writer)"
                );
                continue;
            }
            by_table.entry(t).or_default().push(pks);
        }

        for (table_name, row_pks_set) in by_table {
            let changes = scan_registered_table_rows_for_space(
                conn,
                &table_name,
                space_id,
                &row_pks_set,
                after_hlc,
                device_id,
                origin_node,
            )?;
            all_changes.extend(changes);
        }

        // Global sort by transaction-HLC ascending so downstream chunking can
        // respect HLC-group boundaries without further grouping logic.
        all_changes
            .sort_by(|a, b| haex_crdt::compare_hlc_strings(&a.hlc_timestamp, &b.hlc_timestamp));

        Ok(all_changes)
    })
}

/// Like [`scan_space_scoped_tables_for_local_changes`] but restricted to
/// [`MEMBERSHIP_SYSTEM_TABLES`] only. Use this for the push phase when the
/// member holds a `space/read` UCAN: those tables may be pushed with Read
/// capability, whereas `haex_peer_shares` (the only other space-scoped table)
/// requires Write. Including peer_shares in a Read-only push batch causes the
/// leader to reject the entire batch, leaving the push cursor stuck at t=0.
pub fn scan_membership_tables_for_local_changes(
    db: &DbConnection,
    space_id: &str,
    after_hlc: Option<&str>,
    device_id: &str,
    origin_node: Option<u128>,
) -> Result<Vec<LocalColumnChange>, DatabaseError> {
    scan_space_scoped_tables_for_local_changes(db, space_id, after_hlc, device_id, origin_node).map(
        |changes| {
            changes
                .into_iter()
                .filter(|c| MEMBERSHIP_SYSTEM_TABLES.contains(&c.table_name.as_str()))
                .collect()
        },
    )
}
