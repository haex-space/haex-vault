//! The level of a vault's log and when "Ausführlich" ends (spec 098 FR-003, FR-004; research R4).
//!
//! The level is one row of `haex_diagnostic_log_no_sync`, a table that does not replicate, so a
//! switch is no sync change and another device never sees it. The row holds the end time (Unix
//! milliseconds) and the id of the app run that switched, so a new start of the app, or a vault
//! file copied to another device, begins at "Standard".

use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use uuid::Uuid;

use super::level::LogLevel;
use crate::error::Result;
use crate::storage::query::Query;

/// How long "Ausführlich" runs before it falls back to "Standard".
pub const VERBOSE_FOR: Duration = Duration::from_secs(24 * 60 * 60);

/// The id of this run of the app, made at its first use.
pub fn run_id() -> &'static str {
    static RUN_ID: OnceLock<String> = OnceLock::new();
    RUN_ID.get_or_init(|| Uuid::new_v4().to_string())
}

/// The level in force and, for "Ausführlich", the end time in Unix milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelState {
    pub level: LogLevel,
    pub until_ms: Option<i64>,
}

impl LevelState {
    pub const STANDARD: Self = Self {
        level: LogLevel::Standard,
        until_ms: None,
    };
}

/// What the stored row says, judged at a moment for an app run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Judged {
    pub state: LevelState,
    /// The row must be written as "Standard": it ended, belongs to another run or is damaged.
    pub rewrite: bool,
}

/// The stored row as read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub level: String,
    pub until_ms: Option<i64>,
    pub run_id: Option<String>,
}

/// The level a row stands for. A missing row is "Standard"; a damaged row, an ended "Ausführlich"
/// and one of another run count as "Standard" and are to be rewritten, so a damaged row never
/// raises the level.
pub fn judge(row: Option<Row>, now_ms: i64, run_id: &str) -> Judged {
    let Some(row) = row else {
        return Judged {
            state: LevelState::STANDARD,
            rewrite: false,
        };
    };
    match LogLevel::parse(&row.level) {
        Some(LogLevel::Standard) => Judged {
            state: LevelState::STANDARD,
            rewrite: false,
        },
        Some(LogLevel::Verbose) => match (row.until_ms, row.run_id.as_deref()) {
            (Some(until_ms), Some(run)) if until_ms > now_ms && run == run_id => Judged {
                state: LevelState {
                    level: LogLevel::Verbose,
                    until_ms: Some(until_ms),
                },
                rewrite: false,
            },
            _ => Judged {
                state: LevelState::STANDARD,
                rewrite: true,
            },
        },
        None => Judged {
            state: LevelState::STANDARD,
            rewrite: true,
        },
    }
}

/// Reads the row and judges it at `now_ms`.
pub fn read(q: &mut impl Query, now_ms: i64, run_id: &str) -> Result<Judged> {
    let row = q.query_row(
        "SELECT level, until_ms, run_id FROM haex_diagnostic_log_no_sync WHERE id = 1",
        &[],
        |r| {
            Ok(Row {
                level: r.get(0)?,
                until_ms: r.get(1)?,
                run_id: r.get(2)?,
            })
        },
    )?;
    Ok(judge(row, now_ms, run_id))
}

/// Writes the level. "Ausführlich" ends `VERBOSE_FOR` after `now_ms` and belongs to `run_id`;
/// "Standard" clears both. A plain `INSERT … ON CONFLICT`, as for the device file access switch.
pub fn switch(
    tx: &mut CrdtTransaction<'_>,
    level: LogLevel,
    run_id: &str,
    now_ms: i64,
) -> Result<LevelState> {
    let state = match level {
        LogLevel::Standard => LevelState::STANDARD,
        LogLevel::Verbose => LevelState {
            level,
            until_ms: Some(now_ms.saturating_add(VERBOSE_FOR.as_millis() as i64)),
        },
    };
    let run = (level == LogLevel::Verbose).then_some(run_id);
    tx.execute(
        "INSERT INTO haex_diagnostic_log_no_sync (id, level, until_ms, run_id) \
         VALUES (1, ?1, ?2, ?3) \
         ON CONFLICT(id) DO UPDATE SET level = excluded.level, until_ms = excluded.until_ms, \
         run_id = excluded.run_id",
        params![level.as_str(), state.until_ms, run],
    )?;
    Ok(state)
}

/// The monotonic end of "Ausführlich" in this run. The wall clock can be set back, which would
/// stretch an end time stored as a date; the monotonic instant of the switch cannot be moved.
pub struct Deadline(Mutex<Option<Instant>>);

impl Deadline {
    pub const fn new() -> Self {
        Self(Mutex::new(None))
    }

    /// "Ausführlich" was switched on at `from`.
    pub fn set(&self, from: Instant) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(from + VERBOSE_FOR);
    }

    pub fn clear(&self) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }

    /// What remains at `now`, if this run switched it on.
    pub fn left(&self, now: Instant) -> Option<Duration> {
        let end = (*self.0.lock().unwrap_or_else(PoisonError::into_inner))?;
        Some(end.saturating_duration_since(now))
    }
}

impl Default for Deadline {
    fn default() -> Self {
        Self::new()
    }
}

/// The monotonic end of this run's "Ausführlich".
pub static DEADLINE: Deadline = Deadline::new();

/// How long "Ausführlich" still runs: the rest up to the stored end time, never more than the
/// rest of the 24 hours counted from the monotonic instant of the switch, and never more than 24
/// hours (a clock set back must not extend it).
pub fn remaining(until_ms: i64, now_ms: i64, monotonic_left: Option<Duration>) -> Duration {
    let wall = Duration::from_millis(until_ms.saturating_sub(now_ms).max(0) as u64);
    let capped = wall.min(VERBOSE_FOR);
    match monotonic_left {
        Some(left) => capped.min(left),
        None => capped,
    }
}

#[cfg(test)]
#[path = "expiry_tests.rs"]
mod tests;
