//! [`CriticalNotificationSink`] — persists critical-failure events to
//! `haex_critical_notifications_no_sync` via its own SQLite connection.
//!
//! The sink runs on a SECOND connection to the same DB file so it can
//! still write when the main vault connection's mutex is poisoned. SQLite
//! is happy with multiple concurrent connections to a WAL-mode database;
//! both connections share file-level locks at the OS level (not Rust
//! mutexes), so poisoning one Rust mutex does not affect the other.
//!
//! ## Dedup invariant (Q3 in the plan)
//!
//! `emit` runs an UPSERT keyed on `(code, location, acknowledged)`:
//! - If a matching unacked row exists, `count` is incremented and
//!   `last_seen` is updated. No new row appears.
//! - If the user has acknowledged a previous row of the same `(code,
//!   location)`, the next occurrence creates a fresh row (the
//!   `acknowledged` value differs, so the unique-index lets the INSERT
//!   succeed). The banner reappears on the next failure of the same kind.

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use ts_rs::TS;
use uuid::Uuid;

use super::CriticalFailureCode;

/// One row from `haex_critical_notifications_no_sync`, shaped for the
/// frontend banner. ts-rs export keeps the TypeScript type in sync.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CriticalNotification {
    pub id: String,
    /// Stable discriminator (see [`CriticalFailureCode`]).
    pub code: String,
    /// Source location — diagnostic only, NOT in the banner message.
    pub location: String,
    /// JSON object for i18n parameter substitution. Stored as a string in
    /// the DB to keep the table SQLite-portable.
    pub params: String,
    /// Number of times this (code, location, acknowledged) tuple has fired.
    pub count: i64,
    pub first_seen: String,
    pub last_seen: String,
    pub acknowledged: bool,
}

/// Summary of a [`CriticalNotificationSink::cleanup`] run. Useful for the
/// periodic cleanup task to log progress without re-querying the table.
#[derive(Debug, Clone)]
pub struct CleanupReport {
    pub deleted_rows: usize,
    /// RFC3339 timestamp used as the cutoff.
    pub cutoff: String,
}

#[derive(Debug, thiserror::Error)]
pub enum SinkError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("sink mutex poisoned — last-resort signal only: stderr above")]
    SinkMutexPoisoned,
    #[error("JSON serialization: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Timestamp formatting: {0}")]
    Time(#[from] time::error::Format),
}

#[derive(Clone)]
pub struct CriticalNotificationSink {
    /// Separate from `state.db` so a poisoned main DB mutex still lets
    /// the sink write. Cheap to clone via `Arc::clone`.
    conn: Arc<Mutex<Connection>>,
}

impl CriticalNotificationSink {
    /// Open a sink against the vault's main DB file. Applies the same
    /// SQLCipher key so the second connection can read the encrypted
    /// table the main connection wrote. The migration runner already
    /// created the table on the main connection — SQLite shares schema
    /// across connections of the same file.
    pub fn open(db_path: &Path, cipher_key: &str) -> Result<Self, SinkError> {
        let conn = Connection::open(db_path)?;
        // pragma_update with a plain string argument is the
        // documented SQLCipher pattern — same as
        // `database::mod::create_encrypted_database_inner`.
        conn.pragma_update(None, "key", cipher_key)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// In-memory factory for tests. Creates the table inline so
    /// the test doesn't need to run the migration journal.
    #[cfg(test)]
    pub fn in_memory() -> Result<Self, SinkError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE haex_critical_notifications_no_sync (
                id TEXT PRIMARY KEY NOT NULL,
                code TEXT NOT NULL,
                location TEXT NOT NULL,
                params TEXT NOT NULL,
                count INTEGER NOT NULL DEFAULT 1,
                first_seen TEXT NOT NULL,
                last_seen TEXT NOT NULL,
                acknowledged INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX haex_critical_notifications_unacked_idx
                ON haex_critical_notifications_no_sync (acknowledged, last_seen)
                WHERE acknowledged = 0;
            CREATE UNIQUE INDEX haex_critical_notifications_dedup_idx
                ON haex_critical_notifications_no_sync (code, location, acknowledged);",
        )?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Persist a critical-failure event. UPSERT on (code, location,
    /// acknowledged) — same triple → `count++` + `last_seen` update; new
    /// triple → fresh row. Each call always succeeds in writing a row
    /// (either inserted or updated), so the banner reflects every emit.
    ///
    /// `location` is the static call-site string (e.g. `"crdt::hlc::next"`).
    /// `params` is a JSON value (typically an object) used by the frontend
    /// for i18n substitution.
    pub fn emit(
        &self,
        code: CriticalFailureCode,
        location: &str,
        params: serde_json::Value,
    ) -> Result<(), SinkError> {
        let conn = self.conn.lock().map_err(|_| SinkError::SinkMutexPoisoned)?;
        let now = OffsetDateTime::now_utc().format(&Rfc3339)?;
        let params_json = serde_json::to_string(&params)?;

        // Single UPSERT statement. The ON CONFLICT clause targets the
        // unique index (code, location, acknowledged). `excluded.*`
        // refers to the values from the proposed INSERT row.
        //
        // first_seen stays at its original value (the INSERT side never
        // sees the existing row); only count, last_seen, and params get
        // refreshed. Keeping the original first_seen lets operators see
        // "this code first fired N hours ago, has happened 5 times".
        let sql = "
            INSERT INTO haex_critical_notifications_no_sync
                (id, code, location, params, count, first_seen, last_seen, acknowledged)
            VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5, 0)
            ON CONFLICT(code, location, acknowledged) DO UPDATE SET
                count = count + 1,
                last_seen = excluded.last_seen,
                params = excluded.params
        ";

        conn.execute(
            sql,
            params![
                Uuid::new_v4().to_string(),
                code.as_str(),
                location,
                params_json,
                now,
            ],
        )?;
        Ok(())
    }

    /// Fetch the newest unacknowledged row for the banner. Returns `None`
    /// if no unacked rows exist (banner stays hidden).
    pub fn newest_unacked(&self) -> Result<Option<CriticalNotification>, SinkError> {
        let conn = self.conn.lock().map_err(|_| SinkError::SinkMutexPoisoned)?;
        let row = conn
            .query_row(
                "SELECT id, code, location, params, count, first_seen, last_seen, acknowledged
                   FROM haex_critical_notifications_no_sync
                  WHERE acknowledged = 0
               ORDER BY last_seen DESC
                  LIMIT 1",
                [],
                row_to_notification,
            )
            .optional()?;
        Ok(row)
    }

    /// Mark a specific row as acknowledged. Returns the number of rows
    /// updated — typically 1, or 0 if the id is unknown (e.g. the row
    /// was already cleaned up between the frontend fetch and the
    /// acknowledge click).
    pub fn acknowledge(&self, id: &str) -> Result<usize, SinkError> {
        let conn = self.conn.lock().map_err(|_| SinkError::SinkMutexPoisoned)?;
        let n = conn.execute(
            "UPDATE haex_critical_notifications_no_sync SET acknowledged = 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(n)
    }

    /// Delete rows older than `retention_days`. Analogous to
    /// `logging::cleanup_logs` but plain SQL — `_no_sync` doesn't run
    /// through `execute_with_crdt`, so no tombstones.
    pub fn cleanup(&self, retention_days: i64) -> Result<CleanupReport, SinkError> {
        let conn = self.conn.lock().map_err(|_| SinkError::SinkMutexPoisoned)?;
        let cutoff = OffsetDateTime::now_utc() - time::Duration::days(retention_days);
        let cutoff_str = cutoff.format(&Rfc3339)?;
        let n = conn.execute(
            "DELETE FROM haex_critical_notifications_no_sync WHERE last_seen < ?1",
            params![cutoff_str],
        )?;
        Ok(CleanupReport {
            deleted_rows: n,
            cutoff: cutoff_str,
        })
    }
}

fn row_to_notification(row: &rusqlite::Row) -> rusqlite::Result<CriticalNotification> {
    Ok(CriticalNotification {
        id: row.get(0)?,
        code: row.get(1)?,
        location: row.get(2)?,
        params: row.get(3)?,
        count: row.get(4)?,
        first_seen: row.get(5)?,
        last_seen: row.get(6)?,
        acknowledged: row.get::<_, i64>(7)? != 0,
    })
}
