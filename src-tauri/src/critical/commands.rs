//! Tauri commands exposing the critical-notification sink to the frontend.
//!
//! The Vue banner uses `critical_notifications_newest_unacked` to decide
//! whether to render itself; `critical_notifications_acknowledge` is wired
//! to the "Verstanden" button; `critical_notifications_cleanup` is invoked
//! alongside `log_cleanup` from the vault-store retention pass.
//!
//! All three are no-ops when no vault is open (`state.critical_sink = None`).

use tauri::{AppHandle, State};

use crate::critical::{sink::CriticalNotification, sink::SinkError};
use crate::database::error::DatabaseError;
use crate::AppState;

/// Default retention for `haex_critical_notifications_no_sync` rows —
/// same shape as `logging::DEFAULT_RETENTION_DAYS`. Keep generous: the
/// table is tiny (acknowledged forensic trail), and operators benefit
/// from "this code first fired 27 days ago" diagnostics.
const DEFAULT_RETENTION_DAYS: i64 = 30;

/// Fetch the newest unacknowledged critical-failure event for the banner.
/// Returns `None` if no vault is open OR if the table is empty.
#[tauri::command]
pub fn critical_notifications_newest_unacked(
    state: State<'_, AppState>,
) -> Result<Option<CriticalNotification>, DatabaseError> {
    let sink_guard = state
        .critical_sink
        .lock()
        .map_err(|e| DatabaseError::LockError { reason: e.to_string() })?;
    match sink_guard.as_ref() {
        Some(sink) => sink
            .newest_unacked()
            .map_err(sink_error_to_db_error),
        None => Ok(None),
    }
}

/// Mark a specific notification as acknowledged. Returns the number of
/// rows updated — typically 1, or 0 if the row was already cleaned up
/// between the frontend's fetch and the user's click.
#[tauri::command]
pub fn critical_notifications_acknowledge(
    state: State<'_, AppState>,
    id: String,
) -> Result<usize, DatabaseError> {
    let sink_guard = state
        .critical_sink
        .lock()
        .map_err(|e| DatabaseError::LockError { reason: e.to_string() })?;
    match sink_guard.as_ref() {
        Some(sink) => sink
            .acknowledge(&id)
            .map_err(sink_error_to_db_error),
        None => Ok(0),
    }
}

/// Delete rows older than the configured retention period. Returns the
/// number of rows deleted. Called from the same vault-store cleanup
/// pass that runs `log_cleanup`.
#[tauri::command]
pub fn critical_notifications_cleanup(
    state: State<'_, AppState>,
) -> Result<usize, DatabaseError> {
    let sink_guard = state
        .critical_sink
        .lock()
        .map_err(|e| DatabaseError::LockError { reason: e.to_string() })?;
    match sink_guard.as_ref() {
        Some(sink) => {
            let report = sink
                .cleanup(DEFAULT_RETENTION_DAYS)
                .map_err(sink_error_to_db_error)?;
            // Log the cleanup result for operator visibility. The
            // CleanupReport.cutoff field exists precisely so callers
            // can produce structured "deleted N rows older than T"
            // diagnostics without re-parsing strings — use it.
            println!(
                "[CRITICAL_CLEANUP] deleted {} row(s) older than {} ({}-day retention)",
                report.deleted_rows, report.cutoff, DEFAULT_RETENTION_DAYS,
            );
            Ok(report.deleted_rows)
        }
        None => Ok(0),
    }
}

/// Relaunch the Tauri application. Tied to the critical-failure flow
/// because Critical-severity banner rows recommend a restart as their
/// primary action; `app.restart()` is more reliable than asking the
/// user to manually close and reopen (they may not realize an
/// already-poisoned vault won't recover by itself).
///
/// Not gated on vault-open state — the banner can technically reach
/// here on a vault-closed warning, and a restart from there is still
/// the right action ("the locked-screen vault was left in a degraded
/// state, restart to get a clean session").
#[tauri::command]
pub fn critical_app_restart(app: AppHandle) -> Result<(), DatabaseError> {
    // `AppHandle::restart()` returns the never type `!` — it terminates
    // the current process and relaunches the binary. The explicit
    // `Ok(())` after it never runs, but spelling it out keeps the
    // function shape obvious to readers and gives a clean compile
    // error if `restart()` ever stops being `!` on a platform that
    // can't restart in-process. The `#[allow]` silences the
    // unreachable-code lint that would otherwise fire on the Ok line.
    #[allow(unreachable_code)]
    {
        app.restart();
        Ok(())
    }
}

/// Flatten the sink's structured error into the existing `DatabaseError`
/// shape the frontend already knows. Sink errors are rare (mutex poison,
/// disk full) and don't carry per-code semantics that the frontend would
/// need to distinguish.
fn sink_error_to_db_error(err: SinkError) -> DatabaseError {
    DatabaseError::DatabaseError {
        reason: format!("critical-notification sink: {err}"),
    }
}
