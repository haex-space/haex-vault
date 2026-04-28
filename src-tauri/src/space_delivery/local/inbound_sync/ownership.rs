//! Per-row ownership enforcement for the membership-system tables that
//! every member is allowed to write (`haex_space_members`,
//! `haex_space_devices`, `haex_device_mls_enrollments`,
//! `haex_mls_sync_keys`).
//!
//! Without this gate any write member could overwrite a foreign-row's
//! `identity_id` or `device_endpoint_id` and impersonate another member.
//! The check resolves the row's *existing* owner from the DB and falls
//! back to the change-set declaration only for fresh inserts; declared
//! owner values for existing rows must match exactly (no rewrites).

use std::collections::{HashMap, HashSet};

use serde_json::Value as JsonValue;

use crate::crdt::scanner::{is_membership_system_table, LocalColumnChange};
use crate::database::DbConnection;
use crate::space_delivery::local::error::DeliveryError;

use super::util::read_existing_column;

/// Maps a membership-system table to the column whose value identifies
/// the row's owner. `None` means "no owner check" — currently used for
/// `haex_mls_sync_keys` where the row is a per-epoch derived value that
/// all members compute identically.
pub(super) fn owner_column_for(table: &str) -> Option<&'static str> {
    match table {
        "haex_space_members" => Some("identity_id"),
        "haex_space_devices" => Some("device_endpoint_id"),
        "haex_device_mls_enrollments" => Some("device_id"),
        // Epoch-derived key, identical across all members of an epoch.
        // CRDT-LWW already lets any write member replace the value; the
        // membership-system whitelist does not widen that surface, so a
        // dedicated ownership column makes no sense here.
        "haex_mls_sync_keys" => None,
        _ => None,
    }
}

/// What identifies the *owner* of a row from the caller's perspective.
enum OwnerKind {
    /// Owner column holds an identity UUID; resolved against
    /// `haex_identities.id` and matched to the caller's audience DID.
    IdentityId,
    /// Owner column holds an Iroh node id; matched directly against the
    /// caller's QUIC peer endpoint id.
    EndpointId,
}

fn owner_kind_for(table: &str) -> Option<OwnerKind> {
    match table {
        "haex_space_members" => Some(OwnerKind::IdentityId),
        "haex_space_devices" | "haex_device_mls_enrollments" => Some(OwnerKind::EndpointId),
        _ => None,
    }
}

/// Resolve a DID to an identity UUID via `haex_identities`. Returns `None`
/// when no identity row matches — a state that should never arise for an
/// active space member (the leader inserts the identity during ClaimInvite).
fn resolve_identity_id_for_did(
    db: &DbConnection,
    did: &str,
) -> Result<Option<String>, DeliveryError> {
    let sql = "SELECT id FROM haex_identities WHERE did = ?1 LIMIT 1".to_string();
    let params = vec![JsonValue::String(did.to_string())];
    let rows = crate::database::core::select_with_crdt(sql, params, db).map_err(|e| {
        DeliveryError::Database {
            reason: format!("resolve_identity_id_for_did: {e}"),
        }
    })?;
    Ok(rows
        .first()
        .and_then(|row| row.first())
        .and_then(|v| v.as_str())
        .map(str::to_string))
}

/// For every membership-system row in `changes`, drop rows whose ownership
/// cannot be verified as belonging to the caller. Returns the accepted changes
/// and a list of reasons for any dropped rows (for logging).
///
/// Unlike a hard-reject of the whole batch, this lets valid rows through while
/// silently discarding rows the caller does not own — e.g. ping-pong re-pushes
/// of rows originally written by the leader on behalf of another member. The
/// security invariant is preserved: foreign-owned rows are never applied.
///
/// `changes` is expected to be the output of
/// [`validate_and_attribute`](super::validate::validate_and_attribute).
pub fn filter_ownership_violations(
    db: &DbConnection,
    peer_endpoint_id: &str,
    audience_did: &str,
    changes: Vec<LocalColumnChange>,
) -> (Vec<LocalColumnChange>, Vec<String>) {
    // Group changes by (table, row_pks) and remember the declared owner-column
    // value, if any.
    let mut per_row: HashMap<(String, String), Option<JsonValue>> = HashMap::new();
    for change in &changes {
        if !is_membership_system_table(&change.table_name) {
            continue;
        }
        let key = (change.table_name.clone(), change.row_pks.clone());
        let owner_col = match owner_column_for(&change.table_name) {
            Some(c) => c,
            None => continue,
        };
        let entry = per_row.entry(key).or_insert(None);
        if change.column_name == owner_col {
            *entry = Some(change.value.clone());
        }
    }

    let mut skip_rows: HashSet<(String, String)> = HashSet::new();
    let mut violations: Vec<String> = Vec::new();

    // Resolve the caller's identity UUID once — reused for all IdentityId checks.
    let caller_identity_id: Option<String> = resolve_identity_id_for_did(db, audience_did)
        .ok()
        .flatten();

    for ((table, row_pks), declared) in per_row {
        let kind = match owner_kind_for(&table) {
            Some(k) => k,
            None => continue,
        };
        let owner_col = owner_column_for(&table).expect("owner_kind_for ⇒ owner_column_for");

        // Always look up the *existing* owner first — peers cannot rewrite it.
        let existing = match read_existing_column(db, &table, &row_pks, owner_col) {
            Ok(v) => v,
            Err(e) => {
                violations.push(format!("ownership lookup failed for {table} row {row_pks}: {e}"));
                skip_rows.insert((table, row_pks));
                continue;
            }
        };

        let existing_owner: Option<String> = match existing {
            Some(JsonValue::String(s)) => Some(s),
            Some(JsonValue::Null) | None => None,
            Some(other) => {
                violations.push(format!(
                    "row {table}/{row_pks}: existing owner column {owner_col} has non-string value {other}",
                ));
                skip_rows.insert((table, row_pks));
                continue;
            }
        };

        let owner_value = match (&existing_owner, &declared) {
            // Existing row — declaration in push must be absent or identical.
            (Some(existing), Some(JsonValue::String(decl))) => {
                if existing != decl {
                    violations.push(format!(
                        "row {table}/{row_pks}: push attempts to change owner from {existing:?} to {decl:?}",
                    ));
                    skip_rows.insert((table, row_pks));
                    continue;
                }
                existing.clone()
            }
            (Some(existing), None) => existing.clone(),
            (Some(_), Some(other)) => {
                violations.push(format!(
                    "row {table}/{row_pks}: declared owner has non-string value {other}",
                ));
                skip_rows.insert((table, row_pks));
                continue;
            }
            // Fresh insert — must declare a string owner.
            (None, Some(JsonValue::String(decl))) => decl.clone(),
            (None, None | Some(JsonValue::Null)) => {
                violations.push(format!(
                    "row {table}/{row_pks}: owner column {owner_col} missing on insert",
                ));
                skip_rows.insert((table, row_pks));
                continue;
            }
            (None, Some(other)) => {
                violations.push(format!(
                    "row {table}/{row_pks}: declared owner has non-string value {other}",
                ));
                skip_rows.insert((table, row_pks));
                continue;
            }
        };

        let ok = match kind {
            OwnerKind::EndpointId => owner_value == peer_endpoint_id,
            OwnerKind::IdentityId => match &caller_identity_id {
                Some(my_identity) => owner_value == *my_identity,
                None => {
                    violations.push(format!(
                        "caller DID {audience_did} has no identity row — cannot verify ownership of {table}",
                    ));
                    skip_rows.insert((table, row_pks));
                    continue;
                }
            },
        };

        if !ok {
            violations.push(format!(
                "row {table}/{row_pks}: owner {owner_value:?} does not match caller (endpoint={peer_endpoint_id}, did={audience_did})",
            ));
            skip_rows.insert((table, row_pks));
        }
    }

    let accepted = if skip_rows.is_empty() {
        changes
    } else {
        changes
            .into_iter()
            .filter(|c| !skip_rows.contains(&(c.table_name.clone(), c.row_pks.clone())))
            .collect()
    };

    (accepted, violations)
}
