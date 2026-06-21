//! Pre-flight filtering of membership-table rows the leader wrote on others'
//! behalf, which this device must not push (they'd fail the server's per-row
//! ownership check and stall the push cursor).

use std::collections::HashSet;

use crate::crdt::scanner::LocalColumnChange;
use crate::database::DbConnection;

/// Separate `changes` into rows this device may push and rows it must skip.
///
/// Returns `(pushable, foreign_max_hlc)`:
/// - `pushable` contains all changes except membership-table rows owned by
///   another identity or endpoint.
/// - `foreign_max_hlc` is the max HLC of any skipped row, so the push cursor
///   can be advanced past rows that will never be pushable.
///
/// Background: when this device acts as leader it writes `haex_space_members`
/// rows for newly joined members (ClaimInvite) and `haex_space_devices` rows
/// for announcing peers. Those rows get the leader's HLC node, so they pass
/// the push-scanner origin filter but fail the server's per-row ownership
/// check. This function drops them pre-flight.
pub(super) fn filter_foreign_membership_rows(
    db: &DbConnection,
    space_id: &str,
    changes: Vec<LocalColumnChange>,
    our_identity_id: Option<&str>,
    our_endpoint_id: &str,
) -> (Vec<LocalColumnChange>, Option<String>) {
    // Collect the row IDs we actually own for the two checked tables.
    let owned_member_ids: HashSet<String> = match our_identity_id {
        Some(identity_id) => query_owned_row_ids(
            db,
            "SELECT id FROM haex_space_members WHERE space_id = ?1 AND identity_id = ?2",
            space_id,
            identity_id,
        ),
        // Unknown identity → can't filter → treat all as owned (safe fallback).
        None => HashSet::new(),
    };

    let owned_device_ids: HashSet<String> = query_owned_row_ids(
        db,
        "SELECT id FROM haex_space_devices WHERE space_id = ?1 AND endpoint_id = ?2",
        space_id,
        our_endpoint_id,
    );

    // Single pass: check ownership per column change against the pre-fetched
    // owned-id sets. Log each foreign row once (deduplicated by row identity).
    let mut pushable: Vec<LocalColumnChange> = Vec::new();
    let mut foreign_max_hlc: Option<String> = None;
    let mut logged_foreign: HashSet<(String, String)> = HashSet::new();

    for change in changes {
        let owned = match change.table_name.as_str() {
            "haex_space_members" => {
                if our_identity_id.is_none() {
                    true // identity unknown → can't filter → pass through
                } else {
                    extract_pk_id(&change.row_pks)
                        .map(|id| owned_member_ids.contains(&id))
                        .unwrap_or(true) // parse failure → don't silently drop
                }
            }
            "haex_space_devices" => extract_pk_id(&change.row_pks)
                .map(|id| owned_device_ids.contains(&id))
                .unwrap_or(true),
            _ => true,
        };

        if owned {
            pushable.push(change);
        } else {
            let row_key = (change.table_name.clone(), change.row_pks.clone());
            if logged_foreign.insert(row_key) {
                eprintln!(
                    "[SyncLoop] Skipping foreign-owned row {}/{} (not owned by this device)",
                    change.table_name, change.row_pks,
                );
            }
            if foreign_max_hlc.as_deref().map_or(true, |cur| {
                crate::crdt::hlc::hlc_is_newer(&change.hlc_timestamp, cur)
            }) {
                foreign_max_hlc = Some(change.hlc_timestamp);
            }
        }
    }

    (pushable, foreign_max_hlc)
}

/// Run a SQL query of the form `SELECT id FROM <table> WHERE space_id = ?1 AND <owner_col> = ?2`
/// and return the matching id values as a `HashSet`.
fn query_owned_row_ids(
    db: &DbConnection,
    sql: &str,
    space_id: &str,
    owner_value: &str,
) -> HashSet<String> {
    crate::database::core::select_with_crdt(
        sql.to_string(),
        vec![
            serde_json::Value::String(space_id.to_string()),
            serde_json::Value::String(owner_value.to_string()),
        ],
        db,
    )
    .ok()
    .map(|rows| {
        rows.into_iter()
            .filter_map(|row| row.into_iter().next())
            .filter_map(|v| match v {
                serde_json::Value::String(s) => Some(s),
                _ => None,
            })
            .collect()
    })
    .unwrap_or_default()
}

/// Extract the `id` value from a `row_pks` JSON string like `{"id":"<uuid>"}`.
fn extract_pk_id(row_pks: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(row_pks)
        .ok()
        .and_then(|m| m.get("id")?.as_str().map(str::to_string))
}
