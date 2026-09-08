//! Space-scoped scanner for outbound CRDT changes (unencrypted, for local sync).
//!
//! The generic row-emission, pagination and schema-partitioning logic lives
//! in `haex_crdt`; this module is the shared-space layer composed on top of
//! it: the whitelists, the `haex_shared_space_sync` register lookup, and the
//! per-space signature resolution. `haex_crdt` is deliberately space-agnostic
//! and stays that way.
//!
//! The frontend counterpart is `src/stores/sync/tableScanner.ts`
//! (`scanTableForChangesAsync`), which produces the same unencrypted
//! column-level changes for local space sync over QUIC (transport
//! encryption).
//!
//! * `scan` — the space-scoped scans: one table scoped by its `space_id`
//!   column, the registry-driven pass for extension tables, and the two
//!   whitelist entry points.
//! * `owner` — the OWNER-ONLY unscoped scans, kept apart because their
//!   output is the full vault and therefore a cross-space-leak hazard.

use crate::crdt::commands::apply::ColumnSig;
use crate::database::error::DatabaseError;
use haex_crdt::{ColumnChange, Paginable};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

mod owner;
mod scan;

pub(crate) use owner::scan_all_crdt_tables_for_owner;
pub use owner::scan_single_column_for_owner;
pub use scan::{
    scan_membership_tables_for_local_changes, scan_space_scoped_tables_for_local_changes,
    scan_table_for_local_changes_scoped,
};

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
pub(super) fn to_local_change(
    change: ColumnChange,
    sig_space_id: Option<&str>,
) -> LocalColumnChange {
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

#[cfg(test)]
mod tests;
