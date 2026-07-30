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
    for change in &changes {
        if !is_space_scoped_table(&change.table_name) {
            // Fallback: extension-owned content tables aren't on the static
            // whitelist; they're registered per-space via
            // `haex_shared_space_sync`. Accept iff the exact
            // (table, row_pks, space_id) triple is registered.
            let lookup = with_connection(db, |conn| {
                is_registered_for_space(conn, &change.table_name, &change.row_pks, space_id)
            });
            match lookup {
                Ok(true) => {} // allowed via registry
                Ok(false) => {
                    return InboundSyncPushOutcome::Rejected {
                        reason: format!(
                            "Table {} row {} is not allowed in space-scoped sync (not whitelisted, not registered for space {})",
                            change.table_name, change.row_pks, space_id
                        ),
                    };
                }
                Err(e) => {
                    // Fail-CLOSED. A DB error MUST NOT collapse into
                    // accept ("permissive on error" bypass), and it MUST
                    // NOT be silently swallowed as an unregistered-reject
                    // either — surface the underlying failure in the
                    // reason so operators see why we rejected.
                    return InboundSyncPushOutcome::Rejected {
                        reason: format!(
                            "Registry lookup failed for {} row {}: {}",
                            change.table_name, change.row_pks, e
                        ),
                    };
                }
            }
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
