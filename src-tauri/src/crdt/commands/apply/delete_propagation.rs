use crate::crdt::trigger::{
    get_table_schema, is_safe_identifier, ColumnInfo, DELETED_ROWS_TABLE,
    SHARED_SPACE_DELETED_ROWS_TABLE, SHARED_SPACE_SYNC_TABLE,
};
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

    // Find local row PK by querying on the conflicting columns parsed from the error message.
    // conflict_key has the shape "<table>.<col>[, <table>.<col>]*" — strip table prefix from each part.
    let schema_col_names: std::collections::HashSet<&str> =
        schema.iter().map(|c| c.name.as_str()).collect();
    let conflict_cols: Vec<String> = conflict_key
        .split(", ")
        .filter_map(|part| {
            let col = part.rsplit('.').next().unwrap_or("").trim();
            if is_safe_identifier(col) && schema_col_names.contains(col) {
                Some(col.to_string())
            } else {
                None
            }
        })
        .collect();

    let pk_select = pk_columns
        .iter()
        .map(|col| format!("\"{}\"", col.name))
        .collect::<Vec<_>>()
        .join(", ");

    // Build a targeted WHERE clause when we have valid conflict columns AND the remote
    // row carries values for all of them; otherwise fall back to "{}".
    let conflict_values: Option<Vec<JsonValue>> = if conflict_cols.is_empty() {
        None
    } else {
        let vals: Vec<JsonValue> = conflict_cols
            .iter()
            .filter_map(|c| remote_row_data.get(c).cloned())
            .collect();
        if vals.len() == conflict_cols.len() {
            Some(vals)
        } else {
            None
        }
    };

    let local_pk_json = match conflict_values {
        None => "{}".to_string(),
        Some(values) => match json_values_to_sql_params(&values) {
            // Conversion failure degrades to the fallback — the conflict
            // entry must still be recorded below.
            Err(_) => "{}".to_string(),
            Ok(sql_params) => {
                let where_clause = conflict_cols
                    .iter()
                    .enumerate()
                    .map(|(i, c)| format!("\"{}\" = ?{}", c, i + 1))
                    .collect::<Vec<_>>()
                    .join(" AND ");
                let query_sql = format!(
                    "SELECT {} FROM \"{}\" WHERE {} LIMIT 1",
                    pk_select, table_name, where_clause
                );
                let param_refs: Vec<&dyn rusqlite::ToSql> = sql_params
                    .iter()
                    .map(|v| v as &dyn rusqlite::ToSql)
                    .collect();
                tx.query_row(&query_sql, param_refs.as_slice(), |row| {
                    let mut local_pk = serde_json::Map::new();
                    for (i, pk_col) in pk_columns.iter().enumerate() {
                        if let Ok(val) = row.get::<_, String>(i) {
                            local_pk.insert(pk_col.name.clone(), JsonValue::String(val));
                        }
                    }
                    Ok(serde_json::to_string(&local_pk).unwrap_or_else(|_| "{}".to_string()))
                })
                .unwrap_or_else(|_| "{}".to_string())
            }
        },
    };

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

        // Defense-in-depth against malformed delete-log rows: require that
        // row_pks names *exactly* the target table's PK columns. A composite
        // key like (a, b) with only `{"a": ...}` would otherwise build a
        // partial WHERE and over-delete every row matching `a`.
        let schema = match get_table_schema(tx, &target_table) {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "[SYNC RUST] Skipping delete-log {} for '{}': failed to load schema: {}",
                    id, target_table, e
                );
                continue;
            }
        };
        let expected_pk_names: HashSet<&str> = schema
            .iter()
            .filter(|c| c.is_pk)
            .map(|c| c.name.as_str())
            .collect();
        let provided_pk_names: HashSet<&str> = row_pks.keys().map(|k| k.as_str()).collect();
        if expected_pk_names.is_empty() || expected_pk_names != provided_pk_names {
            eprintln!(
                "[SYNC RUST] Skipping delete-log {} for '{}': row_pks keys {:?} \
                 do not match table PK columns {:?}",
                id, target_table, provided_pk_names, expected_pk_names
            );
            continue;
        }

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
        let target_row_hlc: Option<String> =
            match tx.query_row(&select_hlc_sql, param_refs.as_slice(), |row| row.get(0)) {
                Ok(hlc) => Some(hlc),
                Err(rusqlite::Error::QueryReturnedNoRows) => None,
                Err(e) => return Err(DatabaseError::from(e)),
            };
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

/// Task 6 apply-side receiver for the per-space delete-log
/// (`haex_shared_space_deleted_rows`, ADR 0002 §6.5).
///
/// For each id in `delete_log_ids`, reads
/// `(space_id, table_name, row_pks, haex_hlc)` from the delete-log and:
///   1. DELETEs the target row from `table_name` with the standard
///      resurrection check (haex_hlc > delete_hlc → skip).
///   2. DELETEs the register entry
///      `(table_name, row_pks, space_id)` from `haex_shared_space_sync`.
///
/// Callers MUST have set `triggers_enabled = 0` first — both DELETEs are
/// caught by the Task 4/5 triggers otherwise and would re-emit into the
/// delete-log and loop.
///
/// A single malformed delete-log row must never abort the whole pass (would
/// wedge the sync cursor permanently). Errors on any one row are logged and
/// the loop continues, matching `propagate_deleted_rows_to_target_tables`'
/// contract.
pub(super) fn propagate_shared_space_deleted_rows_to_target_tables(
    tx: &rusqlite::Transaction,
    delete_log_ids: &HashSet<String>,
) -> Result<(), DatabaseError> {
    for id in delete_log_ids {
        let result = tx.query_row(
            &format!(
                "SELECT space_id, table_name, row_pks, haex_hlc \
                 FROM \"{SHARED_SPACE_DELETED_ROWS_TABLE}\" WHERE id = ?1"
            ),
            params![id],
            |row| {
                let space_id: String = row.get(0)?;
                let table_name: String = row.get(1)?;
                let row_pks: String = row.get(2)?;
                let delete_hlc: Option<String> = row.get(3)?;
                Ok((space_id, table_name, row_pks, delete_hlc))
            },
        );

        let (space_id, target_table, row_pks_json, delete_hlc_opt) = match result {
            Ok(r) => r,
            Err(rusqlite::Error::QueryReturnedNoRows) => continue,
            Err(e) => return Err(DatabaseError::from(e)),
        };

        let delete_hlc = match delete_hlc_opt {
            Some(h) => h,
            None => {
                eprintln!("[SYNC RUST] Skipping shared-space delete-log {id}: haex_hlc is NULL");
                continue;
            }
        };

        if !is_safe_identifier(&target_table) {
            eprintln!(
                "[SYNC RUST] Skipping shared-space delete-log {id}: unsafe target table {target_table}"
            );
            continue;
        }

        let row_pks: serde_json::Map<String, JsonValue> = match serde_json::from_str(&row_pks_json)
        {
            Ok(m) => m,
            Err(e) => {
                eprintln!("[SYNC RUST] Invalid row_pks JSON on shared-space delete-log {id}: {e}");
                continue;
            }
        };

        let schema = match get_table_schema(tx, &target_table) {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "[SYNC RUST] Skipping shared-space delete-log {id} for '{target_table}': failed to load schema: {e}"
                );
                continue;
            }
        };

        // Same PK-name safety as propagate_deleted_rows_to_target_tables.
        let expected_pk_names: HashSet<&str> = schema
            .iter()
            .filter(|c| c.is_pk)
            .map(|c| c.name.as_str())
            .collect();
        let provided_pk_names: HashSet<&str> = row_pks.keys().map(|k| k.as_str()).collect();
        if expected_pk_names.is_empty() || expected_pk_names != provided_pk_names {
            eprintln!(
                "[SYNC RUST] Skipping shared-space delete-log {id} for '{target_table}': \
                 row_pks keys {provided_pk_names:?} do not match table PK columns {expected_pk_names:?}"
            );
            continue;
        }

        let (where_clause, values) = match build_pk_where_from_map(&row_pks) {
            Some(parts) => parts,
            None => {
                eprintln!(
                    "[SYNC RUST] Skipping shared-space delete-log {id} for '{target_table}': \
                     row_pks contains unsafe or empty PK columns — refusing partial WHERE"
                );
                continue;
            }
        };
        let sql_params = json_values_to_sql_params(&values)?;
        let param_refs: Vec<&dyn rusqlite::ToSql> = sql_params
            .iter()
            .map(|v| v as &dyn rusqlite::ToSql)
            .collect();

        // Resurrection check (same semantic as owner-domain path).
        let select_hlc_sql =
            format!("SELECT haex_hlc FROM \"{target_table}\" WHERE {where_clause}");
        let target_row_hlc: Option<String> =
            match tx.query_row(&select_hlc_sql, param_refs.as_slice(), |row| row.get(0)) {
                Ok(hlc) => Some(hlc),
                Err(rusqlite::Error::QueryReturnedNoRows) => None,
                Err(e) => return Err(DatabaseError::from(e)),
            };
        if !should_propagate_delete(&delete_hlc, target_row_hlc.as_deref()) {
            eprintln!(
                "[SYNC RUST] Skipping shared-space delete-log {id} for '{target_table}': \
                 target row has newer haex_hlc ({target_row_hlc:?} > {delete_hlc}) — resurrected"
            );
            continue;
        }

        // Task 7 register-check gate: before applying, look up whether the
        // (table, row, space) is currently registered. Three outcomes:
        //   a. registered → legitimate apply.
        //   b. not registered here, not registered anywhere → race with a
        //      local unshare (Option C) — idempotent no-op cleanup path
        //      via the residual check further down.
        //   c. not registered here but registered in ANOTHER space → the
        //      remote is trying to delete a row that lives in a space
        //      they aren't authorized for. Log with `NotSharedInSpace`
        //      severity; the residual check below then keeps the row
        //      intact (residual > 0 → skip row DELETE), so no data is lost.
        let target_space_registered: bool = tx
            .query_row(
                &format!(
                    "SELECT 1 FROM \"{SHARED_SPACE_SYNC_TABLE}\" \
                     WHERE table_name = ?1 AND row_pks = ?2 AND space_id = ?3 LIMIT 1"
                ),
                params![&target_table, &row_pks_json, &space_id],
                |_| Ok(true),
            )
            .unwrap_or(false);
        if !target_space_registered {
            let any_space_registered: bool = tx
                .query_row(
                    &format!(
                        "SELECT 1 FROM \"{SHARED_SPACE_SYNC_TABLE}\" \
                         WHERE table_name = ?1 AND row_pks = ?2 LIMIT 1"
                    ),
                    params![&target_table, &row_pks_json],
                    |_| Ok(true),
                )
                .unwrap_or(false);
            if any_space_registered {
                eprintln!(
                    "[SYNC RUST] Shared-space delete-log {id} for '{target_table}' \
                     in '{space_id}': target row registered in a DIFFERENT space \
                     (suspected forgery — NotSharedInSpace). Row will be kept by \
                     the residual-register guard below."
                );
            } else {
                eprintln!(
                    "[SYNC RUST] Shared-space delete-log {id} for '{target_table}' \
                     in '{space_id}': target register entry absent (likely a race \
                     with a local unshare); applying idempotent cleanup."
                );
            }
        }

        // Register cleanup: per (table, row, space). Other spaces' entries
        // for the same row survive by design (see the multi-space test).
        // Run BEFORE the business row DELETE so that "row also present
        // in another space" logic in downstream consumers still sees the
        // consistent register state at this point.
        let register_delete_sql = format!(
            "DELETE FROM \"{SHARED_SPACE_SYNC_TABLE}\" \
             WHERE table_name = ?1 AND row_pks = ?2 AND space_id = ?3"
        );
        if let Err(e) = tx.execute(
            &register_delete_sql,
            params![&target_table, &row_pks_json, &space_id],
        ) {
            eprintln!(
                "[SYNC RUST] Shared-space register cleanup failed for '{target_table}' in space '{space_id}': {e}"
            );
            // Fall through — apply-loop must not abort mid-batch.
        }

        // Business row DELETE — only if the row no longer belongs to any
        // other space (otherwise we would strip a row still shared into
        // another live space). If any register entries remain for this
        // (table, row_pks), keep the row.
        let residual_register_count: i64 = tx
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM \"{SHARED_SPACE_SYNC_TABLE}\" \
                     WHERE table_name = ?1 AND row_pks = ?2"
                ),
                params![&target_table, &row_pks_json],
                |r| r.get(0),
            )
            .unwrap_or(0);

        if residual_register_count == 0 {
            let delete_sql = format!("DELETE FROM \"{target_table}\" WHERE {where_clause}");
            match tx.execute(&delete_sql, param_refs.as_slice()) {
                Ok(n) => {
                    if n > 0 {
                        eprintln!(
                            "[SYNC RUST] Shared-space delete-log propagation: removed {n} row(s) from '{target_table}'"
                        );
                    }
                }
                Err(e) => {
                    eprintln!(
                        "[SYNC RUST] Shared-space delete-log propagation failed for '{target_table}': {e}"
                    );
                }
            }
        } else {
            eprintln!(
                "[SYNC RUST] Shared-space delete-log {id}: row still shared into {residual_register_count} other space(s), keeping business row"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use rusqlite::Connection;

    /// Build a minimal in-memory DB with:
    ///  - `t`: table with UUID pk `id` and UNIQUE(device_id, key)
    ///  - `haex_crdt_conflicts`: minimal schema for the INSERT inside create_conflict_entry
    fn setup_conflict_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE t (
                id TEXT PRIMARY KEY,
                device_id TEXT NOT NULL,
                key TEXT NOT NULL,
                UNIQUE(device_id, key)
             );
             CREATE TABLE haex_crdt_conflicts (
                id TEXT PRIMARY KEY,
                table_name TEXT NOT NULL,
                conflict_type TEXT NOT NULL,
                local_row_id TEXT,
                remote_row_id TEXT,
                local_row_data TEXT,
                remote_row_data TEXT,
                local_timestamp TEXT,
                remote_timestamp TEXT,
                conflict_key TEXT,
                detected_at TEXT,
                resolved INTEGER
             );",
        )
        .unwrap();
        conn
    }

    fn schema_for_t() -> Vec<ColumnInfo> {
        vec![
            ColumnInfo {
                name: "id".to_string(),
                column_type: "TEXT".to_string(),
                is_pk: true,
            },
            ColumnInfo {
                name: "device_id".to_string(),
                column_type: "TEXT".to_string(),
                is_pk: false,
            },
            ColumnInfo {
                name: "key".to_string(),
                column_type: "TEXT".to_string(),
                is_pk: false,
            },
        ]
    }

    #[test]
    fn conflict_entry_records_the_conflicting_row() {
        let conn = setup_conflict_db();
        // Insert a local row with a known id
        conn.execute(
            "INSERT INTO t (id, device_id, key) VALUES ('local-id-1', 'dev-abc', 'mykey')",
            [],
        )
        .unwrap();

        let mut remote_row: serde_json::Map<String, JsonValue> = serde_json::Map::new();
        remote_row.insert(
            "id".to_string(),
            JsonValue::String("remote-id-999".to_string()),
        );
        remote_row.insert(
            "device_id".to_string(),
            JsonValue::String("dev-abc".to_string()),
        );
        remote_row.insert("key".to_string(), JsonValue::String("mykey".to_string()));

        let tx = conn.unchecked_transaction().unwrap();
        create_conflict_entry(
            &tx,
            "t",
            "UNIQUE constraint failed: t.device_id, t.key",
            &remote_row,
            "1/abc",
            &schema_for_t(),
        )
        .unwrap();
        tx.commit().unwrap();

        let local_row_id: String = conn
            .query_row(
                "SELECT local_row_id FROM haex_crdt_conflicts LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();

        let parsed: serde_json::Value = serde_json::from_str(&local_row_id).unwrap();
        assert_eq!(
            parsed["id"],
            JsonValue::String("local-id-1".to_string()),
            "local_row_id should contain the LOCAL row's id, got: {local_row_id}"
        );
    }

    #[test]
    fn conflict_entry_falls_back_on_unparseable_key() {
        let conn = setup_conflict_db();
        let remote_row: serde_json::Map<String, JsonValue> = serde_json::Map::new();

        let tx = conn.unchecked_transaction().unwrap();
        let result = create_conflict_entry(
            &tx,
            "t",
            "some other error without the UNIQUE prefix",
            &remote_row,
            "1/abc",
            &schema_for_t(),
        );
        tx.commit().unwrap();
        assert!(result.is_ok());

        let local_row_id: String = conn
            .query_row(
                "SELECT local_row_id FROM haex_crdt_conflicts LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(local_row_id, "{}", "should fall back to empty map");
    }

    #[test]
    fn conflict_entry_falls_back_when_remote_lacks_conflict_values() {
        let conn = setup_conflict_db();
        // Remote row data does NOT contain device_id / key
        let mut remote_row: serde_json::Map<String, JsonValue> = serde_json::Map::new();
        remote_row.insert(
            "id".to_string(),
            JsonValue::String("remote-only-id".to_string()),
        );

        let tx = conn.unchecked_transaction().unwrap();
        let result = create_conflict_entry(
            &tx,
            "t",
            "UNIQUE constraint failed: t.device_id, t.key",
            &remote_row,
            "1/abc",
            &schema_for_t(),
        );
        tx.commit().unwrap();
        assert!(result.is_ok());

        let local_row_id: String = conn
            .query_row(
                "SELECT local_row_id FROM haex_crdt_conflicts LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            local_row_id, "{}",
            "should fall back to empty map when remote values missing"
        );
    }

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

    // =====================================================================
    // Task 6 — Apply-side receiver for the per-space delete-log.
    //
    // A remote INSERT into haex_shared_space_deleted_rows carries all the
    // info needed to converge on a share removal or hard-delete in a shared
    // space:
    //  - the business row: `DELETE FROM {table} WHERE {row_pks}`
    //  - the register entry: `DELETE FROM haex_shared_space_sync
    //                         WHERE table_name=? AND row_pks=? AND space_id=?`
    // Both DELETEs run under triggers_enabled=0 so the cascade + fanout
    // triggers from Tasks 4/5 don't re-emit.
    // =====================================================================

    fn setup_shared_space_apply_fixture() -> Connection {
        let conn = Connection::open_in_memory().unwrap();

        // Business + register + delete-log schemas (mirrors production).
        conn.execute_batch(
            "CREATE TABLE haex_crdt_configs_no_sync (
                 key TEXT PRIMARY KEY NOT NULL,
                 value TEXT NOT NULL,
                 type TEXT NOT NULL
             );
             INSERT INTO haex_crdt_configs_no_sync (key, type, value)
             VALUES ('triggers_enabled', 'system', '0');

             CREATE TABLE ext_notes_items (
                 id TEXT PRIMARY KEY NOT NULL,
                 body TEXT,
                 haex_hlc TEXT,
                 haex_column_hlcs TEXT NOT NULL DEFAULT '{}',
                 haex_column_sigs TEXT NOT NULL DEFAULT '{}'
             );

             CREATE TABLE haex_shared_space_sync (
                 id TEXT PRIMARY KEY NOT NULL,
                 table_name TEXT NOT NULL,
                 row_pks TEXT NOT NULL,
                 space_id TEXT NOT NULL,
                 haex_hlc TEXT
             );

             CREATE TABLE haex_shared_space_deleted_rows (
                 id TEXT PRIMARY KEY NOT NULL,
                 space_id TEXT NOT NULL,
                 table_name TEXT NOT NULL,
                 row_pks TEXT NOT NULL,
                 haex_hlc TEXT
             );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn shared_space_delete_log_apply_removes_business_row_and_register_entry() {
        let conn = setup_shared_space_apply_fixture();
        // Seed: an extension row shared into SPACE_X.
        conn.execute(
            "INSERT INTO ext_notes_items (id, body, haex_hlc)
             VALUES ('note-1', 'hello', '1/abcd')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO haex_shared_space_sync (id, table_name, row_pks, space_id, haex_hlc)
             VALUES ('reg-1', 'ext_notes_items', '{\"id\":\"note-1\"}', 'SPACE_X', '1/abcd')",
            [],
        )
        .unwrap();

        // Simulate a remote push of a delete-log row into haex_shared_space_deleted_rows.
        conn.execute(
            "INSERT INTO haex_shared_space_deleted_rows (id, space_id, table_name, row_pks, haex_hlc)
             VALUES ('del-1', 'SPACE_X', 'ext_notes_items', '{\"id\":\"note-1\"}', '2/abcd')",
            [],
        )
        .unwrap();

        // Apply.
        let mut ids: HashSet<String> = HashSet::new();
        ids.insert("del-1".to_string());
        let tx = conn.unchecked_transaction().unwrap();
        propagate_shared_space_deleted_rows_to_target_tables(&tx, &ids).unwrap();
        tx.commit().unwrap();

        // Business row is gone.
        let row_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ext_notes_items WHERE id = 'note-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(row_count, 0, "business row must be removed on apply");

        // Register entry for (table, row, space) is gone.
        let reg_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM haex_shared_space_sync \
                 WHERE table_name = 'ext_notes_items' AND row_pks = '{\"id\":\"note-1\"}' AND space_id = 'SPACE_X'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(reg_count, 0, "register entry must be removed on apply");
    }

    #[test]
    fn shared_space_delete_log_apply_preserves_resurrection_bug_free() {
        // Resurrection: if the business row was written AFTER the delete-log
        // HLC, the delete must be skipped. Same pattern as
        // propagate_deleted_rows_to_target_tables.
        let conn = setup_shared_space_apply_fixture();
        conn.execute(
            "INSERT INTO ext_notes_items (id, body, haex_hlc)
             VALUES ('note-2', 'newer', '99/abcd')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO haex_shared_space_sync (id, table_name, row_pks, space_id, haex_hlc)
             VALUES ('reg-2', 'ext_notes_items', '{\"id\":\"note-2\"}', 'SPACE_X', '1/abcd')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO haex_shared_space_deleted_rows (id, space_id, table_name, row_pks, haex_hlc)
             VALUES ('del-2', 'SPACE_X', 'ext_notes_items', '{\"id\":\"note-2\"}', '2/abcd')",
            [],
        )
        .unwrap();

        let mut ids: HashSet<String> = HashSet::new();
        ids.insert("del-2".to_string());
        let tx = conn.unchecked_transaction().unwrap();
        propagate_shared_space_deleted_rows_to_target_tables(&tx, &ids).unwrap();
        tx.commit().unwrap();

        let row_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ext_notes_items WHERE id = 'note-2'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            row_count, 1,
            "resurrected row (newer haex_hlc than delete) must be kept"
        );
    }

    // Task 7: register-check authorization gate.
    #[test]
    fn shared_space_delete_log_apply_rejects_when_row_not_shared_in_that_space() {
        // Attacker forgery: remote sends a delete-log for (haex_calendar,
        // row-never-in-X, SPACE_X). Row lives in the target table but is
        // registered for SPACE_Y, not SPACE_X. Apply must not touch it.
        let conn = setup_shared_space_apply_fixture();
        // A generic "calendar" table with the same shape as extension tables.
        conn.execute_batch(
            "CREATE TABLE haex_calendar (
                 id TEXT PRIMARY KEY NOT NULL,
                 title TEXT,
                 haex_hlc TEXT
             )",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO haex_calendar (id, title, haex_hlc)
             VALUES ('cal-1', 'Meeting', '1/abcd')",
            [],
        )
        .unwrap();
        // Row is registered in SPACE_Y — never in SPACE_X.
        conn.execute(
            "INSERT INTO haex_shared_space_sync (id, table_name, row_pks, space_id, haex_hlc)
             VALUES ('reg-y', 'haex_calendar', '{\"id\":\"cal-1\"}', 'SPACE_Y', '1/abcd')",
            [],
        )
        .unwrap();
        // Attacker's forged delete-log for SPACE_X.
        conn.execute(
            "INSERT INTO haex_shared_space_deleted_rows (id, space_id, table_name, row_pks, haex_hlc)
             VALUES ('del-forge', 'SPACE_X', 'haex_calendar', '{\"id\":\"cal-1\"}', '2/abcd')",
            [],
        )
        .unwrap();

        let mut ids: HashSet<String> = HashSet::new();
        ids.insert("del-forge".to_string());
        let tx = conn.unchecked_transaction().unwrap();
        propagate_shared_space_deleted_rows_to_target_tables(&tx, &ids).unwrap();
        tx.commit().unwrap();

        // Row stays intact.
        let row_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM haex_calendar WHERE id = 'cal-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(row_count, 1, "forged delete must not remove the row");
        // Register entry for SPACE_Y also stays intact.
        let reg_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM haex_shared_space_sync \
                 WHERE table_name = 'haex_calendar' AND row_pks = '{\"id\":\"cal-1\"}' AND space_id = 'SPACE_Y'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            reg_count, 1,
            "SPACE_Y register entry must not be touched by a forged SPACE_X delete"
        );
    }

    #[test]
    fn shared_space_delete_log_apply_is_idempotent_on_race_with_local_unshare() {
        // Race: local user unshared SPACE_X → local delete-log for
        // (T, R, SPACE_X) got emitted; remote's delete-log for the same
        // (T, R, SPACE_X) arrives after. Apply must be a no-op (no error,
        // no side effect) — the register entry is already gone.
        let conn = setup_shared_space_apply_fixture();
        conn.execute(
            "INSERT INTO ext_notes_items (id, body, haex_hlc)
             VALUES ('note-race', 'raced', '1/abcd')",
            [],
        )
        .unwrap();
        // Local delete-log entry (the local unshare has already emitted).
        conn.execute(
            "INSERT INTO haex_shared_space_deleted_rows (id, space_id, table_name, row_pks, haex_hlc)
             VALUES ('del-local', 'SPACE_X', 'ext_notes_items', '{\"id\":\"note-race\"}', '2/abcd')",
            [],
        )
        .unwrap();
        // Remote's redundant delete-log entry for the same (T, R, SPACE_X).
        conn.execute(
            "INSERT INTO haex_shared_space_deleted_rows (id, space_id, table_name, row_pks, haex_hlc)
             VALUES ('del-remote', 'SPACE_X', 'ext_notes_items', '{\"id\":\"note-race\"}', '3/abcd')",
            [],
        )
        .unwrap();
        // NB: no register entry — this is the race precondition.

        let mut ids: HashSet<String> = HashSet::new();
        ids.insert("del-remote".to_string());
        let tx = conn.unchecked_transaction().unwrap();
        // Must not error — race is a legitimate flow.
        propagate_shared_space_deleted_rows_to_target_tables(&tx, &ids).unwrap();
        tx.commit().unwrap();
    }

    #[test]
    fn shared_space_delete_log_apply_only_removes_matching_space_register_entry() {
        // A row shared into multiple spaces: the delete-log applies only to
        // ONE space's register. Other spaces' register entries survive.
        let conn = setup_shared_space_apply_fixture();
        conn.execute(
            "INSERT INTO ext_notes_items (id, body, haex_hlc)
             VALUES ('note-3', 'shared', '1/abcd')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO haex_shared_space_sync (id, table_name, row_pks, space_id, haex_hlc)
             VALUES ('reg-x', 'ext_notes_items', '{\"id\":\"note-3\"}', 'SPACE_X', '1/abcd'),
                    ('reg-y', 'ext_notes_items', '{\"id\":\"note-3\"}', 'SPACE_Y', '1/abcd')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO haex_shared_space_deleted_rows (id, space_id, table_name, row_pks, haex_hlc)
             VALUES ('del-3', 'SPACE_X', 'ext_notes_items', '{\"id\":\"note-3\"}', '2/abcd')",
            [],
        )
        .unwrap();

        let mut ids: HashSet<String> = HashSet::new();
        ids.insert("del-3".to_string());
        let tx = conn.unchecked_transaction().unwrap();
        propagate_shared_space_deleted_rows_to_target_tables(&tx, &ids).unwrap();
        tx.commit().unwrap();

        // The SPACE_Y register entry stays put; the SPACE_X one is gone.
        let surviving_spaces: Vec<String> = conn
            .prepare(
                "SELECT space_id FROM haex_shared_space_sync \
                 WHERE table_name = 'ext_notes_items' AND row_pks = '{\"id\":\"note-3\"}' \
                 ORDER BY space_id",
            )
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            surviving_spaces,
            vec!["SPACE_Y".to_string()],
            "delete-log for SPACE_X must not touch SPACE_Y's register entry"
        );

        // Business row: still shared into Y — depending on ADR semantics we
        // keep it. Per §6.5 the apply-side removes the business row when
        // the LAST space unshares. For this simple test we assert the row
        // stays because Y still lists it — this is intentionally strict:
        // the apply must be per-space, not row-globally destructive.
        let row_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ext_notes_items WHERE id = 'note-3'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            row_count, 1,
            "business row stays as long as another space still lists it in the register"
        );
    }
}
