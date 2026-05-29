// src-tauri/src/crdt/cleanup.rs

use crate::crdt::trigger::DELETED_ROWS_TABLE;
use crate::table_names::TABLE_CRDT_CONFIGS;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use ts_rs::TS;
use uhlc::Timestamp;

/// Result of the delete-log cleanup operation.
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CleanupResult {
    /// Number of delete-log rows hard-deleted.
    pub tombstones_deleted: usize,
    /// Kept for backwards compatibility with the old tombstone API (always 1 now).
    pub applied_deleted: usize,
    /// Total entries deleted.
    pub total_deleted: usize,
}

/// RAII guard that turns off `PRAGMA foreign_keys` for the duration of a
/// block and turns it back on when the guard goes out of scope.
///
/// Use this instead of manual `pragma_update(..., "OFF") ... pragma_update(..., "ON")`
/// pairs: if anything between the calls returns early via `?` the manual
/// version leaves FK checks disabled on a shared `Connection`, silently
/// breaking referential integrity for subsequent queries on the same
/// connection.
pub(crate) struct ForeignKeyGuard<'a>(&'a Connection);

impl<'a> ForeignKeyGuard<'a> {
    pub(crate) fn disable(conn: &'a Connection) -> Result<Self, rusqlite::Error> {
        conn.execute("PRAGMA foreign_keys = OFF", [])?;
        Ok(Self(conn))
    }
}

impl Drop for ForeignKeyGuard<'_> {
    fn drop(&mut self) {
        let _ = self.0.execute("PRAGMA foreign_keys = ON", []);
    }
}

/// Run `f` with `PRAGMA foreign_keys` turned off, re-enabling unconditionally
/// when `f` returns — even on `Err` or panic. Use this instead of manual
/// OFF/ON pairs in code paths that open a transaction: `Connection::transaction`
/// requires `&mut`, which conflicts with the RAII guard's shared borrow.
///
/// Generic over the error type so callers can use their own error enum
/// (e.g. `DatabaseError`) as long as it implements `From<rusqlite::Error>`.
///
/// Panic-safety: `f` runs under `catch_unwind`. If it panics, the FK pragma
/// is restored and the original payload is re-raised — without this, a
/// panic inside `f` would leave the shared connection with FK disabled and
/// later non-CRDT queries would silently skip referential-integrity checks.
pub(crate) fn with_fk_disabled<R, E, F>(conn: &mut Connection, f: F) -> Result<R, E>
where
    F: FnOnce(&mut Connection) -> Result<R, E>,
    E: From<rusqlite::Error>,
{
    conn.execute("PRAGMA foreign_keys = OFF", [])
        .map_err(E::from)?;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(conn)));
    let _ = conn.execute("PRAGMA foreign_keys = ON", []);
    match result {
        Ok(r) => r,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

#[cfg(test)]
mod fk_guard_tests {
    use super::*;
    use rusqlite::Connection;

    fn fk_state(conn: &Connection) -> bool {
        conn.query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))
            .unwrap()
            == 1
    }

    #[test]
    fn guard_disables_on_construction() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();
        assert!(fk_state(&conn));

        let _guard = ForeignKeyGuard::disable(&conn).unwrap();
        assert!(!fk_state(&conn), "guard must disable FK on construction");
    }

    #[test]
    fn guard_reenables_on_drop_via_block_end() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();
        {
            let _guard = ForeignKeyGuard::disable(&conn).unwrap();
            assert!(!fk_state(&conn));
        }
        assert!(fk_state(&conn), "guard must re-enable FK when block ends");
    }

    #[test]
    fn guard_reenables_on_drop_via_early_return() {
        // Simulates the bug: when a function uses `?` to bubble up an
        // error between manual OFF/ON calls, FK stays disabled. With the
        // guard, an early return drops the guard and restores FK.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();

        fn body(conn: &Connection) -> Result<(), rusqlite::Error> {
            let _guard = ForeignKeyGuard::disable(conn)?;
            // Bubble up an error mid-block.
            conn.execute("INVALID SQL", [])?;
            Ok(())
        }

        let result = body(&conn);
        assert!(result.is_err(), "test setup expects body to fail");
        assert!(
            fk_state(&conn),
            "FK must be re-enabled even when body fails partway"
        );
    }

    // ------------------------------------------------------------------
    // with_fk_disabled: closure form that survives the &mut borrow taken
    // by Connection::transaction(). Same correctness goal as the RAII
    // guard — FK must be re-enabled on every exit path.
    // ------------------------------------------------------------------

    #[test]
    fn with_fk_disabled_observes_fk_off_inside_body() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();

        let _: Result<(), rusqlite::Error> = with_fk_disabled(&mut conn, |c| {
            assert!(!fk_state(c), "FK must be OFF inside the body");
            Ok(())
        });
    }

    #[test]
    fn with_fk_disabled_reenables_on_ok() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();

        let result: Result<(), rusqlite::Error> = with_fk_disabled(&mut conn, |_| Ok(()));
        result.unwrap();
        assert!(fk_state(&conn));
    }

    #[test]
    fn with_fk_disabled_reenables_on_err() {
        // This is the specific bug we are fixing: a `?` in the middle of
        // the body returning Err must NOT leave FK disabled on the
        // shared Connection for subsequent operations.
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();

        let result: Result<(), rusqlite::Error> =
            with_fk_disabled(&mut conn, |c| {
                c.execute("INVALID SQL", [])?;
                Ok(())
            });

        assert!(result.is_err(), "test setup expects body to fail");
        assert!(
            fk_state(&conn),
            "FK must be re-enabled even when body returns Err — this is the bug fix"
        );
    }

    #[test]
    fn with_fk_disabled_reenables_on_panic() {
        // Defense in depth: a panic inside the body would otherwise leave the
        // shared Connection with FK disabled, silently breaking referential
        // integrity for every subsequent query on this connection.
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _: Result<(), rusqlite::Error> =
                with_fk_disabled(&mut conn, |_| panic!("simulated panic in body"));
        }));

        assert!(result.is_err(), "panic must propagate to caller");
        assert!(
            fk_state(&conn),
            "FK must be re-enabled even when body panics — defense in depth"
        );
    }

    #[test]
    fn with_fk_disabled_transaction_pattern_works() {
        // The actual call shape used in apply_remote_changes_to_db:
        // open a transaction, do stuff, commit. The closure helper must
        // not conflict with the &mut borrow the transaction takes.
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();
        conn.execute("CREATE TABLE t (id INTEGER PRIMARY KEY)", []).unwrap();

        let result: Result<(), rusqlite::Error> = with_fk_disabled(&mut conn, |c| {
            let tx = c.transaction()?;
            tx.execute("INSERT INTO t (id) VALUES (1)", [])?;
            tx.commit()?;
            Ok(())
        });
        result.unwrap();

        assert!(fk_state(&conn));
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}

/// Computes the cutoff HLC time for cleanup.
///
/// Returns `None` if the cutoff would overflow `i64`. SQLite stores integers
/// as signed 64-bit, so an `as i64` cast on a `u64 > i64::MAX` would wrap to
/// a negative value and silently skew the `DELETE … WHERE … < ?1` comparison.
fn compute_cutoff_hlc_num(current_hlc_num: u64, retention_days: u32) -> Option<i64> {
    let ns_per_day: u64 = 24 * 60 * 60 * 1_000_000_000;
    let retention_ns = u64::from(retention_days).saturating_mul(ns_per_day);
    let cutoff = current_hlc_num.saturating_sub(retention_ns);
    i64::try_from(cutoff).ok()
}

/// Cleans up old delete-log entries. Deletes rows from `haex_deleted_rows`
/// whose `haex_hlc` is older than `retention_days`.
///
/// `retention_days == 0` hard-deletes every delete-log entry.
pub fn cleanup_deleted_rows(
    conn: &Connection,
    retention_days: u32,
) -> Result<CleanupResult, rusqlite::Error> {
    eprintln!(
        "🧹 [cleanup_deleted_rows] Called with retention_days={}",
        retention_days
    );

    let _fk_guard = ForeignKeyGuard::disable(conn)?;

    let deleted = if retention_days == 0 {
        let delete_sql = format!("DELETE FROM \"{}\"", DELETED_ROWS_TABLE);
        conn.execute(&delete_sql, [])?
    } else {
        let query = format!(
            "SELECT value FROM {} WHERE key = ?1 AND type = 'hlc'",
            TABLE_CRDT_CONFIGS
        );
        let current_hlc_str: Option<String> = conn
            .query_row(&query, ["hlc_timestamp"], |row| row.get(0))
            .ok();

        let current_hlc_str = match current_hlc_str {
            Some(s) => s,
            None => {
                eprintln!("No HLC timestamp found in config, skipping cleanup");
                return Ok(CleanupResult {
                    tombstones_deleted: 0,
                    applied_deleted: 0,
                    total_deleted: 0,
                });
            }
        };

        let current_timestamp = Timestamp::from_str(&current_hlc_str).map_err(|e| {
            eprintln!("Failed to parse HLC timestamp '{current_hlc_str}': {e:?}");
            rusqlite::Error::InvalidQuery
        })?;

        let current_hlc_num = current_timestamp.get_time().as_u64();
        let cutoff_hlc_num = match compute_cutoff_hlc_num(current_hlc_num, retention_days) {
            Some(v) => v,
            None => {
                eprintln!(
                    "HLC cutoff exceeds i64::MAX (current_hlc_num={current_hlc_num}, retention_days={retention_days}); skipping cleanup"
                );
                return Ok(CleanupResult {
                    tombstones_deleted: 0,
                    applied_deleted: 0,
                    total_deleted: 0,
                });
            }
        };

        let delete_sql = format!(
            "DELETE FROM \"{}\"
             WHERE haex_hlc IS NOT NULL
             AND CAST(substr(haex_hlc, 1, instr(haex_hlc, '/') - 1) AS INTEGER) < ?1",
            DELETED_ROWS_TABLE
        );
        conn.execute(&delete_sql, [cutoff_hlc_num])?
    };

    if deleted > 0 {
        eprintln!("Cleaned up {deleted} entries from {DELETED_ROWS_TABLE}");
    }

    Ok(CleanupResult {
        tombstones_deleted: deleted,
        applied_deleted: 1,
        total_deleted: deleted,
    })
}

/// Gets statistics about CRDT tables.
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CrdtStats {
    /// Total number of rows across all CRDT-enabled tables.
    pub total_entries: i64,
    /// Always 0 with the delete-log architecture (kept for backwards compat).
    pub pending_upload: i64,
    /// Number of CRDT-enabled tables.
    pub pending_apply: i64,
    /// Alias of `total_entries` minus delete-log entries.
    pub applied: i64,
    /// Alias of `total_entries` (kept for backwards compat).
    pub insert_count: i64,
    /// Alias of `applied` (kept for backwards compat).
    pub update_count: i64,
    /// Number of rows currently in `haex_deleted_rows`.
    pub delete_count: i64,
}

pub fn get_crdt_stats(conn: &Connection) -> Result<CrdtStats, rusqlite::Error> {
    let mut total_entries: i64 = 0;
    let mut crdt_table_count: i64 = 0;

    // CRDT tables are identified by having the `haex_hlc` column (added by the
    // CrdtTransformer at CREATE TABLE time). This excludes local-only tables
    // (`*_no_sync`), system tables (`sqlite_*`), and the delete-log table itself,
    // since that is counted separately below.
    let mut stmt = conn.prepare(
        "SELECT m.name FROM sqlite_master m \
         WHERE m.type = 'table' \
         AND m.name NOT LIKE 'sqlite_%' \
         AND m.name NOT LIKE '%_no_sync' \
         AND m.name != ?1 \
         AND EXISTS (SELECT 1 FROM pragma_table_info(m.name) WHERE name = 'haex_hlc')",
    )?;

    let table_names: Vec<String> = stmt
        .query_map([DELETED_ROWS_TABLE], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;

    for table_name in table_names {
        crdt_table_count += 1;

        let count: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM \"{}\"", table_name),
            [],
            |row| row.get(0),
        )?;
        total_entries += count;
    }

    let delete_count: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM \"{}\"", DELETED_ROWS_TABLE),
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // In the delete-log model, deleted rows no longer sit in the main tables —
    // every row counted in `total_entries` is already "applied" / active.
    let applied = total_entries;

    Ok(CrdtStats {
        total_entries,
        pending_upload: 0,
        pending_apply: crdt_table_count,
        applied,
        insert_count: total_entries,
        update_count: applied,
        delete_count,
    })
}

#[cfg(test)]
mod cutoff_tests {
    use super::*;

    const NS_PER_DAY: u64 = 24 * 60 * 60 * 1_000_000_000;

    #[test]
    fn normal_case_subtracts_retention() {
        let current: u64 = 2 * NS_PER_DAY;
        let cutoff = compute_cutoff_hlc_num(current, 1).expect("should fit i64");
        assert_eq!(cutoff as u64, NS_PER_DAY);
    }

    #[test]
    fn saturates_at_zero_when_retention_exceeds_current() {
        let cutoff = compute_cutoff_hlc_num(NS_PER_DAY, 30).expect("zero fits i64");
        assert_eq!(cutoff, 0);
    }

    #[test]
    fn rejects_cutoff_above_i64_max() {
        // current_hlc_num near u64::MAX, retention small → result still > i64::MAX
        // Old code: `as i64` would wrap to a large negative i64 and silently
        // make the DELETE comparison meaningless.
        let current = u64::MAX;
        assert_eq!(compute_cutoff_hlc_num(current, 1), None);
    }

    #[test]
    fn accepts_cutoff_exactly_at_i64_max() {
        let current = i64::MAX as u64;
        let cutoff = compute_cutoff_hlc_num(current, 0).expect("i64::MAX fits i64");
        assert_eq!(cutoff, i64::MAX);
    }

    #[test]
    fn rejects_cutoff_one_past_i64_max() {
        let current = (i64::MAX as u64) + 1;
        assert_eq!(compute_cutoff_hlc_num(current, 0), None);
    }
}
