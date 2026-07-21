//! Autonomous sync loop for local spaces.
//!
//! Runs entirely in Rust: connects to leader, pushes dirty changes,
//! pulls remote changes, applies them to local DB, and emits Tauri events.
//!
//! Split across submodules:
//! - [`driver`]: lifecycle (`start_peer_sync_loop`, `connect_for_mode`,
//!   `run_sync_loop`).
//! - [`cycle`]: the per-cycle orchestrator (`run_sync_cycle`).
//! - [`push`]: outbound scan/chunk/push phase.
//! - [`pull`]: paginated inbound pull phase + apply helpers.
//! - [`pending_columns`]: owner-vault pending-column recovery.
//! - [`mls`]: MLS message processing, rejoin, KeyPackage refill.
//! - [`membership`]: foreign-membership-row filtering helpers.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{watch, Notify};

mod cycle;
mod driver;
mod membership;
mod mls;
mod pending_columns;
mod pull;
mod push;

// Public API re-exports — external callers use these via
// `space_delivery::local::sync_loop::<item>`.
pub use driver::start_peer_sync_loop;
pub use pull::local_to_remote_change;

/// Selects what the push phase scans and which phases run in a sync cycle.
///
/// The default, [`SyncMode::SpaceScoped`], is the existing shared-space sync:
/// only the space-scoped whitelist (filtered by `space_id`) is pushed and the
/// MLS phases run. [`SyncMode::OwnerVault`] is serverless sync of the owner's
/// OWN vault across the owner's OWN devices — it pushes the FULL CRDT table
/// set (no `space_id` filter) and skips the MLS phases.
#[derive(Clone)]
pub enum SyncMode {
    /// Existing behaviour: space-scoped whitelist push + MLS phases.
    SpaceScoped,
    /// Owner-mesh behaviour: full-vault push over the caller-resolved table
    /// list, no membership filtering, no MLS phases. The caller MUST resolve
    /// the table list (the loop never derives it).
    OwnerVault { tables: Vec<String> },
}

/// Sync-loop logging helper — persists to `haex_logs_no_sync` via
/// [`crate::logging::LogSink`], plus stderr for dev / container-log
/// diagnostics.
///
/// ## Historical rationale, retained as a safety-in-depth note
///
/// Before 2026-07-21 this helper was stderr-only because `haex_logs` was
/// itself CRDT-synced: every "pulled N changes" line became a synced row →
/// marked dirty → pushed to the owner's other devices → they logged
/// "pulled 1 change" → pushed back → ∞. A field report showed the count
/// climbing into the thousands with 3+ devices, and stderr in production
/// went to `/dev/null` so the client-side loop was completely blind.
///
/// The rename to `haex_logs_no_sync` + dedicated [`LogSink`] connection
/// (`docs/plans/2026-07-21-haex-logs-no-sync.md`) removed both drivers of
/// that loop: `discover_crdt_tables` no longer picks up the log table, and
/// writes bypass `execute_with_crdt` entirely. Persisting sync-loop rows
/// here is now safe *and* diagnostically valuable — the next 2nd-device
/// freeze repro can be inspected from the persistent log viewer instead
/// of hoping stderr survived.
///
/// The JS console interceptor's `[SYNC]`-prefix strip is now
/// belt-and-suspenders, not load-bearing.
pub(super) fn log_sync(app_handle: &tauri::AppHandle, level: &str, message: &str) {
    use tauri::Manager;

    // `log_to_db` also emits an `eprintln!` internally with the same
    // `[source] [level] message` shape, so no explicit stderr line here.
    let sink_snapshot = app_handle
        .state::<crate::AppState>()
        .log_sink_snapshot();
    crate::logging::log_to_db(sink_snapshot.as_ref(), level, "SyncLoop", message, None);
}

/// Default poll interval between sync cycles.
pub(super) const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Maximum backoff duration for reconnection attempts.
pub(super) const MAX_RECONNECT_BACKOFF: Duration = Duration::from_secs(60);

/// Handle to a running sync loop. Call `stop()` to terminate.
pub struct SyncLoopHandle {
    stop_sender: watch::Sender<bool>,
    wakeup: Arc<Notify>,
    task: tokio::task::JoinHandle<()>,
}

impl SyncLoopHandle {
    /// Signal the sync loop to stop.
    pub fn stop(&self) {
        let _ = self.stop_sender.send(true);
    }

    /// Cut the current `POLL_INTERVAL` sleep short so the next sync cycle
    /// starts immediately. Multiple calls before the loop wakes up coalesce
    /// into a single wake (Notify::notify_one semantics) — the cycle itself
    /// is the rate limit, not this signal.
    ///
    /// Calling this while a cycle is already running is a no-op for the
    /// current cycle and a wake for the next sleep.
    pub fn wakeup(&self) {
        self.wakeup.notify_one();
    }

    /// Check if the sync loop task has finished.
    pub fn is_finished(&self) -> bool {
        self.task.is_finished()
    }
}

#[cfg(test)]
#[path = "../sync_loop_tests.rs"]
mod tests;

/// Returns the current UTC time in SQLite `datetime('now')` format: `YYYY-MM-DD HH:MM:SS`.
///
/// This matches the format used by CRDT dirty table triggers so that the
/// `last_modified <= ?` comparison works correctly.
pub(super) fn sqlite_datetime_now() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
    )
}
