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

use haex_crdt::{ApplyOutcome, ApplyPolicy, ColumnDecision, RemoteChanges, RowDecision, RowInput};
use rusqlite::Transaction;

use super::schema_recovery::{run_schema_auto_upgrade, write_pending_table_markers};
use crate::crdt::column_sig::storage::SigRecord;

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

    /// Placeholder — real admission/decoding logic lands in Task 3. Not yet
    /// wired into any live call site (`apply_remote_changes_to_db_scoped`
    /// still runs the old hand-rolled loop until Task 7's cutover), so this
    /// body is never actually exercised yet.
    fn prepare_row(&mut self, _tx: &Transaction<'_>, row: RowInput<'_>) -> haex_crdt::Result<RowDecision> {
        Ok(RowDecision::Columns(
            row.eligible_indices
                .iter()
                .map(|_| ColumnDecision::Skip)
                .collect(),
        ))
    }

    fn before_commit(
        &mut self,
        tx: &Transaction<'_>,
        changes: &RemoteChanges,
        outcome: &ApplyOutcome,
    ) -> haex_crdt::Result<()> {
        write_pending_table_markers(tx, changes, outcome, &self.failed_upgrade_tables)
            .map_err(|e| haex_crdt::Error::Message(e.to_string()))?;
        Ok(())
    }
}
