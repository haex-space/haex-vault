//! Per-row space-scope enforcement: the gate that closes the
//! cross-space PK-collision attack surface.
//!
//! [`validate_and_attribute`](super::validate::validate_and_attribute)
//! only checks the `space_id` *column* in the payload — when the column
//! is omitted, an attacker who is a member of two spaces can mutate a
//! foreign-space row by targeting its primary key alone. This module
//! reads the row's existing `space_id` from the DB and rejects any push
//! whose request space disagrees.

use std::collections::HashMap;

use serde_json::Value as JsonValue;

use crate::crdt::scanner::LocalColumnChange;
use crate::database::DbConnection;

use super::util::read_existing_column;

/// For every touched row, verify that its existing `space_id` (if the
/// row exists) matches the request space, and that fresh inserts declare
/// `space_id` matching the request. Rejects the whole batch on the first
/// violation.
///
/// This complements the column-level `space_id` check in
/// [`validate_and_attribute`](super::validate::validate_and_attribute):
/// that one only fires when the payload writes the `space_id` column. A
/// caller who is a member of spaces A and B could otherwise mutate an
/// existing B-row through `SyncPush { space_id: "A" }` just by targeting
/// the B row id in `row_pks` and omitting the `space_id` column entirely
/// — the row PK matches across spaces, and downstream apply resolves
/// rows by PK alone. Reading the existing row's `space_id` closes that
/// gap.
///
/// `changes` is expected to be the output of
/// [`validate_and_attribute`](super::validate::validate_and_attribute)
/// (table-whitelisted and `authored_by_did` already attributed).
pub fn enforce_row_space_scope(
    db: &DbConnection,
    request_space_id: &str,
    changes: &[LocalColumnChange],
) -> Result<(), String> {
    // Group changes by (table, row_pks) and remember any space_id column
    // value the change set itself declares for the row. validate_and_attribute
    // already guarantees that any declared space_id equals request_space_id,
    // so its presence simply means "this change-set asserts a space_id" —
    // i.e. it covers the fresh-insert case.
    let mut per_row: HashMap<(String, String), bool> = HashMap::new();
    for change in changes {
        let key = (change.table_name.clone(), change.row_pks.clone());
        let entry = per_row.entry(key).or_insert(false);
        if change.column_name == "space_id" {
            *entry = true;
        }
    }

    for ((table, row_pks), declares_space_id) in per_row {
        let existing = read_existing_column(db, &table, &row_pks, "space_id")
            .map_err(|e| format!("space scope lookup failed for {table} row {row_pks}: {e}"))?;

        match existing {
            // Existing row — its space_id is authoritative; the request
            // space must equal it. validate_and_attribute already rejects
            // declared space_id values that disagree with request_space_id,
            // so we only need to verify the existing row's scope here.
            Some(JsonValue::String(real)) => {
                if real != request_space_id {
                    return Err(format!(
                        "row {table}/{row_pks}: existing row belongs to space {real:?}, push targeted {request_space_id:?}",
                    ));
                }
            }
            Some(JsonValue::Null) | None => {
                // Fresh insert — the change-set must declare a space_id so
                // the inserted row carries the correct scope. Without this,
                // a peer could push a brand-new row with no space_id column
                // and apply would write NULL, leaving an unscoped record.
                if !declares_space_id {
                    return Err(format!(
                        "row {table}/{row_pks}: insert must declare space_id column",
                    ));
                }
            }
            Some(other) => {
                return Err(format!(
                    "row {table}/{row_pks}: existing space_id has non-string value {other}",
                ));
            }
        }
    }

    Ok(())
}
