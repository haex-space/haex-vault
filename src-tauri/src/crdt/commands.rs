use crate::crdt::trigger::{get_table_schema as get_table_schema_internal, ColumnInfo, HLC_TIMESTAMP_COLUMN, COLUMN_HLCS_COLUMN};
use crate::database::core::{with_connection, ValueConverter};
use crate::database::error::DatabaseError;
use crate::table_names::TABLE_CRDT_CONFIGS;
use crate::AppState;
use rusqlite::params;
use rusqlite::types::Value as SqlValue;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;
use ts_rs::TS;
use uuid::Uuid;

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
    let remote_row_json = serde_json::to_string(remote_row_data).map_err(|e| {
        DatabaseError::SerializationError {
            reason: format!("Failed to serialize remote row: {}", e),
        }
    })?;

    // Extract PKs from schema
    let pk_columns: Vec<_> = schema.iter().filter(|col| col.is_pk).collect();

    // Build remote PK JSON
    let remote_pk: serde_json::Map<String, JsonValue> = pk_columns.iter()
        .filter_map(|pk_col| remote_row_data.get(&pk_col.name).map(|v| (pk_col.name.clone(), v.clone())))
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

    let local_pk_json = tx.query_row(&query_sql, [], |row| {
        let mut local_pk = serde_json::Map::new();
        for (i, pk_col) in pk_columns.iter().enumerate() {
            if let Ok(val) = row.get::<_, String>(i) {
                local_pk.insert(pk_col.name.clone(), JsonValue::String(val));
            }
        }
        Ok(serde_json::to_string(&local_pk).unwrap_or_else(|_| "{}".to_string()))
    }).unwrap_or_else(|_| "{}".to_string());

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

    eprintln!("[SYNC RUST] Created conflict entry {} for table {}", conflict_id, table_name);

    Ok(())
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
/// Validates batch completeness before applying changes
/// Note: lastPullServerTimestamp is now updated by the TypeScript layer after successful apply
#[tauri::command]
pub fn apply_remote_changes_in_transaction(
    changes: Vec<RemoteColumnChange>,
    backend_id: String,
    max_hlc: String,
    state: State<'_, AppState>,
) -> Result<(), DatabaseError> {
    eprintln!("[SYNC RUST] ========== APPLY REMOTE CHANGES START ==========");
    eprintln!("[SYNC RUST] Changes count: {}, backend_id: {}, max_hlc: {}", changes.len(), backend_id, max_hlc);

    // Validate batch completeness
    eprintln!("[SYNC RUST] Validating batch completeness...");
    validate_batch_completeness(&changes)?;
    eprintln!("[SYNC RUST] Batch validation passed");

    with_connection(&state.db, |conn| {
        // Disable foreign key constraints BEFORE starting the transaction
        // IMPORTANT: PRAGMA foreign_keys cannot be changed inside a transaction!
        // See: https://sqlite.org/foreignkeys.html
        // "It is not possible to enable or disable foreign key constraints in the middle
        // of a multi-statement transaction. Attempting to do so does not return an error;
        // it simply has no effect."
        eprintln!("[SYNC RUST] Disabling foreign_keys BEFORE transaction");
        conn.pragma_update(None, "foreign_keys", "OFF")
            .map_err(DatabaseError::from)?;

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
        tx.execute(&disable_sql, [])
            .map_err(DatabaseError::from)?;

        // Group changes by (table, row) so we can insert/update all columns of a row together
        let mut row_changes: HashMap<(String, String), Vec<RemoteColumnChange>> = HashMap::new();
        for change in changes {
            let key = (change.table_name.clone(), change.row_pks.clone());
            row_changes.entry(key).or_insert_with(Vec::new).push(change);
        }

        // Apply changes grouped by row
        eprintln!("[SYNC RUST] Applying {} rows total", row_changes.len());

        for ((_table_name, row_pks_str), row_change_list) in row_changes {
            // Use the first change to get common data
            let first_change = &row_change_list[0];
            eprintln!("[SYNC RUST] Processing table: {}, PKs: {}, columns: {}",
                first_change.table_name, row_pks_str, row_change_list.len());

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

            // Parse row PKs (same for all changes in this row)
            let row_pks: serde_json::Map<String, JsonValue> =
                serde_json::from_str(&row_pks_str).map_err(|e| {
                    DatabaseError::SerializationError {
                        reason: format!("Failed to parse row PKs: {}", e),
                    }
                })?;

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
            let mut columns_to_update: Vec<(String, JsonValue, String)> = Vec::new(); // (column_name, json_value, hlc)
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
                        change.decrypted_value.clone(),
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

                    let mut params_vec: Vec<SqlValue> = Vec::new();

                    // Add column values (convert JSON to SQL values)
                    for (_, json_value, _) in &columns_to_update {
                        params_vec.push(ValueConverter::json_to_rusqlite_value(json_value)?);
                    }

                    // Add HLCs and timestamp
                    params_vec.push(SqlValue::Text(new_hlcs_json));
                    params_vec.push(SqlValue::Text(max_hlc_for_row.clone()));

                    // Add PK values for WHERE clause
                    for pk_val in &pk_values {
                        params_vec.push(SqlValue::Text(pk_val.to_string().trim_matches('"').to_string()));
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

                    // Debug: Log what columns we're about to insert
                    eprintln!("[SYNC RUST] INSERT for table {}: schema={} cols, updating={} cols",
                        first_change.table_name, schema.len(), columns_to_update.len());
                    eprintln!("[SYNC RUST]   Column names: {:?}", columns_to_update.iter().map(|(n, _, _)| n).collect::<Vec<_>>());

                    // Add PKs first
                    for col in &pk_columns {
                        columns.push(col.name.clone());
                        if let Some(pk_val) = row_pks.get(&col.name) {
                            values.push(SqlValue::Text(pk_val.to_string().trim_matches('"').to_string()));
                        }
                    }

                    // Add changed columns (convert JSON to SQL values)
                    for (col_name, json_value, _) in &columns_to_update {
                        columns.push(col_name.clone());
                        values.push(ValueConverter::json_to_rusqlite_value(json_value)?);
                    }

                    // Add CRDT metadata
                    columns.push(COLUMN_HLCS_COLUMN.to_string());
                    columns.push(HLC_TIMESTAMP_COLUMN.to_string());
                    values.push(SqlValue::Text(new_hlcs_json));
                    values.push(SqlValue::Text(max_hlc_for_row.clone()));

                    let placeholders = vec!["?"; columns.len()].join(", ");
                    let insert_sql = format!(
                        "INSERT INTO \"{}\" ({}) VALUES ({})",
                        first_change.table_name,
                        columns.join(", "),
                        placeholders
                    );

                    let params_refs: Vec<&dyn rusqlite::ToSql> =
                        values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

                    // Try to insert - if it fails with constraint, log detailed error
                    match tx.execute(&insert_sql, &*params_refs) {
                        Ok(_) => {}, // Success - continue
                        Err(rusqlite::Error::SqliteFailure(err, msg))
                            if err.code == rusqlite::ErrorCode::ConstraintViolation => {
                            // Log the constraint violation details
                            let error_msg = msg.as_deref().unwrap_or("Unknown constraint violation");
                            eprintln!("[SYNC RUST] Constraint violation for table {}: {}",
                                first_change.table_name, error_msg);
                            eprintln!("[SYNC RUST] Failed INSERT SQL: {}", insert_sql);
                            eprintln!("[SYNC RUST] Values: {:?}", values);

                            // Check if it's a UNIQUE constraint violation
                            if error_msg.contains("UNIQUE constraint failed") {
                                eprintln!("[SYNC RUST] UNIQUE constraint conflict - creating conflict entry");

                                // Build remote row data from all columns being inserted
                                let mut remote_row_data = serde_json::Map::new();
                                for (i, col_name) in columns.iter().enumerate() {
                                    if let Some(sql_value) = values.get(i) {
                                        let json_value = ValueConverter::rusqlite_value_to_json(sql_value);
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
                                    eprintln!("[SYNC RUST] Failed to create conflict entry: {:?}", e);
                                }

                                continue; // Skip this row and continue with next
                            }

                            // For other constraints (CHECK, etc.), re-throw the error
                            return Err(DatabaseError::from(rusqlite::Error::SqliteFailure(err, msg)));
                        }
                        Err(e) => {
                            eprintln!("[SYNC RUST] INSERT failed for table {}: {:?}", first_change.table_name, e);
                            return Err(DatabaseError::from(e));
                        }
                    }
                }
            }
        }

        // Update lastPushHlcTimestamp for this backend to prevent re-pushing the data we just pulled
        // Note: lastPullServerTimestamp is now updated by TypeScript using the server timestamp
        eprintln!("[SYNC RUST] Updating last_push_hlc_timestamp to {}", max_hlc);
        tx.execute(
            "UPDATE haex_sync_backends SET last_push_hlc_timestamp = ? WHERE id = ?",
            params![&max_hlc, &backend_id],
        )
        .map_err(DatabaseError::from)?;

        // Re-enable triggers before committing
        eprintln!("[SYNC RUST] Re-enabling triggers");
        let enable_sql = format!(
            "INSERT INTO {TABLE_CRDT_CONFIGS} (key, type, value) VALUES ('triggers_enabled', 'system', '1')
             ON CONFLICT(key) DO UPDATE SET value = '1'"
        );
        tx.execute(&enable_sql, [])
            .map_err(DatabaseError::from)?;

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

        // Re-enable foreign key constraints after transaction is complete
        // This is done on the connection, not in a transaction
        eprintln!("[SYNC RUST] Re-enabling foreign_keys");
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(DatabaseError::from)?;

        Ok(())
    })
}

