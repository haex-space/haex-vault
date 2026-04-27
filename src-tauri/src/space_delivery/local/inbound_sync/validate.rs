//! Pure-transform validation of inbound CRDT pushes: table whitelist,
//! `space_id` column scoping, and `authored_by_did` strip + re-injection
//! from the validated UCAN audience.
//!
//! No DB access. The caller (`authorize_inbound_sync_push`) chains this
//! before the row-scope and ownership gates, which then talk to the DB.

use std::collections::HashMap;

use serde_json::Value as JsonValue;

use crate::crdt::hlc::hlc_is_newer;
use crate::crdt::scanner::{is_space_scoped_table, LocalColumnChange};

use super::InboundSyncPushOutcome;

/// Validate, scope-check, and attribute an inbound SyncPush batch.
///
/// See the module doc-comment for the contract. The `ucan_audience` is
/// expected to be the validated UCAN audience for the request — i.e. the
/// Space-Member-DID the leader already confirmed is an active member of
/// `space_id` via the membership check.
pub fn validate_and_attribute(
    space_id: &str,
    ucan_audience: &str,
    changes: Vec<LocalColumnChange>,
) -> InboundSyncPushOutcome {
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
        });
    }

    InboundSyncPushOutcome::Accepted { changes: stripped }
}
