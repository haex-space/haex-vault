//! Test-only Tauri commands, gated behind the `e2e-hooks` Cargo feature.
//!
//! Used by haex-e2e-tests companion specs to construct multi-peer adversarial
//! scenarios (forged shared-space delete signals, cross-space suppression
//! attacks) that a legit peer cannot emit through the normal UI or sync path.
//! NEVER compile into release binaries shipped to end users — these commands
//! bypass the wire-side authorization pipeline by design so specs can drive
//! the apply-side invariants (positive register-gate, resurrection check,
//! residual-register handling) in isolation.

use std::collections::HashSet;

use rusqlite::{params, Transaction};
use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;
use uuid::Uuid;

use super::grouping::build_pk_where_from_map;
use crate::crdt::trigger::{SHARED_SPACE_DELETED_ROWS_TABLE, SHARED_SPACE_SYNC_TABLE};
use crate::database::core::with_connection;
use crate::database::error::DatabaseError;
use crate::database::DbConnection;
use crate::AppState;

/// Snapshot of the observable state around a `(space_id, table_name, row)`
/// tuple. Captured before and after the seeded propagation runs so the caller
/// can derive what happened without instrumenting the propagation code path.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TestSharedSpaceRowSnapshot {
    /// Row currently registered for `(table, row_pks, space_id)` in
    /// `haex_shared_space_sync`.
    pub target_space_registered: bool,
    /// Row registered for `(table, row_pks)` in ANY space.
    pub any_space_registered: bool,
    /// Business row present in `table_name` for `row_pks`.
    pub business_row_exists: bool,
}

/// Derived outcome of the propagation, if it ran.
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TestPropagationOutcome {
    /// `run_propagation = false` — the delete-log entry was seeded but the
    /// propagation function was not called. Before and after snapshots are
    /// identical.
    NotRun,
    /// Positive gate cleared, register-DELETE fired, residual-register check
    /// showed no other space still lists the row, so the business-row DELETE
    /// also fired. Row + register both gone.
    AppliedFullDelete,
    /// Positive gate cleared for `space_id`, register-DELETE fired, but the
    /// row is still registered in at least one other space so the business
    /// row was kept.
    AppliedRegisterOnly,
    /// The target space had no register entry for the row, but another space
    /// did — `NotSharedInSpace` (suspected forgery). No changes.
    NotSharedInSpaceForgery,
    /// The target space had no register entry, and no other space did either
    /// — treated as unshare-race. No changes.
    UnshareRace,
    /// The business row's local `haex_hlc` was newer than the delete-log
    /// entry's `haex_hlc`. Propagation short-circuits before the register
    /// gate. No changes.
    ResurrectionSuppressed,
    /// Fallback for a state transition the derivation logic does not
    /// recognise. Included so specs can fail loudly rather than accept a
    /// wrong-shape apply.
    Unknown,
}

/// Report returned by [`test_seed_shared_space_delete_log_entry`].
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TestSeedDeleteLogReport {
    /// Randomly generated UUID assigned to the synthesised delete-log entry.
    /// Present in `haex_shared_space_deleted_rows` after this call returns
    /// regardless of propagation outcome.
    pub delete_log_id: String,
    /// Mirrors the `run_propagation` argument.
    pub propagated: bool,
    /// Derived from the before/after snapshot deltas.
    pub outcome: TestPropagationOutcome,
    pub before: TestSharedSpaceRowSnapshot,
    pub after: TestSharedSpaceRowSnapshot,
}

fn probe_state(
    tx: &Transaction<'_>,
    table_name: &str,
    row_pks_map: &serde_json::Map<String, serde_json::Value>,
    row_pks_json: &str,
    space_id: &str,
) -> Result<TestSharedSpaceRowSnapshot, DatabaseError> {
    let target_space_registered = tx
        .query_row(
            &format!(
                "SELECT 1 FROM \"{SHARED_SPACE_SYNC_TABLE}\" \
                 WHERE table_name = ?1 AND row_pks = ?2 AND space_id = ?3 LIMIT 1"
            ),
            params![table_name, row_pks_json, space_id],
            |_| Ok(true),
        )
        .optional_bool()?;

    let any_space_registered = tx
        .query_row(
            &format!(
                "SELECT 1 FROM \"{SHARED_SPACE_SYNC_TABLE}\" \
                 WHERE table_name = ?1 AND row_pks = ?2 LIMIT 1"
            ),
            params![table_name, row_pks_json],
            |_| Ok(true),
        )
        .optional_bool()?;

    let business_row_exists = match build_pk_where_from_map(row_pks_map) {
        Some((where_clause, values)) => {
            let sql_params = crate::crdt::commands::helpers::json_values_to_sql_params(&values)?;
            let param_refs: Vec<&dyn rusqlite::ToSql> = sql_params
                .iter()
                .map(|v| v as &dyn rusqlite::ToSql)
                .collect();
            tx.query_row(
                &format!("SELECT 1 FROM \"{table_name}\" WHERE {where_clause} LIMIT 1"),
                param_refs.as_slice(),
                |_| Ok(true),
            )
            .optional_bool()?
        }
        None => false,
    };

    Ok(TestSharedSpaceRowSnapshot {
        target_space_registered,
        any_space_registered,
        business_row_exists,
    })
}

trait OptionalBool<T> {
    fn optional_bool(self) -> Result<bool, DatabaseError>;
}

impl OptionalBool<bool> for Result<bool, rusqlite::Error> {
    fn optional_bool(self) -> Result<bool, DatabaseError> {
        match self {
            Ok(_) => Ok(true),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
            Err(e) => Err(DatabaseError::from(e)),
        }
    }
}

fn derive_outcome(
    run_propagation: bool,
    before: &TestSharedSpaceRowSnapshot,
    after: &TestSharedSpaceRowSnapshot,
) -> TestPropagationOutcome {
    if !run_propagation {
        return TestPropagationOutcome::NotRun;
    }

    // Register gate paths — no state changes on register or business row.
    if !before.target_space_registered
        && before.any_space_registered
        && !after.target_space_registered
        && after.any_space_registered
        && before.business_row_exists == after.business_row_exists
    {
        return TestPropagationOutcome::NotSharedInSpaceForgery;
    }
    if !before.target_space_registered
        && !before.any_space_registered
        && !after.target_space_registered
        && !after.any_space_registered
        && before.business_row_exists == after.business_row_exists
    {
        return TestPropagationOutcome::UnshareRace;
    }

    // Resurrection: registered before, but nothing was cleaned up.
    if before.target_space_registered
        && after.target_space_registered
        && before.business_row_exists == after.business_row_exists
    {
        return TestPropagationOutcome::ResurrectionSuppressed;
    }

    // Positive gate cleared: register removed for target space.
    if before.target_space_registered && !after.target_space_registered {
        if after.any_space_registered {
            // Register elsewhere kept the business row alive.
            if before.business_row_exists && after.business_row_exists {
                return TestPropagationOutcome::AppliedRegisterOnly;
            }
        } else if before.business_row_exists && !after.business_row_exists {
            return TestPropagationOutcome::AppliedFullDelete;
        }
    }

    TestPropagationOutcome::Unknown
}

/// Impl for [`test_seed_shared_space_delete_log_entry`]. Split out so the
/// Rust unit tests can exercise the seeding + snapshot logic against an
/// in-memory `Connection` without the Tauri harness.
///
/// `row_pks_json` MUST be canonical JSON matching the encoding produced by
/// the receiver-side register-cascade trigger (PK-definition order, no
/// whitespace) — otherwise the propagation function will silently no-op the
/// register match.
pub(crate) fn seed_shared_space_delete_log_entry_impl(
    db: &DbConnection,
    space_id: &str,
    table_name: &str,
    row_pks_json: &str,
    hlc: &str,
    run_propagation: bool,
) -> Result<TestSeedDeleteLogReport, DatabaseError> {
    let row_pks_map: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(row_pks_json).map_err(|e| DatabaseError::ExecutionError {
            sql: format!("<test seed row_pks_json={row_pks_json}>"),
            reason: format!("invalid row_pks JSON: {e}"),
            table: Some(table_name.to_string()),
        })?;

    let delete_log_id = Uuid::new_v4().to_string();

    with_connection(db, |conn| {
        let tx = conn.transaction().map_err(DatabaseError::from)?;

        let before = probe_state(&tx, table_name, &row_pks_map, row_pks_json, space_id)?;

        tx.execute(
            &format!(
                "INSERT INTO \"{SHARED_SPACE_DELETED_ROWS_TABLE}\" \
                 (id, space_id, table_name, row_pks, haex_hlc) VALUES (?, ?, ?, ?, ?)"
            ),
            params![&delete_log_id, space_id, table_name, row_pks_json, hlc],
        )
        .map_err(DatabaseError::from)?;

        if run_propagation {
            let mut ids: HashSet<String> = HashSet::new();
            ids.insert(delete_log_id.clone());
            super::delete_propagation::propagate_shared_space_deleted_rows_to_target_tables(
                &tx, &ids,
            )?;
        }

        let after = probe_state(&tx, table_name, &row_pks_map, row_pks_json, space_id)?;

        tx.commit().map_err(DatabaseError::from)?;

        Ok(TestSeedDeleteLogReport {
            delete_log_id: delete_log_id.clone(),
            propagated: run_propagation,
            outcome: derive_outcome(run_propagation, &before, &after),
            before,
            after,
        })
    })
}

/// Test-only Tauri command that injects a synthetic
/// `haex_shared_space_deleted_rows` entry into the local vault AS IF received
/// from a remote peer, and optionally runs
/// `propagate_shared_space_deleted_rows_to_target_tables` against it. See the
/// module-level docs for the security posture.
#[tauri::command(rename_all = "camelCase")]
pub fn test_seed_shared_space_delete_log_entry(
    space_id: String,
    table_name: String,
    row_pks_json: String,
    hlc: String,
    run_propagation: bool,
    state: State<'_, AppState>,
) -> Result<TestSeedDeleteLogReport, DatabaseError> {
    seed_shared_space_delete_log_entry_impl(
        &state.db,
        &space_id,
        &table_name,
        &row_pks_json,
        &hlc,
        run_propagation,
    )
}
