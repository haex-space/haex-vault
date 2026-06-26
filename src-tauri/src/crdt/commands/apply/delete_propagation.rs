use crate::crdt::trigger::{is_safe_identifier, ColumnInfo, DELETED_ROWS_TABLE};
use crate::database::error::DatabaseError;
use rusqlite::params;
use serde_json::Value as JsonValue;
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use super::super::helpers::json_values_to_sql_params;
use super::grouping::build_pk_where_from_map;

/// Creates a conflict entry when a UNIQUE constraint is violated
/// Stores remote data + both PKs (local and remote differ due to UNIQUE conflict)
pub(super) fn create_conflict_entry(
    tx: &rusqlite::Transaction,
    table_name: &str,
    error_msg: &str,
    remote_row_data: &serde_json::Map<String, JsonValue>,
    remote_timestamp: &str,
    schema: &[ColumnInfo],
) -> Result<(), DatabaseError> {
    // Extract the conflicting columns from error message
    // Example: "UNIQUE constraint failed: haex_settings.device_id, haex_settings.key"
    let conflict_key = if let Some(cols) = error_msg.strip_prefix("UNIQUE constraint failed: ") {
        cols.to_string()
    } else {
        error_msg.to_string()
    };

    // Serialize remote row data
    let remote_row_json =
        serde_json::to_string(remote_row_data).map_err(|e| DatabaseError::SerializationError {
            reason: format!("Failed to serialize remote row: {}", e),
        })?;

    // Extract PKs from schema
    let pk_columns: Vec<_> = schema.iter().filter(|col| col.is_pk).collect();

    // Build remote PK JSON
    let remote_pk: serde_json::Map<String, JsonValue> = pk_columns
        .iter()
        .filter_map(|pk_col| {
            remote_row_data
                .get(&pk_col.name)
                .map(|v| (pk_col.name.clone(), v.clone()))
        })
        .collect();
    let remote_pk_json = serde_json::to_string(&remote_pk).unwrap_or_else(|_| "{}".to_string());

    // Find local row PK - we don't know which exact row conflicts, so query with LIMIT 1
    // The UI will need to properly identify the conflicting row using the conflict_key
    let pk_select = pk_columns
        .iter()
        .map(|col| col.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    let query_sql = format!("SELECT {} FROM \"{}\" LIMIT 1", pk_select, table_name);

    let local_pk_json = tx
        .query_row(&query_sql, [], |row| {
            let mut local_pk = serde_json::Map::new();
            for (i, pk_col) in pk_columns.iter().enumerate() {
                if let Ok(val) = row.get::<_, String>(i) {
                    local_pk.insert(pk_col.name.clone(), JsonValue::String(val));
                }
            }
            Ok(serde_json::to_string(&local_pk).unwrap_or_else(|_| "{}".to_string()))
        })
        .unwrap_or_else(|_| "{}".to_string());

    // Generate conflict ID and timestamp
    let conflict_id = Uuid::new_v4().to_string();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let detected_at = format!("{}", timestamp);

    tx.execute(
        "INSERT INTO haex_crdt_conflicts (
            id, table_name, conflict_type, local_row_id, remote_row_id,
            local_row_data, remote_row_data, local_timestamp, remote_timestamp,
            conflict_key, detected_at, resolved
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            &conflict_id,
            table_name,
            "UNIQUE",
            &local_pk_json,
            &remote_pk_json,
            "", // UI fetches full row via local_row_id
            &remote_row_json,
            "", // UI fetches local timestamp
            remote_timestamp,
            &conflict_key,
            &detected_at,
            false,
        ],
    )
    .map_err(DatabaseError::from)?;

    eprintln!(
        "[SYNC RUST] Created conflict entry {} for table {}",
        conflict_id, table_name
    );

    Ok(())
}

/// Decide whether to honour a delete-log entry, given the HLC of the entry
/// and the HLC of the row currently sitting in the target table (if any).
///
/// CRDT semantics: a delete is just another timestamped operation. If the
/// target row carries a `haex_hlc` strictly newer than the delete-log entry,
/// the row was inserted/updated *after* the delete and must be kept (a
/// "resurrection"). Without this check, propagation unconditionally drops
/// the row, breaking last-write-wins for the insert-after-delete case.
pub(super) fn should_propagate_delete(delete_log_hlc: &str, target_row_hlc: Option<&str>) -> bool {
    match target_row_hlc {
        // Row doesn't exist locally → nothing to delete, but reporting
        // "should propagate" is harmless and keeps logging consistent.
        None => true,
        Some(target) => {
            // Honour the delete unless the target row is strictly newer.
            crate::crdt::hlc::compare_hlc_strings(target, delete_log_hlc)
                != std::cmp::Ordering::Greater
        }
    }
}

/// True if a delete-log entry at `delete_hlc` shadows an insert at `insert_hlc`
/// — i.e. the insert is NOT strictly newer, so applying it would resurrect a
/// deleted row. Sibling of `should_propagate_delete` (delete wins on tie).
pub(super) fn delete_shadows_insert(delete_hlc: &str, insert_hlc: &str) -> bool {
    crate::crdt::hlc::compare_hlc_strings(insert_hlc, delete_hlc) != std::cmp::Ordering::Greater
}

/// Whether an insert for `insert_pks` at `insert_hlc` must be suppressed because
/// a delete-log candidate for the SAME row (parsed-map equality — order- and
/// serializer-agnostic) carries a shadowing HLC.
///
/// Match is type-strict (`serde_json::Value` equality): a PK serialized as a
/// JSON number on one side and a string on the other will NOT match. In
/// practice the synced, delete-tracked tables use TEXT (UUID) PKs, so this is
/// safe today; non-text PK types (BLOB, integer) are covered by the canonical
/// `row_pks` follow-up (open-points F5). A miss here fails toward NOT
/// suppressing (status-quo resurrection), never toward a wrong suppression.
pub(super) fn insert_suppressed_by_deletes(
    insert_pks: &serde_json::Map<String, JsonValue>,
    insert_hlc: &str,
    candidates: &[(serde_json::Map<String, JsonValue>, String)],
) -> bool {
    candidates.iter().any(|(del_pks, del_hlc)| {
        del_pks == insert_pks && delete_shadows_insert(del_hlc, insert_hlc)
    })
}

/// Applies pending delete-log entries to their target tables.
///
/// For each row id in `delete_log_ids`, reads `(table_name, row_pks)` from
/// `haex_deleted_rows` and issues a `DELETE` on the target table. Assumes the
/// caller has already disabled CRDT triggers (`triggers_enabled = 0`), so the
/// DELETE does not re-append to the delete-log.
pub(super) fn propagate_deleted_rows_to_target_tables(
    tx: &rusqlite::Transaction,
    delete_log_ids: &HashSet<String>,
) -> Result<(), DatabaseError> {
    for id in delete_log_ids {
        let result = tx.query_row(
            &format!(
                "SELECT table_name, row_pks, haex_hlc FROM \"{}\" WHERE id = ?1",
                DELETED_ROWS_TABLE
            ),
            params![id],
            |row| {
                let table_name: String = row.get(0)?;
                let row_pks: String = row.get(1)?;
                let delete_hlc: String = row.get(2)?;
                Ok((table_name, row_pks, delete_hlc))
            },
        );

        let (target_table, row_pks_json, delete_hlc) = match result {
            Ok(r) => r,
            Err(rusqlite::Error::QueryReturnedNoRows) => continue,
            Err(e) => return Err(DatabaseError::from(e)),
        };

        if !is_safe_identifier(&target_table) {
            eprintln!(
                "[SYNC RUST] Skipping propagation for unsafe target table: {}",
                target_table
            );
            continue;
        }

        let row_pks: serde_json::Map<String, JsonValue> = match serde_json::from_str(&row_pks_json)
        {
            Ok(m) => m,
            Err(e) => {
                eprintln!(
                    "[SYNC RUST] Invalid row_pks JSON for delete-log {}: {}",
                    id, e
                );
                continue;
            }
        };

        // All-or-nothing safety: if any PK column name fails the safe-
        // identifier check we must skip the entire row. Building a
        // partial WHERE from the remaining columns would match more
        // rows than intended (potentially every row in the table).
        let (where_clause, values) = match build_pk_where_from_map(&row_pks) {
            Some(parts) => parts,
            None => {
                eprintln!(
                    "[SYNC RUST] Skipping delete-log {} for '{}': row_pks contains \
                     unsafe or empty PK columns — refusing to issue a partial WHERE",
                    id, target_table
                );
                continue;
            }
        };
        let sql_params = json_values_to_sql_params(&values)?;
        let param_refs: Vec<&dyn rusqlite::ToSql> = sql_params
            .iter()
            .map(|v| v as &dyn rusqlite::ToSql)
            .collect();

        // Resurrection check: if the target row was inserted/updated after
        // this delete-log entry, the row's haex_hlc is strictly newer and
        // we must NOT propagate the delete.
        let select_hlc_sql = format!(
            "SELECT haex_hlc FROM \"{}\" WHERE {}",
            target_table, where_clause
        );
        let target_row_hlc: Option<String> = tx
            .query_row(&select_hlc_sql, param_refs.as_slice(), |row| row.get(0))
            .ok();
        if !should_propagate_delete(&delete_hlc, target_row_hlc.as_deref()) {
            eprintln!(
                "[SYNC RUST] Skipping delete-log {} for '{}': target row \
                 has newer haex_hlc ({:?} > {}) — resurrected",
                id, target_table, target_row_hlc, delete_hlc
            );
            continue;
        }

        let delete_sql = format!("DELETE FROM \"{}\" WHERE {}", target_table, where_clause);

        match tx.execute(&delete_sql, param_refs.as_slice()) {
            Ok(n) => {
                if n > 0 {
                    eprintln!(
                        "[SYNC RUST] Delete-log propagation: removed {} row(s) from '{}'",
                        n, target_table
                    );
                }
            }
            Err(e) => {
                eprintln!(
                    "[SYNC RUST] Delete-log propagation failed for '{}': {}",
                    target_table, e
                );
                // Fall through — do not abort the whole sync on a single failure
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_propagates_when_target_does_not_exist_locally() {
        assert!(should_propagate_delete("5/abcdef", None));
    }

    #[test]
    fn delete_propagates_when_target_row_is_older() {
        // Target row was last modified at HLC 3; delete-log claims HLC 5.
        // The delete is newer → propagate.
        assert!(should_propagate_delete("5/abcdef", Some("3/abcdef")));
    }

    #[test]
    fn delete_propagates_when_target_row_has_equal_hlc() {
        // Equal timestamps: tie-break by node id (built into the
        // comparator). We treat equal-or-older target rows as "delete
        // wins" to keep idempotent re-application stable.
        assert!(should_propagate_delete("5/abcdef", Some("5/abcdef")));
    }

    #[test]
    fn delete_skipped_when_target_row_is_strictly_newer() {
        // This is the bug fix: an insert/update at HLC 10 must survive a
        // delete-log entry at HLC 5. Without this, the row inserted
        // after the delete would be wiped on the next apply.
        assert!(!should_propagate_delete("5/abcdef", Some("10/abcdef")));
    }

    #[test]
    fn delete_skipped_when_target_row_is_far_newer() {
        // Sanity: large HLC gap, same node.
        assert!(!should_propagate_delete("100/abcdef", Some("1000/abcdef")));
    }
}
