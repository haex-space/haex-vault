// src-tauri/src/database/core.rs

/// Statement breakpoint marker used by Drizzle migrations
pub const DRIZZLE_STATEMENT_BREAKPOINT: &str = "--> statement-breakpoint";

use crate::crdt::hlc::HlcService;
use crate::crdt::trigger::{HLC_FUNCTION_NAME, UUID_FUNCTION_NAME};
use crate::database::connection_context::ConnectionContext;
use crate::database::error::DatabaseError;
use crate::database::DbConnection;
use crate::extension::database::executor::SqlExecutor;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use regex::Regex;
use rusqlite::functions::FunctionFlags;
use rusqlite::types::Value as SqlValue;
use rusqlite::{
    types::{Value as RusqliteValue, ValueRef},
    Connection, OpenFlags, ToSql,
};
use serde_json::Value as JsonValue;
use sqlparser::ast::{
    Expr, FromTable, ObjectName, ObjectNamePart, Query, Select, SetExpr, Statement, TableFactor,
    TableObject,
};
use sqlparser::dialect::SQLiteDialect;
use sqlparser::parser::Parser;
use std::sync::LazyLock;
use uuid::Uuid;

/// Removes the "main." schema prefix that sqlparser-rs adds when serializing SQL.
/// SQLite doesn't need this prefix and it causes "no such table" errors.
///
/// Uses AST-based transformation to safely strip `main.` only from table references,
/// preserving occurrences inside string literals. Falls back to regex for unparseable SQL.
pub fn strip_main_schema_prefix(sql: &str) -> String {
    let dialect = SQLiteDialect {};
    if let Ok(mut statements) = Parser::parse_sql(&dialect, sql) {
        for statement in &mut statements {
            strip_main_from_statement(statement);
        }
        statements
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .join("; ")
    } else {
        // Fallback: regex for unparseable SQL (e.g. PRAGMAs)
        static RE: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r#"\bmain\.(["'`]?\w)"#).expect("Invalid regex for main. prefix")
        });
        RE.replace_all(sql, "$1").to_string()
    }
}

/// Removes the `main` schema qualifier from an `ObjectName` if present.
/// e.g. `["main", "users"]` becomes `["users"]`
fn strip_main_from_object_name(name: &mut ObjectName) {
    if name.0.len() >= 2 {
        if let Some(ObjectNamePart::Identifier(ident)) = name.0.first() {
            if ident.value.eq_ignore_ascii_case("main") {
                name.0.remove(0);
            }
        }
    }
}

/// Walk a Statement AST and strip `main.` schema prefixes from all table references.
fn strip_main_from_statement(statement: &mut Statement) {
    match statement {
        Statement::Query(query) => {
            strip_main_from_query(query);
        }
        Statement::Insert(insert) => {
            if let TableObject::TableName(ref mut name) = insert.table {
                strip_main_from_object_name(name);
            }
            if let Some(ref mut source) = insert.source {
                strip_main_from_query(source);
            }
        }
        Statement::Update(update) => {
            strip_main_from_table_factor(&mut update.table.relation);
            if let Some(ref mut selection) = update.selection {
                strip_main_from_expr(selection);
            }
        }
        Statement::Delete(delete) => {
            match &mut delete.from {
                FromTable::WithFromKeyword(ref mut table_refs)
                | FromTable::WithoutKeyword(ref mut table_refs) => {
                    for table_ref in table_refs.iter_mut() {
                        strip_main_from_table_factor(&mut table_ref.relation);
                        for join in &mut table_ref.joins {
                            strip_main_from_table_factor(&mut join.relation);
                        }
                    }
                }
            }
            for name in &mut delete.tables {
                strip_main_from_object_name(name);
            }
            if let Some(ref mut selection) = delete.selection {
                strip_main_from_expr(selection);
            }
        }
        Statement::CreateTable(create) => {
            strip_main_from_object_name(&mut create.name);
        }
        Statement::AlterTable(alter) => {
            strip_main_from_object_name(&mut alter.name);
        }
        Statement::Drop { ref mut names, .. } => {
            for name in names.iter_mut() {
                strip_main_from_object_name(name);
            }
        }
        Statement::CreateIndex(create_index) => {
            strip_main_from_object_name(&mut create_index.table_name);
        }
        _ => {}
    }
}

fn strip_main_from_query(query: &mut Query) {
    strip_main_from_set_expr(&mut query.body);
}

fn strip_main_from_set_expr(set_expr: &mut SetExpr) {
    match set_expr {
        SetExpr::Select(select) => {
            strip_main_from_select(select);
        }
        SetExpr::Query(query) => {
            strip_main_from_query(query);
        }
        SetExpr::SetOperation {
            ref mut left,
            ref mut right,
            ..
        } => {
            strip_main_from_set_expr(left);
            strip_main_from_set_expr(right);
        }
        _ => {}
    }
}

fn strip_main_from_select(select: &mut Select) {
    for table_ref in &mut select.from {
        strip_main_from_table_factor(&mut table_ref.relation);
        for join in &mut table_ref.joins {
            strip_main_from_table_factor(&mut join.relation);
        }
    }
    if let Some(ref mut selection) = select.selection {
        strip_main_from_expr(selection);
    }
}

fn strip_main_from_table_factor(table_factor: &mut TableFactor) {
    match table_factor {
        TableFactor::Table { ref mut name, .. } => {
            strip_main_from_object_name(name);
        }
        TableFactor::Derived {
            ref mut subquery, ..
        } => {
            strip_main_from_query(subquery);
        }
        TableFactor::NestedJoin {
            ref mut table_with_joins,
            ..
        } => {
            strip_main_from_table_factor(&mut table_with_joins.relation);
            for join in &mut table_with_joins.joins {
                strip_main_from_table_factor(&mut join.relation);
            }
        }
        _ => {}
    }
}

fn strip_main_from_expr(expr: &mut Expr) {
    match expr {
        Expr::Subquery(ref mut subquery) => {
            strip_main_from_query(subquery);
        }
        Expr::BinaryOp {
            ref mut left,
            ref mut right,
            ..
        } => {
            strip_main_from_expr(left);
            strip_main_from_expr(right);
        }
        Expr::UnaryOp { ref mut expr, .. } => {
            strip_main_from_expr(expr);
        }
        Expr::InSubquery {
            ref mut expr,
            ref mut subquery,
            ..
        } => {
            strip_main_from_expr(expr);
            strip_main_from_query(subquery);
        }
        Expr::Between {
            ref mut expr,
            ref mut low,
            ref mut high,
            ..
        } => {
            strip_main_from_expr(expr);
            strip_main_from_expr(low);
            strip_main_from_expr(high);
        }
        Expr::Nested(ref mut inner) => {
            strip_main_from_expr(inner);
        }
        _ => {}
    }
}

/// Öffnet und initialisiert eine Datenbank mit Verschlüsselung.
///
/// Registers the `gen_uuid` and `current_hlc` UDFs and wires commit/rollback
/// hooks so the transaction-scoped HLC slot is cleared at the end of every
/// transaction.
pub fn open_and_init_db(
    path: &str,
    key: &str,
    create: bool,
    hlc_service: HlcService,
    context: ConnectionContext,
) -> Result<Connection, DatabaseError> {
    let flags = if create {
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
    } else {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    };

    let conn =
        Connection::open_with_flags(path, flags).map_err(|e| DatabaseError::ConnectionFailed {
            path: path.to_string(),
            reason: e.to_string(),
        })?;

    conn.pragma_update(None, "key", key)
        .map_err(|e| DatabaseError::PragmaError {
            pragma: "key".to_string(),
            reason: e.to_string(),
        })?;

    // Enable foreign key constraints
    // This must be set for PRAGMA defer_foreign_keys to work
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(|e| DatabaseError::PragmaError {
            pragma: "foreign_keys".to_string(),
            reason: e.to_string(),
        })?;

    // Verify foreign keys are enabled
    let fk_enabled: i32 = conn
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .map_err(|e| DatabaseError::PragmaError {
            pragma: "foreign_keys (verify)".to_string(),
            reason: e.to_string(),
        })?;

    if fk_enabled == 1 {
        println!("✅ Foreign key constraints enabled.");
    } else {
        eprintln!("❌ Failed to enable foreign key constraints.");
    }

    // Register custom UUID function for SQLite triggers
    conn.create_scalar_function(
        UUID_FUNCTION_NAME,
        0,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_INNOCUOUS,
        |_ctx| Ok(Uuid::new_v4().to_string()),
    )
    .map_err(|e| DatabaseError::DatabaseError {
        reason: format!("Failed to register {UUID_FUNCTION_NAME} function: {e}"),
    })?;

    // Register transaction-scoped HLC UDF. All calls within a single SQLite
    // transaction (explicit or auto-commit) return the same timestamp.
    register_current_hlc_udf(&conn, hlc_service, context.clone())?;
    install_tx_hlc_hooks(&conn, context)?;

    let journal_mode: String = conn
        .query_row("PRAGMA journal_mode=WAL;", [], |row| row.get(0))
        .map_err(|e| DatabaseError::PragmaError {
            pragma: "journal_mode=WAL".to_string(),
            reason: e.to_string(),
        })?;

    if journal_mode.eq_ignore_ascii_case("wal") {
        println!("WAL mode successfully enabled.");
    } else {
        eprintln!("Failed to enable WAL mode, journal_mode is '{journal_mode}'.");
    }

    Ok(conn)
}

/// Registers the `current_hlc()` UDF on a connection. Extracted so tests that
/// create bare in-memory connections can use the same registration logic.
pub fn register_current_hlc_udf(
    conn: &Connection,
    hlc_service: HlcService,
    context: ConnectionContext,
) -> Result<(), DatabaseError> {
    // Flags explained:
    // - UTF8: default string encoding for TEXT args/return.
    // - INNOCUOUS: safe to call from trigger/view context when
    //   `trusted_schema=OFF` (no side effects, no access to attacker-
    //   controlled state).
    // - DETERMINISTIC: zero-arg deterministic functions are constant-folded
    //   by the query planner, so two `current_hlc()` calls inside the same
    //   statement evaluate to the same value even without our slot cache.
    //   The cache covers the cross-statement case inside a write tx.
    conn.create_scalar_function(
        HLC_FUNCTION_NAME,
        0,
        FunctionFlags::SQLITE_UTF8
            | FunctionFlags::SQLITE_INNOCUOUS
            | FunctionFlags::SQLITE_DETERMINISTIC,
        move |_ctx| {
            context
                .current_or_new_tx_hlc(&hlc_service)
                .map(|ts| ts.to_string())
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(e)))
        },
    )
    .map_err(|e| DatabaseError::DatabaseError {
        reason: format!("Failed to register {HLC_FUNCTION_NAME} function: {e}"),
    })
}

/// Wires commit_hook, rollback_hook and update_hook so the per-transaction
/// HLC slot behaves correctly:
/// - commit_hook / rollback_hook: clear the slot and the write-pending flag at
///   the end of every transaction.
/// - update_hook: flip the write-pending flag on the first row-level
///   INSERT/UPDATE/DELETE in a transaction, so that a stray read-only
///   `SELECT current_hlc()` cannot poison the HLC of a later write.
pub fn install_tx_hlc_hooks(conn: &Connection, context: ConnectionContext) -> Result<(), DatabaseError> {
    let ctx_commit = context.clone();
    conn.commit_hook(Some(move || {
        ctx_commit.reset_tx_slot();
        false
    }))
    .map_err(|e| DatabaseError::DatabaseError {
        reason: format!("Failed to install commit_hook: {e}"),
    })?;

    let ctx_rollback = context.clone();
    conn.rollback_hook(Some(move || {
        ctx_rollback.reset_tx_slot();
    }))
    .map_err(|e| DatabaseError::DatabaseError {
        reason: format!("Failed to install rollback_hook: {e}"),
    })?;

    let ctx_update = context;
    conn.update_hook(Some(move |_action, _db: &str, _table: &str, _row_id: i64| {
        ctx_update.mark_write_pending();
    }))
    .map_err(|e| DatabaseError::DatabaseError {
        reason: format!("Failed to install update_hook: {e}"),
    })?;
    Ok(())
}

/// Utility für SQL-Parsing - parst ein einzelnes SQL-Statement
pub fn parse_single_statement(sql: &str) -> Result<Statement, DatabaseError> {
    let dialect = SQLiteDialect {};
    let statements = Parser::parse_sql(&dialect, sql).map_err(|e| DatabaseError::ParseError {
        reason: e.to_string(),
        sql: sql.to_string(),
    })?;

    statements
        .into_iter()
        .next()
        .ok_or(DatabaseError::ParseError {
            reason: "No SQL statement found".to_string(),
            sql: sql.to_string(),
        })
}

/// Utility für SQL-Parsing - parst mehrere SQL-Statements
pub fn parse_sql_statements(sql: &str) -> Result<Vec<Statement>, DatabaseError> {
    let dialect = SQLiteDialect {};

    // Normalize whitespace: replace multiple whitespaces (including newlines, tabs) with single space
    let normalized_sql = sql.split_whitespace().collect::<Vec<&str>>().join(" ");

    Parser::parse_sql(&dialect, &normalized_sql).map_err(|e| DatabaseError::ParseError {
        reason: format!("Failed to parse SQL: {e}"),
        sql: sql.to_string(),
    })
}

/// Prüft ob ein Statement ein RETURNING Clause hat (AST-basiert, sicher)
pub fn statement_has_returning(statement: &Statement) -> bool {
    match statement {
        Statement::Insert(insert) => insert.returning.is_some(),
        Statement::Update(update) => update.returning.is_some(),
        Statement::Delete(delete) => delete.returning.is_some(),
        _ => false,
    }
}

pub struct ValueConverter;

impl ValueConverter {
    pub fn json_to_rusqlite_value(json_val: &JsonValue) -> Result<SqlValue, DatabaseError> {
        match json_val {
            JsonValue::Null => Ok(SqlValue::Null),
            JsonValue::Bool(b) => {
                // SQLite hat keinen Bool-Typ; verwende Integer 0/1
                Ok(SqlValue::Integer(if *b { 1 } else { 0 }))
            }
            JsonValue::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(SqlValue::Integer(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(SqlValue::Real(f))
                } else {
                    // Fallback: als Text
                    Ok(SqlValue::Text(n.to_string()))
                }
            }
            JsonValue::String(s) => Ok(SqlValue::Text(s.clone())),
            JsonValue::Array(_) | JsonValue::Object(_) => {
                // Arrays/Objects als JSON-Text speichern
                serde_json::to_string(json_val)
                    .map(SqlValue::Text)
                    .map_err(|e| DatabaseError::SerializationError {
                        reason: format!("Failed to serialize JSON param: {e}"),
                    })
            }
        }
    }

    pub fn convert_params(params: &[JsonValue]) -> Result<Vec<SqlValue>, DatabaseError> {
        params.iter().map(Self::json_to_rusqlite_value).collect()
    }

    /// Converts an owned SqlValue to JSON by delegating to convert_value_ref_to_json.
    pub fn rusqlite_value_to_json(sql_value: &SqlValue) -> JsonValue {
        let value_ref = match sql_value {
            SqlValue::Null => ValueRef::Null,
            SqlValue::Integer(n) => ValueRef::Integer(*n),
            SqlValue::Real(f) => ValueRef::Real(*f),
            SqlValue::Text(s) => ValueRef::Text(s.as_bytes()),
            SqlValue::Blob(b) => ValueRef::Blob(b),
        };
        convert_value_ref_to_json(value_ref).unwrap_or(JsonValue::Null)
    }
}

/// Execute SQL mit CRDT-Transformation (für Drizzle-Integration)
/// Diese Funktion sollte von Drizzle verwendet werden, um CRDT-Support zu erhalten
/// Unterstützt RETURNING-Klausel: Falls vorhanden, werden die Ergebnis-Rows zurückgegeben
pub fn execute_with_crdt(
    sql: String,
    params: Vec<JsonValue>,
    connection: &DbConnection,
    hlc_service: &std::sync::MutexGuard<crate::crdt::hlc::HlcService>,
) -> Result<Vec<Vec<JsonValue>>, DatabaseError> {
    // Parse statement to check for RETURNING clause (AST-basiert)
    let statement = parse_single_statement(&sql)?;
    let has_returning = statement_has_returning(&statement);

    with_connection(connection, |conn| {
        let tx = conn.transaction().map_err(DatabaseError::from)?;

        let result = if has_returning {
            let (_modified_tables, rows) =
                SqlExecutor::query_internal(&tx, hlc_service, &sql, &params)?;
            rows
        } else {
            let _modified_tables =
                SqlExecutor::execute_internal(&tx, hlc_service, &sql, &params)?;
            vec![]
        };

        tx.commit().map_err(DatabaseError::from)?;
        Ok(result)
    })
}

/// Execute SQL OHNE CRDT-Transformation (für spezielle Fälle)
pub fn execute(
    sql: String,
    params: Vec<JsonValue>,
    connection: &DbConnection,
) -> Result<Vec<Vec<JsonValue>>, DatabaseError> {
    // Konvertiere Parameter
    let params_converted: Vec<RusqliteValue> = params
        .iter()
        .map(ValueConverter::json_to_rusqlite_value)
        .collect::<Result<Vec<_>, _>>()?;
    let params_sql: Vec<&dyn ToSql> = params_converted.iter().map(|v| v as &dyn ToSql).collect();

    let has_returning = {
        let stmt = parse_single_statement(&sql)?;
        statement_has_returning(&stmt)
    };

    with_connection(connection, |conn| {
        if has_returning {
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
        } else {
            conn.execute(&sql, &params_sql[..]).map_err(|e| {
                let table_name = extract_primary_table_name_from_sql(&sql).unwrap_or(None);
                DatabaseError::ExecutionError {
                    sql: sql.clone(),
                    reason: e.to_string(),
                    table: table_name,
                }
            })?;
            Ok(vec![])
        }
    })
}

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

/// Konvertiert rusqlite ValueRef zu JSON
pub fn convert_value_ref_to_json(value_ref: ValueRef) -> Result<JsonValue, DatabaseError> {
    let json_val = match value_ref {
        ValueRef::Null => JsonValue::Null,
        ValueRef::Integer(i) => JsonValue::Number(i.into()),
        ValueRef::Real(f) => JsonValue::Number(
            serde_json::Number::from_f64(f).unwrap_or_else(|| serde_json::Number::from(0)),
        ),
        ValueRef::Text(t) => {
            let s = String::from_utf8_lossy(t).to_string();
            JsonValue::String(s)
        }
        ValueRef::Blob(b) => {
            // BLOBs als Base64-String zurückgeben
            JsonValue::String(STANDARD.encode(b))
        }
    };
    Ok(json_val)
}
// Extrahiert alle Tabellennamen aus einem SQL-Statement über AST-Parsing
pub fn extract_table_names_from_sql(sql: &str) -> Result<Vec<String>, DatabaseError> {
    let statement = parse_single_statement(sql)?;
    Ok(extract_table_names_from_statement(&statement))
}

/// Extrahiert den ersten/primären Tabellennamen aus einem SQL-Statement
pub fn extract_primary_table_name_from_sql(sql: &str) -> Result<Option<String>, DatabaseError> {
    let table_names = extract_table_names_from_sql(sql)?;
    Ok(table_names.into_iter().next())
}

/// Extrahiert alle Tabellennamen aus einem AST Statement
pub fn extract_table_names_from_statement(statement: &Statement) -> Vec<String> {
    let mut tables = Vec::new();

    match statement {
        Statement::Query(query) => {
            extract_tables_from_query_recursive(query, &mut tables);
        }
        Statement::Insert(insert) => {
            if let TableObject::TableName(name) = &insert.table {
                tables.push(name.to_string());
            }
            // Traverse INSERT...SELECT subqueries
            if let Some(source) = &insert.source {
                extract_tables_from_query_recursive(source, &mut tables);
            }
        }
        Statement::Update(update) => {
            extract_tables_from_table_factor(&update.table.relation, &mut tables);
            // Traverse SET clause subqueries (e.g. SET col = (SELECT ...))
            for assignment in &update.assignments {
                extract_tables_from_expr_recursive(&assignment.value, &mut tables);
            }
            // Traverse WHERE clause subqueries
            if let Some(selection) = &update.selection {
                extract_tables_from_expr_recursive(selection, &mut tables);
            }
        }
        Statement::Delete(delete) => {
            use sqlparser::ast::FromTable;
            match &delete.from {
                FromTable::WithFromKeyword(table_refs) | FromTable::WithoutKeyword(table_refs) => {
                    for table_ref in table_refs {
                        extract_tables_from_table_factor(&table_ref.relation, &mut tables);
                    }
                }
            }
            // Fallback für DELETE-Syntax ohne FROM
            for table_name in &delete.tables {
                tables.push(table_name.to_string());
            }
            // Traverse WHERE clause subqueries
            if let Some(selection) = &delete.selection {
                extract_tables_from_expr_recursive(selection, &mut tables);
            }
        }
        Statement::CreateTable(create) => {
            tables.push(create.name.to_string());
        }
        Statement::AlterTable(alter) => {
            tables.push(alter.name.to_string());
        }
        Statement::Drop { names, .. } => {
            for name in names {
                tables.push(name.to_string());
            }
        }
        Statement::CreateIndex(create_index) => {
            tables.push(create_index.table_name.to_string());
        }
        Statement::Truncate(truncate) => {
            for table_name in &truncate.table_names {
                tables.push(table_name.to_string());
            }
        }
        // Weitere Statement-Typen können hier hinzugefügt werden
        _ => {
            // Für unbekannte Statement-Typen geben wir eine leere Liste zurück
        }
    }

    tables
}

/// Extrahiert Tabellennamen rekursiv aus Query-Strukturen
fn extract_tables_from_query_recursive(query: &Query, tables: &mut Vec<String>) {
    extract_tables_from_set_expr_recursive(&query.body, tables);
}

/// Extrahiert Tabellennamen aus SELECT-Statements
fn extract_tables_from_select(select: &Select, tables: &mut Vec<String>) {
    // FROM clause
    for table_ref in &select.from {
        extract_tables_from_table_factor(&table_ref.relation, tables);

        // JOINs
        for join in &table_ref.joins {
            extract_tables_from_table_factor(&join.relation, tables);
        }
    }
    if let Some(selection) = &select.selection {
        extract_tables_from_expr_recursive(selection, tables);
    }
}

fn extract_tables_from_expr_recursive(expr: &Expr, tables: &mut Vec<String>) {
    match expr {
        // This is the key: we found a subquery!
        Expr::Subquery(subquery) => {
            extract_tables_from_query_recursive(subquery, tables);
        }
        // These expressions can contain other expressions
        Expr::BinaryOp { left, right, .. } => {
            extract_tables_from_expr_recursive(left, tables);
            extract_tables_from_expr_recursive(right, tables);
        }
        Expr::UnaryOp { expr, .. } => {
            extract_tables_from_expr_recursive(expr, tables);
        }
        Expr::InSubquery { expr, subquery, .. } => {
            extract_tables_from_expr_recursive(expr, tables);
            extract_tables_from_query_recursive(subquery, tables);
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            extract_tables_from_expr_recursive(expr, tables);
            extract_tables_from_expr_recursive(low, tables);
            extract_tables_from_expr_recursive(high, tables);
        }
        // ... other expression types can be added here if needed
        _ => {
            // Other expressions (like literals, column names, etc.) don't contain tables.
        }
    }
}
/// Extrahiert Tabellennamen aus TableFactor-Strukturen
fn extract_tables_from_table_factor(table_factor: &TableFactor, tables: &mut Vec<String>) {
    match table_factor {
        TableFactor::Table { name, .. } => {
            tables.push(name.to_string());
        }
        TableFactor::Derived { subquery, .. } => {
            extract_tables_from_query_recursive(subquery, tables);
        }
        TableFactor::TableFunction { .. } => {
            // Table functions haben normalerweise keine direkten Tabellennamen
        }
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => {
            extract_tables_from_table_factor(&table_with_joins.relation, tables);
            for join in &table_with_joins.joins {
                extract_tables_from_table_factor(&join.relation, tables);
            }
        }
        _ => {
            // TableFunction, UNNEST, JsonTable, etc. haben normalerweise keine direkten Tabellennamen
            // oder sind nicht relevant für SQLite
        }
    }
}

/// Extrahiert Tabellennamen rekursiv aus SetExpr-Strukturen.
/// Diese Funktion enthält die eigentliche rekursive Logik.
fn extract_tables_from_set_expr_recursive(set_expr: &SetExpr, tables: &mut Vec<String>) {
    match set_expr {
        SetExpr::Select(select) => {
            extract_tables_from_select(select, tables);
        }
        SetExpr::Query(sub_query) => {
            extract_tables_from_set_expr_recursive(&sub_query.body, tables);
        }
        SetExpr::SetOperation { left, right, .. } => {
            extract_tables_from_set_expr_recursive(left, tables);
            extract_tables_from_set_expr_recursive(right, tables);
        }

        SetExpr::Values(_)
        | SetExpr::Table(_)
        | SetExpr::Insert(_)
        | SetExpr::Update(_)
        | SetExpr::Merge(_)
        | SetExpr::Delete(_) => {}
    }
}

pub fn with_connection<T, F>(connection: &DbConnection, f: F) -> Result<T, DatabaseError>
where
    F: FnOnce(&mut Connection) -> Result<T, DatabaseError>,
{
    let mut db_lock = connection
        .0
        .lock()
        .map_err(|e| DatabaseError::MutexPoisoned {
            reason: e.to_string(),
        })?;

    let conn = db_lock.as_mut().ok_or(DatabaseError::ConnectionError {
        reason: "Connection to vault failed".to_string(),
    })?;

    f(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_extract_simple_select() {
        let sql = "SELECT * FROM users";
        let tables = extract_table_names_from_sql(sql).unwrap();
        assert_eq!(tables, vec!["users"]);
    }

    #[test]
    fn test_extract_select_with_join() {
        let sql = "SELECT u.name, p.title FROM users u JOIN posts p ON u.id = p.user_id";
        let tables = extract_table_names_from_sql(sql).unwrap();
        assert_eq!(tables, vec!["users", "posts"]);
    }

    #[test]
    fn test_extract_insert() {
        let sql = "INSERT INTO users (name, email) VALUES (?, ?)";
        let tables = extract_table_names_from_sql(sql).unwrap();
        assert_eq!(tables, vec!["users"]);
    }

    #[test]
    fn test_extract_update() {
        let sql = "UPDATE users SET name = ? WHERE id = ?";
        let tables = extract_table_names_from_sql(sql).unwrap();
        assert_eq!(tables, vec!["users"]);
    }

    #[test]
    fn test_extract_delete() {
        let sql = "DELETE FROM users WHERE id = ?";
        let tables = extract_table_names_from_sql(sql).unwrap();
        assert_eq!(tables, vec!["users"]);
    }

    #[test]
    fn test_extract_create_table() {
        let sql = "CREATE TABLE new_table (id INTEGER, name TEXT)";
        let tables = extract_table_names_from_sql(sql).unwrap();
        assert_eq!(tables, vec!["new_table"]);
    }

    #[test]
    fn test_extract_subquery() {
        let sql = "SELECT * FROM (SELECT id FROM users) AS sub";
        let tables = extract_table_names_from_sql(sql).unwrap();
        assert_eq!(tables, vec!["users"]);
    }

    #[test]
    fn test_extract_primary_table() {
        let sql = "SELECT u.name FROM users u JOIN posts p ON u.id = p.user_id";
        let primary_table = extract_primary_table_name_from_sql(sql).unwrap();
        assert_eq!(primary_table, Some("users".to_string()));
    }

    #[test]
    fn test_extract_complex_query() {
        let sql = r#"
            SELECT u.name, COUNT(p.id) as post_count 
            FROM users u 
            LEFT JOIN posts p ON u.id = p.user_id 
            WHERE u.created_at > (SELECT MIN(created_at) FROM sessions)
            GROUP BY u.id
        "#;
        let tables = extract_table_names_from_sql(sql).unwrap();
        assert_eq!(tables, vec!["users", "posts", "sessions"]);
    }

    #[test]
    fn test_invalid_sql() {
        let sql = "INVALID SQL";
        let result = extract_table_names_from_sql(sql);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_single_statement() {
        let sql = "SELECT * FROM users WHERE id = ?";
        let result = parse_single_statement(sql);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Statement::Query(_)));
    }

    #[test]
    fn test_parse_invalid_sql() {
        let sql = "INVALID SQL STATEMENT";
        let result = parse_single_statement(sql);
        assert!(matches!(result, Err(DatabaseError::ParseError { .. })));
    }

    #[test]
    fn test_convert_value_ref_to_json() {
        use rusqlite::types::ValueRef;

        assert_eq!(
            convert_value_ref_to_json(ValueRef::Null).unwrap(),
            JsonValue::Null
        );
        assert_eq!(
            convert_value_ref_to_json(ValueRef::Integer(42)).unwrap(),
            JsonValue::Number(42.into())
        );
        assert_eq!(
            convert_value_ref_to_json(ValueRef::Text(b"hello")).unwrap(),
            JsonValue::String("hello".to_string())
        );
    }

    // Test für die neuen AST-basierten Funktionen
    #[test]
    fn test_extract_table_names_comprehensive() {
        // Test verschiedene SQL-Statement-Typen
        assert_eq!(
            extract_primary_table_name_from_sql("SELECT * FROM users WHERE id = 1").unwrap(),
            Some("users".to_string())
        );
        assert_eq!(
            extract_primary_table_name_from_sql("INSERT INTO products (name) VALUES ('test')")
                .unwrap(),
            Some("products".to_string())
        );
        assert_eq!(
            extract_primary_table_name_from_sql("UPDATE orders SET status = 'completed'").unwrap(),
            Some("orders".to_string())
        );
        assert_eq!(
            extract_primary_table_name_from_sql("DELETE FROM customers").unwrap(),
            Some("customers".to_string())
        );
    }

    #[test]
    fn test_statement_has_returning_insert() {
        // INSERT ohne RETURNING
        let stmt = parse_single_statement("INSERT INTO users (name) VALUES ('test')").unwrap();
        assert!(!statement_has_returning(&stmt));

        // INSERT mit RETURNING
        let stmt_ret =
            parse_single_statement("INSERT INTO users (name) VALUES ('test') RETURNING id, name")
                .unwrap();
        assert!(statement_has_returning(&stmt_ret));

        // INSERT mit RETURNING *
        let stmt_ret_all =
            parse_single_statement("INSERT INTO users (name) VALUES ('test') RETURNING *").unwrap();
        assert!(statement_has_returning(&stmt_ret_all));
    }

    #[test]
    fn test_statement_has_returning_update() {
        // UPDATE ohne RETURNING
        let stmt = parse_single_statement("UPDATE users SET name = 'new' WHERE id = 1").unwrap();
        assert!(!statement_has_returning(&stmt));

        // UPDATE mit RETURNING
        let stmt_ret =
            parse_single_statement("UPDATE users SET name = 'new' WHERE id = 1 RETURNING id, name")
                .unwrap();
        assert!(statement_has_returning(&stmt_ret));
    }

    #[test]
    fn test_statement_has_returning_delete() {
        // DELETE ohne RETURNING
        let stmt = parse_single_statement("DELETE FROM users WHERE id = 1").unwrap();
        assert!(!statement_has_returning(&stmt));

        // DELETE mit RETURNING
        let stmt_ret =
            parse_single_statement("DELETE FROM users WHERE id = 1 RETURNING id, name").unwrap();
        assert!(statement_has_returning(&stmt_ret));
    }

    #[test]
    fn test_statement_has_returning_select() {
        // SELECT hat kein RETURNING (immer false)
        let stmt = parse_single_statement("SELECT * FROM users").unwrap();
        assert!(!statement_has_returning(&stmt));
    }

    #[test]
    fn test_gen_uuid_produces_distinct_values() {
        let conn = Connection::open_in_memory().unwrap();
        conn.create_scalar_function(
            UUID_FUNCTION_NAME,
            0,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_INNOCUOUS,
            |_ctx| Ok(Uuid::new_v4().to_string()),
        )
        .unwrap();

        conn.execute_batch(
            "CREATE TABLE test_uuids (id TEXT NOT NULL, other_id TEXT NOT NULL);",
        )
        .unwrap();

        conn.execute(
            &format!("INSERT INTO test_uuids (id, other_id) VALUES ({fn_name}(), {fn_name}());", fn_name = UUID_FUNCTION_NAME),
            [],
        )
        .unwrap();

        let (id, other_id): (String, String) = conn
            .query_row("SELECT id, other_id FROM test_uuids", [], |row| {
                Ok((row.get(0).unwrap(), row.get(1).unwrap()))
            })
            .unwrap();

        assert_ne!(
            id, other_id,
            "Two gen_uuid() calls in the same INSERT must produce different values"
        );
    }

    #[test]
    fn test_strip_main_schema_preserves_string_literals() {
        let sql = "SELECT * FROM main.users WHERE notes LIKE '%main.table%'";
        let result = strip_main_schema_prefix(sql);
        assert!(
            !result.contains("main.users"),
            "Should strip main. from table ref"
        );
        assert!(
            result.contains("%main.table%"),
            "Should NOT strip main. inside string literal"
        );
    }

    // ---- current_hlc() UDF + transaction-scope HLC ---------------------

    fn setup_hlc_test_connection(device_id: &str) -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory connection");
        let hlc = HlcService::new_for_testing(device_id);
        let ctx = ConnectionContext::new();
        register_current_hlc_udf(&conn, hlc, ctx.clone()).expect("register current_hlc");
        install_tx_hlc_hooks(&conn, ctx).expect("install tx-hlc hooks");
        conn
    }

    #[test]
    fn test_current_hlc_differs_across_autocommit_statements() {
        let conn = setup_hlc_test_connection("hlc-across-stmts");
        let first: String = conn
            .query_row("SELECT current_hlc()", [], |row| row.get(0))
            .unwrap();
        // Any non-query statement forces the auto-commit transaction to close.
        conn.execute_batch("CREATE TABLE _tick (id INTEGER);").unwrap();
        let second: String = conn
            .query_row("SELECT current_hlc()", [], |row| row.get(0))
            .unwrap();
        assert_ne!(
            first, second,
            "current_hlc() must differ across separate auto-commit transactions"
        );
    }

    #[test]
    fn test_current_hlc_same_across_writes_in_one_tx() {
        // The transaction-scope invariant only applies to *writes*: multiple
        // INSERT/UPDATE/DELETE statements inside one tx must share one HLC.
        // Bare read-only `SELECT current_hlc()` calls intentionally draw fresh
        // timestamps so a stray probe cannot poison the HLC of a later write.
        let mut conn = setup_hlc_test_connection("hlc-explicit-tx-writes");
        conn.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, hlc TEXT);")
            .unwrap();
        let tx = conn.transaction().expect("begin tx");
        tx.execute("INSERT INTO t (id, hlc) VALUES (1, current_hlc())", [])
            .unwrap();
        tx.execute("INSERT INTO t (id, hlc) VALUES (2, current_hlc())", [])
            .unwrap();
        tx.commit().unwrap();
        let (a, b): (String, String) = conn
            .query_row(
                "SELECT (SELECT hlc FROM t WHERE id=1), (SELECT hlc FROM t WHERE id=2)",
                [],
                |row| Ok((row.get(0).unwrap(), row.get(1).unwrap())),
            )
            .unwrap();
        assert_eq!(
            a, b,
            "two writes within one explicit transaction must share one HLC"
        );
    }

    #[test]
    fn test_readonly_probe_does_not_poison_next_write_tx() {
        // Regression test for the CodeRabbit-identified poisoning scenario:
        // a bare `SELECT current_hlc()` outside any write must not dictate
        // the HLC that a later write transaction receives.
        let conn = setup_hlc_test_connection("hlc-no-poison");
        conn.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, hlc TEXT);")
            .unwrap();
        let probed: String = conn
            .query_row("SELECT current_hlc()", [], |row| row.get(0))
            .unwrap();
        conn.execute("INSERT INTO t (id, hlc) VALUES (1, current_hlc())", [])
            .unwrap();
        let persisted: String = conn
            .query_row("SELECT hlc FROM t WHERE id=1", [], |row| row.get(0))
            .unwrap();
        assert_ne!(
            probed, persisted,
            "the probed value must not be reused by the subsequent write"
        );
    }

    #[test]
    fn test_current_hlc_reset_on_rollback() {
        let mut conn = setup_hlc_test_connection("hlc-rollback");
        let tx = conn.transaction().expect("begin tx");
        let a: String = tx
            .query_row("SELECT current_hlc()", [], |row| row.get(0))
            .unwrap();
        tx.rollback().unwrap();
        let b: String = conn
            .query_row("SELECT current_hlc()", [], |row| row.get(0))
            .unwrap();
        assert_ne!(a, b, "current_hlc() must be fresh after a rollback");
    }
}
