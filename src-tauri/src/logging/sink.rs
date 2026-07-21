//! [`LogSink`] — persists log rows to `haex_logs_no_sync` via a dedicated
//! SQLite connection.
//!
//! Same-file second-connection pattern as
//! [`crate::critical::CriticalNotificationSink`]. Splitting the log-write
//! path off the main HLC-owning DB connection buys two things:
//!
//! 1. A blocked / poisoned main DB mutex no longer silences logging —
//!    the sink writes on its own `Arc<Mutex<Connection>>`, so log rows
//!    (including the sync-loop telemetry routed here in Phase 3) still
//!    land even while the main pipeline is wedged. This is the point of
//!    the whole change: the 2nd-device freeze becomes observable.
//! 2. Log writes bypass CRDT entirely. The `_no_sync` suffix already
//!    drops the table from `discover_crdt_tables`; using a separate
//!    connection additionally guarantees writes never enter
//!    `execute_with_crdt`, so no HLC lock, no dirty-table push, no
//!    owner-device replication of log rows.
//!
//! See `docs/plans/2026-07-21-haex-logs-no-sync.md` for the rationale.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::{types::ToSqlOutput, Connection, ToSql};
use serde_json::Value as JsonValue;

use crate::logging::queries::SQL_INSERT_LOG_FULL;

#[derive(Debug, thiserror::Error)]
pub enum LogSinkError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("sink mutex poisoned — last-resort signal only: stderr above")]
    SinkMutexPoisoned,
    #[error("SQLCipher is not active on the sink connection — `PRAGMA cipher_version` returned empty; sink would write plaintext to an encrypted file")]
    SqlcipherInactive,
}

#[derive(Clone)]
pub struct LogSink {
    /// Separate from `state.db` so a poisoned / long-locked main DB mutex
    /// still lets the sink write. Cheap to clone via `Arc::clone`.
    conn: Arc<Mutex<Connection>>,
}

impl LogSink {
    /// Open a sink against the vault's main DB file. Applies the same
    /// SQLCipher key so this second connection can read/write the same
    /// encrypted table the main connection sees.
    ///
    /// Defense-in-depth: verifies SQLCipher is actually active after
    /// applying the key (mirrors [`crate::critical::CriticalNotificationSink::open`]).
    /// Without the check, a future build that accidentally drops the
    /// SQLCipher feature would silently open the DB as plaintext and
    /// the first `write()` would surface a confusing "file is not a
    /// database" — at the exact moment we most need diagnostic logs.
    ///
    /// Sets a 500 ms `busy_timeout` so a concurrent main-connection
    /// write doesn't cause `write()` to fail immediately with
    /// `SQLITE_BUSY` — SQLite retries internally until the lock frees.
    pub fn open(db_path: &Path, cipher_key: &str) -> Result<Self, LogSinkError> {
        let conn = Connection::open(db_path)?;
        conn.pragma_update(None, "key", cipher_key)?;

        let cipher_version: String = conn
            .query_row("PRAGMA cipher_version", [], |row| row.get(0))
            .map_err(|_| LogSinkError::SqlcipherInactive)?;
        if cipher_version.is_empty() {
            return Err(LogSinkError::SqlcipherInactive);
        }

        conn.busy_timeout(Duration::from_millis(500))?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Insert one log row. All fields explicit (no HLC, no CRDT).
    /// `id` and `timestamp` are caller-supplied so the sink stays
    /// deterministic under tests and callers keep control over their
    /// own id/timestamp policy.
    #[allow(clippy::too_many_arguments)]
    pub fn write(
        &self,
        id: &str,
        timestamp: &str,
        level: &str,
        source: &str,
        extension_id: Option<&str>,
        message: &str,
        metadata: Option<&str>,
        device_id: &str,
    ) -> Result<(), LogSinkError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| LogSinkError::SinkMutexPoisoned)?;
        conn.execute(
            &SQL_INSERT_LOG_FULL,
            rusqlite::params![
                id,
                timestamp,
                level,
                source,
                extension_id,
                message,
                metadata,
                device_id,
            ],
        )?;
        Ok(())
    }

    /// Run an arbitrary write / delete on the sink's connection, returning
    /// the number of affected rows. Used by [`crate::logging::cleanup_logs`]
    /// to route the retention DELETEs off the CRDT path — the SQL is built
    /// in `cleanup_logs` from the same constants as before, only the
    /// executor changes.
    ///
    /// Kept `pub(crate)` intentionally: the sink is not meant as a general
    /// SQL escape hatch, only as the logging module's private
    /// second-connection helper.
    pub(crate) fn execute(
        &self,
        sql: &str,
        params: &[JsonValue],
    ) -> Result<usize, LogSinkError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| LogSinkError::SinkMutexPoisoned)?;
        let boxed: Vec<Box<dyn ToSql>> = params
            .iter()
            .map(|v| json_to_boxed_tosql(v))
            .collect();
        let refs: Vec<&dyn ToSql> = boxed.iter().map(|b| b.as_ref()).collect();
        Ok(conn.execute(sql, refs.as_slice())?)
    }

    /// SQL of the 0009 migration that creates the `haex_logs_no_sync`
    /// table without CRDT columns. Embedded at compile time so the
    /// in-memory test fixture stays byte-for-byte identical to
    /// production — a future schema change to the log table is
    /// automatically picked up by tests on next build.
    ///
    /// The FOREIGN KEY to `haex_extensions` is intentional — SQLite
    /// only enforces FKs when `PRAGMA foreign_keys = ON`, which the
    /// sink does not set, so the reference table need not exist in
    /// the in-memory fixture.
    #[cfg(test)]
    const MIGRATION_SQL: &str =
        include_str!("../../database/migrations/0009_haex_logs_no_sync.sql");

    /// In-memory factory for tests. Applies the 0009 migration split
    /// on drizzle's `--> statement-breakpoint` marker (same split the
    /// live migration runner uses).
    /// Test-only constructor that wraps an *existing* `Arc<Mutex<Connection>>`.
    /// Used by suites (e.g. `space_delivery::local::auth_gate` tests) that
    /// need the sink writes and the read-back query to land in the same
    /// in-memory DB — production splits them across two OS file handles
    /// but tests only have one `Connection::open_in_memory()` per fixture.
    /// The caller owns the schema setup.
    #[cfg(test)]
    pub fn from_connection(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    /// Test-only accessor exposing the underlying connection Arc so
    /// callers can run read-back queries against the same DB the sink
    /// wrote to. Not exposed in production — the sink is meant to be
    /// write-only from external code.
    #[cfg(test)]
    pub fn conn_for_tests(&self) -> Arc<Mutex<Connection>> {
        Arc::clone(&self.conn)
    }

    #[cfg(test)]
    pub fn in_memory() -> Result<Self, LogSinkError> {
        let conn = Connection::open_in_memory()?;
        conn.busy_timeout(Duration::from_millis(500))?;
        // The 0009 CREATE references haex_extensions via a FK. In production
        // that table is created by the baseline migration; in the sink's
        // isolated in-memory fixture it does not exist, so FK enforcement
        // (default ON in rusqlite) would fail every INSERT. The sink writes
        // never rely on the FK — extension_id is either NULL or an opaque
        // string — so disabling FKs here is a pure test-fixture concern.
        conn.pragma_update(None, "foreign_keys", false)?;
        for statement in
            Self::MIGRATION_SQL.split(crate::database::core::DRIZZLE_STATEMENT_BREAKPOINT)
        {
            let statement = statement.trim();
            if statement.is_empty() {
                continue;
            }
            conn.execute_batch(statement)?;
        }
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }
}

fn json_to_boxed_tosql(v: &JsonValue) -> Box<dyn ToSql> {
    match v {
        JsonValue::Null => Box::new(NullParam),
        JsonValue::Bool(b) => Box::new(*b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Box::new(i)
            } else if let Some(f) = n.as_f64() {
                Box::new(f)
            } else {
                Box::new(n.to_string())
            }
        }
        JsonValue::String(s) => Box::new(s.clone()),
        other => Box::new(other.to_string()),
    }
}

struct NullParam;
impl ToSql for NullParam {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(rusqlite::types::Value::Null))
    }
}

#[cfg(test)]
mod tests;
