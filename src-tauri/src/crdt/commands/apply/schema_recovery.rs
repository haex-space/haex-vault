//! Dev-mode CRDT-column/-trigger auto-upgrade and the pending-table recovery
//! marker, run from [`super::policy::VaultApplyPolicy`]'s `begin` and
//! `before_commit` hooks.
//!
//! Hoisted out of the old per-row loop: `begin` sees the whole batch, so the
//! upgrade attempt now runs once per distinct table touched by the batch
//! rather than once per row.

use std::collections::HashSet;

use haex_crdt::{ApplyOutcome, RemoteChanges, SkipReason};
use rusqlite::{params, Transaction};

use crate::crdt::shared_space_trigger::{
    ensure_crdt_columns, get_table_schema, install_crdt_with_shared_space, TriggerSetupResult,
    COLUMN_HLCS_COLUMN, COLUMN_SIGS_COLUMN, HLC_TIMESTAMP_COLUMN,
};
use crate::database::error::DatabaseError;
use crate::table_names::TABLE_CRDT_PENDING_TABLES;

/// For every distinct table named in `changes`, ensure it carries the CRDT
/// metadata columns (and, if it had none of them at all, the generic +
/// shared-space triggers). A table that doesn't exist at all is left alone —
/// there is nothing to upgrade, and the core's own structural check
/// classifies it as `MissingTable` on its own.
///
/// A table whose upgrade attempt fails is recorded in
/// `failed_upgrade_tables`; the core will separately (and correctly) classify
/// every row of that table as `MissingCrdtMetadata`, since the schema still
/// lacks the columns after the failed attempt — no extra skip logic is
/// needed in `prepare_row` for this case.
pub(super) fn run_schema_auto_upgrade(
    tx: &Transaction,
    changes: &RemoteChanges,
    failed_upgrade_tables: &mut HashSet<String>,
) -> Result<(), DatabaseError> {
    let mut seen: HashSet<&str> = HashSet::new();
    for change in changes {
        if !seen.insert(change.table_name.as_str()) {
            continue;
        }
        let table_name = change.table_name.as_str();

        let schema = get_table_schema(tx, table_name).map_err(DatabaseError::from)?;
        if schema.is_empty() {
            // Table doesn't exist — not this hook's problem to fix.
            continue;
        }

        let has_core_crdt_columns = schema.iter().any(|c| c.name == HLC_TIMESTAMP_COLUMN)
            && schema.iter().any(|c| c.name == COLUMN_HLCS_COLUMN);
        let has_column_sigs = schema.iter().any(|c| c.name == COLUMN_SIGS_COLUMN);
        if has_core_crdt_columns && has_column_sigs {
            continue;
        }

        eprintln!(
            "[SYNC RUST] Table '{table_name}' missing CRDT columns (created in dev mode?) - upgrading now"
        );
        let upgrade = ensure_crdt_columns(tx, table_name).and_then(|columns_added| {
            // Adding only the signature metadata column to an existing CRDT
            // table does not require trigger recreation. This also keeps
            // minimal test/dev schemas from acquiring triggers whose support
            // tables they intentionally omit.
            if has_core_crdt_columns {
                Ok((columns_added, false))
            } else {
                install_crdt_with_shared_space(tx, table_name, true).map(|result| {
                    let triggers_created = matches!(result, TriggerSetupResult::Success);
                    (columns_added, triggers_created)
                })
            }
        });
        match upgrade {
            Ok((columns_added, triggers_created)) => {
                eprintln!(
                    "[SYNC RUST] Upgraded '{table_name}': columns={columns_added}, triggers={triggers_created}"
                );
            }
            Err(e) => {
                eprintln!(
                    "[SYNC RUST] Failed to upgrade '{table_name}': {e} - skipping this table"
                );
                failed_upgrade_tables.insert(table_name.to_string());
            }
        }
    }
    Ok(())
}

/// Single `before_commit` pass writing one `INSERT OR IGNORE` pending-table
/// marker per distinct table that either the core classified as
/// `MissingTable`/`MissingCrdtMetadata` this batch, or whose `begin`-phase
/// upgrade attempt failed (known directly, without consulting `outcome`).
pub(super) fn write_pending_table_markers(
    tx: &Transaction,
    changes: &RemoteChanges,
    outcome: &ApplyOutcome,
    failed_upgrade_tables: &HashSet<String>,
) -> Result<(), DatabaseError> {
    let mut tables: HashSet<&str> = HashSet::new();
    for skipped in &outcome.skipped {
        if matches!(
            skipped.reason,
            SkipReason::MissingTable | SkipReason::MissingCrdtMetadata
        ) {
            tables.insert(changes[skipped.input_index].table_name.as_str());
        }
    }
    for table_name in failed_upgrade_tables {
        tables.insert(table_name.as_str());
    }

    for table_name in tables {
        tx.execute(
            &format!("INSERT OR IGNORE INTO {TABLE_CRDT_PENDING_TABLES} (table_name) VALUES (?)"),
            params![table_name],
        )
        .map_err(DatabaseError::from)?;
    }
    Ok(())
}
