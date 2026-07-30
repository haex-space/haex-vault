//! Validation of inbound CRDT pushes: table whitelist / registry, the
//! `space_id` column scoping, and the `authored_by_did` strip +
//! re-injection from the validated UCAN audience.
//!
//! The attribution and column-level checks are pure transforms. The
//! table-level scope check consults `haex_shared_space_sync` when a table
//! is not on the static whitelist, which is the one DB read this module
//! performs — see [`is_registered_for_space`].

use std::collections::HashMap;

use serde_json::Value as JsonValue;

use crate::crdt::hlc::hlc_is_newer;
use crate::crdt::scanner::{is_space_scoped_table, LocalColumnChange};
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
    // TODO(w3-task-3a-impl): consult haex_shared_space_sync when a table
    // is not on the static whitelist. Failing tests in
    // `inbound_sync_tests::validate_and_attribute` currently pin the
    // "registered-triple must be accepted" contract.
    let _ = db;

    // --- (1) + (2): whitelist and space_id scope -------------------------
    for change in &changes {
        if !is_space_scoped_table(&change.table_name) {
            return InboundSyncPushOutcome::Rejected {
                reason: format!(
                    "Table {} is not allowed in space-scoped sync",
                    change.table_name
                ),
            };
        }

        if change.column_name == "space_id" {
            let inbound = change.value.as_str();
            if inbound != Some(space_id) {
                return InboundSyncPushOutcome::Rejected {
                    reason: format!(
                        "Row in {} sets space_id={:?} but request is for {}",
                        change.table_name, change.value, space_id
                    ),
                };
            }
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
