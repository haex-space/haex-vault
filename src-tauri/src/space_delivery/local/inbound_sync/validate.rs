//! Validation of inbound CRDT pushes: table whitelist / registry, the
//! `space_id` column scoping, and the `authored_by_did` strip +
//! re-injection from the validated UCAN audience.
//!
//! The attribution and column-level checks are pure transforms. The
//! table-level scope check invokes one DB read via
//! [`crate::crdt::scanner::is_registered_for_space`] — this module does not
//! define the read itself, it consults `haex_shared_space_sync` through
//! that shared helper (which Task 4's outbound scanner also uses).

use std::collections::HashMap;

use serde_json::Value as JsonValue;

use crate::crdt::hlc::hlc_is_newer;
use crate::crdt::scanner::{is_registered_for_space, is_space_scoped_table, LocalColumnChange};
use crate::database::core::with_connection;
use crate::database::error::DatabaseError;
use crate::database::DbConnection;

use super::InboundSyncPushOutcome;

/// Validate, scope-check, and attribute an inbound SyncPush batch.
///
/// See the module doc-comment for the contract. The `ucan_audience` is
/// expected to be the validated UCAN audience for the request — i.e. the
/// Space-Member-DID the leader already confirmed is an active member of
/// `space_id` via the membership check.
///
/// A change is table-scope-accepted iff its table is on the static
/// [`SPACE_SCOPED_CRDT_TABLES`][crate::crdt::scanner::SPACE_SCOPED_CRDT_TABLES]
/// whitelist OR the `(table_name, row_pks, space_id)` triple is registered
/// in `haex_shared_space_sync` (extension-owned content tables). The
/// registry lookup fails CLOSED — a DB error on the lookup rejects the
/// batch rather than accepting it as unregistered.
pub fn validate_and_attribute(
    db: &DbConnection,
    space_id: &str,
    ucan_audience: &str,
    changes: Vec<LocalColumnChange>,
) -> InboundSyncPushOutcome {
    // --- (1) + (2): whitelist / registry and space_id scope --------------
    //
    // Registration is a per-`(table_name, row_pks)` property — every
    // column change on the same row shares the same registry answer.
    // Acquire the DB connection once for the whole batch and memoize
    // lookups so a batch of N column changes on M unique rows does M
    // (rather than N) `SELECT`s against `haex_shared_space_sync`.
    //
    // A DB error is threaded out as `Err` and turned into a fail-CLOSED
    // `Rejected` below — it must not collapse into accept and must not
    // be silently swallowed as unregistered-reject.
    enum ScopeReject {
        NotRegistered {
            table: String,
            row_pks: String,
        },
        SpaceIdMismatch {
            table: String,
            inbound: JsonValue,
        },
        LookupError {
            table: String,
            row_pks: String,
            error: DatabaseError,
        },
    }

    let scope_result: Result<Result<(), ScopeReject>, DatabaseError> =
        with_connection(db, |conn| {
            let mut seen: HashMap<(&str, &str), bool> = HashMap::new();
            for change in &changes {
                if !is_space_scoped_table(&change.table_name) {
                    // Fallback: extension-owned content tables aren't on the
                    // static whitelist; they're registered per-space via
                    // `haex_shared_space_sync`. Accept iff the exact
                    // (table, row_pks, space_id) triple is registered.
                    let key = (change.table_name.as_str(), change.row_pks.as_str());
                    let registered = match seen.get(&key) {
                        Some(v) => *v,
                        None => match is_registered_for_space(conn, key.0, key.1, space_id) {
                            Ok(v) => {
                                seen.insert(key, v);
                                v
                            }
                            Err(e) => {
                                return Ok(Err(ScopeReject::LookupError {
                                    table: change.table_name.clone(),
                                    row_pks: change.row_pks.clone(),
                                    error: e,
                                }));
                            }
                        },
                    };
                    if !registered {
                        return Ok(Err(ScopeReject::NotRegistered {
                            table: change.table_name.clone(),
                            row_pks: change.row_pks.clone(),
                        }));
                    }
                }

                if change.column_name == "space_id" {
                    let inbound = change.value.as_str();
                    if inbound != Some(space_id) {
                        return Ok(Err(ScopeReject::SpaceIdMismatch {
                            table: change.table_name.clone(),
                            inbound: change.value.clone(),
                        }));
                    }
                }
            }
            Ok(Ok(()))
        });

    match scope_result {
        Ok(Ok(())) => {}
        Ok(Err(ScopeReject::NotRegistered { table, row_pks })) => {
            return InboundSyncPushOutcome::Rejected {
                reason: format!(
                    "Table {table} row {row_pks} is not allowed in space-scoped sync (not whitelisted, not registered for space {space_id})",
                ),
            };
        }
        Ok(Err(ScopeReject::SpaceIdMismatch { table, inbound })) => {
            return InboundSyncPushOutcome::Rejected {
                reason: format!(
                    "Row in {table} sets space_id={inbound:?} but request is for {space_id}",
                ),
            };
        }
        Ok(Err(ScopeReject::LookupError {
            table,
            row_pks,
            error,
        })) => {
            return InboundSyncPushOutcome::Rejected {
                reason: format!("Registry lookup failed for {table} row {row_pks}: {error}",),
            };
        }
        Err(e) => {
            // DB / lock error acquiring the connection itself — also
            // fail-CLOSED so `Rejected` is the only path a caller sees
            // when the check cannot be performed.
            return InboundSyncPushOutcome::Rejected {
                reason: format!("Registry lookup failed (connection): {e}"),
            };
        }
    }

    // --- (3): strip client-supplied authored_by_did ----------------------
    let mut stripped: Vec<LocalColumnChange> = changes
        .into_iter()
        .filter(|c| c.column_name != "authored_by_did")
        .collect();

    // --- (3): collect max HLC + device_id per unique (table, row) --------
    // The injected authored_by_did carries the max HLC seen in its row-
    // group so the CRDT merge treats it as the most recent authoritative
    // write for the column. Using the row's own device_id keeps the
    // scanner's (table, row, column, device)-dedup intact.
    let mut per_row: HashMap<(String, String), (String, String)> = HashMap::new();
    for change in &stripped {
        let key = (change.table_name.clone(), change.row_pks.clone());
        per_row
            .entry(key)
            .and_modify(|(hlc, device_id): &mut (String, String)| {
                if hlc_is_newer(&change.hlc_timestamp, hlc) {
                    *hlc = change.hlc_timestamp.clone();
                    *device_id = change.device_id.clone();
                }
            })
            .or_insert((change.hlc_timestamp.clone(), change.device_id.clone()));
    }

    // --- (3): inject exactly one authored_by_did per unique row ----------
    for ((table_name, row_pks), (hlc, device_id)) in per_row {
        stripped.push(LocalColumnChange {
            table_name,
            row_pks,
            column_name: "authored_by_did".to_string(),
            hlc_timestamp: hlc,
            value: JsonValue::String(ucan_audience.to_string()),
            device_id,
            sig: None,
        });
    }

    InboundSyncPushOutcome::Accepted { changes: stripped }
}
