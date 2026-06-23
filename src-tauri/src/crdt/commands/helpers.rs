use crate::crdt::trigger::ColumnInfo;
use crate::database::core::ValueConverter;
use crate::database::error::DatabaseError;
use rusqlite::types::Value as SqlValue;
use serde_json::Value as JsonValue;

/// Converts a vector of JSON values to SQL values for use in queries.
/// This ensures consistent handling of null values (JsonValue::Null -> SqlValue::Null)
/// instead of incorrectly converting them to the string "null".
pub(super) fn json_values_to_sql_params(
    values: &[JsonValue],
) -> Result<Vec<SqlValue>, DatabaseError> {
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
pub(super) fn build_pk_where_clause(
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
