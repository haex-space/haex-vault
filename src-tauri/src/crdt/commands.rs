use crate::crdt::trigger::{get_table_schema as get_table_schema_internal, ColumnInfo};
use crate::database::core::with_connection;
use crate::database::error::DatabaseError;
use crate::AppState;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use tauri::State;
use ts_rs::TS;

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
            .prepare("SELECT table_name, last_modified FROM haex_crdt_dirty_tables ORDER BY last_modified ASC")
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

/// Clears a specific table from the dirty tables tracker
#[tauri::command]
pub fn clear_dirty_table(
    table_name: String,
    state: State<'_, AppState>,
) -> Result<(), DatabaseError> {
    with_connection(&state.db, |conn| {
        conn.execute(
            "DELETE FROM haex_crdt_dirty_tables WHERE table_name = ?1",
            [&table_name],
        )
        .map_err(DatabaseError::from)?;

        Ok(())
    })
}

/// Clears all dirty tables
#[tauri::command]
pub fn clear_all_dirty_tables(state: State<'_, AppState>) -> Result<(), DatabaseError> {
    with_connection(&state.db, |conn| {
        conn.execute("DELETE FROM haex_crdt_dirty_tables", [])
            .map_err(DatabaseError::from)?;

        Ok(())
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteColumnChange {
    pub table_name: String,
    pub row_pks: String, // JSON string
    pub column_name: String,
    pub hlc_timestamp: String,
    pub batch_id: String,
    pub batch_seq: usize,
    pub batch_total: usize,
    pub decrypted_value: JsonValue, // Already decrypted in frontend
}

/// Validates that all parts of each batch are present
fn validate_batch_completeness(changes: &[RemoteColumnChange]) -> Result<(), DatabaseError> {
    use std::collections::{HashMap, HashSet};

    // Group changes by batch_id
    let mut batches: HashMap<String, HashSet<usize>> = HashMap::new();
    let mut batch_totals: HashMap<String, usize> = HashMap::new();

    for change in changes {
        batches
            .entry(change.batch_id.clone())
            .or_insert_with(HashSet::new)
            .insert(change.batch_seq);

        // Store batch_total (should be same for all changes in a batch)
        batch_totals.insert(change.batch_id.clone(), change.batch_total);
    }

    // Validate each batch
    for (batch_id, seq_numbers) in batches {
        let expected_total = batch_totals.get(&batch_id).copied().unwrap_or(0);

        // Check if we have all sequence numbers from 1 to batch_total
        if seq_numbers.len() != expected_total {
            return Err(DatabaseError::ExecutionError {
                sql: "batch validation".to_string(),
                reason: format!(
                    "Incomplete batch {}: expected {} changes, got {}",
                    batch_id,
                    expected_total,
                    seq_numbers.len()
                ),
                table: None,
            });
        }

        // Check if sequence numbers are 1..=batch_total
        for seq in 1..=expected_total {
            if !seq_numbers.contains(&seq) {
                return Err(DatabaseError::ExecutionError {
                    sql: "batch validation".to_string(),
                    reason: format!(
                        "Missing sequence number {} in batch {} (total: {})",
                        seq, batch_id, expected_total
                    ),
                    table: None,
                });
            }
        }
    }

    Ok(())
}

/// Applies remote changes in a single transaction
/// Also updates lastPullHlcTimestamp for the backend atomically
/// Validates batch completeness before applying changes
#[tauri::command]
pub fn apply_remote_changes_in_transaction(
    changes: Vec<RemoteColumnChange>,
    backend_id: String,
    max_hlc: String,
    state: State<'_, AppState>,
) -> Result<(), DatabaseError> {
    // Validate batch completeness
    validate_batch_completeness(&changes)?;

    with_connection(&state.db, |conn| {
        // Start transaction - all changes in the batch are applied atomically
        let tx = conn.transaction().map_err(DatabaseError::from)?;

        // Defer foreign key constraint checking until the end of the transaction
        // This allows applying changes in any order, but still validates all constraints at commit time
        // This setting is automatically reset when the transaction ends (commit or rollback)
        tx.execute("PRAGMA defer_foreign_keys = ON", [])
            .map_err(DatabaseError::from)?;

        // Group changes by (table, row) so we can insert/update all columns of a row together
        let mut row_changes: HashMap<(String, String), Vec<RemoteColumnChange>> = HashMap::new();
        for change in changes {
            let key = (change.table_name.clone(), change.row_pks.clone());
            row_changes.entry(key).or_insert_with(Vec::new).push(change);
        }

        // Apply changes grouped by row
        for ((_table_name, row_pks_str), row_change_list) in row_changes {
            // Use the first change to get common data
            let first_change = &row_change_list[0];

            // Parse row PKs (same for all changes in this row)
            let row_pks: serde_json::Map<String, JsonValue> =
                serde_json::from_str(&row_pks_str).map_err(|e| {
                    DatabaseError::SerializationError {
                        reason: format!("Failed to parse row PKs: {}", e),
                    }
                })?;

            // Get table schema to identify PK columns
            let schema = get_table_schema_internal(&tx, &first_change.table_name)
                .map_err(DatabaseError::from)?;
            let pk_columns: Vec<_> = schema.iter().filter(|col| col.is_pk).collect();

            // Build WHERE clause for PKs
            let pk_where: Vec<String> = pk_columns
                .iter()
                .map(|col| format!("{} = ?", col.name))
                .collect();
            let pk_where_clause = pk_where.join(" AND ");

            // Check if row exists and get current HLCs
            let check_sql = format!(
                "SELECT haex_column_hlcs FROM \"{}\" WHERE {}",
                first_change.table_name, pk_where_clause
            );

            let pk_values: Vec<JsonValue> = pk_columns
                .iter()
                .filter_map(|col| row_pks.get(&col.name).cloned())
                .collect();

            let current_hlcs: Option<String> = {
                let mut stmt = tx.prepare(&check_sql).map_err(DatabaseError::from)?;
                let params: Vec<String> = pk_values
                    .iter()
                    .map(|v| v.to_string().trim_matches('"').to_string())
                    .collect();
                let params_refs: Vec<&dyn rusqlite::ToSql> =
                    params.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

                stmt.query_row(&*params_refs, |row| row.get(0)).ok()
            };

            // Track if row exists
            let row_exists = current_hlcs.is_some();

            // Parse current HLCs
            let mut column_hlcs: serde_json::Map<String, JsonValue> = if let Some(hlcs_str) =
                current_hlcs
            {
                serde_json::from_str(&hlcs_str).unwrap_or_default()
            } else {
                serde_json::Map::new()
            };

            // Collect all column changes that are newer than current
            let mut columns_to_update: Vec<(String, String, String)> = Vec::new(); // (column_name, value, hlc)
            let mut max_hlc_for_row = first_change.hlc_timestamp.clone();

            for change in &row_change_list {
                let current_hlc = column_hlcs
                    .get(&change.column_name)
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if change.hlc_timestamp.as_str() > current_hlc {
                    // Remote change is newer, include it
                    column_hlcs.insert(
                        change.column_name.clone(),
                        JsonValue::String(change.hlc_timestamp.clone()),
                    );
                    columns_to_update.push((
                        change.column_name.clone(),
                        change.decrypted_value.to_string(),
                        change.hlc_timestamp.clone(),
                    ));

                    // Track max HLC for row timestamp
                    if change.hlc_timestamp > max_hlc_for_row {
                        max_hlc_for_row = change.hlc_timestamp.clone();
                    }
                }
            }

            // Only apply if there are columns to update
            if !columns_to_update.is_empty() {
                let new_hlcs_json =
                    serde_json::to_string(&column_hlcs).map_err(|e| {
                        DatabaseError::SerializationError {
                            reason: format!("Failed to serialize column HLCs: {}", e),
                        }
                    })?;

                if row_exists {
                    // Row exists, update it with all changed columns
                    let set_clauses: Vec<String> = columns_to_update
                        .iter()
                        .map(|(col_name, _, _)| format!("{} = ?", col_name))
                        .collect();

                    let update_sql = format!(
                        "UPDATE \"{}\" SET {}, haex_column_hlcs = ?, haex_timestamp = ? WHERE {}",
                        first_change.table_name,
                        set_clauses.join(", "),
                        pk_where_clause
                    );

                    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

                    // Add column values
                    for (_, value, _) in &columns_to_update {
                        params_vec.push(Box::new(value.clone()));
                    }

                    // Add HLCs and timestamp
                    params_vec.push(Box::new(new_hlcs_json));
                    params_vec.push(Box::new(max_hlc_for_row.clone()));

                    // Add PK values for WHERE clause
                    for pk_val in &pk_values {
                        params_vec.push(Box::new(pk_val.to_string().trim_matches('"').to_string()));
                    }

                    let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec
                        .iter()
                        .map(|b| b.as_ref() as &dyn rusqlite::ToSql)
                        .collect();

                    tx.execute(&update_sql, &*params_refs)
                        .map_err(DatabaseError::from)?;
                } else {
                    // Row doesn't exist, insert it with all changed columns + PKs
                    let mut columns = Vec::new();
                    let mut values: Vec<String> = Vec::new();

                    // Add PKs first
                    for col in &pk_columns {
                        columns.push(col.name.clone());
                        if let Some(pk_val) = row_pks.get(&col.name) {
                            values.push(pk_val.to_string().trim_matches('"').to_string());
                        }
                    }

                    // Add changed columns
                    for (col_name, value, _) in &columns_to_update {
                        columns.push(col_name.clone());
                        values.push(value.clone());
                    }

                    // Add CRDT metadata
                    columns.push("haex_column_hlcs".to_string());
                    columns.push("haex_timestamp".to_string());
                    values.push(new_hlcs_json);
                    values.push(max_hlc_for_row);

                    let placeholders = vec!["?"; columns.len()].join(", ");
                    let insert_sql = format!(
                        "INSERT INTO \"{}\" ({}) VALUES ({})",
                        first_change.table_name,
                        columns.join(", "),
                        placeholders
                    );

                    let params_refs: Vec<&dyn rusqlite::ToSql> =
                        values.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

                    // Try to insert - if it fails with UNIQUE constraint, create a conflict entry
                    match tx.execute(&insert_sql, &*params_refs) {
                        Ok(_) => {}, // Success - continue
                        Err(rusqlite::Error::SqliteFailure(err, msg))
                            if err.code == rusqlite::ErrorCode::ConstraintViolation => {
                            // Check if it's a UNIQUE constraint violation
                            if let Some(error_msg) = msg {
                                if error_msg.contains("UNIQUE constraint failed") {
                                    eprintln!("UNIQUE constraint conflict detected for table {}: {}",
                                        first_change.table_name, error_msg);

                                    // Create conflict entry - for now just log, later we'll implement full conflict tracking
                                    // TODO: Implement create_conflict_entry function
                                    continue; // Skip this row and continue with next
                                }
                            }
                            // Re-throw if it's not a UNIQUE constraint we can handle
                            return Err(DatabaseError::from(rusqlite::Error::SqliteFailure(err, None)));
                        }
                        Err(e) => return Err(DatabaseError::from(e)), // Other errors
                    }
                }
            }
        }

        // Update lastPullHlcTimestamp for this backend
        tx.execute(
            "UPDATE haex_sync_backends SET last_pull_hlc_timestamp = ? WHERE id = ?",
            params![&max_hlc, &backend_id],
        )
        .map_err(DatabaseError::from)?;

        // Commit transaction
        // Note: Foreign keys are re-enabled automatically by the ForeignKeyGuard when it drops
        tx.commit().map_err(DatabaseError::from)?;

        Ok(())
    })
}

