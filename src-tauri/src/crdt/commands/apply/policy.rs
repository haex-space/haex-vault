//! `VaultApplyPolicy` — vault's [`haex_crdt::ApplyPolicy`] implementation for
//! the inbound CRDT apply pipeline.
//!
//! Wires vault's own concerns (the shared-space registry gate, per-space
//! signature verification, exact SQLite value decoding, vault's nested
//! signature-map storage, dev-mode schema auto-upgrade, constraint recovery,
//! per-space delete-log fan-out, pending-table/pending-column recovery
//! markers, and the sync-backend push cursor) into the six `ApplyPolicy`
//! hooks. See `haex_crdt`'s `crdt::apply::policy` module docs for the trait
//! contract every hook here must honor.

use std::collections::{HashMap, HashSet};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use haex_crdt::{
    ApplyOutcome, ApplyPolicy, ColumnDecision, ConstraintDecision, RemoteChanges, RowDecision,
    RowInput, RowWrite, SignatureWrite,
};
use rusqlite::{params, Transaction};

use super::conflicts::create_conflict_entry;
use super::finish::{
    propagate_shared_space_deletes, update_backend_cursor, write_pending_column_markers,
};
use super::registry_row_gate::{build_incoming_registry_change, RegistryRowChangeOutcome};
use super::schema_recovery::{run_schema_auto_upgrade, write_pending_table_markers};
use super::signatures::{ensure_identity_stub, resolve_row_space_id_for_sig, verify_change_sig};
use super::types::{from_crate_change, RemoteColumnChange};
use crate::crdt::column_sig::storage::{upsert_column_sigs, SigRecord};
use crate::crdt::registry_row_sig::puller_verify::verify_incoming_registry_change;
use crate::crdt::shared_space_trigger::ColumnInfo;
use crate::database::core::ValueConverter;
use crate::table_names::{
    COL_SHARED_SPACE_SYNC_ROW_SIG, TABLE_CRDT_PENDING_COLUMNS, TABLE_SHARED_SPACE_SYNC,
};

use super::super::helpers::build_pk_where_clause;

/// Batch-scoped state computed once per [`haex_crdt::apply_remote_changes`]
/// call and consulted from every hook.
pub(super) struct VaultApplyPolicy {
    /// The space this pull was scoped to (`None` for personal-vault sync).
    /// See `signatures::resolve_row_space_id_for_sig` for why this matters.
    pub(super) expected_space_id: Option<String>,
    /// `(backend_id, max_hlc)` for a server-sync pull; `None` for local
    /// delivery. Consumed in `before_commit` to update the push cursor.
    pub(super) backend_info: Option<(String, String)>,
    /// Whether per-column signatures are required on this batch. Computed
    /// once in `begin` (needs `&Transaction` to query `is_owner_space`) and
    /// read by every row's `prepare_row`.
    pub(super) enforce_sigs: bool,
    /// Tables whose dev-mode CRDT-column auto-upgrade attempt (in `begin`)
    /// failed this batch. Every row of such a table must skip in
    /// `prepare_row` — though in practice the core's own structural schema
    /// check already classifies it as `MissingCrdtMetadata` for free, since
    /// the schema genuinely still lacks the columns. Also consulted directly
    /// in `before_commit` to write pending-table markers.
    pub(super) failed_upgrade_tables: HashSet<String>,
    /// `input_index -> (space_id, verified signature)` for every column
    /// `prepare_row` accepted with a verified signature this batch.
    /// `after_row` consumes this to call `upsert_column_sigs` for the
    /// columns that actually won and were written — `RowWrite`'s
    /// `WrittenColumn` carries the written value but not the verification
    /// outcome that produced it, so this is where that association survives
    /// between the two hooks.
    pub(super) verified_sigs: HashMap<usize, (String, SigRecord)>,
}

impl VaultApplyPolicy {
    pub(super) fn new(expected_space_id: Option<&str>, backend_info: Option<(&str, &str)>) -> Self {
        Self {
            expected_space_id: expected_space_id.map(str::to_string),
            backend_info: backend_info.map(|(id, hlc)| (id.to_string(), hlc.to_string())),
            enforce_sigs: false,
            failed_upgrade_tables: HashSet::new(),
            verified_sigs: HashMap::new(),
        }
    }
}

impl ApplyPolicy for VaultApplyPolicy {
    /// No additional whole-batch veto beyond what the core's own
    /// `preflight_batch` already runs (identifier safety, HLC validity and
    /// drift) — vault's former identifier-validation loop is fully redundant
    /// with that and has been deleted, not moved here.
    fn preflight(&mut self, _changes: &RemoteChanges) -> haex_crdt::Result<()> {
        Ok(())
    }

    fn begin(&mut self, tx: &Transaction<'_>, changes: &RemoteChanges) -> haex_crdt::Result<()> {
        // Whether per-column signatures are required on this batch. Owner-
        // vault sync between two devices of the same identity carries an
        // `expected_space_id` (the vault space id) but is intentionally
        // UNSIGNED on the write side — see the historical rationale kept on
        // the old call site (now here). Shared-space applies (non-owner
        // space) keep the strict gate: unsigned changes are dropped. When
        // there is no vault space at all, `is_owner_space` returns false, so
        // the safe default is "enforce" whenever an `expected_space_id` was
        // given.
        self.enforce_sigs = match self.expected_space_id.as_deref() {
            Some(sid) => !crate::owner_sync::scope::is_owner_space(tx, sid)?,
            None => false,
        };

        run_schema_auto_upgrade(tx, changes, &mut self.failed_upgrade_tables)
            .map_err(|e| haex_crdt::Error::Message(e.to_string()))?;

        Ok(())
    }

    fn prepare_row(
        &mut self,
        tx: &Transaction<'_>,
        row: RowInput<'_>,
    ) -> haex_crdt::Result<RowDecision> {
        // Reconstruct owned `RemoteColumnChange`s for the row's FULL change
        // group (not just `eligible_indices`) so the existing, unmodified
        // registry-gate and per-space-sig helpers — which all take
        // `&RemoteColumnChange` and haven't changed shape — keep working
        // verbatim. Built in the same order as `row.changes`, so an index
        // into one is the same index into the other.
        let full_row_changes: Vec<RemoteColumnChange> = row
            .changes
            .iter()
            .map(|ic| from_crate_change(ic.change))
            .collect();

        // Stage 5b — row-level registry-row-sig gate for
        // `haex_shared_space_sync`. Runs BEFORE the per-column sig gate
        // below: a bad row_sig drops this row's ENTIRE change set
        // atomically, unlike a per-column sig failure which only drops that
        // one column. Skipped when the local schema predates the `row_sig`
        // column — nothing to verify against.
        if row.table_name.eq_ignore_ascii_case(TABLE_SHARED_SPACE_SYNC)
            && row
                .schema
                .iter()
                .any(|c| c.name == COL_SHARED_SPACE_SYNC_ROW_SIG)
        {
            let pk_columns: Vec<&ColumnInfo> = row.schema.iter().filter(|c| c.is_pk).collect();
            let (pk_where_clause, pk_values_for_query) =
                build_pk_where_clause(&pk_columns, row.row_pks);
            let outcome = build_incoming_registry_change(
                tx,
                &pk_where_clause,
                &pk_values_for_query,
                row.row_pks,
                &full_row_changes,
            )
            .map_err(|e| haex_crdt::Error::Message(e.to_string()))?;
            match outcome {
                RegistryRowChangeOutcome::NothingSignedTouched => {}
                RegistryRowChangeOutcome::RowSigOnlyBatch {
                    space_id,
                    authored_by_did,
                } => {
                    eprintln!(
                        "[SYNC RUST] Rejected registry row {} in '{}' (space_id='{}', authored_by_did='{}') — batch touched ONLY row_sig with no signed-payload column; a bare row_sig cannot be verified and would let a stale-but-valid signature overwrite the persisted one (possible replay)",
                        row.row_pks_json, row.table_name, space_id, authored_by_did
                    );
                    return Ok(RowDecision::Skip);
                }
                RegistryRowChangeOutcome::MissingFreshRowSig(touched_signed_columns) => {
                    eprintln!(
                        "[SYNC RUST] Rejected registry row {} in '{}' — signed column(s) {:?} changed without a fresh row_sig in the same batch",
                        row.row_pks_json, row.table_name, touched_signed_columns
                    );
                    tx.execute(
                        &format!(
                            "INSERT OR IGNORE INTO {} (table_name, column_name, row_pks) VALUES (?, ?, ?)",
                            TABLE_CRDT_PENDING_COLUMNS
                        ),
                        params![row.table_name, COL_SHARED_SPACE_SYNC_ROW_SIG, row.row_pks_json],
                    )
                    .map_err(haex_crdt::Error::Sqlite)?;
                    return Ok(RowDecision::Skip);
                }
                RegistryRowChangeOutcome::RequiredFieldExplicitlyNull(null_columns) => {
                    eprintln!(
                        "[SYNC RUST] Rejected registry row {} in '{}' — required column(s) {:?} were explicitly set to null (never legitimate; dropping data in transit or forgery attempt)",
                        row.row_pks_json, row.table_name, null_columns
                    );
                    return Ok(RowDecision::Skip);
                }
                RegistryRowChangeOutcome::Ready { change, persisted } => {
                    if let Err(err) = verify_incoming_registry_change(&change, persisted.as_ref()) {
                        eprintln!(
                            "[SYNC RUST] Rejected registry row {} in '{}' (claimed authored_by_did='{}') — {:?}",
                            row.row_pks_json, row.table_name, change.authored_by_did, err
                        );
                        return Ok(RowDecision::Skip);
                    }
                }
            }
        }

        // Precompute the trustworthy space anchor once per row — only
        // needed when at least one change in the row carries a signature.
        let row_space_id_for_sig: Option<String> =
            if full_row_changes.iter().any(|c| c.sig.is_some()) {
                let pk_columns: Vec<&ColumnInfo> = row.schema.iter().filter(|c| c.is_pk).collect();
                let (pk_where_clause, pk_values_for_query) =
                    build_pk_where_clause(&pk_columns, row.row_pks);
                resolve_row_space_id_for_sig(
                    tx,
                    row.table_name,
                    &pk_where_clause,
                    &pk_values_for_query,
                    &full_row_changes,
                    row.schema,
                    self.expected_space_id.as_deref(),
                )
                .map_err(|e| haex_crdt::Error::Message(e.to_string()))?
            } else {
                None
            };

        let mut decisions = Vec::with_capacity(row.eligible_indices.len());
        for &idx in row.eligible_indices {
            let change = &full_row_changes[idx];
            let input_index = row.changes[idx].input_index;

            // Shared-space applies fail closed on missing signatures.
            // `authored_by_did` is legacy leader-attributed metadata, not
            // authoritative authorship; it remains the sole unsigned
            // compatibility column until the schema drops it. Owner-space
            // applies (`enforce_sigs == false`) skip this gate — signed
            // changes still verify below regardless of the flag, so there
            // is no downgrade path from signed to unsigned on that route.
            if self.enforce_sigs && change.sig.is_none() && change.column_name != "authored_by_did"
            {
                eprintln!(
                    "[SYNC RUST] Dropping unsigned shared-space change on {}.{}",
                    row.table_name, change.column_name
                );
                decisions.push(ColumnDecision::Skip);
                continue;
            }

            let value = match &change.sig {
                Some(sig) => {
                    match verify_change_sig(
                        change,
                        sig,
                        row_space_id_for_sig.as_deref(),
                        row.table_name,
                        row.row_pks_json,
                    ) {
                        Ok(()) => {
                            ensure_identity_stub(tx, &sig.author_did)
                                .map_err(|e| haex_crdt::Error::Message(e.to_string()))?;
                            let sql_value = sig
                                .storage_class
                                .restore(&change.decrypted_value)
                                .map_err(haex_crdt::Error::Message)?;
                            let sig_bytes_vec = BASE64.decode(&sig.sig).map_err(|e| {
                                haex_crdt::Error::Message(format!(
                                    "verified signature stopped decoding: {e}"
                                ))
                            })?;
                            let sig_bytes: [u8; 64] = sig_bytes_vec.try_into().map_err(|_| {
                                haex_crdt::Error::Message(
                                    "verified signature has wrong length".to_string(),
                                )
                            })?;
                            // `row_space_id_for_sig` must be `Some` here —
                            // `verify_change_sig` errors on `None` (space_id
                            // unavailable), so a successful verify implies it.
                            if let Some(space_id) = row_space_id_for_sig.clone() {
                                self.verified_sigs.insert(
                                    input_index,
                                    (
                                        space_id,
                                        SigRecord {
                                            author_did: sig.author_did.clone(),
                                            sig: sig_bytes,
                                            storage_class: sig.storage_class,
                                        },
                                    ),
                                );
                            }
                            sql_value
                        }
                        Err(reason) => {
                            eprintln!(
                                "[SYNC RUST] Dropping change with invalid sig on {}.{}: {}",
                                row.table_name, change.column_name, reason
                            );
                            decisions.push(ColumnDecision::Skip);
                            continue;
                        }
                    }
                }
                None => ValueConverter::json_to_rusqlite_value(&change.decrypted_value)
                    .map_err(|e| haex_crdt::Error::Message(e.to_string()))?,
            };

            decisions.push(ColumnDecision::Accept {
                value,
                signature: SignatureWrite::Keep,
            });
        }
        Ok(RowDecision::Columns(decisions))
    }

    fn after_row(&mut self, tx: &Transaction<'_>, written: RowWrite<'_>) -> haex_crdt::Result<()> {
        // Only for columns that actually carried a verified signature this
        // batch — `upsert_column_sigs` unchanged from before this cutover.
        for col in written.columns {
            if let Some((space_id, sig)) = self.verified_sigs.get(&col.input_index) {
                upsert_column_sigs(
                    tx,
                    written.table_name,
                    written.row_pks_json,
                    &col.change.column_name,
                    space_id,
                    sig,
                )
                .map_err(|e| haex_crdt::Error::Message(e.to_string()))?;
            }
        }
        Ok(())
    }

    /// Only called for a genuine NOT NULL or UNIQUE INSERT failure, after
    /// that INSERT's savepoint has already been rolled back (per the trait
    /// doc). NOT NULL: a partial change set can never satisfy the NOT NULL
    /// columns — self-heals on a later full re-pull. UNIQUE: record a
    /// conflict entry from the attempted values before skipping the row.
    fn on_insert_constraint(
        &mut self,
        tx: &Transaction<'_>,
        attempted: RowWrite<'_>,
        error: &rusqlite::Error,
    ) -> haex_crdt::Result<ConstraintDecision> {
        let is_not_null = matches!(
            error,
            rusqlite::Error::SqliteFailure(e, _)
                if e.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_NOTNULL
        );
        if is_not_null {
            eprintln!(
                "[SYNC RUST] Skipping row in '{}' — partial change set cannot satisfy NOT NULL columns (incomplete sync data)",
                attempted.table_name
            );
            return Ok(ConstraintDecision::SkipRow);
        }

        // UNIQUE (the only other kind the crate promises to call this hook
        // for) — build the attempted row's data from its PKs plus every
        // staged column, and record a conflict entry.
        let error_msg = match error {
            rusqlite::Error::SqliteFailure(_, msg) => {
                msg.as_deref().unwrap_or("Unknown constraint violation")
            }
            _ => "Unknown constraint violation",
        };
        eprintln!("[SYNC RUST] UNIQUE constraint conflict - creating conflict entry");

        let mut remote_row_data: serde_json::Map<String, serde_json::Value> =
            serde_json::Map::new();
        for (k, v) in attempted.row_pks {
            remote_row_data.insert(k.clone(), v.clone());
        }
        for col in attempted.columns {
            let json_value = ValueConverter::rusqlite_value_to_json(col.value);
            remote_row_data.insert(col.change.column_name.clone(), json_value);
        }

        if let Err(e) = create_conflict_entry(
            tx,
            attempted.table_name,
            error_msg,
            &remote_row_data,
            attempted.row_hlc,
            attempted.schema,
        ) {
            eprintln!("[SYNC RUST] Failed to create conflict entry: {:?}", e);
        }

        Ok(ConstraintDecision::SkipRow)
    }

    fn before_commit(
        &mut self,
        tx: &Transaction<'_>,
        changes: &RemoteChanges,
        outcome: &ApplyOutcome,
    ) -> haex_crdt::Result<()> {
        write_pending_table_markers(tx, changes, outcome, &self.failed_upgrade_tables)
            .map_err(|e| haex_crdt::Error::Message(e.to_string()))?;
        write_pending_column_markers(tx, changes, outcome)
            .map_err(|e| haex_crdt::Error::Message(e.to_string()))?;

        // Owner-domain delete-log propagation is the crate's job now,
        // already run before `before_commit`. The per-space delete-log
        // stays entirely vault's (D-3: the crate never learns what a space
        // is).
        propagate_shared_space_deletes(tx, changes, outcome)
            .map_err(|e| haex_crdt::Error::Message(e.to_string()))?;

        update_backend_cursor(
            tx,
            self.backend_info
                .as_ref()
                .map(|(id, hlc)| (id.as_str(), hlc.as_str())),
        )
        .map_err(|e| haex_crdt::Error::Message(e.to_string()))?;

        Ok(())
    }
}
