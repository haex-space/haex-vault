//! Running extension SQL (contracts/sql-policy.md §Ausführung, T059): pre-check, permission
//! decision, then haex-crdt's guarded read or write with the authorizer and a run-time limit.
//! Writes go through the CRDT transformer like every other write of holzi (FR-028).

use std::cell::Cell;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use haex_crdt::db::error::DatabaseError;
use haex_crdt::rusqlite::types::Value as SqlValue;
use haex_crdt::rusqlite::{self, Row, ToSql};
use haex_crdt::{GuardedWriteOptions, SqlGuard};
use serde::Serialize;
use serde_json::{json, Value};
use uuid::Uuid;

use super::ast_check::{check, cte_names};
use super::authorizer;
use super::migrate::Tables;
use super::parse::{parse_one, returns_rows, violation};
use super::policy::SqlPolicy;
use super::values::{encoded_size, to_json};
use crate::extensions::default_limits;
use crate::extensions::error::{BridgeError, ExtensionErrorCode};
use crate::extensions::permissions::Decision;
use crate::storage::query::Query;
use crate::vault_gate::VaultDb;

/// `extension_limits` of one extension (data-model.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_rows: u64,
    pub max_concurrent: u64,
    pub max_sql_bytes: u64,
    pub timeout_ms: u64,
    pub max_response_bytes: u64,
    /// Run time of one model job in milliseconds (spec 055).
    pub model_job_timeout_ms: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_rows: default_limits::MAX_ROWS,
            max_concurrent: default_limits::MAX_CONCURRENT,
            max_sql_bytes: default_limits::MAX_SQL_BYTES,
            timeout_ms: default_limits::TIMEOUT_MS,
            max_response_bytes: default_limits::MAX_RESPONSE_BYTES,
            model_job_timeout_ms: default_limits::MODEL_JOB_TIMEOUT_MS,
        }
    }
}

/// The limits of an extension; without a row the defaults.
pub fn limits_of(q: &mut impl Query, extension_id: Uuid) -> haex_crdt::Result<Limits> {
    let row = q.query_row(
        "SELECT max_rows, max_concurrent, max_sql_bytes, timeout_ms, max_response_bytes, \
         model_job_timeout_ms FROM extension_limits WHERE extension_id = ?1",
        &[&extension_id.to_string()],
        |r| {
            Ok(Limits {
                max_rows: r.get::<_, i64>(0)?.max(0) as u64,
                max_concurrent: r.get::<_, i64>(1)?.max(1) as u64,
                max_sql_bytes: r.get::<_, i64>(2)?.max(0) as u64,
                timeout_ms: r.get::<_, i64>(3)?.max(1) as u64,
                max_response_bytes: r.get::<_, i64>(4)?.max(0) as u64,
                model_job_timeout_ms: r.get::<_, i64>(5)?.max(1) as u64,
            })
        },
    )?;
    Ok(row.unwrap_or_default())
}

/// The answer of a query or statement, as the vault-sdk reads it.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SqlResult {
    pub rows: Vec<Vec<Value>>,
    pub columns: Vec<String>,
    pub rows_affected: u64,
    pub last_insert_id: Option<i64>,
}

fn limit(message: &str) -> BridgeError {
    BridgeError::new(ExtensionErrorCode::LimitExceeded, message)
}

/// Raised inside a row callback when the result grows over a limit; the statement stops there.
#[derive(Debug)]
struct LimitHit;

impl std::fmt::Display for LimitHit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("result limit")
    }
}

impl std::error::Error for LimitHit {}

fn is_limit_hit(error: &haex_crdt::Error) -> bool {
    matches!(
        error.sqlite_error(),
        Some(rusqlite::Error::UserFunctionError(inner)) if inner.is::<LimitHit>()
    )
}

/// Maps a failure of the guarded run to the bridge's codes; nothing of holzi's schema leaks.
fn map_error(error: haex_crdt::Error) -> BridgeError {
    if is_limit_hit(&error) {
        return limit("result too large");
    }
    match &error {
        haex_crdt::Error::Database(DatabaseError::SqlGuardDenied { .. }) => {
            violation("form not attributable")
        }
        haex_crdt::Error::Database(DatabaseError::MultipleStatements { .. }) => {
            violation("exactly one statement is allowed")
        }
        haex_crdt::Error::Database(
            DatabaseError::SqlGuardInterrupted { .. } | DatabaseError::TransactionAborted { .. },
        ) => limit("time limit exceeded"),
        haex_crdt::Error::Database(DatabaseError::TransactionTooLarge { .. }) => {
            limit("transaction too large")
        }
        haex_crdt::Error::Database(DatabaseError::ValueTooLarge { .. }) => limit("value too large"),
        _ => {
            let message = error
                .sqlite_error()
                .map_or_else(|| "database error".to_owned(), ToString::to_string);
            match message.strip_prefix(NO_SUCH_TABLE) {
                Some(name) => no_such_table(name),
                None => BridgeError::new(ExtensionErrorCode::Database, message),
            }
        }
    }
}

/// A statement that passed the pre-check and the permission decision.
pub struct Checked {
    sql: String,
    params: Vec<SqlValue>,
    returns_rows: bool,
    is_query: bool,
    /// An `INSERT` (or `REPLACE`): only then the answer carries `lastInsertId`.
    is_insert: bool,
    ctes: HashSet<String>,
    /// Lower-case names of the tables the statement may write (the authorizer's view).
    writable: HashSet<String>,
}

fn full_name(table: &crate::extensions::ids::ExtensionTable) -> String {
    format!("{}{}", table.prefix, table.table)
}

/// The lower-case names of the tables of the database (a `WITH` name may not shadow one).
pub fn existing_tables(db: &VaultDb) -> Result<HashSet<String>, BridgeError> {
    db.read_blocking(|q| {
        q.query_map(
            "SELECT lower(name) FROM sqlite_master WHERE type IN ('table', 'view')",
            &[],
            |r| r.get::<_, String>(0),
        )
    })
    .map(|names| names.into_iter().collect())
    .map_err(|_| BridgeError::new(ExtensionErrorCode::Database, "database unavailable"))
}

/// SQLite's answer for a missing table, in one spelling: lower case, without `main.`, so it does not
/// tell a table that never existed from one that holzi hides.
fn no_such_table(name: &str) -> BridgeError {
    let name = name.to_ascii_lowercase();
    let name = name.strip_prefix("main.").unwrap_or(&name);
    BridgeError::new(
        ExtensionErrorCode::Database,
        format!("{NO_SUCH_TABLE}{name}"),
    )
}

const NO_SUCH_TABLE: &str = "no such table: ";

/// Pre-checks one statement and decides its permissions: a foreign table without a permission
/// is 1004 (ask) or 1002 (denied), with `{resourceType, action, target}` in `details`.
pub fn prepare(
    sql: &str,
    params: Vec<SqlValue>,
    policy: &SqlPolicy,
    limits: &Limits,
    existing: &HashSet<String>,
) -> Result<Checked, BridgeError> {
    if sql.len() as u64 > limits.max_sql_bytes {
        return Err(limit("statement too long"));
    }
    let statement = parse_one(sql)?;
    let access = check(&statement, sql, &policy.own, existing)?;
    for (table, write) in access
        .reads
        .iter()
        .map(|t| (t, false))
        .chain(access.writes.iter().map(|t| (t, true)))
    {
        let code = match policy.decide(table, write) {
            Decision::Allow if policy.is_present(&table.prefix) => continue,
            // Granted, but the extension is gone: the same answer as a table that does not exist
            // (US6 scenario 5), also when its tables were kept.
            Decision::Allow => return Err(no_such_table(&full_name(table))),
            Decision::Deny => ExtensionErrorCode::PermissionDenied,
            Decision::Prompt => ExtensionErrorCode::PermissionPromptRequired,
        };
        return Err(
            BridgeError::new(code, "permission required").with_details(json!({
                "resourceType": "database",
                "action": if write { "readWrite" } else { "read" },
                "target": full_name(table),
            })),
        );
    }
    Ok(Checked {
        sql: sql.to_owned(),
        params,
        returns_rows: returns_rows(&statement),
        is_query: matches!(statement, haex_crdt::sqlparser::ast::Statement::Query(_)),
        is_insert: matches!(statement, haex_crdt::sqlparser::ast::Statement::Insert(_)),
        ctes: cte_names(&statement),
        writable: access.writes.iter().map(full_name).collect(),
    })
}

/// The authorizer alone, for the bypass corpus (T052): the statement skips the pre-check and the
/// permission decision; only what the authorizer needs is taken from the syntax tree.
#[cfg(test)]
pub(crate) fn prepare_unchecked(sql: &str, params: Vec<SqlValue>) -> Checked {
    use haex_crdt::sqlparser::dialect::SQLiteDialect;
    use haex_crdt::sqlparser::parser::Parser;
    let parsed = Parser::parse_sql(&SQLiteDialect {}, sql).ok();
    let only = parsed.as_ref().filter(|s| s.len() == 1).map(|s| &s[0]);
    Checked {
        sql: sql.to_owned(),
        params,
        returns_rows: only.is_some_and(returns_rows),
        is_query: matches!(only, Some(haex_crdt::sqlparser::ast::Statement::Query(_))),
        is_insert: matches!(only, Some(haex_crdt::sqlparser::ast::Statement::Insert(_))),
        ctes: only.map(cte_names).unwrap_or_default(),
        writable: only
            .and_then(|s| super::ast_check::write_targets(s).ok())
            .unwrap_or_default()
            .into_iter()
            .collect(),
    }
}

fn guard(policy: Arc<SqlPolicy>, checked: &[Checked], limits: &Limits) -> SqlGuard {
    let writable: HashSet<String> = checked
        .iter()
        .flat_map(|c| c.writable.iter().cloned())
        .collect();
    let ctes: HashSet<String> = checked
        .iter()
        .flat_map(|c| c.ctes.iter().cloned())
        .collect();
    let deadline = Instant::now() + Duration::from_millis(limits.timeout_ms);
    SqlGuard {
        authorizer: authorizer::runtime(policy, writable, ctes),
        progress: Some((1000, Arc::new(move || Instant::now() > deadline))),
        // No value or row larger than the whole answer may be built (`zeroblob(1e9)`).
        max_value_bytes: Some(usize::try_from(limits.max_response_bytes).unwrap_or(usize::MAX)),
    }
}

/// Maps rows within the row and response limits.
struct RowCollector {
    rows: Cell<u64>,
    bytes: Cell<u64>,
    limits: Limits,
}

impl RowCollector {
    fn new(limits: Limits) -> Self {
        Self {
            rows: Cell::new(0),
            bytes: Cell::new(0),
            limits,
        }
    }

    fn row(&self, row: &Row<'_>) -> rusqlite::Result<Vec<Value>> {
        self.rows.set(self.rows.get() + 1);
        if self.rows.get() > self.limits.max_rows {
            return Err(rusqlite::Error::UserFunctionError(Box::new(LimitHit)));
        }
        let mut values = Vec::with_capacity(row.as_ref().column_count());
        for i in 0..row.as_ref().column_count() {
            let value = row.get_ref(i)?;
            // Checked before the value is copied and encoded: one huge value never doubles in memory.
            self.bytes
                .set(self.bytes.get() + encoded_size(value) as u64);
            if self.bytes.get() > self.limits.max_response_bytes {
                return Err(rusqlite::Error::UserFunctionError(Box::new(LimitHit)));
            }
            values.push(to_json(value).0);
        }
        Ok(values)
    }
}

fn keep_marked<T>(items: Vec<T>, keep: &[bool]) -> Vec<T> {
    items
        .into_iter()
        .zip(keep)
        .filter_map(|(item, keep)| keep.then_some(item))
        .collect()
}

/// Drops the sync columns (`haex_*`) of a result, e.g. from `SELECT *`.
fn without_sync_columns(
    columns: Vec<String>,
    rows: Vec<Vec<Value>>,
) -> (Vec<String>, Vec<Vec<Value>>) {
    let keep: Vec<bool> = columns
        .iter()
        .map(|c| !c.to_ascii_lowercase().starts_with("haex_"))
        .collect();
    let rows = rows
        .into_iter()
        .map(|row| keep_marked(row, &keep))
        .collect();
    (keep_marked(columns, &keep), rows)
}

fn sql_refs(params: &[SqlValue]) -> Vec<&dyn ToSql> {
    params.iter().map(|p| p as &dyn ToSql).collect()
}

/// Runs checked statements. One query alone runs on the read-only view; anything else runs in
/// one guarded write, all or nothing.
pub fn run(
    db: &VaultDb,
    policy: Arc<SqlPolicy>,
    limits: &Limits,
    checked: Vec<Checked>,
) -> Result<SqlResult, BridgeError> {
    run_in(db, policy, limits, checked, Tables::Synced)
}

/// [`run`] for a caller whose own tables are `tables`: a development version (US12) writes its
/// tables without CRDT columns in haex-crdt's local mode, which still stamps every synced table.
pub fn run_in(
    db: &VaultDb,
    policy: Arc<SqlPolicy>,
    limits: &Limits,
    checked: Vec<Checked>,
    tables: Tables,
) -> Result<SqlResult, BridgeError> {
    let guard = guard(policy, &checked, limits);
    let collector = RowCollector::new(*limits);
    if let [only] = checked.as_slice() {
        if only.is_query {
            let result = db
                .read_guarded_blocking(&guard, |conn| {
                    conn.query_with_columns(
                        &only.sql,
                        rusqlite::params_from_iter(only.params.iter()),
                        |row| collector.row(row),
                    )
                })
                .map_err(map_error)?;
            let (columns, rows) = without_sync_columns(result.columns, result.rows);
            return Ok(SqlResult {
                rows,
                columns,
                rows_affected: 0,
                last_insert_id: None,
            });
        }
    }
    let options = GuardedWriteOptions {
        local: tables == Tables::DeviceLocal,
        ..GuardedWriteOptions::default()
    };
    db.write_guarded_blocking(&guard, options, |tx| {
        let mut result = SqlResult::default();
        for statement in &checked {
            let params = sql_refs(&statement.params);
            let changed = if statement.returns_rows {
                let rows =
                    tx.query_with_columns(&statement.sql, &params, |row| collector.row(row))?;
                let changed = if statement.is_query {
                    0
                } else {
                    rows.rows.len() as u64
                };
                (result.columns, result.rows) = without_sync_columns(rows.columns, rows.rows);
                changed
            } else {
                tx.execute(&statement.sql, &params)? as u64
            };
            result.rows_affected += changed;
            // The connection's last rowid belongs to whatever holzi inserted last; only an insert of
            // this statement that wrote a row may report it.
            if statement.is_insert && changed > 0 {
                result.last_insert_id = Some(tx.last_insert_rowid());
            }
        }
        Ok(result)
    })
    .map_err(map_error)
}

#[cfg(test)]
#[path = "exec_tests.rs"]
mod tests;
