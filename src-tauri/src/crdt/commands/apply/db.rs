use crate::crdt::column_sig::storage::{upsert_column_sigs, SigRecord};
use crate::crdt::hlc::{hlc_is_newer, hlc_max, HlcError, HlcService};
use crate::crdt::registry_row_sig::puller_verify::verify_incoming_registry_change;
use crate::crdt::trigger;
use crate::crdt::trigger::{
    get_table_schema as get_table_schema_internal, is_safe_identifier, ColumnInfo,
    COLUMN_HLCS_COLUMN, DELETED_ROWS_TABLE, HLC_TIMESTAMP_COLUMN, SHARED_SPACE_DELETED_ROWS_TABLE,
};
use crate::database::core::{with_connection, ValueConverter};
use crate::database::error::DatabaseError;
use crate::table_names::{
    COL_SHARED_SPACE_SYNC_ROW_SIG, TABLE_CRDT_CONFIGS, TABLE_CRDT_PENDING_COLUMNS,
    TABLE_CRDT_PENDING_TABLES, TABLE_SHARED_SPACE_SYNC,
};
use crate::AppState;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rusqlite::params;
use rusqlite::types::Value as SqlValue;
use rusqlite::{OptionalExtension, Transaction};
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};
use tauri::State;

use super::super::helpers::{build_pk_where_clause, json_values_to_sql_params};
use super::delete_propagation::{
    create_conflict_entry, insert_suppressed_by_deletes, propagate_deleted_rows_to_target_tables,
    propagate_shared_space_deleted_rows_to_target_tables,
};
use super::grouping::{group_by_transaction_hlc, group_row_changes_in_hlc_order};
use super::registry_row_gate::{build_incoming_registry_change, RegistryRowChangeOutcome};
use super::types::{ColumnSig, RemoteColumnChange};

/// Idempotently insert a stub `haex_identities` row for a DID we've never
/// seen before, so downstream FKs referencing `haex_identities.did` don't
/// fail when the first inbound row for a foreign author lands.
///
/// The stub uses the DID itself as its `id` (both columns are TEXT). Once
/// the real identity handshake arrives — e.g. the invite-claim flow — the
/// row is UPDATE-merged in place by the usual CRDT path; there is no
/// separate reconciliation step.
///
/// This function is the Rust replacement for the DB-trigger-based stub
/// creation that the older schema relied on (ADR 0002 §6, §D). It is called
/// from [`apply_remote_changes_to_db`] for every column change that carries
/// a **valid** column signature (Runde 5 sig-verifier plumbing). Invalid
/// or missing sigs must NOT create a stub — that would let a peer flood
/// `haex_identities` with attacker-picked DIDs.
///
/// `name` is written explicitly (with the DID as the placeholder label)
/// because the column is `TEXT NOT NULL` with **no default**: omitting it
/// makes SQLite raise a NOT NULL constraint violation that `OR IGNORE`
/// then swallows, so the stub is silently never created and the FK it was
/// meant to satisfy still dangles. `source` relies on its schema default
/// (`'contact'`), which is the correct provenance for a DID we only know
/// from an inbound signature.
fn ensure_identity_stub(tx: &Transaction, did: &str) -> Result<(), DatabaseError> {
    tx.execute(
        "INSERT OR IGNORE INTO haex_identities (id, did, name) VALUES (?1, ?1, ?1)",
        [did],
    )
    .map_err(DatabaseError::from)?;
    Ok(())
}

/// Compute the `space_id` that a column-sig on this row must have been
/// signed under.
///
/// **The locally persisted `space_id` always wins.** The batch-supplied
/// `space_id` column change is only consulted when the row does not exist
/// locally yet (the INSERT path), and even then only if it agrees with
/// `expected_space_id` — the space the caller scoped this pull to.
///
/// Precedence is not cosmetic. `space_id` arrives as an ordinary,
/// unauthenticated column change off the wire, so trusting it over the
/// stored value hands the attacker the verification anchor: a peer holding
/// a signing key for its own space `S_evil` could push
/// `{space_id: "S_evil", <target column>: <value>}` at a row that locally
/// belongs to `S_victim`. Both changes then verify under `S_evil`, and the
/// column update lands on the victim row — defeating the space binding in
/// the preimage (ADR 0002 §4b) precisely because the verifier let the
/// attacker pick the binding.
///
/// Returns `Ok(None)` when no trustworthy anchor exists (table has no
/// `space_id` column, the row is new and the caller gave no
/// `expected_space_id`, or the batch's claimed space contradicts it).
/// `verify_change_sig` turns that into a per-change rejection, so an
/// unanchored row drops its signed changes rather than applying them
/// unverified.
fn resolve_row_space_id_for_sig(
    tx: &Transaction,
    table_name: &str,
    pk_where_clause: &str,
    pk_values_for_query: &[JsonValue],
    row_change_list: &[RemoteColumnChange],
    schema: &[ColumnInfo],
    expected_space_id: Option<&str>,
) -> Result<Option<String>, DatabaseError> {
    // (1) Authoritative source: the row's own persisted space_id.
    if schema.iter().any(|c| c.name == "space_id") {
        let sql = format!(
            "SELECT space_id FROM \"{}\" WHERE {}",
            table_name, pk_where_clause
        );
        let mut stmt = tx.prepare(&sql).map_err(DatabaseError::from)?;
        let params = json_values_to_sql_params(pk_values_for_query)?;
        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
        let persisted: Option<Option<String>> = stmt
            .query_row(&*params_refs, |row| row.get::<_, Option<String>>(0))
            .optional()
            .map_err(DatabaseError::from)?;
        if let Some(space_id) = persisted.flatten() {
            return Ok(Some(space_id));
        }
    }

    // (2) INSERT path — the row is new, so there is nothing persisted to
    // anchor on. Fall back to the batch's claimed space_id, but only after
    // cross-checking it against the space this pull was scoped to. Without
    // an expected space there is no way to tell an honest claim from a
    // forged one, so we refuse to guess.
    let expected = match expected_space_id {
        Some(e) => e,
        None => return Ok(None),
    };
    let claimed = row_change_list
        .iter()
        .find(|c| c.column_name == "space_id")
        .and_then(|c| match &c.decrypted_value {
            JsonValue::String(s) => Some(s.as_str()),
            _ => None,
        });
    match claimed {
        // No claim in the batch: the pull scope is still a valid anchor —
        // every change in this batch was fetched for `expected`.
        None => Ok(Some(expected.to_string())),
        Some(c) if c == expected => Ok(Some(expected.to_string())),
        Some(c) => {
            eprintln!(
                "[SYNC RUST] Refusing sig anchor for new row in '{}': batch claims space_id '{}' but pull is scoped to '{}'",
                table_name, c, expected
            );
            Ok(None)
        }
    }
}

/// Verify a single `RemoteColumnChange`'s attached column signature against
/// the row's `space_id`. Returns `Ok(())` on a good sig, `Err(reason)` when
/// the sig is malformed, the space_id is unavailable, or the Ed25519 check
/// fails. The caller drops the change from `columns_to_update` on `Err` and
/// keeps the rest of the batch flowing (row-scoped rejection — Phase-2
/// pattern from ADR 0002 §6).
///
fn verify_change_sig(
    change: &RemoteColumnChange,
    sig: &ColumnSig,
    row_space_id: Option<&str>,
    table_name: &str,
    row_pks: &str,
) -> Result<(), String> {
    let space_id =
        row_space_id.ok_or_else(|| "space_id unavailable — cannot verify sig".to_string())?;
    if sig.sig.len() > 88 {
        return Err("signature exceeds the 64-byte Ed25519 wire size".to_string());
    }
    if sig.storage_class == crate::crdt::column_sig::value_bytes::StorageClass::Blob {
        if let Some(encoded) = change.decrypted_value.as_str() {
            let max = crate::crdt::column_sig::limits::MAX_VALUE_BYTES_LEN;
            if encoded.len() > max * 4 / 3 + 4 {
                return Err("BLOB value exceeds the column-signature size limit".to_string());
            }
        }
    }
    let sql_value = sig.storage_class.restore(&change.decrypted_value)?;
    let value_bytes_vec = crate::crdt::column_sig::value_bytes::to_canonical_bytes(&sql_value);
    let sig_bytes = BASE64
        .decode(&sig.sig)
        .map_err(|e| format!("malformed sig base64: {e}"))?;
    crate::crdt::column_sig::verify::verify_column_sig(
        space_id.as_bytes(),
        table_name.as_bytes(),
        row_pks.as_bytes(),
        change.column_name.as_bytes(),
        change.hlc_timestamp.as_bytes(),
        &sig.author_did,
        &value_bytes_vec,
        &sig_bytes,
    )
    .map_err(|e| format!("verify_column_sig: {e:?}"))
}

/// Applies remote changes in a single transaction, with HLC-ordered grouping.
/// Note: lastPullServerTimestamp is now updated by the TypeScript layer after successful apply
///
/// `space_id` is the space this pull was scoped to. It is the only
/// trustworthy anchor for verifying a signature on a row that does not exist
/// locally yet — see [`resolve_row_space_id_for_sig`]. `None` for
/// personal-vault sync, where nothing is signed.
#[tauri::command]
pub fn apply_remote_changes_in_transaction(
    changes: Vec<RemoteColumnChange>,
    backend_id: String,
    max_hlc: String,
    space_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), DatabaseError> {
    // Lock HLC via `lock_or_fail` so a poisoned mutex fails LOUD with a
    // banner row. Previous behaviour was `.lock().ok().map(...)` which
    // silently passed `hlc_service=None` to `apply_remote_changes_to_db`
    // — that path applies the remote changes WITHOUT advancing the local
    // HLC clock, so subsequent local writes carry stale timestamps that
    // lose merge conflicts on the next sync round.
    let hlc_service = state.lock_or_fail(
        &state.hlc,
        crate::critical::CriticalFailureCode::HlcMutexPoisoned,
        "crdt::commands::apply_remote_changes_in_transaction",
        serde_json::json!({}),
    )?;
    apply_remote_changes_to_db_scoped(
        &state.db,
        changes,
        Some((&backend_id, &max_hlc)),
        Some(&*hlc_service),
        space_id.as_deref(),
    )
}

/// Inner implementation that applies remote CRDT changes to a database connection.
///
/// If `backend_info` is `Some((backend_id, max_hlc))`, updates `haex_sync_backends`
/// with the push HLC timestamp (used by server sync). For local delivery, pass `None`.
///
/// If `hlc_service` is provided, the local HLC clock is advanced past the highest
/// received remote timestamp after applying all changes. This ensures future local
/// operations generate timestamps strictly greater than any received remote timestamp,
/// preventing incomplete rows on the server during push.
///
/// Equivalent to [`apply_remote_changes_to_db_scoped`] with no expected
/// space. Callers that know which space the batch was pulled for should use
/// the scoped variant so column signatures on newly inserted rows can be
/// anchored — without it, such rows have no trustworthy `space_id` and their
/// signed changes are dropped rather than verified against a
/// batch-supplied (i.e. attacker-supplied) space.
pub fn apply_remote_changes_to_db(
    db: &crate::database::DbConnection,
    changes: Vec<RemoteColumnChange>,
    backend_info: Option<(&str, &str)>,
    hlc_service: Option<&HlcService>,
) -> Result<(), DatabaseError> {
    apply_remote_changes_to_db_scoped(db, changes, backend_info, hlc_service, None)
}

/// [`apply_remote_changes_to_db`] plus the space this batch was pulled for.
///
/// See [`resolve_row_space_id_for_sig`] for why the expected space matters:
/// it is the cross-check that stops a peer from choosing the space its own
/// signatures are verified under.
pub fn apply_remote_changes_to_db_scoped(
    db: &crate::database::DbConnection,
    changes: Vec<RemoteColumnChange>,
    backend_info: Option<(&str, &str)>,
    hlc_service: Option<&HlcService>,
    expected_space_id: Option<&str>,
) -> Result<(), DatabaseError> {
    eprintln!("[SYNC RUST] ========== APPLY REMOTE CHANGES START ==========");
    eprintln!(
        "[SYNC RUST] Changes count: {}, backend: {}",
        changes.len(),
        backend_info.map(|(id, _)| id).unwrap_or("local-delivery"),
    );

    // Group changes by transaction-HLC and apply groups in ascending HLC order
    // so cross-table transactions (e.g. parent + child insert) land together.
    let grouped = group_by_transaction_hlc(changes);
    let changes: Vec<RemoteColumnChange> = grouped
        .into_iter()
        .flat_map(|(_hlc, group)| group.into_iter())
        .collect();

    // Validate all table and column names from remote changes to prevent SQL injection
    for change in &changes {
        if !is_safe_identifier(&change.table_name) {
            return Err(DatabaseError::ValidationError {
                reason: format!(
                    "Invalid table name '{}' in remote change",
                    change.table_name
                ),
            });
        }
        if !is_safe_identifier(&change.column_name) {
            return Err(DatabaseError::ValidationError {
                reason: format!(
                    "Invalid column name '{}' in table '{}'",
                    change.column_name, change.table_name
                ),
            });
        }
    }
    eprintln!("[SYNC RUST] Identifier validation passed");

    with_connection(db, |conn| {
        // Disable foreign key constraints for the duration of the apply
        // pass, re-enabling unconditionally on every exit path (including
        // mid-body errors). PRAGMA foreign_keys cannot be changed inside a
        // transaction, so the toggle must wrap the transaction.
        // See: https://sqlite.org/foreignkeys.html
        eprintln!("[SYNC RUST] Disabling foreign_keys BEFORE transaction");
        let applied_hlc_timestamps = crate::crdt::cleanup::with_fk_disabled(conn, |conn| {
            // Start transaction - all changes in the batch are applied atomically
            eprintln!("[SYNC RUST] Starting transaction...");
            let tx = conn.transaction().map_err(DatabaseError::from)?;

            // Whether per-column signatures are required on this batch. Owner-
            // vault sync between two devices of the same identity carries an
            // `expected_space_id` (the vault space id) but is intentionally
            // UNSIGNED on the write side: `sign_column_for_spaces` only signs
            // rows the register maps into a space, so owner-private rows have
            // `haex_column_sigs = {}`. Peer legitimacy on that path is already
            // established by QUIC-level DID auth plus the peer's row in
            // `haex_space_devices`; per-column sig enforcement adds nothing on
            // top and would silently drop every unsigned owner-private change
            // on the receiver (deletes included, since `haex_deleted_rows` is a
            // normal CRDT-synced table).
            //
            // Shared-space applies (non-owner space) keep the strict Phase-1
            // gate: unsigned changes are dropped. When there is no vault space
            // at all, `is_owner_space` returns false, so the safe default is
            // "enforce" whenever an `expected_space_id` was given.
            //
            // Computed once per apply pass because `expected_space_id` is stable
            // for the whole batch.
            let enforce_sigs = match expected_space_id {
                Some(sid) => !crate::owner_sync::scope::is_owner_space(&tx, sid)
                    .map_err(DatabaseError::from)?,
                None => false,
            };

            // Disable triggers temporarily to prevent marking tables as dirty
            // when applying remote changes (we don't want to re-sync changes we just pulled)
            eprintln!("[SYNC RUST] Disabling triggers for remote changes");
            let disable_sql = format!(
            "INSERT INTO {TABLE_CRDT_CONFIGS} (key, type, value) VALUES ('triggers_enabled', 'system', '0')
             ON CONFLICT(key) DO UPDATE SET value = '0'"
        );
            tx.execute(&disable_sql, []).map_err(DatabaseError::from)?;

            // Collect side-data needed after the apply loop:
            //   1. all HLC timestamps for advancing the local clock,
            //   2. IDs of haex_deleted_rows entries arriving in this batch so
            //      the corresponding DELETE on the target table can run after
            //      the apply loop (triggers are still disabled then).
            let mut all_hlc_timestamps: Vec<String> = Vec::with_capacity(changes.len());
            let mut inbound_delete_log_ids: HashSet<String> = HashSet::new();
            // Symmetric collector for Task 6: shared-space per-space delete-log
            // entries applied via `propagate_shared_space_deleted_rows_to_target_tables`
            // after the main apply loop, while triggers are still disabled.
            let mut inbound_shared_space_delete_log_ids: HashSet<String> = HashSet::new();
            for change in &changes {
                all_hlc_timestamps.push(change.hlc_timestamp.clone());
                let target_id_set: Option<&mut HashSet<String>> = match change.table_name.as_str() {
                    n if n == DELETED_ROWS_TABLE => Some(&mut inbound_delete_log_ids),
                    n if n == SHARED_SPACE_DELETED_ROWS_TABLE => {
                        Some(&mut inbound_shared_space_delete_log_ids)
                    }
                    _ => None,
                };
                if let Some(set) = target_id_set {
                    if let Ok(map) =
                        serde_json::from_str::<serde_json::Map<String, JsonValue>>(&change.row_pks)
                    {
                        if let Some(JsonValue::String(id)) = map.get("id") {
                            set.insert(id.clone());
                        }
                    }
                }
            }

            // Group by (table, row) so all columns of one row are written
            // together — and keep iteration ordered by the row's earliest
            // HLC. Plain HashMap iteration would discard the careful HLC
            // ordering that group_by_transaction_hlc just established.
            let row_changes = group_row_changes_in_hlc_order(changes);

            // Pre-loaded delete-log entries (parsed `row_pks` map + HLC) grouped
            // by target `table_name`, for the row-absent insert branch below.
            // Previous implementation loaded lazily per-table on first absent
            // row, which scaled with `|haex_deleted_rows|` re-issued for every
            // new table touched. Loading once collapses the whole apply pass to
            // a single sweep. The transaction (`tx`) is the only writer to
            // `haex_deleted_rows` in this pass, so the snapshot is consistent
            // with what's on disk for the duration of one apply.
            //
            // Absent-table = empty slice (matches the lazy-load semantics:
            // "no entries for that table" yielded `&[]`).
            let shadowing_deletes_by_table: HashMap<
                String,
                Vec<(serde_json::Map<String, JsonValue>, String)>,
            > = {
                let mut map: HashMap<String, Vec<(serde_json::Map<String, JsonValue>, String)>> =
                    HashMap::new();
                let mut stmt = tx
                    .prepare(&format!(
                        "SELECT table_name, row_pks, haex_hlc FROM \"{}\"",
                        DELETED_ROWS_TABLE
                    ))
                    .map_err(DatabaseError::from)?;
                // `haex_hlc` is added to `haex_deleted_rows` via a nullable
                // ALTER (see `ensure_crdt_columns`), so a legacy or directly-
                // inserted row could leave it NULL. Read it as `Option<String>`
                // and skip NULL entries — a single bad row must not abort the
                // entire apply pass (would wedge the pull cursor permanently).
                let mapped = stmt
                    .query_map([], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<String>>(2)?,
                        ))
                    })
                    .map_err(DatabaseError::from)?;
                for r in mapped {
                    let (table_name, pks_str, del_hlc) = r.map_err(DatabaseError::from)?;
                    if let (Some(del_hlc), Ok(pks_map)) = (
                        del_hlc,
                        serde_json::from_str::<serde_json::Map<String, JsonValue>>(&pks_str),
                    ) {
                        map.entry(table_name).or_default().push((pks_map, del_hlc));
                    }
                }
                // NOTE: The per-space delete log (haex_shared_space_deleted_rows)
                // is intentionally NOT unioned here. A per-space signal is only
                // authoritative in the context of ITS space's register — pouring
                // it into the global shadow map lets a forged or unshare-only
                // signal from space X suppress a legitimate insert scoped to
                // space Y (the register-check gate in
                // propagate_shared_space_deleted_rows_to_target_tables would
                // reject the delete, but the shadow-map bypasses that gate).
                //
                // The remaining resurrection gap — an old insert arriving after
                // a shared-space delete has been applied but before the
                // compaction anchor has advanced — is accepted by design:
                //   1. Register-check gate + per-space anchor already reject
                //      the shared-space delete's own resurrection vector.
                //   2. A resurrected business row without a register entry is
                //      user-visible as an unregistered stray, not a security
                //      breach.
                //   3. Client-side refresh-pull on `BelowCompactionAnchor`
                //      (follow-up ticket) closes the recovery window.
                //
                // If we ever need space-scoped shadowing here, the correct
                // shape is a per-(table, row, space_id) map plus space-context
                // at insert-check time — not a global union.
                map
            };

            // Apply changes grouped by row
            for ((_table_name, row_pks_str), row_change_list) in row_changes {
                // Use the first change to get common data
                let first_change = &row_change_list[0];

                // Get table schema to identify PK columns
                // If table doesn't exist (e.g., from a dev extension not installed here), skip it
                let mut schema = get_table_schema_internal(&tx, &first_change.table_name)
                    .map_err(DatabaseError::from)?;

                if schema.is_empty() {
                    eprintln!(
                    "[SYNC RUST] Skipping table '{}' - table does not exist (extension not installed?)",
                    first_change.table_name
                );
                    // Record the skipped table so the server-path cursor can be
                    // reset after the extension is installed (plan 010).
                    // P2P self-heals via per-session re-pull from 0; this marker
                    // exists only for the persisted server cursor (see plan 010).
                    tx.execute(
                        &format!(
                            "INSERT OR IGNORE INTO {} (table_name) VALUES (?)",
                            TABLE_CRDT_PENDING_TABLES
                        ),
                        params![&first_change.table_name],
                    )
                    .map_err(DatabaseError::from)?;
                    continue;
                }

                // Ensure table has CRDT columns (haex_hlc, haex_column_hlcs)
                // This handles tables created in dev mode that don't have CRDT columns yet.
                // When sync data arrives, we know it's from a production extension, so we need CRDT.
                let has_core_crdt_columns =
                    schema.iter().any(|col| col.name == HLC_TIMESTAMP_COLUMN)
                        && schema.iter().any(|col| col.name == COLUMN_HLCS_COLUMN);
                let has_column_sigs = schema
                    .iter()
                    .any(|col| col.name == crate::crdt::trigger::COLUMN_SIGS_COLUMN);
                if !has_core_crdt_columns || !has_column_sigs {
                    eprintln!(
                    "[SYNC RUST] Table '{}' missing CRDT columns (created in dev mode?) - upgrading now",
                    first_change.table_name
                );
                    let upgrade = trigger::ensure_crdt_columns(&tx, &first_change.table_name)
                        .and_then(|columns_added| {
                            // Adding only the signature metadata column to an
                            // existing CRDT table does not require trigger
                            // recreation. This also keeps minimal test/dev
                            // schemas from acquiring triggers whose support
                            // tables they intentionally omit.
                            if has_core_crdt_columns {
                                Ok((columns_added, false))
                            } else {
                                trigger::setup_triggers_for_table(
                                    &tx,
                                    &first_change.table_name,
                                    true,
                                )
                                .map(|result| {
                                    let triggers_created =
                                        matches!(result, trigger::TriggerSetupResult::Success);
                                    (columns_added, triggers_created)
                                })
                            }
                        });
                    match upgrade {
                        Ok((columns_added, triggers_created)) => {
                            eprintln!(
                                "[SYNC RUST] Upgraded '{}': columns={}, triggers={}",
                                first_change.table_name, columns_added, triggers_created
                            );
                            schema = get_table_schema_internal(&tx, &first_change.table_name)
                                .map_err(DatabaseError::from)?;
                        }
                        Err(e) => {
                            eprintln!(
                                "[SYNC RUST] Failed to upgrade '{}': {} - skipping this table",
                                first_change.table_name, e
                            );
                            // Record the skipped table so the server-path cursor can be
                            // reset after the CRDT-column upgrade succeeds (plan 010).
                            // P2P self-heals via per-session re-pull from 0; this marker
                            // exists only for the persisted server cursor (see plan 010).
                            tx.execute(
                                &format!(
                                    "INSERT OR IGNORE INTO {} (table_name) VALUES (?)",
                                    TABLE_CRDT_PENDING_TABLES
                                ),
                                params![&first_change.table_name],
                            )
                            .map_err(DatabaseError::from)?;
                            continue;
                        }
                    }
                }

                // Parse row PKs (same for all changes in this row)
                let row_pks: serde_json::Map<String, JsonValue> =
                    serde_json::from_str(&row_pks_str).map_err(|e| {
                        DatabaseError::SerializationError {
                            reason: format!("Failed to parse row PKs: {}", e),
                        }
                    })?;

                let pk_columns: Vec<_> = schema.iter().filter(|col| col.is_pk).collect();

                // Build WHERE clause for PKs, handling NULL values properly
                let (pk_where_clause, pk_values_for_query) =
                    build_pk_where_clause(&pk_columns, &row_pks);

                // Check if row exists and get current HLCs
                let check_sql = format!(
                    "SELECT haex_column_hlcs, haex_hlc FROM \"{}\" WHERE {}",
                    first_change.table_name, pk_where_clause
                );

                let current_hlcs: Option<(String, String)> = {
                    let mut stmt = tx.prepare(&check_sql).map_err(DatabaseError::from)?;
                    let params = json_values_to_sql_params(&pk_values_for_query)?;
                    let params_refs: Vec<&dyn rusqlite::ToSql> =
                        params.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

                    // Only `QueryReturnedNoRows` means "row absent" — any other
                    // error (locking, schema mismatch, etc.) must surface so the
                    // caller does not silently treat a transient failure as
                    // "no existing row" and overwrite live state.
                    match stmt.query_row(&*params_refs, |row| {
                        let column_hlcs: String = row.get(0)?;
                        let row_hlc: Option<String> = row.get(1)?;
                        Ok((column_hlcs, row_hlc.unwrap_or_default()))
                    }) {
                        Ok(pair) => Some(pair),
                        Err(rusqlite::Error::QueryReturnedNoRows) => None,
                        Err(e) => return Err(DatabaseError::from(e)),
                    }
                };

                // Track if row exists
                let row_exists = current_hlcs.is_some();

                // Parse current HLCs
                let (current_row_hlc, mut column_hlcs): (
                    String,
                    serde_json::Map<String, JsonValue>,
                ) = if let Some((hlcs_str, row_hlc)) = current_hlcs {
                    (row_hlc, serde_json::from_str(&hlcs_str).unwrap_or_default())
                } else {
                    (String::new(), serde_json::Map::new())
                };

                // Build a set of existing column names for quick lookup
                let existing_columns: std::collections::HashSet<&str> =
                    schema.iter().map(|col| col.name.as_str()).collect();

                // Stage 5b (Task B.5) — row-level registry-row-sig gate.
                // Runs BEFORE the per-column sig gate below: a bad row_sig
                // drops this row's ENTIRE change set atomically, unlike a
                // per-column sig failure which only drops that one column.
                // Stacks on top of (does not replace) the per-column gate —
                // that one still runs afterwards for defense-in-depth.
                //
                // Case-insensitive per the B.3.1 pattern (Concern B).
                // Skips gracefully — mirroring `sign_registry_row_self`'s own
                // guard — when the local schema predates migration 0014
                // (no `row_sig` column yet): nothing to verify against.
                if first_change
                    .table_name
                    .eq_ignore_ascii_case(TABLE_SHARED_SPACE_SYNC)
                    && existing_columns.contains(COL_SHARED_SPACE_SYNC_ROW_SIG)
                {
                    let outcome = build_incoming_registry_change(
                        &tx,
                        &pk_where_clause,
                        &pk_values_for_query,
                        &row_pks,
                        &row_change_list,
                    )?;
                    match outcome {
                        RegistryRowChangeOutcome::NothingSignedTouched => {}
                        RegistryRowChangeOutcome::RowSigOnlyBatch {
                            space_id,
                            authored_by_did,
                        } => {
                            eprintln!(
                                "[SYNC RUST] Rejected registry row {} in '{}' (space_id='{}', authored_by_did='{}') — batch touched ONLY row_sig with no signed-payload column; a bare row_sig cannot be verified and would let a stale-but-valid signature overwrite the persisted one (possible replay)",
                                row_pks_str, first_change.table_name, space_id, authored_by_did
                            );
                            continue;
                        }
                        RegistryRowChangeOutcome::MissingFreshRowSig(touched_signed_columns) => {
                            eprintln!(
                                "[SYNC RUST] Rejected registry row {} in '{}' — signed column(s) {:?} changed without a fresh row_sig in the same batch",
                                row_pks_str, first_change.table_name, touched_signed_columns
                            );
                            // Column-level CRDT can split a row across pull
                            // windows. Record the missing row_sig so recovery
                            // can reset the server cursor and re-pull it.
                            tx.execute(
                                &format!(
                                    "INSERT OR IGNORE INTO {} (table_name, column_name, row_pks) VALUES (?, ?, ?)",
                                    TABLE_CRDT_PENDING_COLUMNS
                                ),
                                params![
                                    &first_change.table_name,
                                    COL_SHARED_SPACE_SYNC_ROW_SIG,
                                    &first_change.row_pks
                                ],
                            )
                            .map_err(DatabaseError::from)?;
                            continue;
                        }
                        RegistryRowChangeOutcome::RequiredFieldExplicitlyNull(null_columns) => {
                            eprintln!(
                                "[SYNC RUST] Rejected registry row {} in '{}' — required column(s) {:?} were explicitly set to null (never legitimate; dropping data in transit or forgery attempt)",
                                row_pks_str, first_change.table_name, null_columns
                            );
                            continue;
                        }
                        RegistryRowChangeOutcome::Ready { change, persisted } => {
                            if let Err(err) =
                                verify_incoming_registry_change(&change, persisted.as_ref())
                            {
                                eprintln!(
                                    "[SYNC RUST] Rejected registry row {} in '{}' (claimed authored_by_did='{}') — {:?}",
                                    row_pks_str, first_change.table_name, change.authored_by_did, err
                                );
                                continue;
                            }
                        }
                    }
                }

                // Precompute the trustworthy space anchor once per row.
                let row_space_id_for_sig: Option<String> =
                    if row_change_list.iter().any(|c| c.sig.is_some()) {
                        resolve_row_space_id_for_sig(
                            &tx,
                            &first_change.table_name,
                            &pk_where_clause,
                            &pk_values_for_query,
                            &row_change_list,
                            &schema,
                            expected_space_id,
                        )?
                    } else {
                        None
                    };

                // Collect all column changes that are newer than current
                // (column_name, exact SQLite value, hlc, verified sig to persist)
                let mut columns_to_update: Vec<(String, SqlValue, String, Option<SigRecord>)> =
                    Vec::new();
                let mut max_hlc_for_row = first_change.hlc_timestamp.clone();

                for change in &row_change_list {
                    // Skip columns that don't exist in the local schema
                    // This handles schema version differences between devices
                    if !existing_columns.contains(change.column_name.as_str()) {
                        eprintln!(
                        "[SYNC RUST] Skipping unknown column '{}' in table '{}' - column not in local schema (older app version?)",
                        change.column_name, first_change.table_name
                    );

                        // Track this row's owed column as pending. Row-aware: the marker
                        // carries the owed row's PKs so P2P recovery can clear per
                        // (table, column, row_pks) and a row-incomplete peer dump can't
                        // drop a still-owed value (silent loss).
                        tx.execute(
                            &format!(
                                "INSERT OR IGNORE INTO {} (table_name, column_name, row_pks) VALUES (?, ?, ?)",
                                TABLE_CRDT_PENDING_COLUMNS
                            ),
                            params![
                                &first_change.table_name,
                                &change.column_name,
                                &first_change.row_pks
                            ],
                        )
                        .map_err(DatabaseError::from)?;

                        // Deliberately do NOT record this column's HLC into
                        // `haex_column_hlcs`. That map is the per-column HLC of
                        // the last *applied* value; a skipped (never-applied)
                        // column must not appear there. If we recorded its HLC
                        // `H` here, the post-migration recovery re-pull — which
                        // carries the SAME original HLC `H` — would be gated out
                        // by the strict `hlc_is_newer(H, H)` check (`H > H` is
                        // false) and silently no-op. Leaving it absent means
                        // recovery applies normally (`H > ""`). The
                        // pending-columns table above is the tracker for skipped
                        // columns; re-skipping on each subsequent pre-migration
                        // sync is harmless (idempotent INSERT OR IGNORE).
                        continue;
                    }

                    // Shared-space applies fail closed on missing signatures.
                    // `authored_by_did` is legacy leader-attributed metadata,
                    // not authoritative authorship; it remains the sole
                    // unsigned compatibility column until the schema drops it.
                    //
                    // Owner-space applies (`enforce_sigs == false`) skip this
                    // gate — see the `enforce_sigs` computation above. Signed
                    // changes still verify below regardless of the flag, so
                    // there is no downgrade path from signed to unsigned on
                    // the owner-space route.
                    if enforce_sigs
                        && change.sig.is_none()
                        && change.column_name != "authored_by_did"
                    {
                        eprintln!(
                            "[SYNC RUST] Dropping unsigned shared-space change on {}.{}",
                            first_change.table_name, change.column_name
                        );
                        continue;
                    }

                    let verified_sig = if let Some(sig) = &change.sig {
                        match verify_change_sig(
                            change,
                            sig,
                            row_space_id_for_sig.as_deref(),
                            &first_change.table_name,
                            &first_change.row_pks,
                        ) {
                            Ok(()) => {
                                ensure_identity_stub(&tx, &sig.author_did)?;
                                let bytes = BASE64.decode(&sig.sig).map_err(|e| {
                                    DatabaseError::SerializationError {
                                        reason: format!("verified signature stopped decoding: {e}"),
                                    }
                                })?;
                                let sig_bytes: [u8; 64] = bytes.try_into().map_err(|_| {
                                    DatabaseError::SerializationError {
                                        reason: "verified signature has wrong length".to_string(),
                                    }
                                })?;
                                Some(SigRecord {
                                    author_did: sig.author_did.clone(),
                                    sig: sig_bytes,
                                    storage_class: sig.storage_class,
                                })
                            }
                            Err(reason) => {
                                eprintln!(
                                    "[SYNC RUST] Dropping change with invalid sig on {}.{}: {}",
                                    first_change.table_name, change.column_name, reason
                                );
                                continue;
                            }
                        }
                    } else {
                        None
                    };

                    let current_hlc = column_hlcs
                        .get(&change.column_name)
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    if hlc_is_newer(change.hlc_timestamp.as_str(), current_hlc) {
                        let sql_value = match &change.sig {
                            Some(sig) => sig
                                .storage_class
                                .restore(&change.decrypted_value)
                                .map_err(|reason| DatabaseError::SerializationError { reason })?,
                            None => {
                                ValueConverter::json_to_rusqlite_value(&change.decrypted_value)?
                            }
                        };
                        // Remote change is newer, include it
                        column_hlcs.insert(
                            change.column_name.clone(),
                            JsonValue::String(change.hlc_timestamp.clone()),
                        );
                        columns_to_update.push((
                            change.column_name.clone(),
                            sql_value,
                            change.hlc_timestamp.clone(),
                            verified_sig,
                        ));

                        // Track max HLC for row timestamp
                        if hlc_is_newer(&change.hlc_timestamp, &max_hlc_for_row) {
                            max_hlc_for_row = change.hlc_timestamp.clone();
                        }
                    }
                }

                // Only apply if there are columns to update
                if !columns_to_update.is_empty() {
                    let new_hlcs_json = serde_json::to_string(&column_hlcs).map_err(|e| {
                        DatabaseError::SerializationError {
                            reason: format!("Failed to serialize column HLCs: {}", e),
                        }
                    })?;

                    if row_exists {
                        // Never regress the row-level haex_hlc: an incoming batch can legally be
                        // older than the row's current HLC (column-level CRDT). The row HLC feeds
                        // the delete-resurrection comparison, so regressing it would let an older
                        // remote delete win against a newer local write.
                        if hlc_is_newer(&current_row_hlc, &max_hlc_for_row) {
                            max_hlc_for_row = current_row_hlc.clone();
                        }

                        // Row exists, update it with all changed columns
                        let set_clauses: Vec<String> = columns_to_update
                            .iter()
                            .map(|(col_name, _, _, _)| format!("\"{}\" = ?", col_name))
                            .collect();

                        let update_sql = format!(
                            "UPDATE \"{}\" SET {}, haex_column_hlcs = ?, haex_hlc = ? WHERE {}",
                            first_change.table_name,
                            set_clauses.join(", "),
                            pk_where_clause
                        );

                        let mut params_vec: Vec<SqlValue> = Vec::new();

                        // Add exact SQLite values reconstructed from the signed
                        // storage class.
                        for (_col_name, sql_value, _, _) in &columns_to_update {
                            params_vec.push(sql_value.clone());
                        }

                        // Add HLCs and timestamp
                        params_vec.push(SqlValue::Text(new_hlcs_json));
                        params_vec.push(SqlValue::Text(max_hlc_for_row.clone()));

                        // Add PK values for WHERE clause (only non-NULL values, NULL uses IS NULL)
                        for sql_val in json_values_to_sql_params(&pk_values_for_query)? {
                            params_vec.push(sql_val);
                        }

                        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec
                            .iter()
                            .map(|v| v as &dyn rusqlite::ToSql)
                            .collect();

                        tx.execute(&update_sql, &*params_refs)
                            .map_err(DatabaseError::from)?;
                    } else {
                        // Delete-resurrection guard: a row absent locally must NOT be
                        // (re)inserted if a delete-log entry for it carries an HLC
                        // newer-or-equal to this insert. propagate_deleted_rows_to_target_tables
                        // only revisits deletes arriving in the current batch, so a delete
                        // stored in a prior batch (or before this row's table existed) would
                        // otherwise be silently resurrected by a later, older-HLC insert. Match
                        // on the parsed row_pks map so it is independent of key order /
                        // serializer differences between the scanner (sorted) and the DELETE
                        // trigger (PK-definition order).
                        //
                        // Look up the per-table shadowing-delete entries from the
                        // apply-pass-wide pre-load above. Absent table = empty
                        // slice (preserves the lazy-load semantics where a table
                        // with no matching delete-log entries fell through to a
                        // plain insert).
                        let empty: Vec<(serde_json::Map<String, JsonValue>, String)> = Vec::new();
                        let shadowing_deletes = shadowing_deletes_by_table
                            .get(&first_change.table_name)
                            .unwrap_or(&empty);
                        if insert_suppressed_by_deletes(
                            &row_pks,
                            &max_hlc_for_row,
                            shadowing_deletes,
                        ) {
                            eprintln!(
                                "[SYNC RUST] Suppressing resurrection insert into '{}' for row {} — shadowed by a newer delete-log entry",
                                first_change.table_name, row_pks_str
                            );
                            continue;
                        }

                        // Row doesn't exist, insert it with all changed columns + PKs
                        let mut columns = Vec::new();
                        let mut values: Vec<SqlValue> = Vec::new();

                        // Add PKs first (use json_values_to_sql_params for consistent null handling)
                        let pk_json_values: Vec<JsonValue> = pk_columns
                            .iter()
                            .filter_map(|col| row_pks.get(&col.name).cloned())
                            .collect();
                        let pk_sql_values = json_values_to_sql_params(&pk_json_values)?;
                        for (col, sql_val) in pk_columns.iter().zip(pk_sql_values.into_iter()) {
                            columns.push(col.name.clone());
                            values.push(sql_val);
                        }

                        // Add changed columns with their exact SQLite classes.
                        for (col_name, sql_value, _, _) in &columns_to_update {
                            columns.push(col_name.clone());
                            values.push(sql_value.clone());
                        }

                        // Add CRDT metadata
                        columns.push(COLUMN_HLCS_COLUMN.to_string());
                        columns.push(HLC_TIMESTAMP_COLUMN.to_string());
                        values.push(SqlValue::Text(new_hlcs_json));
                        values.push(SqlValue::Text(max_hlc_for_row.clone()));

                        let placeholders = vec!["?"; columns.len()].join(", ");
                        let quoted_columns: Vec<String> =
                            columns.iter().map(|c| format!("\"{}\"", c)).collect();
                        let insert_sql = format!(
                            "INSERT INTO \"{}\" ({}) VALUES ({})",
                            first_change.table_name,
                            quoted_columns.join(", "),
                            placeholders
                        );

                        let params_refs: Vec<&dyn rusqlite::ToSql> =
                            values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

                        // Try to insert - if it fails with constraint, log detailed error
                        match tx.execute(&insert_sql, &*params_refs) {
                            Ok(_) => {} // Success - continue
                            Err(rusqlite::Error::SqliteFailure(err, msg))
                                if err.code == rusqlite::ErrorCode::ConstraintViolation =>
                            {
                                // Log the constraint violation details
                                let error_msg =
                                    msg.as_deref().unwrap_or("Unknown constraint violation");
                                eprintln!(
                                    "[SYNC RUST] Constraint violation for table {}: {}",
                                    first_change.table_name, error_msg
                                );
                                eprintln!("[SYNC RUST] Failed INSERT SQL: {}", insert_sql);
                                eprintln!("[SYNC RUST] Values: {:?}", values);

                                // A NOT NULL violation on an *absent* row means
                                // the change set carried only a subset of the
                                // row's columns — a later column-level update
                                // whose creation columns are below the pull
                                // cursor, or a row that is itself partial on the
                                // leader (column-level CRDT lets a row's columns
                                // arrive out of order). A partial INSERT can never
                                // satisfy the NOT NULL columns, so there is
                                // nothing to insert here.
                                //
                                // Crucially, throwing wedges the entire sync loop:
                                // the apply errors, the pull cursor never advances
                                // (sync_loop only advances last_pull_timestamp on
                                // success), and the same batch is re-pulled every
                                // cycle forever. Skip the row instead — exactly
                                // like the UNIQUE path below — so the rest of the
                                // batch applies and the cursor moves on. It
                                // self-heals when the remaining columns later
                                // propagate: a full pull on loop restart delivers
                                // all of the row's columns together and inserts it.
                                if error_msg.contains("NOT NULL constraint failed") {
                                    eprintln!(
                                        "[SYNC RUST] Skipping row in '{}' — partial change set cannot satisfy NOT NULL columns (incomplete sync data). Received columns: {:?}",
                                        first_change.table_name,
                                        row_change_list
                                            .iter()
                                            .map(|c| &c.column_name)
                                            .collect::<Vec<_>>()
                                    );
                                    continue; // Skip this row, keep applying the batch
                                }

                                // Check if it's a UNIQUE constraint violation
                                if error_msg.contains("UNIQUE constraint failed") {
                                    eprintln!("[SYNC RUST] UNIQUE constraint conflict - creating conflict entry");

                                    // Build remote row data from all columns being inserted
                                    let mut remote_row_data = serde_json::Map::new();
                                    for (i, col_name) in columns.iter().enumerate() {
                                        if let Some(sql_value) = values.get(i) {
                                            let json_value =
                                                ValueConverter::rusqlite_value_to_json(sql_value);
                                            remote_row_data.insert(col_name.clone(), json_value);
                                        }
                                    }

                                    // Create conflict entry
                                    if let Err(e) = create_conflict_entry(
                                        &tx,
                                        &first_change.table_name,
                                        error_msg,
                                        &remote_row_data,
                                        &max_hlc_for_row,
                                        &schema,
                                    ) {
                                        eprintln!(
                                            "[SYNC RUST] Failed to create conflict entry: {:?}",
                                            e
                                        );
                                    }

                                    continue; // Skip this row and continue with next
                                }

                                // For other constraints (CHECK, etc.), re-throw the error
                                return Err(DatabaseError::from(rusqlite::Error::SqliteFailure(
                                    err, msg,
                                )));
                            }
                            Err(e) => {
                                eprintln!(
                                    "[SYNC RUST] INSERT failed for table {}: {:?}",
                                    first_change.table_name, e
                                );
                                return Err(DatabaseError::from(e));
                            }
                        }
                    }

                    // A verified signature is CRDT metadata too. Persist it
                    // only after the corresponding value write succeeded so a
                    // receiver can relay the change without re-signing it.
                    if let Some(space_id) = row_space_id_for_sig.as_deref() {
                        for (column, _, _, sig) in &columns_to_update {
                            if let Some(sig) = sig {
                                upsert_column_sigs(
                                    &tx,
                                    &first_change.table_name,
                                    &first_change.row_pks,
                                    column,
                                    space_id,
                                    sig,
                                )
                                .map_err(DatabaseError::from)?;
                            }
                        }
                    }
                }
            }

            // Propagate delete-log entries received in this batch to their target tables.
            // Triggers are still disabled, so the DELETEs won't re-log into haex_deleted_rows.
            if !inbound_delete_log_ids.is_empty() {
                eprintln!(
                    "[SYNC RUST] Propagating {} delete-log entries to target tables",
                    inbound_delete_log_ids.len()
                );
                propagate_deleted_rows_to_target_tables(&tx, &inbound_delete_log_ids)?;
            }

            // Task 6: propagate per-space delete-log entries. Each entry removes
            // the register entry for (table, row, space), and removes the target
            // row iff no other space still lists it. Triggers stay disabled so
            // the Task 4/5 fanout/cascade doesn't re-emit.
            //
            // Interaction with the owner-space sig-enforce exemption above:
            // owner-vault sync (`enforce_sigs == false`) can carry
            // `haex_shared_space_deleted_rows` entries between the owner's own
            // devices without per-column signatures. That's intentional —
            // authorization of the actual DELETE does NOT rely on
            // `enforce_sigs` here. `propagate_shared_space_deleted_rows_to_target_tables`
            // reads the entry's own `space_id` and gates on the local
            // `haex_shared_space_sync` register for that space (register-check),
            // failing closed on DB errors. So on the owner-sync route the
            // sig-enforcement being off just lets the delete-log entry LAND;
            // whether to actually delete the business row is still decided by
            // the receiver's register state (which owner-sync mirrors from the
            // sender). Shared-space delivery (non-owner `expected_space_id`)
            // keeps `enforce_sigs == true`, so unsigned per-space delete-log
            // entries drop at the outer gate before ever reaching propagation.
            if !inbound_shared_space_delete_log_ids.is_empty() {
                eprintln!(
                    "[SYNC RUST] Propagating {} shared-space delete-log entries",
                    inbound_shared_space_delete_log_ids.len()
                );
                propagate_shared_space_deleted_rows_to_target_tables(
                    &tx,
                    &inbound_shared_space_delete_log_ids,
                )?;
            }

            // Update lastPushHlcTimestamp for this backend to prevent re-pushing the data we just pulled
            // Note: lastPullServerTimestamp is now updated by TypeScript using the server timestamp
            // Only applicable for server sync (not local delivery)
            if let Some((backend_id, max_hlc)) = backend_info {
                eprintln!(
                    "[SYNC RUST] Updating last_push_hlc_timestamp to {}",
                    max_hlc
                );
                tx.execute(
                    "UPDATE haex_sync_backends SET last_push_hlc_timestamp = ? WHERE id = ?",
                    params![max_hlc, backend_id],
                )
                .map_err(DatabaseError::from)?;
            }

            // Re-enable triggers before committing
            eprintln!("[SYNC RUST] Re-enabling triggers");
            let enable_sql = format!(
            "INSERT INTO {TABLE_CRDT_CONFIGS} (key, type, value) VALUES ('triggers_enabled', 'system', '1')
             ON CONFLICT(key) DO UPDATE SET value = '1'"
        );
            tx.execute(&enable_sql, []).map_err(DatabaseError::from)?;

            // Commit transaction (with FK constraints disabled)
            eprintln!("[SYNC RUST] Committing transaction");
            match tx.commit() {
                Ok(_) => {
                    eprintln!("[SYNC RUST] Transaction committed successfully");
                }
                Err(e) => {
                    eprintln!("[SYNC RUST] Transaction commit failed: {:?}", e);
                    return Err(DatabaseError::from(e));
                }
            }

            Ok(all_hlc_timestamps)
        })?;
        // FK constraints are now re-enabled by with_fk_disabled (even if
        // the closure above returned Err mid-body).

        // Advance the local HLC clock past the highest received remote timestamp.
        // This ensures future local operations generate timestamps > any remote HLC,
        // so all columns of locally created rows are pushed (not filtered by lastPushHlcTimestamp).
        if let Some(hlc) = hlc_service {
            // Use backend_info max_hlc if available (server sync), otherwise
            // compute from the changes themselves (local delivery).
            let max_hlc_str = match backend_info {
                Some((_, hlc_str)) if !hlc_str.is_empty() => hlc_str.to_string(),
                _ => {
                    let max = hlc_max(applied_hlc_timestamps.iter().map(|s| s.as_str()));
                    max.unwrap_or_default().to_string()
                }
            };
            // Runs after tx.commit() by design: a crash between commit and advance is
            // healed by the idempotent re-apply of the same batch on the next cycle.
            if let Err(e) = hlc.advance_past_remote(&max_hlc_str) {
                match e {
                    // A malformed max-HLC cannot be fixed by retrying the same batch —
                    // log loudly and continue so the sync loop does not wedge.
                    HlcError::Parse(_) => {
                        eprintln!(
                            "[SYNC RUST] CRITICAL: HLC advance skipped (unparseable max HLC): {e:?}"
                        );
                    }
                    // NotInitialized / MutexPoisoned / other service-state errors:
                    // fail the apply so the pull cursor does not advance and the batch
                    // (idempotent) is retried after restart.
                    other => {
                        return Err(DatabaseError::DatabaseError {
                            reason: format!("HLC advance failed after apply: {other:?}"),
                        });
                    }
                }
            }
        }

        Ok(())
    })
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::crdt::column_sig::sign::sign_column;
    use crate::crdt::column_sig::value_bytes;
    use crate::database::migrations::{
        clear_pending_table_inner, get_recoverable_pending_tables_inner,
    };
    use crate::database::DbConnection;
    use std::sync::{Arc, Mutex};

    // Minimal apply harness: the CRDT configs table (for the triggers-enabled
    // toggle) + a target table with a NOT NULL no-default column (`space_id`)
    // next to a nullable one (`avatar`), mirroring haex_space_devices.
    fn setup_db() -> DbConnection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(&format!(
            "CREATE TABLE {TABLE_CRDT_CONFIGS} (key TEXT PRIMARY KEY, type TEXT, value TEXT);
             CREATE TABLE {DELETED_ROWS_TABLE} (
                 id TEXT PRIMARY KEY,
                 table_name TEXT NOT NULL,
                 row_pks TEXT NOT NULL,
                 haex_hlc TEXT,
                 haex_column_hlcs TEXT NOT NULL DEFAULT '{{}}'
             );
             CREATE TABLE devices (
                 id TEXT PRIMARY KEY,
                 space_id TEXT NOT NULL,
                 avatar TEXT,
                 haex_hlc TEXT,
                 haex_column_hlcs TEXT NOT NULL DEFAULT '{{}}'
             );"
        ))
        .unwrap();
        DbConnection(Arc::new(Mutex::new(Some(conn))))
    }

    // Extended harness that also creates the pending tables marker table.
    fn setup_db_with_pending_tables() -> DbConnection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(&format!(
            "CREATE TABLE {TABLE_CRDT_CONFIGS} (key TEXT PRIMARY KEY, type TEXT, value TEXT);
             CREATE TABLE {TABLE_CRDT_PENDING_TABLES} (table_name TEXT PRIMARY KEY NOT NULL);
             CREATE TABLE {DELETED_ROWS_TABLE} (
                 id TEXT PRIMARY KEY,
                 table_name TEXT NOT NULL,
                 row_pks TEXT NOT NULL,
                 haex_hlc TEXT,
                 haex_column_hlcs TEXT NOT NULL DEFAULT '{{}}'
             );"
        ))
        .unwrap();
        DbConnection(Arc::new(Mutex::new(Some(conn))))
    }

    fn change(pk: &str, col: &str, val: &str, hlc: &str) -> RemoteColumnChange {
        RemoteColumnChange {
            table_name: "devices".to_string(),
            row_pks: pk.to_string(),
            column_name: col.to_string(),
            hlc_timestamp: hlc.to_string(),
            decrypted_value: JsonValue::String(val.to_string()),
            sig: None,
        }
    }

    fn row_count(db: &DbConnection, where_sql: &str) -> i64 {
        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        conn.query_row(
            &format!("SELECT COUNT(*) FROM devices WHERE {where_sql}"),
            [],
            |r| r.get(0),
        )
        .unwrap()
    }

    // Regression for the haex_space_devices sync wedge: a change set carrying
    // only a nullable column for a row that doesn't exist locally (its NOT NULL
    // creation columns are below the pull cursor, or the row is partial on the
    // leader) can never satisfy a partial INSERT. Throwing here wedged the
    // whole sync loop forever — the cursor never advanced and the same batch
    // was re-pulled every cycle. The apply must skip the row and succeed.
    #[test]
    fn partial_insert_missing_notnull_is_skipped_not_wedged() {
        let db = setup_db();
        let changes = vec![change(
            r#"{"id":"dev-1"}"#,
            "avatar",
            "face.png",
            "2/abcdef",
        )];

        let result = apply_remote_changes_to_db(&db, changes, None, None);

        assert!(
            result.is_ok(),
            "partial-column INSERT must not error the whole apply: {result:?}"
        );
        assert_eq!(
            row_count(&db, "id = 'dev-1'"),
            0,
            "row with a missing NOT NULL column must be skipped, not inserted"
        );
    }

    // The skip is surgical: a complete row in the same batch still applies and
    // the apply as a whole succeeds (so the sync cursor advances).
    #[test]
    fn complete_row_applies_while_partial_sibling_is_skipped() {
        let db = setup_db();
        let changes = vec![
            change(r#"{"id":"ok"}"#, "space_id", "s1", "1/abcdef"),
            change(r#"{"id":"ok"}"#, "avatar", "a.png", "1/abcdef"),
            change(r#"{"id":"bad"}"#, "avatar", "b.png", "2/abcdef"),
        ];

        let result = apply_remote_changes_to_db(&db, changes, None, None);

        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            row_count(&db, "id = 'ok'"),
            1,
            "complete row must be inserted"
        );
        assert_eq!(
            row_count(&db, "id = 'bad'"),
            0,
            "partial row must be skipped"
        );
    }

    // A change for a table that does not exist locally inserts a marker into
    // haex_crdt_pending_tables_no_sync, and the apply still returns Ok (the
    // sync cursor must advance past this batch).
    #[test]
    fn missing_table_inserts_pending_marker_and_returns_ok() {
        let db = setup_db_with_pending_tables();
        let changes = vec![RemoteColumnChange {
            table_name: "haex_ext_not_installed".to_string(),
            row_pks: r#"{"id":"row-1"}"#.to_string(),
            column_name: "value".to_string(),
            hlc_timestamp: "1/aabbcc".to_string(),
            decrypted_value: JsonValue::String("data".to_string()),
            sig: None,
        }];

        let result = apply_remote_changes_to_db(&db, changes, None, None);
        assert!(
            result.is_ok(),
            "apply must return Ok even when a table is missing: {result:?}"
        );

        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        let count: i64 = conn
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM {} WHERE table_name = 'haex_ext_not_installed'",
                    TABLE_CRDT_PENDING_TABLES
                ),
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 1,
            "pending-table marker must be inserted for the skipped table"
        );
    }

    // get_recoverable_pending_tables_inner returns a marker only once the table
    // exists locally; clear_pending_table_inner removes it.
    #[test]
    fn recoverable_pending_tables_filtered_by_existence_and_clearable() {
        let db = setup_db_with_pending_tables();

        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();

        // Seed a marker for a table that does NOT yet exist.
        conn.execute(
            &format!(
                "INSERT INTO {} (table_name) VALUES ('haex_not_yet')",
                TABLE_CRDT_PENDING_TABLES
            ),
            [],
        )
        .unwrap();

        // Not recoverable yet — the table doesn't exist.
        let before = get_recoverable_pending_tables_inner(conn).unwrap();
        assert!(
            before.is_empty(),
            "marker for non-existent table must not be recoverable: {before:?}"
        );

        // Create the table locally (simulates extension install).
        conn.execute_batch("CREATE TABLE haex_not_yet (id TEXT PRIMARY KEY)")
            .unwrap();

        let after = get_recoverable_pending_tables_inner(conn).unwrap();
        assert_eq!(
            after,
            vec!["haex_not_yet".to_string()],
            "marker must be returned once the table exists locally"
        );

        // Clear the marker.
        clear_pending_table_inner(conn, "haex_not_yet").unwrap();

        let cleared = get_recoverable_pending_tables_inner(conn).unwrap();
        assert!(
            cleared.is_empty(),
            "marker must be gone after clear: {cleared:?}"
        );
    }

    // Regression: applying an older remote change to column B must not regress
    // the row's haex_hlc below the current value (set by a newer local change
    // to column A). A regressed haex_hlc would let an older remote delete win
    // against the newer local write (delete_propagation.rs resurrection check).
    #[test]
    fn row_hlc_never_regresses_when_applying_older_changes() {
        let db = setup_db();

        // Seed an existing row: column A written at T=10, column B written at T=3,
        // so the row's haex_hlc is T=10.
        {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.execute(
                "INSERT INTO devices (id, space_id, avatar, haex_hlc, haex_column_hlcs) \
                 VALUES ('dev-1', 's1', 'old.png', '10/aaa', '{\"space_id\":\"10/aaa\",\"avatar\":\"3/aaa\"}')",
                [],
            )
            .unwrap();
        }

        // Apply a remote change to column B (avatar) with HLC T=5 — newer than
        // the column B's current HLC (T=3) so it applies, but older than the
        // row's current haex_hlc (T=10).
        let changes = vec![change(r#"{"id":"dev-1"}"#, "avatar", "new.png", "5/aaa")];
        let result = apply_remote_changes_to_db(&db, changes, None, None);
        assert!(result.is_ok(), "apply must succeed: {result:?}");

        // Column B must carry the new value and its column HLC must be T=5.
        let (avatar_val, col_hlcs_str, row_hlc): (String, String, String) = {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row(
                "SELECT avatar, haex_column_hlcs, haex_hlc FROM devices WHERE id = 'dev-1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap()
        };
        assert_eq!(avatar_val, "new.png", "column B value must be updated");
        let col_hlcs: serde_json::Map<String, JsonValue> =
            serde_json::from_str(&col_hlcs_str).unwrap();
        assert_eq!(
            col_hlcs.get("avatar").and_then(|v| v.as_str()),
            Some("5/aaa"),
            "column B HLC must be T=5"
        );
        assert_eq!(
            row_hlc, "10/aaa",
            "row haex_hlc must not regress below T=10 after applying an older T=5 change"
        );
    }

    // -----------------------------------------------------------------------
    // Runde-5 sig-verifier plumbing (Task G1c + G1d)
    // -----------------------------------------------------------------------

    use crate::ucan::verify::did_key_from_public_key;
    use ed25519_dalek::SigningKey;

    /// Extension of `setup_db()` that also has `haex_identities` so the
    /// Runde-5 `ensure_identity_stub` path has somewhere to insert into,
    /// and seeds a device row so sig verification can read its `space_id`.
    fn setup_db_with_identities() -> DbConnection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(&format!(
            "CREATE TABLE {TABLE_CRDT_CONFIGS} (key TEXT PRIMARY KEY, type TEXT, value TEXT);
             CREATE TABLE {DELETED_ROWS_TABLE} (
                 id TEXT PRIMARY KEY,
                 table_name TEXT NOT NULL,
                 row_pks TEXT NOT NULL,
                 haex_hlc TEXT,
                 haex_column_hlcs TEXT NOT NULL DEFAULT '{{}}'
             );
             -- `name TEXT NOT NULL` without a default mirrors migration 0000
             -- and is load-bearing: a stub INSERT that omits it is silently
             -- swallowed by `OR IGNORE`, which is the bug
             -- `apply_ensures_identity_stub_for_new_author_did` must be able
             -- to see.
             CREATE TABLE haex_identities (
                 id TEXT PRIMARY KEY NOT NULL,
                 did TEXT NOT NULL,
                 name TEXT NOT NULL,
                 source TEXT DEFAULT 'contact' NOT NULL
             );
             CREATE UNIQUE INDEX haex_identities_did_unique ON haex_identities (did);
             CREATE TABLE devices (
                 id TEXT PRIMARY KEY,
                 space_id TEXT NOT NULL,
                 avatar TEXT,
                 haex_hlc TEXT,
                 haex_column_hlcs TEXT NOT NULL DEFAULT '{{}}'
             );
             INSERT INTO devices (id, space_id, avatar, haex_hlc, haex_column_hlcs) \
              VALUES ('dev-1', 's1', 'old.png', '10/aaa', '{{\"space_id\":\"10/aaa\",\"avatar\":\"3/aaa\"}}');"
        ))
        .unwrap();
        DbConnection(Arc::new(Mutex::new(Some(conn))))
    }

    /// Runde 5 G1d — a change carrying a well-formed but invalid Ed25519
    /// signature is dropped from the batch (row-scoped rejection). The
    /// target row stays untouched and the invalid author DID is NOT stub-
    /// inserted into `haex_identities` (that path only fires for verified
    /// sigs — otherwise a peer could flood the table with attacker DIDs).
    ///
    /// The signature is produced by a real key over a *different* value
    /// than the one claimed in the change, so verification fails on
    /// `VerifyColumnSigError::InvalidSignature` — not on a malformed-DID
    /// short-circuit, which would make this test pass for the wrong reason.
    #[test]
    fn apply_rejects_change_with_invalid_signature() {
        let db = setup_db_with_identities();

        let seed: [u8; 32] = rand::random();
        let signing_key = SigningKey::from_bytes(&seed);
        let did = did_key_from_public_key(&signing_key.verifying_key());
        let space_id = "s1"; // seeded on the row in setup
        let hlc = "20/xxx"; // > existing avatar HLC 3/aaa so it would apply if verified

        // Sign a *different* value than the one the change actually carries,
        // so the recomputed preimage on the verifier side never matches.
        let signed_value_bytes = value_bytes::to_canonical_bytes(&SqlValue::Text(
            "value-that-was-not-tampered.png".to_string(),
        ));
        let sig = sign_column(
            &signing_key,
            space_id.as_bytes(),
            b"devices",
            br#"{"id":"dev-1"}"#,
            b"avatar",
            hlc.as_bytes(),
            did.as_bytes(),
            &signed_value_bytes,
        );

        let change = RemoteColumnChange {
            table_name: "devices".to_string(),
            row_pks: r#"{"id":"dev-1"}"#.to_string(),
            column_name: "avatar".to_string(),
            hlc_timestamp: hlc.to_string(),
            decrypted_value: JsonValue::String("tampered.png".to_string()),
            sig: Some(ColumnSig {
                author_did: did.clone(),
                sig: BASE64.encode(sig.to_bytes()),
                storage_class: crate::crdt::column_sig::value_bytes::StorageClass::Text,
            }),
        };

        apply_remote_changes_to_db(&db, vec![change], None, None)
            .expect("apply must succeed (row-scoped rejection, not batch abort)");

        // The row's avatar must not have been overwritten.
        let avatar: String = {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row("SELECT avatar FROM devices WHERE id = 'dev-1'", [], |r| {
                r.get(0)
            })
            .unwrap()
        };
        assert_eq!(
            avatar, "old.png",
            "invalid-sig change must be dropped, existing value preserved"
        );

        // No identity stub — invalid sigs must not seed `haex_identities`.
        let stub_count: i64 = {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row(
                "SELECT COUNT(*) FROM haex_identities WHERE did = ?",
                [&did],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(
            stub_count, 0,
            "invalid-sig author DID must NOT get a stub row"
        );
    }

    #[test]
    fn apply_scoped_rejects_unsigned_shared_space_change() {
        let db = setup_db_with_identities();
        let change = RemoteColumnChange {
            table_name: "devices".to_string(),
            row_pks: r#"{"id":"dev-1"}"#.to_string(),
            column_name: "avatar".to_string(),
            hlc_timestamp: "20/xxx".to_string(),
            decrypted_value: JsonValue::String("unsigned.png".to_string()),
            sig: None,
        };

        apply_remote_changes_to_db_scoped(&db, vec![change], None, None, Some("s1"))
            .expect("unsigned rejection is row-scoped");

        let avatar: String = {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row("SELECT avatar FROM devices WHERE id = 'dev-1'", [], |r| {
                r.get(0)
            })
            .unwrap()
        };
        assert_eq!(avatar, "old.png");
    }

    /// Runde 5 G1c — a change with a **valid** sig from a DID we've never
    /// seen locally triggers `ensure_identity_stub`, which INSERT OR IGNOREs
    /// a row into `haex_identities` so downstream FKs / joins can bind. The
    /// change itself also lands (updates the row).
    #[test]
    fn apply_ensures_identity_stub_for_new_author_did() {
        let db = setup_db_with_identities();

        let seed: [u8; 32] = rand::random();
        let signing_key = SigningKey::from_bytes(&seed);
        let did = did_key_from_public_key(&signing_key.verifying_key());
        let space_id = "s1"; // seeded on the row in setup
        let new_avatar = "verified.png";
        let hlc = "20/xxx";

        let value_bytes_vec =
            value_bytes::to_canonical_bytes(&SqlValue::Text(new_avatar.to_string()));
        let sig = sign_column(
            &signing_key,
            space_id.as_bytes(),
            b"devices",
            br#"{"id":"dev-1"}"#,
            b"avatar",
            hlc.as_bytes(),
            did.as_bytes(),
            &value_bytes_vec,
        );

        let change = RemoteColumnChange {
            table_name: "devices".to_string(),
            row_pks: r#"{"id":"dev-1"}"#.to_string(),
            column_name: "avatar".to_string(),
            hlc_timestamp: hlc.to_string(),
            decrypted_value: JsonValue::String(new_avatar.to_string()),
            sig: Some(ColumnSig {
                author_did: did.clone(),
                sig: BASE64.encode(sig.to_bytes()),
                storage_class: crate::crdt::column_sig::value_bytes::StorageClass::Text,
            }),
        };

        apply_remote_changes_to_db(&db, vec![change], None, None).expect("apply must succeed");

        let stub_count: i64 = {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row(
                "SELECT COUNT(*) FROM haex_identities WHERE did = ?",
                [&did],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(
            stub_count, 1,
            "verified-sig new author DID must produce exactly one stub"
        );

        // And the change itself must have landed.
        let avatar: String = {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row("SELECT avatar FROM devices WHERE id = 'dev-1'", [], |r| {
                r.get(0)
            })
            .unwrap()
        };
        assert_eq!(avatar, new_avatar);
    }

    /// Helper: sign `value` for `space_id` on `devices.avatar` at `hlc` and
    /// wrap it in a ready-to-apply `RemoteColumnChange`.
    fn signed_avatar_change(
        signing_key: &SigningKey,
        space_id: &str,
        value: &str,
        hlc: &str,
    ) -> RemoteColumnChange {
        let did = did_key_from_public_key(&signing_key.verifying_key());
        let value_bytes_vec = value_bytes::to_canonical_bytes(&SqlValue::Text(value.to_string()));
        let sig = sign_column(
            signing_key,
            space_id.as_bytes(),
            b"devices",
            br#"{"id":"dev-1"}"#,
            b"avatar",
            hlc.as_bytes(),
            did.as_bytes(),
            &value_bytes_vec,
        );
        RemoteColumnChange {
            table_name: "devices".to_string(),
            row_pks: r#"{"id":"dev-1"}"#.to_string(),
            column_name: "avatar".to_string(),
            hlc_timestamp: hlc.to_string(),
            decrypted_value: JsonValue::String(value.to_string()),
            sig: Some(ColumnSig {
                author_did: did,
                sig: BASE64.encode(sig.to_bytes()),
                storage_class: crate::crdt::column_sig::value_bytes::StorageClass::Text,
            }),
        }
    }

    fn read_avatar(db: &DbConnection) -> String {
        let guard = db.0.lock().unwrap();
        let conn = guard.as_ref().unwrap();
        conn.query_row("SELECT avatar FROM devices WHERE id = 'dev-1'", [], |r| {
            r.get(0)
        })
        .unwrap()
    }

    /// The verification anchor must be the row's PERSISTED `space_id`, never
    /// a `space_id` column change riding along in the same batch.
    ///
    /// Attack shape: a peer holds a signing key for its own space `s_evil`
    /// and pushes `{space_id: "s_evil", avatar: "pwned.png"}` at `dev-1`,
    /// which locally belongs to `s1`. If the resolver preferred the batch's
    /// claim, both changes would verify under `s_evil` and the avatar update
    /// would land on the victim row — the space binding in the preimage
    /// (ADR 0002 §4b) defeated because the attacker chose the binding.
    #[test]
    fn apply_anchors_sig_on_persisted_space_id_not_batch_claim() {
        let db = setup_db_with_identities();

        let seed: [u8; 32] = rand::random();
        let attacker_key = SigningKey::from_bytes(&seed);
        let hlc = "20/xxx";

        // Both changes are correctly signed — but for `s_evil`, not `s1`.
        let avatar_change = signed_avatar_change(&attacker_key, "s_evil", "pwned.png", hlc);
        let mut space_change = signed_avatar_change(&attacker_key, "s_evil", "s_evil", hlc);
        space_change.column_name = "space_id".to_string();

        apply_remote_changes_to_db(&db, vec![space_change, avatar_change], None, None)
            .expect("apply must succeed — rejection is column-scoped, not fatal");

        assert_eq!(
            read_avatar(&db),
            "old.png",
            "sig signed for a foreign space must not verify against the row's own space"
        );
    }

    /// Same row, same attacker key, but this time signed for the row's real
    /// space `s1` — proving the test above fails for the right reason (wrong
    /// space) rather than because the harness is broken.
    #[test]
    fn apply_accepts_sig_matching_persisted_space_id() {
        let db = setup_db_with_identities();

        let seed: [u8; 32] = rand::random();
        let key = SigningKey::from_bytes(&seed);
        let mut space_change = signed_avatar_change(&key, "s1", "s_evil", "20/xxx");
        space_change.column_name = "space_id".to_string();
        let avatar_change = signed_avatar_change(&key, "s1", "accepted.png", "20/xxx");

        apply_remote_changes_to_db(&db, vec![space_change, avatar_change], None, None)
            .expect("apply must succeed");

        assert_eq!(read_avatar(&db), "accepted.png");
    }

    /// A row that does not exist locally has no persisted anchor. Without an
    /// `expected_space_id` from the caller there is no way to tell an honest
    /// claim from a forged one, so signed changes are dropped rather than
    /// verified against whatever the batch asserts.
    #[test]
    fn apply_drops_signed_insert_when_no_expected_space_is_given() {
        let db = setup_db_with_identities();

        let seed: [u8; 32] = rand::random();
        let key = SigningKey::from_bytes(&seed);
        let mut space_change = signed_avatar_change(&key, "s_new", "s_new", "20/xxx");
        space_change.column_name = "space_id".to_string();
        let mut avatar_change = signed_avatar_change(&key, "s_new", "new-row.png", "20/xxx");
        avatar_change.row_pks = r#"{"id":"dev-2"}"#.to_string();
        space_change.row_pks = r#"{"id":"dev-2"}"#.to_string();

        apply_remote_changes_to_db(&db, vec![space_change, avatar_change], None, None)
            .expect("apply must succeed");

        let count: i64 = {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row("SELECT COUNT(*) FROM devices WHERE id = 'dev-2'", [], |r| {
                r.get(0)
            })
            .unwrap()
        };
        assert_eq!(
            count, 0,
            "unanchored signed insert must be dropped, not applied unverified"
        );
    }

    /// With the pull scope supplied, a signed INSERT whose claimed `space_id`
    /// agrees with that scope verifies and lands.
    #[test]
    fn apply_scoped_accepts_signed_insert_matching_expected_space() {
        let db = setup_db_with_identities();

        let seed: [u8; 32] = rand::random();
        let key = SigningKey::from_bytes(&seed);
        // NOTE: row_pks must match what was signed, so build then retarget
        // both changes consistently before signing is irrelevant — the pks
        // are part of the preimage, so sign for dev-2 directly.
        let did = did_key_from_public_key(&key.verifying_key());
        let hlc = "20/xxx";
        let mk = |column: &str, value: &str| {
            let vb = value_bytes::to_canonical_bytes(&SqlValue::Text(value.to_string()));
            let sig = sign_column(
                &key,
                b"s_new",
                b"devices",
                br#"{"id":"dev-2"}"#,
                column.as_bytes(),
                hlc.as_bytes(),
                did.as_bytes(),
                &vb,
            );
            RemoteColumnChange {
                table_name: "devices".to_string(),
                row_pks: r#"{"id":"dev-2"}"#.to_string(),
                column_name: column.to_string(),
                hlc_timestamp: hlc.to_string(),
                decrypted_value: JsonValue::String(value.to_string()),
                sig: Some(ColumnSig {
                    author_did: did.clone(),
                    sig: BASE64.encode(sig.to_bytes()),
                    storage_class: crate::crdt::column_sig::value_bytes::StorageClass::Text,
                }),
            }
        };

        apply_remote_changes_to_db_scoped(
            &db,
            vec![mk("space_id", "s_new"), mk("avatar", "new-row.png")],
            None,
            None,
            Some("s_new"),
        )
        .expect("apply must succeed");

        let avatar: Option<String> = {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row("SELECT avatar FROM devices WHERE id = 'dev-2'", [], |r| {
                r.get(0)
            })
            .ok()
        };
        assert_eq!(avatar.as_deref(), Some("new-row.png"));
    }

    /// The cross-check bites: a signed INSERT claiming a space other than the
    /// one the pull was scoped to is refused even though its signature is
    /// internally consistent.
    #[test]
    fn apply_scoped_rejects_signed_insert_claiming_a_different_space() {
        let db = setup_db_with_identities();

        let seed: [u8; 32] = rand::random();
        let key = SigningKey::from_bytes(&seed);
        let did = did_key_from_public_key(&key.verifying_key());
        let hlc = "20/xxx";
        let mk = |column: &str, value: &str| {
            let vb = value_bytes::to_canonical_bytes(&SqlValue::Text(value.to_string()));
            let sig = sign_column(
                &key,
                b"s_evil",
                b"devices",
                br#"{"id":"dev-2"}"#,
                column.as_bytes(),
                hlc.as_bytes(),
                did.as_bytes(),
                &vb,
            );
            RemoteColumnChange {
                table_name: "devices".to_string(),
                row_pks: r#"{"id":"dev-2"}"#.to_string(),
                column_name: column.to_string(),
                hlc_timestamp: hlc.to_string(),
                decrypted_value: JsonValue::String(value.to_string()),
                sig: Some(ColumnSig {
                    author_did: did.clone(),
                    sig: BASE64.encode(sig.to_bytes()),
                    storage_class: crate::crdt::column_sig::value_bytes::StorageClass::Text,
                }),
            }
        };

        apply_remote_changes_to_db_scoped(
            &db,
            vec![mk("space_id", "s_evil"), mk("avatar", "new-row.png")],
            None,
            None,
            Some("s_expected"),
        )
        .expect("apply must succeed");

        let count: i64 = {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row("SELECT COUNT(*) FROM devices WHERE id = 'dev-2'", [], |r| {
                r.get(0)
            })
            .unwrap()
        };
        assert_eq!(count, 0, "space-mismatched signed insert must be dropped");
    }

    // -----------------------------------------------------------------------
    // Owner-space trust: unsigned changes must land when the expected space
    // is this vault's own owner-space (VAULT-type row in haex_spaces). See
    // owner_sync::scope::is_owner_space for the rationale.
    // -----------------------------------------------------------------------

    /// Test schema mirroring `setup_db_with_identities` but *with* a
    /// `haex_spaces` row of `type='vault'` so `is_owner_space` can resolve.
    /// `owner_space_id` is the vault-space id.
    fn setup_db_with_owner_space(owner_space_id: &str) -> DbConnection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(&format!(
            "CREATE TABLE {TABLE_CRDT_CONFIGS} (key TEXT PRIMARY KEY, type TEXT, value TEXT);
             CREATE TABLE {DELETED_ROWS_TABLE} (
                 id TEXT PRIMARY KEY,
                 table_name TEXT NOT NULL,
                 row_pks TEXT NOT NULL,
                 haex_hlc TEXT,
                 haex_column_hlcs TEXT NOT NULL DEFAULT '{{}}'
             );
             CREATE TABLE haex_identities (
                 id TEXT PRIMARY KEY NOT NULL,
                 did TEXT NOT NULL,
                 name TEXT NOT NULL,
                 source TEXT DEFAULT 'contact' NOT NULL
             );
             CREATE UNIQUE INDEX haex_identities_did_unique ON haex_identities (did);
             CREATE TABLE haex_spaces (
                 id TEXT PRIMARY KEY,
                 type TEXT NOT NULL,
                 owner_identity_id TEXT
             );
             CREATE TABLE devices (
                 id TEXT PRIMARY KEY,
                 space_id TEXT NOT NULL,
                 avatar TEXT,
                 haex_hlc TEXT,
                 haex_column_hlcs TEXT NOT NULL DEFAULT '{{}}'
             );
             INSERT INTO haex_identities (id, did, name) VALUES ('id-owner', 'did:key:zOwner', 'owner');
             INSERT INTO haex_spaces (id, type, owner_identity_id) VALUES ('{owner_space_id}', 'vault', 'id-owner');
             INSERT INTO devices (id, space_id, avatar, haex_hlc, haex_column_hlcs) \
              VALUES ('dev-1', '{owner_space_id}', 'old.png', '10/aaa', '{{\"space_id\":\"10/aaa\",\"avatar\":\"3/aaa\"}}');"
        ))
        .unwrap();
        DbConnection(Arc::new(Mutex::new(Some(conn))))
    }

    /// Owner-sync between two devices of the same identity carries
    /// `expected_space_id` = the vault-space id, but the wire payload is
    /// unsigned by design (`sign_column_for_spaces` yields `{}` for owner-
    /// private rows). Before the trust-own-vault gate, this batch was
    /// silently dropped and every owner-private CRDT row failed to converge
    /// (see the failing e2e test
    /// `tests/sync/owner-sync-delete-convergence.spec.ts`).
    #[test]
    fn apply_remote_changes_to_db_scoped_accepts_unsigned_when_expected_space_is_owner_space() {
        let owner_space = "vault-owner-space";
        let db = setup_db_with_owner_space(owner_space);

        let change = RemoteColumnChange {
            table_name: "devices".to_string(),
            row_pks: r#"{"id":"dev-1"}"#.to_string(),
            column_name: "avatar".to_string(),
            hlc_timestamp: "20/xxx".to_string(),
            decrypted_value: JsonValue::String("unsigned-owner.png".to_string()),
            sig: None,
        };

        apply_remote_changes_to_db_scoped(&db, vec![change], None, None, Some(owner_space))
            .expect("apply must succeed for owner-space unsigned change");

        let avatar: String = {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row("SELECT avatar FROM devices WHERE id = 'dev-1'", [], |r| {
                r.get(0)
            })
            .unwrap()
        };
        assert_eq!(
            avatar, "unsigned-owner.png",
            "unsigned owner-space change must land: sig enforcement is off on the owner-space route"
        );
    }

    /// Regression guard: the "shared-space unsigned change is dropped"
    /// semantic must survive even with the owner-space gate in place. When
    /// `expected_space_id` points to a space that is NOT this vault's
    /// owner-space (either a shared space id or an id that does not exist in
    /// `haex_spaces` at all), the strict Phase-1 gate stays on.
    #[test]
    fn apply_remote_changes_to_db_scoped_rejects_unsigned_when_expected_space_is_shared() {
        let owner_space = "vault-owner-space";
        let db = setup_db_with_owner_space(owner_space);

        // Shared space id (any id != the vault-space id). No row for it needs
        // to exist in haex_spaces — is_owner_space is a positive check on the
        // vault-space row, everything else is "not owner-space" → enforce.
        let shared_space = "shared-space-abc";

        let change = RemoteColumnChange {
            table_name: "devices".to_string(),
            row_pks: r#"{"id":"dev-1"}"#.to_string(),
            column_name: "avatar".to_string(),
            hlc_timestamp: "20/xxx".to_string(),
            decrypted_value: JsonValue::String("dropped.png".to_string()),
            sig: None,
        };

        apply_remote_changes_to_db_scoped(&db, vec![change], None, None, Some(shared_space))
            .expect("apply must succeed — rejection is column-scoped, not fatal");

        let avatar: String = {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row("SELECT avatar FROM devices WHERE id = 'dev-1'", [], |r| {
                r.get(0)
            })
            .unwrap()
        };
        assert_eq!(
            avatar, "old.png",
            "unsigned shared-space change must be dropped: sig enforcement stays on for non-owner spaces"
        );
    }

    // -----------------------------------------------------------------------
    // Adversarial apply-pipeline hardening (security review follow-up).
    //
    // Findings, verified directly against this file's logic:
    //
    //  - CRDT conflict-resolution rule: per-COLUMN Hybrid-Logical-Clock
    //    last-write-wins with a STRICT greater-than gate (`hlc_is_newer`,
    //    `crdt::hlc`). Equal or older claimed HLCs never overwrite — this is
    //    NOT "last received wins": a stale/replayed op received after a
    //    newer one has already landed is silently dropped, not blindly
    //    applied. See `apply_v10_then_receiving_stale_v7_does_not_roll_back`.
    //
    //  - Forged HLC handling: REJECT, not "accept but reorder". A signed
    //    column change's Ed25519 preimage includes the claimed
    //    `hlc_timestamp` bytes (`verify_change_sig` -> `build_preimage`), so
    //    an attacker who takes a validly-signed change and swaps only the
    //    wire `hlc_timestamp` (e.g. to force a false LWW win) invalidates
    //    the signature. See `apply_rejects_change_with_forged_hlc_timestamp`.
    // -----------------------------------------------------------------------

    /// Scenario 1a: replaying an identical, unsigned change set (INSERT +
    /// follow-up UPDATE-shaped re-delivery) must be a no-op the second time —
    /// same value, same per-column HLCs, no duplicate row. Simulates a
    /// network/server re-sending an already-accepted push.
    #[test]
    fn apply_is_idempotent_when_identical_unsigned_change_set_is_applied_twice() {
        let db = setup_db();
        let changes = || {
            vec![
                change(r#"{"id":"dev-idem"}"#, "space_id", "s1", "5/aaa"),
                change(r#"{"id":"dev-idem"}"#, "avatar", "first.png", "5/aaa"),
            ]
        };

        apply_remote_changes_to_db(&db, changes(), None, None).expect("first apply must succeed");
        let read_state = |db: &DbConnection| -> (String, String, String) {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row(
                "SELECT avatar, haex_column_hlcs, haex_hlc FROM devices WHERE id = 'dev-idem'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap()
        };
        let after_first = read_state(&db);

        // Replay: the exact same wire batch delivered a second time.
        apply_remote_changes_to_db(&db, changes(), None, None)
            .expect("replay of an already-applied batch must not error");
        let after_replay = read_state(&db);

        assert_eq!(
            after_first, after_replay,
            "replaying an identical change set must not change avatar/column-HLCs/row-HLC"
        );
        assert_eq!(
            row_count(&db, "id = 'dev-idem'"),
            1,
            "replay must not create a duplicate row"
        );
    }

    /// Scenario 1b: replaying an identical, validly-signed change must also
    /// be idempotent at the signature-verification layer — in particular it
    /// must not seed a second `haex_identities` stub for the same author DID
    /// (ties I2/I7 together: authenticity + replay resistance).
    #[test]
    fn apply_is_idempotent_when_signed_change_is_replayed() {
        let db = setup_db_with_identities();

        let seed: [u8; 32] = rand::random();
        let signing_key = SigningKey::from_bytes(&seed);
        let did = did_key_from_public_key(&signing_key.verifying_key());
        let space_id = "s1"; // seeded on the row in setup
        let new_avatar = "replayed.png";
        let hlc = "20/xxx";

        let make_change = || signed_avatar_change(&signing_key, space_id, new_avatar, hlc);

        apply_remote_changes_to_db(&db, vec![make_change()], None, None)
            .expect("first apply of a validly-signed change must succeed");
        // Replay: the identical signed wire message delivered a second time
        // — e.g. a malicious or buggy server re-sending an already-accepted
        // push.
        apply_remote_changes_to_db(&db, vec![make_change()], None, None)
            .expect("replay of an already-verified signed change must not error");

        assert_eq!(read_avatar(&db), new_avatar);

        let stub_count: i64 = {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row(
                "SELECT COUNT(*) FROM haex_identities WHERE did = ?",
                [&did],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(
            stub_count, 1,
            "replaying an already-verified signed change must not duplicate the identity stub"
        );
    }

    /// Scenario 2: a change carrying a genuinely valid Ed25519 signature
    /// (Alice really signed this exact preimage) but whose wire envelope
    /// claims a DIFFERENT author DID (Bob's) must be rejected. The attacker
    /// does not hold Bob's private key — they are attempting to relabel an
    /// honestly-signed operation as someone else's. `author_did` is itself
    /// part of the signed preimage (`build_preimage`) and also selects which
    /// public key verification uses, so this fails on both counts.
    #[test]
    fn apply_rejects_change_with_forged_author_did() {
        let db = setup_db_with_identities();

        let alice_seed: [u8; 32] = rand::random();
        let alice_key = SigningKey::from_bytes(&alice_seed);
        let alice_did = did_key_from_public_key(&alice_key.verifying_key());

        // Bob is an unrelated identity the attacker wants to frame — the
        // attacker never touches bob's private key.
        let bob_seed: [u8; 32] = rand::random();
        let bob_key = SigningKey::from_bytes(&bob_seed);
        let bob_did = did_key_from_public_key(&bob_key.verifying_key());

        let space_id = "s1"; // seeded on the row in setup
        let hlc = "20/xxx";
        let new_avatar = "framed.png";

        // Alice signs honestly, over her own DID — a completely legitimate
        // signature for a completely legitimate change.
        let mut change = signed_avatar_change(&alice_key, space_id, new_avatar, hlc);
        // Attacker relabels the wire envelope's author_did to Bob's,
        // keeping Alice's genuine signature bytes untouched.
        change.sig.as_mut().unwrap().author_did = bob_did.clone();

        apply_remote_changes_to_db(&db, vec![change], None, None)
            .expect("apply must succeed — rejection is column-scoped, not fatal");

        assert_eq!(
            read_avatar(&db),
            "old.png",
            "author-forged change must be dropped, existing value preserved"
        );

        let stub_count: i64 = {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row(
                "SELECT COUNT(*) FROM haex_identities WHERE did IN (?, ?)",
                [&alice_did, &bob_did],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(
            stub_count, 0,
            "a rejected forged-author change must not seed either identity"
        );
    }

    /// Scenario 5: a change carrying a valid signature for HLC `H`, but
    /// whose wire envelope claims a different (here: far-future, LWW-
    /// winning) HLC `H'`. Finding: this codebase's answer is REJECT, not
    /// "accept but order correctly" — `hlc_timestamp` bytes are part of the
    /// signed preimage (`verify_change_sig` -> `build_preimage`), so
    /// swapping the claimed HLC without re-signing invalidates the
    /// signature and the change is dropped before it can win any LWW race.
    #[test]
    fn apply_rejects_change_with_forged_hlc_timestamp() {
        let db = setup_db_with_identities();

        let seed: [u8; 32] = rand::random();
        let signing_key = SigningKey::from_bytes(&seed);
        let space_id = "s1"; // seeded on the row in setup
        let signed_hlc = "20/xxx"; // what was actually signed
        let claimed_hlc = "999999/xxx"; // forged: far future, would win any LWW race
        let new_avatar = "forged-time.png";

        // Attacker takes a legitimately-signed change and swaps ONLY the
        // claimed hlc_timestamp on the wire, hoping the inflated HLC wins
        // the per-column LWW race without needing a fresh signature.
        let mut change = signed_avatar_change(&signing_key, space_id, new_avatar, signed_hlc);
        change.hlc_timestamp = claimed_hlc.to_string();

        apply_remote_changes_to_db(&db, vec![change], None, None)
            .expect("apply must succeed — rejection is column-scoped, not fatal");

        assert_eq!(
            read_avatar(&db),
            "old.png",
            "a claimed HLC that does not match what was actually signed must be rejected \
             (hlc_timestamp is part of the signed preimage) — the codebase's handling of a \
             forged HLC is REJECT, not accept-and-reorder"
        );
    }

    /// Scenario 4: two SEPARATE apply() calls on the same column — a newer
    /// value (HLC 10) followed by a stale one (HLC 7) arriving afterwards
    /// (out-of-order delivery, or a lagging/malicious server replaying an
    /// old push). The actual CRDT rule is per-column HLC last-write-wins
    /// with a strict greater-than gate, NOT "last received wins": the stale
    /// op must lose even though it is the one most recently delivered.
    #[test]
    fn apply_v10_then_receiving_stale_v7_does_not_roll_back() {
        let db = setup_db();
        {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.execute(
                "INSERT INTO devices (id, space_id, avatar, haex_hlc, haex_column_hlcs) \
                 VALUES ('dev-1', 's1', 'seed.png', '0/aaa', '{}')",
                [],
            )
            .unwrap();
        }

        apply_remote_changes_to_db(
            &db,
            vec![change(r#"{"id":"dev-1"}"#, "avatar", "v10", "10/aaa")],
            None,
            None,
        )
        .expect("v10 must apply");

        // Stale v7 delivered AFTER v10 was already applied and committed.
        apply_remote_changes_to_db(
            &db,
            vec![change(r#"{"id":"dev-1"}"#, "avatar", "v7", "7/aaa")],
            None,
            None,
        )
        .expect("stale delivery must not error — rejection is silent/row-scoped");

        let (avatar, row_hlc): (String, String) = {
            let guard = db.0.lock().unwrap();
            let conn = guard.as_ref().unwrap();
            conn.query_row(
                "SELECT avatar, haex_hlc FROM devices WHERE id = 'dev-1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
        };
        assert_eq!(
            avatar, "v10",
            "a stale (older-HLC) op received after a newer one must not roll back the value"
        );
        assert_eq!(
            row_hlc, "10/aaa",
            "row HLC must not regress from a stale delivery"
        );
    }

    /// Scenario 3: a fixed set of 3 independent column-updates to the SAME
    /// field, delivered via 3 separate apply() calls in every one of the
    /// 3! = 6 possible orders (simulating out-of-order network delivery).
    /// Every ordering must converge to the same final state — the
    /// highest-HLC write wins regardless of delivery order, never
    /// "whichever happened to arrive last physically".
    #[test]
    fn apply_converges_to_same_state_regardless_of_delivery_order() {
        // (value, hlc) triples — deliberately NOT HLC-sorted in this list.
        let op_specs = [("v10", "10/aaa"), ("v30", "30/aaa"), ("v20", "20/aaa")];
        let orderings: [[usize; 3]; 6] = [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ];

        let mut results: Vec<(String, String, String)> = Vec::new();
        for order in orderings {
            let db = setup_db();
            {
                let guard = db.0.lock().unwrap();
                let conn = guard.as_ref().unwrap();
                conn.execute(
                    "INSERT INTO devices (id, space_id, avatar, haex_hlc, haex_column_hlcs) \
                     VALUES ('dev-1', 's1', 'seed.png', '0/aaa', '{}')",
                    [],
                )
                .unwrap();
            }
            for i in order {
                let (val, hlc) = op_specs[i];
                apply_remote_changes_to_db(
                    &db,
                    vec![change(r#"{"id":"dev-1"}"#, "avatar", val, hlc)],
                    None,
                    None,
                )
                .expect("each individual delivery must succeed");
            }
            let state: (String, String, String) = {
                let guard = db.0.lock().unwrap();
                let conn = guard.as_ref().unwrap();
                conn.query_row(
                    "SELECT avatar, haex_column_hlcs, haex_hlc FROM devices WHERE id = 'dev-1'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .unwrap()
            };
            results.push(state);
        }

        let baseline = results[0].clone();
        assert_eq!(
            baseline.0, "v30",
            "the highest-HLC write (30/aaa) must win regardless of delivery order"
        );
        for (i, r) in results.iter().enumerate() {
            assert_eq!(
                r, &baseline,
                "delivery order {:?} produced a different final state than order {:?} — \
                 the CRDT merge must be commutative",
                orderings[i], orderings[0]
            );
        }
    }
}
