//! Table scanner for outbound CRDT changes (unencrypted, for local sync).
//!
//! This is the Rust equivalent of `src/stores/sync/tableScanner.ts` (`scanTableForChangesAsync`).
//! It produces unencrypted column-level changes for local space sync over QUIC,
//! which provides transport encryption.
//!
//! The generic row-emission, pagination and schema-partitioning logic lives in
//! `haex_crdt`; this module is the shared-space layer composed on top of it:
//! the whitelists, the `haex_shared_space_sync` register lookup, and the
//! per-space signature resolution. `haex_crdt` is deliberately space-agnostic
//! and stays that way.

use crate::crdt::commands::apply::ColumnSig;
use crate::database::core::with_connection;
use crate::database::error::DatabaseError;
use crate::database::DbConnection;
use haex_crdt::{get_table_schema, ColumnChange, Paginable, ScanFilters};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, HashSet};

/// Whitelist of CRDT tables that may be synchronised between peers of a
/// shared space. Everything else (identities, sync backends, vault settings,
/// pending invites, UCAN chains, extension tables …) is considered vault-
/// private and must **never** be shipped across a space-delivery stream.
///
/// The UCAN delegation chain itself travels inside each delegated token
/// (`proofs` field), so `haex_ucan_tokens` does not need to be synced either.
pub const SPACE_SCOPED_CRDT_TABLES: &[&str] = &[
    "haex_space_devices",
    "haex_space_members",
    "haex_peer_shares",
    "haex_mls_sync_keys",
    "haex_device_mls_enrollments",
    // Register itself: unshare = delete a row here without deleting the
    // business row. Members must see the register-DELETE to hide the row.
    "haex_shared_space_sync",
    // Per-space delete-log (ADR 0002 §6.5): DELETE cascade on business tables
    // writes here; apply-path on receiver removes both the business row and
    // the register entry, gated by a register-check.
    "haex_shared_space_deleted_rows",
    // Per-space anti-resurrection anchor (ADR 0002 §6.5): retention job
    // advances this to the max HLC pruned from the delete-log; pushes with
    // hlc < anchor are rejected so a stale peer cannot re-introduce a row
    // whose delete-signal has been pruned.
    "haex_space_compaction_anchors",
    // Phase 4 Round F1: cross-space file-sharing grant registry. Each row
    // records "content object X is shared with space Y" — space-scoped by
    // its `space_id` column so a Space Alpha member's device never
    // receives a grant row for Space Beta. Space members see grants as
    // soon as the owner's CRDT push arrives, without a bucket LIST.
    "haex_file_grants",
    // Phase 4 Round F1: per-space, per-member scoped-S3-credential
    // distribution. Payload is AEAD-sealed under the space's current MLS
    // epoch key so only members can extract it; scoping by `space_id`
    // keeps a member of one space from ever seeing another space's
    // credentials on the wire.
    "haex_s3_shared_access",
];

/// Subset of [`SPACE_SCOPED_CRDT_TABLES`] that every member — including
/// read-only ones — must be able to push, because the rows describe the
/// member's own existence in the group:
///
/// - `haex_space_members`     — own membership row
/// - `haex_space_devices`     — own device registration
/// - `haex_mls_sync_keys`     — own MLS KeyPackages (so others can encrypt to us)
/// - `haex_device_mls_enrollments` — own MLS enrollment artifact
///
/// `haex_peer_shares` is intentionally **not** here: that table holds rows
/// like "I host folder X under endpoint Y" which is genuine user content.
/// A read-only member must not be able to publish shares.
///
/// The leader still re-injects `authored_by_did` from the UCAN audience in
/// `inbound_sync::validate_and_attribute`, so a read-only member cannot
/// forge a row claiming to belong to someone else.
pub const MEMBERSHIP_SYSTEM_TABLES: &[&str] = &[
    "haex_space_devices",
    "haex_space_members",
    "haex_mls_sync_keys",
    "haex_device_mls_enrollments",
];

/// Returns true if `table_name` may be synchronised as part of a shared space.
pub fn is_space_scoped_table(table_name: &str) -> bool {
    SPACE_SCOPED_CRDT_TABLES.contains(&table_name)
}

/// Returns `Ok(true)` iff `(table_name, row_pks, space_id)` appears in
/// `haex_shared_space_sync` — i.e. the row has been explicitly registered
/// as belonging to the space via
/// [`extension_space_assign`](crate::extension::spaces::commands::extension_space_assign).
///
/// This is the second half of the "is this row in scope for the space?"
/// decision paired with [`is_space_scoped_table`]: whitelisted tables are
/// always in scope, everything else must be registered per-row via the
/// registry consulted here.
///
/// Call sites (both must apply the same fail-CLOSED semantics):
/// * `space_delivery::local::inbound_sync::validate_and_attribute` — inbound
///   scope check on leader-side accept.
/// * Task 4 outbound scanner — space-scoped push filter, to be added.
///
/// **Fail-CLOSED contract.** A DB failure MUST propagate up as `Err`, not
/// collapse into `Ok(false)` — the caller uses the error to reject the
/// batch while surfacing the underlying cause. Never widen this to
/// `.unwrap_or(false)` or `.ok()`: that would either silently accept an
/// unregistered row (if the semantics were flipped) or hide the DB failure
/// signal that operators need to diagnose the wedge. `.optional()` is safe
/// here because it only converts `QueryReturnedNoRows` to `Ok(None)`.
///
/// The `row_pks` encoding must match what the outbound scanner produces.
/// The **canonical form is schema-declaration order** — the order PK
/// columns are declared in the table's CREATE TABLE. This matches the TS
/// `JSON.stringify` on object literals built in schema-PK-declaration
/// order (which `extension_space_assign` stores verbatim), and the TS
/// reader `tableScanner.ts:446-449` (`json_object('pk1', t."pk1", 'pk2',
/// t."pk2")`, iteration order = schema order). The Rust CRDT scanner
/// builds its PK JSON via an explicit string builder iterating
/// `pk_columns` in schema order — see
/// `haex_crdt::scan_table_for_local_changes`.
///
/// Every writer of `haex_shared_space_sync.row_pks` must produce the same
/// schema-declaration form, or a `HashSet::contains` filter on composite
/// PKs with non-alphabetical schema order like `(col_b, col_a)` will miss
/// the row.
///
/// Note: `crdt::column_sig::register_lookup::canonicalize_row_pks`
/// normalises to alphabetical order — it is used **only** for internal
/// cache-key normalisation and is NOT the wire form. Do not use it to
/// build register entries.
pub fn is_registered_for_space(
    conn: &Connection,
    table_name: &str,
    row_pks: &str,
    space_id: &str,
) -> Result<bool, DatabaseError> {
    conn.query_row(
        "SELECT 1 FROM haex_shared_space_sync \
         WHERE table_name = ?1 AND row_pks = ?2 AND space_id = ?3 \
         LIMIT 1",
        rusqlite::params![table_name, row_pks, space_id],
        |_| Ok(()),
    )
    .optional()
    .map(|opt| opt.is_some())
    .map_err(|e| DatabaseError::QueryError {
        reason: format!("is_registered_for_space({table_name}, {row_pks}, {space_id}): {e}"),
    })
}

/// Returns true if a push targeting `table_name` only requires the caller to
/// hold any valid space capability (Read is enough). See the doc on
/// [`MEMBERSHIP_SYSTEM_TABLES`] for the rationale.
pub fn is_membership_system_table(table_name: &str) -> bool {
    MEMBERSHIP_SYSTEM_TABLES.contains(&table_name)
}

/// A column-level change ready for local transmission (no encryption).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalColumnChange {
    pub table_name: String,
    /// JSON string of PK values, e.g. `{"id":"abc-123"}`
    pub row_pks: String,
    pub column_name: String,
    pub hlc_timestamp: String,
    /// Plain value (not encrypted)
    pub value: JsonValue,
    pub device_id: String,
    /// Per-column signature for the requested shared-space stream. Owner-vault
    /// sync is unscoped and therefore leaves this absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sig: Option<ColumnSig>,
}

impl Paginable for LocalColumnChange {
    fn transaction_hlc(&self) -> &str {
        &self.hlc_timestamp
    }
}

/// Resolves a `haex_crdt` [`ColumnChange`] into vault's per-space
/// [`LocalColumnChange`].
///
/// `ColumnChange::sig` is the **raw** entry from
/// `haex_column_sigs_no_trigger` for that column — vault's whole
/// `{space_id: sig}` object, undecoded, because the crate must not know
/// about spaces. This is the only place that opens it.
///
/// `sig_space_id` is deliberately **decoupled** from the SQL `space_id`
/// filter: extension/content tables carry per-space sigs in
/// `haex_column_sigs_no_trigger` but have no `space_id` column of their own,
/// so the registry-driven path resolves sigs for a space it never filtered
/// on. Owner-vault paths pass `None` and get `sig: None` — that sync is
/// unscoped and has no space to key a signature by.
///
/// A sig entry that fails to decode into [`ColumnSig`] is swallowed
/// (`.ok()`) and travels as `None`, so one malformed sig cannot abort a
/// scan; the receiver's registered-content gate rejects the change instead.
fn to_local_change(change: ColumnChange, sig_space_id: Option<&str>) -> LocalColumnChange {
    let sig = sig_space_id.and_then(|space_id| {
        change
            .sig
            .as_ref()
            .and_then(|by_space| by_space.get(space_id))
            .cloned()
            .and_then(|record| serde_json::from_value(record).ok())
    });

    LocalColumnChange {
        table_name: change.table_name,
        row_pks: change.row_pks,
        column_name: change.column_name,
        hlc_timestamp: change.hlc_timestamp,
        value: change.value,
        device_id: change.device_id,
        sig,
    }
}

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
                target: "crdt::scanner",
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
            target: "crdt::scanner",
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
                    target: "crdt::scanner",
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

#[cfg(test)]
#[path = "scanner_tests.rs"]
mod tests;
