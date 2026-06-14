use crate::crdt::hlc::{hlc_is_newer, hlc_max, HlcService};
use crate::crdt::trigger;
use crate::crdt::trigger::{
    get_table_schema as get_table_schema_internal, is_safe_identifier, ColumnInfo,
    COLUMN_HLCS_COLUMN, DELETED_ROWS_TABLE, HLC_TIMESTAMP_COLUMN,
};
use crate::database::core::{with_connection, ValueConverter};
use crate::database::error::DatabaseError;
use crate::table_names::{TABLE_CRDT_CONFIGS, TABLE_CRDT_DIRTY_TABLES, TABLE_CRDT_PENDING_COLUMNS};
use crate::AppState;
use rusqlite::params;
use rusqlite::types::Value as SqlValue;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;
use ts_rs::TS;
use uuid::Uuid;

/// Converts a vector of JSON values to SQL values for use in queries.
/// This ensures consistent handling of null values (JsonValue::Null -> SqlValue::Null)
/// instead of incorrectly converting them to the string "null".
fn json_values_to_sql_params(values: &[JsonValue]) -> Result<Vec<SqlValue>, DatabaseError> {
    values
        .iter()
        .map(|v| ValueConverter::json_to_rusqlite_value(v))
        .collect()
}

/// Builds a WHERE clause for primary key columns, properly handling NULL values.
///
/// In SQL, `column = NULL` is always FALSE because NULL != NULL.
/// For NULL PK values, we must use `column IS NULL` instead.
///
/// Returns a tuple of:
/// - The WHERE clause string (e.g., `"id" = ? AND "group_id" IS NULL`)
/// - A Vec of JsonValues containing only the non-NULL values for parameterized queries
fn build_pk_where_clause(
    pk_columns: &[&ColumnInfo],
    row_pks: &serde_json::Map<String, JsonValue>,
) -> (String, Vec<JsonValue>) {
    let mut where_parts: Vec<String> = Vec::new();
    let mut params: Vec<JsonValue> = Vec::new();

    for col in pk_columns {
        match row_pks.get(&col.name) {
            Some(JsonValue::Null) | None => {
                // NULL value - use IS NULL (no parameter needed)
                where_parts.push(format!("\"{}\" IS NULL", col.name));
            }
            Some(v) => {
                // Non-NULL value - use = ? with parameter
                where_parts.push(format!("\"{}\" = ?", col.name));
                params.push(v.clone());
            }
        }
    }

    (where_parts.join(" AND "), params)
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DirtyTable {
    pub table_name: String,
    pub last_modified: String,
}

/// Gets table schema information (columns and their properties)
#[tauri::command]
pub fn get_table_schema(
    table_name: String,
    state: State<'_, AppState>,
) -> Result<Vec<ColumnInfo>, DatabaseError> {
    with_connection(&state.db, |conn| {
        Ok(get_table_schema_internal(conn, &table_name).map_err(DatabaseError::from)?)
    })
}

/// Gets all dirty tables that need to be synced
#[tauri::command]
pub fn get_dirty_tables(state: State<'_, AppState>) -> Result<Vec<DirtyTable>, DatabaseError> {
    with_connection(&state.db, |conn| {
        let mut stmt = conn
            .prepare(&format!("SELECT table_name, last_modified FROM {TABLE_CRDT_DIRTY_TABLES} ORDER BY last_modified ASC"))
            .map_err(DatabaseError::from)?;

        let rows = stmt
            .query_map([], |row| {
                Ok(DirtyTable {
                    table_name: row.get(0)?,
                    last_modified: row.get(1)?,
                })
            })
            .map_err(DatabaseError::from)?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(DatabaseError::from)
    })
}

/// Inner logic for clearing a dirty table, callable from Rust without Tauri state.
pub fn clear_dirty_table_inner(
    db: &crate::database::DbConnection,
    table_name: &str,
    before_timestamp: Option<&str>,
) -> Result<(), DatabaseError> {
    with_connection(db, |conn| {
        match before_timestamp {
            Some(ts) => {
                conn.execute(
                    &format!(
                        "DELETE FROM {TABLE_CRDT_DIRTY_TABLES} WHERE table_name = ?1 AND last_modified <= ?2"
                    ),
                    [table_name, ts],
                )
                .map_err(DatabaseError::from)?;
            }
            None => {
                conn.execute(
                    &format!("DELETE FROM {TABLE_CRDT_DIRTY_TABLES} WHERE table_name = ?1"),
                    [table_name],
                )
                .map_err(DatabaseError::from)?;
            }
        }

        Ok(())
    })
}

/// Clears a specific table from the dirty tables tracker.
/// If before_timestamp is provided, only clears entries with last_modified <= that timestamp.
/// This prevents clearing entries that were added AFTER the sync scan started.
#[tauri::command]
pub fn clear_dirty_table(
    table_name: String,
    before_timestamp: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), DatabaseError> {
    clear_dirty_table_inner(&state.db, &table_name, before_timestamp.as_deref())
}

/// Clears all dirty tables
#[tauri::command]
pub fn clear_all_dirty_tables(state: State<'_, AppState>) -> Result<(), DatabaseError> {
    with_connection(&state.db, |conn| {
        conn.execute(&format!("DELETE FROM {TABLE_CRDT_DIRTY_TABLES}"), [])
            .map_err(DatabaseError::from)?;

        Ok(())
    })
}

/// Gets all CRDT-enabled tables (tables with a `haex_hlc` column).
#[tauri::command]
pub fn get_all_crdt_tables(state: State<'_, AppState>) -> Result<Vec<String>, DatabaseError> {
    use crate::database::init::discover_crdt_tables;

    with_connection(&state.db, |conn| discover_crdt_tables(conn))
}

/// Ensures all CRDT tables have proper triggers set up.
/// This should be called after applying synced extension migrations to make sure
/// newly created extension tables have their dirty-table triggers.
/// Returns the number of tables that had triggers created.
#[tauri::command]
pub fn ensure_extension_triggers(state: State<'_, AppState>) -> Result<usize, DatabaseError> {
    use crate::database::init::ensure_triggers_for_all_tables;

    with_connection(&state.db, |conn| ensure_triggers_for_all_tables(conn))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteColumnChange {
    pub table_name: String,
    pub row_pks: String, // JSON string
    pub column_name: String,
    pub hlc_timestamp: String,
    pub decrypted_value: JsonValue, // Already decrypted in frontend
}

/// Creates a conflict entry when a UNIQUE constraint is violated
/// Stores remote data + both PKs (local and remote differ due to UNIQUE conflict)
fn create_conflict_entry(
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

/// Groups a flat list of column changes into transaction-HLC groups and
/// returns them sorted ascending by HLC. All writes issued inside the same
/// sender-side transaction share a timestamp, so `hlc_timestamp` is the
/// semantic grouping key — there is no separate batch id anymore.
fn group_by_transaction_hlc(
    changes: Vec<RemoteColumnChange>,
) -> Vec<(String, Vec<RemoteColumnChange>)> {
    let mut groups: HashMap<String, Vec<RemoteColumnChange>> = HashMap::new();
    for change in changes {
        groups
            .entry(change.hlc_timestamp.clone())
            .or_default()
            .push(change);
    }

    let mut ordered: Vec<(String, Vec<RemoteColumnChange>)> = groups.into_iter().collect();
    ordered.sort_by(|a, b| crate::crdt::hlc::compare_hlc_strings(&a.0, &b.0));
    ordered
}

/// Groups column changes by `(table, row_pks)` and returns rows in ascending
/// order of their earliest HLC timestamp.
///
/// The naive shape — collect changes into a `HashMap<(table, row_pks), …>`
/// and iterate it — discards the careful HLC ordering established by
/// `group_by_transaction_hlc`: HashMap iteration is unordered. When a remote
/// batch contains rows from multiple transactions (e.g. parent inserted at
/// HLC1, child inserted at HLC2 referencing it), HashMap iteration may apply
/// the child first. FK constraints are disabled during apply so that is not
/// itself a hard error, but the apply order then no longer reflects the
/// causal order the sender intended, and any future logic that observes the
/// per-row apply sequence will see nondeterministic results.
///
/// This helper preserves the per-row grouping but sorts the resulting rows
/// by `min(hlc_timestamp)` so the iteration order is deterministic and
/// follows the same causal order as `group_by_transaction_hlc`.
pub(crate) fn group_row_changes_in_hlc_order(
    changes: impl IntoIterator<Item = RemoteColumnChange>,
) -> Vec<((String, String), Vec<RemoteColumnChange>)> {
    let mut map: HashMap<(String, String), Vec<RemoteColumnChange>> = HashMap::new();
    for change in changes {
        map.entry((change.table_name.clone(), change.row_pks.clone()))
            .or_default()
            .push(change);
    }
    let mut entries: Vec<((String, String), Vec<RemoteColumnChange>)> =
        map.into_iter().collect();
    entries.sort_by(|a, b| {
        let a_min = crate::crdt::hlc::hlc_min(a.1.iter().map(|c| c.hlc_timestamp.as_str()));
        let b_min = crate::crdt::hlc::hlc_min(b.1.iter().map(|c| c.hlc_timestamp.as_str()));
        match (a_min, b_min) {
            (Some(am), Some(bm)) => crate::crdt::hlc::compare_hlc_strings(am, bm),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
    });
    entries
}

/// Applies remote changes in a single transaction, with HLC-ordered grouping.
/// Note: lastPullServerTimestamp is now updated by the TypeScript layer after successful apply
#[tauri::command]
pub fn apply_remote_changes_in_transaction(
    changes: Vec<RemoteColumnChange>,
    backend_id: String,
    max_hlc: String,
    state: State<'_, AppState>,
) -> Result<(), DatabaseError> {
    // Lock HLC via `lock_or_fail` so a poisoned mutex fails LOUD with a
    // banner row. Previous behaviour was `.lock().ok().map(...)` which
    // silently passed `hlc_service=None` to `apply_remote_changes_to_db`
    // — that path applies the remote changes WITHOUT advancing the local
    // HLC clock, so subsequent local writes carry stale timestamps that
    // lose merge conflicts on the next sync round.
    let hlc_service = state.lock_or_fail(
        &state.hlc,
        crate::critical::CriticalFailureCode::HlcMutexPoisoned,
        "crdt::commands::apply_remote_changes_in_transaction",
        serde_json::json!({}),
    )?;
    apply_remote_changes_to_db(
        &state.db,
        changes,
        Some((&backend_id, &max_hlc)),
        Some(&*hlc_service),
    )
}

/// Inner implementation that applies remote CRDT changes to a database connection.
///
/// If `backend_info` is `Some((backend_id, max_hlc))`, updates `haex_sync_backends`
/// with the push HLC timestamp (used by server sync). For local delivery, pass `None`.
///
/// If `hlc_service` is provided, the local HLC clock is advanced past the highest
/// received remote timestamp after applying all changes. This ensures future local
/// operations generate timestamps strictly greater than any received remote timestamp,
/// preventing incomplete rows on the server during push.
/// Build a `WHERE …` clause that matches a row by its CRDT primary-key map.
///
/// Returns `Some((where_clause, params))` if every PK column name is a safe
/// identifier; returns `None` if **any** column name fails the safety check.
/// Skipping individual columns is wrong: with a partial WHERE the resulting
/// DELETE matches *more* than the intended row (potentially every row if
/// every column was unsafe). All-or-nothing is the only correct stance.
pub(crate) fn build_pk_where_from_map(
    row_pks: &serde_json::Map<String, JsonValue>,
) -> Option<(String, Vec<JsonValue>)> {
    if row_pks.is_empty() {
        return None;
    }
    let mut where_parts: Vec<String> = Vec::with_capacity(row_pks.len());
    let mut values: Vec<JsonValue> = Vec::with_capacity(row_pks.len());
    for (col_name, value) in row_pks {
        if !is_safe_identifier(col_name) {
            return None;
        }
        match value {
            JsonValue::Null => {
                where_parts.push(format!("\"{}\" IS NULL", col_name));
            }
            _ => {
                where_parts.push(format!("\"{}\" = ?", col_name));
                values.push(value.clone());
            }
        }
    }
    Some((where_parts.join(" AND "), values))
}

/// Decide whether to honour a delete-log entry, given the HLC of the entry
/// and the HLC of the row currently sitting in the target table (if any).
///
/// CRDT semantics: a delete is just another timestamped operation. If the
/// target row carries a `haex_hlc` strictly newer than the delete-log entry,
/// the row was inserted/updated *after* the delete and must be kept (a
/// "resurrection"). Without this check, propagation unconditionally drops
/// the row, breaking last-write-wins for the insert-after-delete case.
fn should_propagate_delete(delete_log_hlc: &str, target_row_hlc: Option<&str>) -> bool {
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

/// Applies pending delete-log entries to their target tables.
///
/// For each row id in `delete_log_ids`, reads `(table_name, row_pks)` from
/// `haex_deleted_rows` and issues a `DELETE` on the target table. Assumes the
/// caller has already disabled CRDT triggers (`triggers_enabled = 0`), so the
/// DELETE does not re-append to the delete-log.
fn propagate_deleted_rows_to_target_tables(
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

        let row_pks: serde_json::Map<String, JsonValue> = match serde_json::from_str(&row_pks_json) {
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
        let param_refs: Vec<&dyn rusqlite::ToSql> =
            sql_params.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

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

        let delete_sql = format!(
            "DELETE FROM \"{}\" WHERE {}",
            target_table, where_clause
        );

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

pub fn apply_remote_changes_to_db(
    db: &crate::database::DbConnection,
    changes: Vec<RemoteColumnChange>,
    backend_info: Option<(&str, &str)>,
    hlc_service: Option<&HlcService>,
) -> Result<(), DatabaseError> {
    eprintln!("[SYNC RUST] ========== APPLY REMOTE CHANGES START ==========");
    eprintln!(
        "[SYNC RUST] Changes count: {}, backend: {}",
        changes.len(),
        backend_info.map(|(id, _)| id).unwrap_or("local-delivery"),
    );

    // Group changes by transaction-HLC and apply groups in ascending HLC order
    // so cross-table transactions (e.g. parent + child insert) land together.
    let grouped = group_by_transaction_hlc(changes);
    let changes: Vec<RemoteColumnChange> = grouped
        .into_iter()
        .flat_map(|(_hlc, group)| group.into_iter())
        .collect();

    // Validate all table and column names from remote changes to prevent SQL injection
    for change in &changes {
        if !is_safe_identifier(&change.table_name) {
            return Err(DatabaseError::ValidationError {
                reason: format!(
                    "Invalid table name '{}' in remote change",
                    change.table_name
                ),
            });
        }
        if !is_safe_identifier(&change.column_name) {
            return Err(DatabaseError::ValidationError {
                reason: format!(
                    "Invalid column name '{}' in table '{}'",
                    change.column_name, change.table_name
                ),
            });
        }
    }
    eprintln!("[SYNC RUST] Identifier validation passed");

    with_connection(db, |conn| {
        // Disable foreign key constraints for the duration of the apply
        // pass, re-enabling unconditionally on every exit path (including
        // mid-body errors). PRAGMA foreign_keys cannot be changed inside a
        // transaction, so the toggle must wrap the transaction.
        // See: https://sqlite.org/foreignkeys.html
        eprintln!("[SYNC RUST] Disabling foreign_keys BEFORE transaction");
        let applied_hlc_timestamps = crate::crdt::cleanup::with_fk_disabled(conn, |conn| {
        // Start transaction - all changes in the batch are applied atomically
        eprintln!("[SYNC RUST] Starting transaction...");
        let tx = conn.transaction().map_err(DatabaseError::from)?;

        // Disable triggers temporarily to prevent marking tables as dirty
        // when applying remote changes (we don't want to re-sync changes we just pulled)
        eprintln!("[SYNC RUST] Disabling triggers for remote changes");
        let disable_sql = format!(
            "INSERT INTO {TABLE_CRDT_CONFIGS} (key, type, value) VALUES ('triggers_enabled', 'system', '0')
             ON CONFLICT(key) DO UPDATE SET value = '0'"
        );
        tx.execute(&disable_sql, []).map_err(DatabaseError::from)?;

        // Collect side-data needed after the apply loop:
        //   1. all HLC timestamps for advancing the local clock,
        //   2. IDs of haex_deleted_rows entries arriving in this batch so
        //      the corresponding DELETE on the target table can run after
        //      the apply loop (triggers are still disabled then).
        let mut all_hlc_timestamps: Vec<String> = Vec::with_capacity(changes.len());
        let mut inbound_delete_log_ids: HashSet<String> = HashSet::new();
        for change in &changes {
            all_hlc_timestamps.push(change.hlc_timestamp.clone());
            if change.table_name == DELETED_ROWS_TABLE {
                if let Ok(map) =
                    serde_json::from_str::<serde_json::Map<String, JsonValue>>(&change.row_pks)
                {
                    if let Some(JsonValue::String(id)) = map.get("id") {
                        inbound_delete_log_ids.insert(id.clone());
                    }
                }
            }
        }

        // Group by (table, row) so all columns of one row are written
        // together — and keep iteration ordered by the row's earliest
        // HLC. Plain HashMap iteration would discard the careful HLC
        // ordering that group_by_transaction_hlc just established.
        let row_changes = group_row_changes_in_hlc_order(changes);

        // Apply changes grouped by row
        for ((_table_name, row_pks_str), row_change_list) in row_changes {
            // Use the first change to get common data
            let first_change = &row_change_list[0];

            // Get table schema to identify PK columns
            // If table doesn't exist (e.g., from a dev extension not installed here), skip it
            let schema = get_table_schema_internal(&tx, &first_change.table_name)
                .map_err(DatabaseError::from)?;

            if schema.is_empty() {
                eprintln!(
                    "[SYNC RUST] Skipping table '{}' - table does not exist (extension not installed?)",
                    first_change.table_name
                );
                continue;
            }

            // Ensure table has CRDT columns (haex_hlc, haex_column_hlcs)
            // This handles tables created in dev mode that don't have CRDT columns yet.
            // When sync data arrives, we know it's from a production extension, so we need CRDT.
            let has_hlcs_column = schema.iter().any(|col| col.name == "haex_column_hlcs");
            if !has_hlcs_column {
                eprintln!(
                    "[SYNC RUST] Table '{}' missing CRDT columns (created in dev mode?) - upgrading now",
                    first_change.table_name
                );
                match trigger::ensure_crdt_columns_and_triggers(&tx, &first_change.table_name) {
                    Ok((columns_added, triggers_created)) => {
                        eprintln!(
                            "[SYNC RUST] Upgraded '{}': columns={}, triggers={}",
                            first_change.table_name, columns_added, triggers_created
                        );
                    }
                    Err(e) => {
                        eprintln!(
                            "[SYNC RUST] Failed to upgrade '{}': {} - skipping this table",
                            first_change.table_name, e
                        );
                        continue;
                    }
                }
            }

            // Parse row PKs (same for all changes in this row)
            let row_pks: serde_json::Map<String, JsonValue> = serde_json::from_str(&row_pks_str)
                .map_err(|e| DatabaseError::SerializationError {
                    reason: format!("Failed to parse row PKs: {}", e),
                })?;

            let pk_columns: Vec<_> = schema.iter().filter(|col| col.is_pk).collect();

            // Build WHERE clause for PKs, handling NULL values properly
            let (pk_where_clause, pk_values_for_query) =
                build_pk_where_clause(&pk_columns, &row_pks);

            // Check if row exists and get current HLCs
            let check_sql = format!(
                "SELECT haex_column_hlcs FROM \"{}\" WHERE {}",
                first_change.table_name, pk_where_clause
            );

            let current_hlcs: Option<String> = {
                let mut stmt = tx.prepare(&check_sql).map_err(DatabaseError::from)?;
                let params = json_values_to_sql_params(&pk_values_for_query)?;
                let params_refs: Vec<&dyn rusqlite::ToSql> =
                    params.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

                // Only `QueryReturnedNoRows` means "row absent" — any other
                // error (locking, schema mismatch, etc.) must surface so the
                // caller does not silently treat a transient failure as
                // "no existing row" and overwrite live state.
                match stmt.query_row(&*params_refs, |row| row.get(0)) {
                    Ok(hlcs) => Some(hlcs),
                    Err(rusqlite::Error::QueryReturnedNoRows) => None,
                    Err(e) => return Err(DatabaseError::from(e)),
                }
            };

            // Track if row exists
            let row_exists = current_hlcs.is_some();

            // Parse current HLCs
            let mut column_hlcs: serde_json::Map<String, JsonValue> =
                if let Some(hlcs_str) = current_hlcs {
                    serde_json::from_str(&hlcs_str).unwrap_or_default()
                } else {
                    serde_json::Map::new()
                };

            // Build a set of existing column names for quick lookup
            let existing_columns: std::collections::HashSet<&str> =
                schema.iter().map(|col| col.name.as_str()).collect();

            // Collect all column changes that are newer than current
            let mut columns_to_update: Vec<(String, JsonValue, String)> = Vec::new(); // (column_name, json_value, hlc)
            let mut max_hlc_for_row = first_change.hlc_timestamp.clone();

            for change in &row_change_list {
                // Skip columns that don't exist in the local schema
                // This handles schema version differences between devices
                if !existing_columns.contains(change.column_name.as_str()) {
                    eprintln!(
                        "[SYNC RUST] Skipping unknown column '{}' in table '{}' - column not in local schema (older app version?)",
                        change.column_name, first_change.table_name
                    );

                    // Track this as a pending column that needs to be pulled after migration
                    // Uses INSERT OR IGNORE to avoid duplicates (composite PK on table_name, column_name)
                    // Only stores table_name + column_name - row PKs come from server during re-pull
                    tx.execute(
                        &format!(
                            "INSERT OR IGNORE INTO {} (table_name, column_name) VALUES (?, ?)",
                            TABLE_CRDT_PENDING_COLUMNS
                        ),
                        params![&first_change.table_name, &change.column_name],
                    ).map_err(DatabaseError::from)?;

                    // Still track the HLC for this column so we know we've "seen" this change
                    // This prevents re-processing when the column is later added via migration
                    column_hlcs.insert(
                        change.column_name.clone(),
                        JsonValue::String(change.hlc_timestamp.clone()),
                    );
                    continue;
                }

                let current_hlc = column_hlcs
                    .get(&change.column_name)
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if hlc_is_newer(change.hlc_timestamp.as_str(), current_hlc) {
                    // Remote change is newer, include it
                    column_hlcs.insert(
                        change.column_name.clone(),
                        JsonValue::String(change.hlc_timestamp.clone()),
                    );
                    columns_to_update.push((
                        change.column_name.clone(),
                        change.decrypted_value.clone(),
                        change.hlc_timestamp.clone(),
                    ));

                    // Track max HLC for row timestamp
                    if hlc_is_newer(&change.hlc_timestamp, &max_hlc_for_row) {
                        max_hlc_for_row = change.hlc_timestamp.clone();
                    }
                }
            }

            // Only apply if there are columns to update
            if !columns_to_update.is_empty() {
                let new_hlcs_json = serde_json::to_string(&column_hlcs).map_err(|e| {
                    DatabaseError::SerializationError {
                        reason: format!("Failed to serialize column HLCs: {}", e),
                    }
                })?;

                if row_exists {
                    // Row exists, update it with all changed columns
                    let set_clauses: Vec<String> = columns_to_update
                        .iter()
                        .map(|(col_name, _, _)| format!("\"{}\" = ?", col_name))
                        .collect();

                    let update_sql = format!(
                        "UPDATE \"{}\" SET {}, haex_column_hlcs = ?, haex_hlc = ? WHERE {}",
                        first_change.table_name,
                        set_clauses.join(", "),
                        pk_where_clause
                    );

                    let mut params_vec: Vec<SqlValue> = Vec::new();

                    // Add column values (convert JSON to SQL values)
                    for (_col_name, json_value, _) in &columns_to_update {
                        let sql_value = ValueConverter::json_to_rusqlite_value(json_value)?;
                        params_vec.push(sql_value);
                    }

                    // Add HLCs and timestamp
                    params_vec.push(SqlValue::Text(new_hlcs_json));
                    params_vec.push(SqlValue::Text(max_hlc_for_row.clone()));

                    // Add PK values for WHERE clause (only non-NULL values, NULL uses IS NULL)
                    for sql_val in json_values_to_sql_params(&pk_values_for_query)? {
                        params_vec.push(sql_val);
                    }

                    let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec
                        .iter()
                        .map(|v| v as &dyn rusqlite::ToSql)
                        .collect();

                    tx.execute(&update_sql, &*params_refs)
                        .map_err(DatabaseError::from)?;
                } else {
                    // Row doesn't exist, insert it with all changed columns + PKs
                    let mut columns = Vec::new();
                    let mut values: Vec<SqlValue> = Vec::new();

                    // Add PKs first (use json_values_to_sql_params for consistent null handling)
                    let pk_json_values: Vec<JsonValue> = pk_columns
                        .iter()
                        .filter_map(|col| row_pks.get(&col.name).cloned())
                        .collect();
                    let pk_sql_values = json_values_to_sql_params(&pk_json_values)?;
                    for (col, sql_val) in pk_columns.iter().zip(pk_sql_values.into_iter()) {
                        columns.push(col.name.clone());
                        values.push(sql_val);
                    }

                    // Add changed columns (convert JSON to SQL values)
                    for (col_name, json_value, _) in &columns_to_update {
                        columns.push(col_name.clone());
                        let sql_value = ValueConverter::json_to_rusqlite_value(json_value)?;
                        values.push(sql_value);
                    }

                    // Add CRDT metadata
                    columns.push(COLUMN_HLCS_COLUMN.to_string());
                    columns.push(HLC_TIMESTAMP_COLUMN.to_string());
                    values.push(SqlValue::Text(new_hlcs_json));
                    values.push(SqlValue::Text(max_hlc_for_row.clone()));

                    let placeholders = vec!["?"; columns.len()].join(", ");
                    let quoted_columns: Vec<String> = columns
                        .iter()
                        .map(|c| format!("\"{}\"", c))
                        .collect();
                    let insert_sql = format!(
                        "INSERT INTO \"{}\" ({}) VALUES ({})",
                        first_change.table_name,
                        quoted_columns.join(", "),
                        placeholders
                    );

                    let params_refs: Vec<&dyn rusqlite::ToSql> =
                        values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

                    // Try to insert - if it fails with constraint, log detailed error
                    match tx.execute(&insert_sql, &*params_refs) {
                        Ok(_) => {} // Success - continue
                        Err(rusqlite::Error::SqliteFailure(err, msg))
                            if err.code == rusqlite::ErrorCode::ConstraintViolation =>
                        {
                            // Log the constraint violation details
                            let error_msg =
                                msg.as_deref().unwrap_or("Unknown constraint violation");
                            eprintln!(
                                "[SYNC RUST] Constraint violation for table {}: {}",
                                first_change.table_name, error_msg
                            );
                            eprintln!("[SYNC RUST] Failed INSERT SQL: {}", insert_sql);
                            eprintln!("[SYNC RUST] Values: {:?}", values);

                            // Check if it's a NOT NULL constraint violation
                            if error_msg.contains("NOT NULL constraint failed") {
                                eprintln!(
                                    "[SYNC RUST] ⚠️ NOT NULL constraint failed! This usually means the sync data is incomplete."
                                );
                                eprintln!(
                                    "[SYNC RUST] Columns in INSERT: {:?}",
                                    columns
                                );
                                eprintln!(
                                    "[SYNC RUST] Received {} changes for this row: {:?}",
                                    row_change_list.len(),
                                    row_change_list.iter().map(|c| &c.column_name).collect::<Vec<_>>()
                                );
                                // Re-throw with detailed error
                                return Err(DatabaseError::ExecutionError {
                                    sql: insert_sql,
                                    reason: format!(
                                        "NOT NULL constraint failed. Received columns: {:?}. This indicates incomplete sync data - the server may not have all columns for this row.",
                                        row_change_list.iter().map(|c| &c.column_name).collect::<Vec<_>>()
                                    ),
                                    table: Some(first_change.table_name.clone()),
                                });
                            }

                            // Check if it's a UNIQUE constraint violation
                            if error_msg.contains("UNIQUE constraint failed") {
                                eprintln!("[SYNC RUST] UNIQUE constraint conflict - creating conflict entry");

                                // Build remote row data from all columns being inserted
                                let mut remote_row_data = serde_json::Map::new();
                                for (i, col_name) in columns.iter().enumerate() {
                                    if let Some(sql_value) = values.get(i) {
                                        let json_value =
                                            ValueConverter::rusqlite_value_to_json(sql_value);
                                        remote_row_data.insert(col_name.clone(), json_value);
                                    }
                                }

                                // Create conflict entry
                                if let Err(e) = create_conflict_entry(
                                    &tx,
                                    &first_change.table_name,
                                    error_msg,
                                    &remote_row_data,
                                    &max_hlc_for_row,
                                    &schema,
                                ) {
                                    eprintln!(
                                        "[SYNC RUST] Failed to create conflict entry: {:?}",
                                        e
                                    );
                                }

                                continue; // Skip this row and continue with next
                            }

                            // For other constraints (CHECK, etc.), re-throw the error
                            return Err(DatabaseError::from(rusqlite::Error::SqliteFailure(
                                err, msg,
                            )));
                        }
                        Err(e) => {
                            eprintln!(
                                "[SYNC RUST] INSERT failed for table {}: {:?}",
                                first_change.table_name, e
                            );
                            return Err(DatabaseError::from(e));
                        }
                    }
                }
            }
        }

        // Propagate delete-log entries received in this batch to their target tables.
        // Triggers are still disabled, so the DELETEs won't re-log into haex_deleted_rows.
        if !inbound_delete_log_ids.is_empty() {
            eprintln!(
                "[SYNC RUST] Propagating {} delete-log entries to target tables",
                inbound_delete_log_ids.len()
            );
            propagate_deleted_rows_to_target_tables(&tx, &inbound_delete_log_ids)?;
        }

        // Update lastPushHlcTimestamp for this backend to prevent re-pushing the data we just pulled
        // Note: lastPullServerTimestamp is now updated by TypeScript using the server timestamp
        // Only applicable for server sync (not local delivery)
        if let Some((backend_id, max_hlc)) = backend_info {
            eprintln!(
                "[SYNC RUST] Updating last_push_hlc_timestamp to {}",
                max_hlc
            );
            tx.execute(
                "UPDATE haex_sync_backends SET last_push_hlc_timestamp = ? WHERE id = ?",
                params![max_hlc, backend_id],
            )
            .map_err(DatabaseError::from)?;
        }

        // Re-enable triggers before committing
        eprintln!("[SYNC RUST] Re-enabling triggers");
        let enable_sql = format!(
            "INSERT INTO {TABLE_CRDT_CONFIGS} (key, type, value) VALUES ('triggers_enabled', 'system', '1')
             ON CONFLICT(key) DO UPDATE SET value = '1'"
        );
        tx.execute(&enable_sql, []).map_err(DatabaseError::from)?;

        // Commit transaction (with FK constraints disabled)
        eprintln!("[SYNC RUST] Committing transaction");
        match tx.commit() {
            Ok(_) => {
                eprintln!("[SYNC RUST] Transaction committed successfully");
            }
            Err(e) => {
                eprintln!("[SYNC RUST] Transaction commit failed: {:?}", e);
                return Err(DatabaseError::from(e));
            }
        }

        Ok(all_hlc_timestamps)
        })?;
        // FK constraints are now re-enabled by with_fk_disabled (even if
        // the closure above returned Err mid-body).

        // Advance the local HLC clock past the highest received remote timestamp.
        // This ensures future local operations generate timestamps > any remote HLC,
        // so all columns of locally created rows are pushed (not filtered by lastPushHlcTimestamp).
        if let Some(hlc) = hlc_service {
            // Use backend_info max_hlc if available (server sync), otherwise
            // compute from the changes themselves (local delivery).
            let max_hlc_str = match backend_info {
                Some((_, hlc_str)) if !hlc_str.is_empty() => hlc_str.to_string(),
                _ => {
                    let max = hlc_max(applied_hlc_timestamps.iter().map(|s| s.as_str()));
                    max.unwrap_or_default().to_string()
                }
            };
            hlc.advance_past_remote(&max_hlc_str);
        }

        Ok(())
    })
}

#[cfg(test)]
mod hlc_grouping_tests {
    use super::*;

    fn change(table: &str, pk: &str, col: &str, hlc: &str) -> RemoteColumnChange {
        RemoteColumnChange {
            table_name: table.to_string(),
            row_pks: pk.to_string(),
            column_name: col.to_string(),
            hlc_timestamp: hlc.to_string(),
            decrypted_value: JsonValue::Null,
        }
    }

    // HLC strings sort lexicographically when same length; use fixed-width
    // numeric prefixes so the relative order is unambiguous.
    const HLC1: &str = "1/abcdef";
    const HLC2: &str = "2/abcdef";
    const HLC3: &str = "3/abcdef";
    const HLC4: &str = "4/abcdef";

    #[test]
    fn helper_emits_rows_in_ascending_min_hlc_order() {
        // Construct three rows whose earliest HLCs are HLC1, HLC2, HLC3 —
        // but feed them in reverse order so HashMap insertion order is
        // visibly wrong. The helper must still produce HLC1 → HLC2 → HLC3.
        let changes = vec![
            change("t", r#"{"id":"c"}"#, "col", HLC3),
            change("t", r#"{"id":"b"}"#, "col", HLC2),
            change("t", r#"{"id":"a"}"#, "col", HLC1),
        ];

        let ordered = group_row_changes_in_hlc_order(changes);

        let keys: Vec<&str> = ordered.iter().map(|(k, _)| k.1.as_str()).collect();
        assert_eq!(
            keys,
            vec![r#"{"id":"a"}"#, r#"{"id":"b"}"#, r#"{"id":"c"}"#],
            "rows must be ordered by ascending min(hlc), regardless of input order"
        );
    }

    #[test]
    fn helper_uses_min_hlc_per_row_for_ordering() {
        // Row A has changes at HLC1 + HLC4; Row B has a single change at
        // HLC2. min(A) = HLC1 < min(B) = HLC2, so A must come before B
        // even though A also contains the latest timestamp in the batch.
        let changes = vec![
            change("t", r#"{"id":"a"}"#, "col1", HLC4),
            change("t", r#"{"id":"b"}"#, "col", HLC2),
            change("t", r#"{"id":"a"}"#, "col2", HLC1),
        ];

        let ordered = group_row_changes_in_hlc_order(changes);

        assert_eq!(ordered.len(), 2, "rows must be grouped per (table, pk)");
        assert_eq!(
            ordered[0].0 .1, r#"{"id":"a"}"#,
            "row A (min HLC = HLC1) must come before row B (min HLC = HLC2)"
        );
        assert_eq!(ordered[0].1.len(), 2, "row A must keep both of its changes");
        assert_eq!(ordered[1].0 .1, r#"{"id":"b"}"#);
    }

    // ------------------------------------------------------------------
    // should_propagate_delete: insert-after-delete resurrection check
    // ------------------------------------------------------------------

    // ------------------------------------------------------------------
    // build_pk_where_from_map: all-or-nothing safety
    // ------------------------------------------------------------------

    fn pk_map(pairs: &[(&str, JsonValue)]) -> serde_json::Map<String, JsonValue> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn pk_where_returns_none_for_empty_map() {
        let empty = serde_json::Map::<String, JsonValue>::new();
        assert!(build_pk_where_from_map(&empty).is_none());
    }

    #[test]
    fn pk_where_handles_safe_identifiers_with_values() {
        let map = pk_map(&[
            ("id", JsonValue::String("x".into())),
            ("group_id", JsonValue::String("g".into())),
        ]);
        let (clause, values) = build_pk_where_from_map(&map).expect("safe");
        assert!(clause.contains("\"id\" = ?"));
        assert!(clause.contains("\"group_id\" = ?"));
        assert!(clause.contains(" AND "));
        assert_eq!(values.len(), 2);
    }

    #[test]
    fn pk_where_uses_is_null_for_null_values() {
        let map = pk_map(&[
            ("id", JsonValue::String("x".into())),
            ("optional", JsonValue::Null),
        ]);
        let (clause, values) = build_pk_where_from_map(&map).expect("safe");
        assert!(clause.contains("\"optional\" IS NULL"));
        // NULL columns do not contribute to the bound parameter list.
        assert_eq!(values.len(), 1);
    }

    #[test]
    fn pk_where_returns_none_when_any_column_is_unsafe() {
        // Bug-fix probe: previously the loop did `continue` on the unsafe
        // column, building a WHERE from the *remaining* columns. The
        // resulting DELETE would match every row that shares those
        // remaining values — potentially every row when every column is
        // unsafe. All-or-nothing is the only safe stance.
        let map = pk_map(&[
            ("id", JsonValue::String("x".into())),
            ("evil; DROP TABLE", JsonValue::String("y".into())),
        ]);
        assert!(
            build_pk_where_from_map(&map).is_none(),
            "row with any unsafe PK column must produce no WHERE clause — \
             building a partial clause from the other columns would match \
             more rows than intended"
        );
    }

    #[test]
    fn pk_where_returns_none_when_only_unsafe_columns() {
        let map = pk_map(&[("evil; --", JsonValue::String("y".into()))]);
        assert!(build_pk_where_from_map(&map).is_none());
    }

    // ------------------------------------------------------------------
    // should_propagate_delete (existing tests)
    // ------------------------------------------------------------------

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

    // ------------------------------------------------------------------

    #[test]
    fn helper_is_deterministic_across_input_orderings() {
        // A direct probe for the bug: build a batch large enough that a
        // plain HashMap iteration order is nearly guaranteed to differ
        // between insertion orderings. The helper must always produce
        // the same sequence regardless of how changes are reshuffled.
        let baseline_changes: Vec<RemoteColumnChange> = (0..16)
            .map(|i| {
                let hlc = format!("{}/abcdef", i);
                change("t", &format!(r#"{{"id":"r{}"}}"#, i), "c", &hlc)
            })
            .collect();

        let baseline = group_row_changes_in_hlc_order(baseline_changes);
        let baseline_keys: Vec<String> =
            baseline.iter().map(|(k, _)| k.1.clone()).collect();

        // Reverse input order and re-run.
        let reversed: Vec<RemoteColumnChange> = (0..16)
            .rev()
            .map(|i| {
                let hlc = format!("{}/abcdef", i);
                change("t", &format!(r#"{{"id":"r{}"}}"#, i), "c", &hlc)
            })
            .collect();
        let reversed_out = group_row_changes_in_hlc_order(reversed);
        let reversed_keys: Vec<String> =
            reversed_out.iter().map(|(k, _)| k.1.clone()).collect();

        assert_eq!(
            baseline_keys, reversed_keys,
            "iteration order must be deterministic and HLC-driven, not \
             dependent on the order changes were collected from the batch"
        );

        // Sanity: the row order matches ascending HLC numeric order.
        // (Cannot use lexicographic compare on the row keys themselves
        // because "r10" < "r2" lexically while HLC says otherwise.)
        let baseline_min_hlcs: Vec<&str> = baseline
            .iter()
            .map(|(_, list)| {
                crate::crdt::hlc::hlc_min(list.iter().map(|c| c.hlc_timestamp.as_str())).unwrap()
            })
            .collect();
        for window in baseline_min_hlcs.windows(2) {
            assert!(
                crate::crdt::hlc::compare_hlc_strings(window[0], window[1])
                    != std::cmp::Ordering::Greater,
                "consecutive rows must be in non-decreasing HLC order"
            );
        }
    }
}
