//! Table scanner for outbound CRDT changes (unencrypted, for local sync).
//!
//! This is the Rust equivalent of `src/stores/sync/tableScanner.ts` (`scanTableForChangesAsync`).
//! It produces unencrypted column-level changes for local space sync over QUIC,
//! which provides transport encryption.

use crate::crdt::commands::apply::ColumnSig;
use crate::crdt::hlc::hlc_is_newer;
use crate::crdt::trigger::{
    get_table_schema, ColumnInfo, COLUMN_HLCS_COLUMN, COLUMN_SIGS_COLUMN, HLC_TIMESTAMP_COLUMN,
};
use crate::database::core::{
    convert_value_ref_to_json, with_connection, MAX_CRDT_TRANSACTION_BYTES,
};
use crate::database::error::DatabaseError;
use crate::database::DbConnection;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// Sync metadata columns to exclude from scanning (not user data).
const EXCLUDED_SYNC_COLUMNS: &[&str] = &[
    "last_push_hlc_timestamp",
    "last_pull_server_timestamp",
    "updated_at",
    "created_at",
];

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
/// The `row_pks` encoding must match what the outbound scanner produces —
/// the CRDT machinery uses canonical JSON like `{"id":"row-1"}`, and
/// `haex_shared_space_sync.row_pks` stores exactly the same form.
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

/// Splits a table schema into PK columns and syncable data columns.
///
/// Data columns exclude:
/// - PK columns
/// - CRDT metadata: `haex_hlc`, `haex_column_hlcs`, `haex_column_sigs`
/// - Sync metadata: `last_push_hlc_timestamp`, `last_pull_server_timestamp`, `updated_at`, `created_at`
fn partition_columns(schema: &[ColumnInfo]) -> (Vec<&ColumnInfo>, Vec<&ColumnInfo>) {
    let pk_columns: Vec<&ColumnInfo> = schema.iter().filter(|c| c.is_pk).collect();
    let data_columns: Vec<&ColumnInfo> = schema
        .iter()
        .filter(|c| {
            !c.is_pk
                && c.name != HLC_TIMESTAMP_COLUMN
                && c.name != COLUMN_HLCS_COLUMN
                && c.name != COLUMN_SIGS_COLUMN
                && !EXCLUDED_SYNC_COLUMNS.contains(&c.name.as_str())
        })
        .collect();
    (pk_columns, data_columns)
}

/// Like `scan_table_for_local_changes` but with two additional predicates:
///
/// * `space_id_filter` — restricts the scan to rows where `space_id = ?`. Used
///   by the space-scoped sync path to prevent leaking rows from other spaces.
/// * `origin_node_filter` — when `Some`, the scanner emits a column change only
///   if its HLC's node-id matches the given `u128`. This stops "ping-pong"
///   re-pushes: rows freshly pulled from a peer carry that peer's HLC node-id
///   and would otherwise be re-scanned and pushed back on the next cycle.
pub fn scan_table_for_local_changes_scoped(
    conn: &Connection,
    table_name: &str,
    after_hlc: Option<&str>,
    device_id: &str,
    space_id_filter: Option<&str>,
    origin_node_filter: Option<u128>,
) -> Result<Vec<LocalColumnChange>, DatabaseError> {
    let schema = get_table_schema(conn, table_name).map_err(DatabaseError::from)?;

    if schema.is_empty() {
        return Ok(Vec::new());
    }

    let (pk_columns, data_columns) = partition_columns(&schema);

    if pk_columns.is_empty() {
        return Err(DatabaseError::ExecutionError {
            sql: format!("PRAGMA table_info(\"{}\")", table_name),
            reason: format!("Table '{}' has no primary key", table_name),
            table: Some(table_name.to_string()),
        });
    }

    // If the caller asked to filter by space_id but the table has no
    // `space_id` column, treat that as "no matching rows" rather than
    // silently returning the whole table.
    let has_space_id_column = schema.iter().any(|c| c.name == "space_id");
    if space_id_filter.is_some() && !has_space_id_column {
        return Ok(Vec::new());
    }

    // Build column list: PKs + data columns + CRDT metadata
    let mut select_columns: Vec<&str> = Vec::new();
    for col in &pk_columns {
        select_columns.push(&col.name);
    }
    for col in &data_columns {
        select_columns.push(&col.name);
    }
    select_columns.push(HLC_TIMESTAMP_COLUMN);
    select_columns.push(COLUMN_HLCS_COLUMN);
    let has_column_sigs = schema.iter().any(|c| c.name == COLUMN_SIGS_COLUMN);
    if has_column_sigs {
        select_columns.push(COLUMN_SIGS_COLUMN);
    }

    let column_list: String = select_columns
        .iter()
        .map(|c| format!("\"{}\"", c))
        .collect::<Vec<_>>()
        .join(", ");

    let mut where_clauses: Vec<String> = Vec::new();
    let mut params: Vec<String> = Vec::new();

    if let Some(hlc) = after_hlc {
        // Admit rows whose row-level HLC is absent (NULL) or empty in addition
        // to those strictly newer than the cursor. A corrupt/legacy row can
        // carry `haex_hlc = ''` while still holding a valid per-column HLC in
        // `haex_column_hlcs`; a bare `"haex_hlc" > ?` prefilter drops it before
        // the per-column fallback below can emit that valid change, so the row
        // could only ever converge on a full scan. The per-column loop re-checks
        // each HLC against `after_hlc`, so widening here cannot leak stale
        // columns — rows with no usable HLC are still skipped.
        where_clauses.push(format!(
            "(\"{col}\" > ?{n} OR \"{col}\" IS NULL OR \"{col}\" = '')",
            col = HLC_TIMESTAMP_COLUMN,
            n = where_clauses.len() + 1
        ));
        params.push(hlc.to_string());
    }
    if let Some(space_id) = space_id_filter {
        where_clauses.push(format!("\"space_id\" = ?{}", where_clauses.len() + 1));
        params.push(space_id.to_string());
    }

    let query = if where_clauses.is_empty() {
        format!("SELECT {} FROM \"{}\"", column_list, table_name)
    } else {
        format!(
            "SELECT {} FROM \"{}\" WHERE {}",
            column_list,
            table_name,
            where_clauses.join(" AND ")
        )
    };

    let mut stmt = conn.prepare(&query).map_err(DatabaseError::from)?;

    let param_refs: Vec<&dyn rusqlite::ToSql> =
        params.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

    let mut rows = stmt
        .query(param_refs.as_slice())
        .map_err(DatabaseError::from)?;

    let mut changes: Vec<LocalColumnChange> = Vec::new();

    while let Some(row) = rows.next().map_err(DatabaseError::from)? {
        // Read all column values into a name -> JsonValue map
        let mut row_map: HashMap<&str, JsonValue> = HashMap::new();
        for (i, col_name) in select_columns.iter().enumerate() {
            let value_ref = row.get_ref(i).map_err(DatabaseError::from)?;
            let json_val = convert_value_ref_to_json(value_ref)?;
            row_map.insert(col_name, json_val);
        }

        // Parse haex_column_hlcs JSON
        let column_hlcs: HashMap<String, String> = match row_map.get(COLUMN_HLCS_COLUMN) {
            Some(JsonValue::String(s)) => serde_json::from_str(s).unwrap_or_default(),
            _ => HashMap::new(),
        };
        let column_sigs: JsonValue = row_map
            .get(COLUMN_SIGS_COLUMN)
            .and_then(JsonValue::as_str)
            .and_then(|raw| serde_json::from_str(raw).ok())
            .unwrap_or_else(|| JsonValue::Object(serde_json::Map::new()));

        // Build PK JSON string
        let pk_map: serde_json::Map<String, JsonValue> = pk_columns
            .iter()
            .filter_map(|pk| {
                row_map
                    .get(pk.name.as_str())
                    .map(|v| (pk.name.clone(), v.clone()))
            })
            .collect();
        let pk_json = serde_json::to_string(&pk_map).unwrap_or_else(|_| "{}".to_string());

        // Row-level HLC as fallback. An empty string is treated as "absent":
        // a corrupt/legacy row can carry `haex_hlc = ''` (e.g. inserted before
        // the HLC trigger existed, or by an older build), and an empty HLC must
        // never be propagated as if it were a real timestamp.
        let row_hlc = match row_map.get(HLC_TIMESTAMP_COLUMN) {
            Some(JsonValue::String(s)) if !s.is_empty() => Some(s.as_str()),
            _ => None,
        };

        // For each data column, emit a change if its HLC > after_hlc
        for col in &data_columns {
            // Treat an empty per-column HLC as absent so it falls back to the
            // row HLC; if both are empty/missing the column has no usable
            // timestamp and is skipped. This stops empty-string HLCs (`""`)
            // from ever being emitted as `hlc_timestamp`. Downstream, every
            // apply ran `compare_hlc_strings("")` per such column — the source
            // of the `[HLC] cannot parse time component of ""` log flood — and
            // the row could never converge (`"" > anything` is always false),
            // so it was re-scanned and re-sent on every full pull forever.
            let col_hlc = column_hlcs
                .get(&col.name)
                .map(|s| s.as_str())
                .filter(|s| !s.is_empty());

            let hlc_to_use = match col_hlc.or(row_hlc) {
                Some(h) => h,
                None => continue, // no usable HLC — skip
            };

            // Check if this column's HLC is newer than after_hlc
            let passes_hlc = match after_hlc {
                Some(threshold) => hlc_is_newer(hlc_to_use, threshold),
                None => true,
            };

            // If the caller asked for origin filtering, only emit columns we
            // wrote ourselves. Rows applied from inbound sync carry the
            // remote peer's node-id and must not be pushed back.
            let passes_origin = match origin_node_filter {
                Some(our_node) => crate::crdt::hlc::hlc_is_from_node(hlc_to_use, our_node),
                None => true,
            };

            if passes_hlc && passes_origin {
                let value = row_map
                    .get(col.name.as_str())
                    .cloned()
                    .unwrap_or(JsonValue::Null);
                let sig = space_id_filter.and_then(|space_id| {
                    column_sigs
                        .get(&col.name)
                        .and_then(|by_space| by_space.get(space_id))
                        .cloned()
                        .and_then(|record| serde_json::from_value(record).ok())
                });

                changes.push(LocalColumnChange {
                    table_name: table_name.to_string(),
                    row_pks: pk_json.clone(),
                    column_name: col.name.clone(),
                    hlc_timestamp: hlc_to_use.to_string(),
                    value,
                    device_id: device_id.to_string(),
                    sig,
                });
            }
        }
    }

    Ok(changes)
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

        // Global sort by transaction-HLC ascending so downstream chunking can
        // respect HLC-group boundaries without further grouping logic.
        all_changes.sort_by(|a, b| {
            crate::crdt::hlc::compare_hlc_strings(&a.hlc_timestamp, &b.hlc_timestamp)
        });

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
/// (all `haex_*` tables carrying a `haex_hlc` column, including vault-private
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
    all_changes
        .sort_by(|a, b| crate::crdt::hlc::compare_hlc_strings(&a.hlc_timestamp, &b.hlc_timestamp));

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

/// The serve-side per-page byte budget for a paginated `SyncPull`.
///
/// One transaction-HLC group is one source transaction, capped at
/// [`MAX_CRDT_TRANSACTION_BYTES`] (ADR 0001) at `execute_with_crdt`. Setting the
/// page budget equal to that cap means a single page always has room for the
/// largest legal transaction (the ≥1 rule in [`paginate_changes`] guarantees
/// even an at-cap group is emitted), so no transaction can ever be too big to
/// page out. The wire frame cap (`protocol::WIRE_FRAME_MAX`) is sized above this
/// to carry such a page plus envelope overhead.
pub(crate) const PULL_PAGE_BUDGET: usize = MAX_CRDT_TRANSACTION_BYTES;

/// Pack whole transaction-HLC groups into one page until adding the next group
/// would exceed `page_budget`, returning `(page, has_more)`.
///
/// HLC == one source transaction, so all changes sharing an `hlc_timestamp`
/// belong to one transaction and are never split across a page boundary. Groups
/// are emitted in ascending HLC order (matching the scanner's global ordering
/// and `group_by_transaction_hlc`), so the client can resume the next page at
/// the MAX HLC of the page just received — the cursor stays HLC-only.
///
/// Packing rule: maintain a running serialized byte total. A group's size is
/// `serde_json::to_vec(&group).map(|v| v.len()).unwrap_or(usize::MAX)` (an
/// unmeasurable group counts as maximal, so it can only ever stand alone). Add
/// the group iff `running + group_size <= page_budget`; otherwise STOP and defer
/// this and every later group (`has_more = true`).
///
/// **≥1 rule:** if the page is still empty when the first group alone exceeds
/// the budget, that group is included anyway (and `has_more = true` if later
/// groups exist) — otherwise an at-or-over-budget transaction could never
/// traverse the wire. Bounded above by `MAX_CRDT_TRANSACTION_BYTES`.
///
/// Pure and deterministic: no I/O.
pub(crate) fn paginate_changes(
    changes: Vec<LocalColumnChange>,
    page_budget: usize,
) -> (Vec<LocalColumnChange>, bool) {
    if changes.is_empty() {
        return (Vec::new(), false);
    }

    // Group by transaction-HLC in ascending order without splitting a group.
    // The scanner already returns changes globally HLC-sorted, but callers may
    // hand us any order, so group via a map and sort the keys — same contract as
    // `group_by_transaction_hlc` (commands.rs), kept local to avoid a
    // RemoteColumnChange round-trip.
    let mut groups: HashMap<String, Vec<LocalColumnChange>> = HashMap::new();
    for change in changes {
        groups
            .entry(change.hlc_timestamp.clone())
            .or_default()
            .push(change);
    }
    let mut ordered: Vec<(String, Vec<LocalColumnChange>)> = groups.into_iter().collect();
    ordered.sort_by(|a, b| crate::crdt::hlc::compare_hlc_strings(&a.0, &b.0));

    let mut page: Vec<LocalColumnChange> = Vec::new();
    let mut running: usize = 0;
    let mut has_more = false;

    for (idx, (_hlc, group)) in ordered.into_iter().enumerate() {
        let group_size = serde_json::to_vec(&group)
            .map(|v| v.len())
            .unwrap_or(usize::MAX);
        let fits = running.saturating_add(group_size) <= page_budget;
        // ≥1 rule: the very first group is always taken, even if oversized.
        if fits || idx == 0 {
            running = running.saturating_add(group_size);
            page.extend(group);
        } else {
            // This group and every later group are deferred to the next page.
            has_more = true;
            break;
        }
    }

    (page, has_more)
}

#[cfg(test)]
#[path = "scanner_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "scanner_pagination_tests.rs"]
mod pagination_tests;
