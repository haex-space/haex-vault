// src-tauri/src/database/core/select.rs

use crate::database::core::connection::with_connection;
use crate::database::core::parsing::parse_single_statement;
use crate::database::core::prefix::strip_main_schema_prefix;
use crate::database::core::value::{convert_value_ref_to_json, ValueConverter};
use crate::database::error::DatabaseError;
use crate::database::DbConnection;
use rusqlite::types::Value as RusqliteValue;
use rusqlite::ToSql;
use serde_json::Value as JsonValue;
use sqlparser::ast::Statement;

pub fn select(
    sql: String,
    params: Vec<JsonValue>,
    connection: &DbConnection,
) -> Result<Vec<Vec<JsonValue>>, DatabaseError> {
    // Validiere SQL-Statement
    let statement = parse_single_statement(&sql)?;

    // Stelle sicher, dass es eine Query ist
    if !matches!(statement, Statement::Query(_)) {
        return Err(DatabaseError::StatementError {
            reason: "Only SELECT statements are allowed in select function".to_string(),
        });
    }

    // Konvertiere Parameter
    let params_converted: Vec<RusqliteValue> = params
        .iter()
        .map(ValueConverter::json_to_rusqlite_value)
        .collect::<Result<Vec<_>, _>>()?;

    let params_sql: Vec<&dyn ToSql> = params_converted.iter().map(|v| v as &dyn ToSql).collect();

    with_connection(connection, |conn| {
        let mut stmt = conn.prepare(&sql)?;
        let num_columns = stmt.column_count();
        let mut rows = stmt.query(&params_sql[..])?;
        let mut result_vec: Vec<Vec<JsonValue>> = Vec::new();

        while let Some(row) = rows.next()? {
            let mut row_values: Vec<JsonValue> = Vec::with_capacity(num_columns);
            for i in 0..num_columns {
                let value_ref = row.get_ref(i)?;
                let json_val = convert_value_ref_to_json(value_ref)?;
                row_values.push(json_val);
            }
            result_vec.push(row_values);
        }
        Ok(result_vec)
    })
}

pub fn select_with_crdt(
    sql: String,
    params: Vec<JsonValue>,
    connection: &DbConnection,
) -> Result<Vec<Vec<JsonValue>>, DatabaseError> {
    use crate::crdt::transformer::CrdtTransformer;

    // Parse the SQL statement
    let statement = parse_single_statement(&sql)?;

    // Extract and transform the Query
    let transformed_sql = if let Statement::Query(mut query) = statement {
        let transformer = CrdtTransformer::new();
        transformer.transform_query(&mut query);
        // Remove "main." schema prefix that sqlparser adds
        strip_main_schema_prefix(&query.to_string())
    } else {
        return Err(DatabaseError::StatementError {
            reason: "Only SELECT statements are allowed in select_with_crdt".to_string(),
        });
    };

    // Convert params and execute
    let params_converted: Vec<RusqliteValue> = params
        .iter()
        .map(ValueConverter::json_to_rusqlite_value)
        .collect::<Result<Vec<_>, _>>()?;
    let params_sql: Vec<&dyn ToSql> = params_converted.iter().map(|v| v as &dyn ToSql).collect();

    with_connection(connection, |conn| {
        let mut stmt = conn.prepare(&transformed_sql)?;
        let num_columns = stmt.column_count();
        let mut rows = stmt.query(&params_sql[..])?;
        let mut result_vec: Vec<Vec<JsonValue>> = Vec::new();

        while let Some(row) = rows.next()? {
            let mut row_values: Vec<JsonValue> = Vec::with_capacity(num_columns);
            for i in 0..num_columns {
                let value_ref = row.get_ref(i)?;
                let json_val = convert_value_ref_to_json(value_ref)?;
                row_values.push(json_val);
            }
            result_vec.push(row_values);
        }
        Ok(result_vec)
    })
}
