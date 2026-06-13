//! Critical-failure pattern: unified handling of mutex poisoning and other
//! non-recoverable failures.
//!
//! See `docs/plans/2026-06-13-critical-failure-pattern.md` for the design.
//!
//! ## Pieces
//!
//! - [`CriticalFailureCode`] — semantic discriminator (`HlcMutexPoisoned`,
//!   `DbMutexPoisoned`, …). One variant per failure class. Each code's
//!   [`severity()`](CriticalFailureCode::severity) is a property of the
//!   code itself, never of the call site (Q2 in the plan).
//! - [`Severity`] — Critical (restart-pflicht) vs Warning (hinweis).
//! - [`MutexPoisonError`] — `Err` type returned by [`lock_or_fail`]. Carries
//!   `code` + `location` so propagation chains stay diagnosable.
//! - [`lock_or_fail`] — `Mutex::lock` replacement: emits a
//!   `[CRITICAL]` stderr line, records a row via the sink, and returns
//!   `Err`. The mutex stays poisoned — subsequent callers also fail. No
//!   silent recovery (per User-Entscheidung 2026-06-13).
//! - [`CriticalNotificationSink`] — owns a SEPARATE SQLite connection on
//!   the same DB file so it can write even when the main vault connection's
//!   mutex is poisoned. UPSERT-deduplicates `(code, location, acknowledged)`
//!   tuples into a `count++` instead of an unbounded row stream (Q3).
//!
//! ## Lifecycle
//!
//! The sink is built when the vault opens (alongside `state.db`) and
//! dropped on vault close. Construction needs the vault path + cipher key
//! so the second SQLite connection can be opened with the same SQLCipher
//! settings as the main one. See [`CriticalNotificationSink::open`].

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

mod codes;
pub mod commands;
mod error;
pub mod sink;

pub use codes::{CriticalFailureCode, Severity};
pub use error::MutexPoisonError;
pub use sink::{CleanupReport, CriticalNotificationSink};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

/// `Mutex::lock` replacement with audit + user-visible notification on
/// poison. Use this instead of `.lock().unwrap()` / `.lock().map_err(...)`
/// everywhere a poisoned mutex represents a real failure (DB connection,
/// HLC service, vault-lock state, …).
///
/// On success: returns the guard like a plain `.lock()`.
///
/// On poison:
/// - Writes a `[CRITICAL]` line to stderr (for CI / Docker / Tauri devtools)
/// - Calls `sink.emit(code, location, params)` to persist a row that the
///   Vue banner will surface. The sink uses its own connection, so it
///   succeeds even when the failing mutex itself protects the main DB.
/// - Returns `Err(MutexPoisonError)`. The mutex stays poisoned by design —
///   subsequent callers will hit the same path. Vault is "degraded" until
///   restart; user decides when to restart via the banner action button.
///
/// `params` is a `serde_json::Value` (typically a JSON object) used by the
/// frontend to substitute dynamic values into the localized message. Pass
/// `serde_json::json!({})` if no params are needed.
pub fn lock_or_fail<'a, T>(
    mutex: &'a Mutex<T>,
    code: CriticalFailureCode,
    location: &'static str,
    sink: &CriticalNotificationSink,
    params: serde_json::Value,
) -> Result<MutexGuard<'a, T>, MutexPoisonError> {
    match mutex.lock() {
        Ok(guard) => Ok(guard),
        Err(poisoned) => handle_poison(poisoned, code, location, sink, params),
    }
}

/// Build the error + stderr-print + sink-emit side-effects in one place so
/// the [`lock_or_fail`] body stays cheap and inlinable.
fn handle_poison<'a, T>(
    _poisoned: PoisonError<MutexGuard<'a, T>>,
    code: CriticalFailureCode,
    location: &'static str,
    sink: &CriticalNotificationSink,
    params: serde_json::Value,
) -> Result<MutexGuard<'a, T>, MutexPoisonError> {
    // 1. Stderr audit. The `[CRITICAL]` marker is grep-able from CI logs
    //    and Docker stdout/stderr captures. We emit before the DB write
    //    so a sink failure doesn't lose the trail.
    eprintln!(
        "[CRITICAL] {location}: mutex poisoned (code={code:?}, severity={severity:?})",
        severity = code.severity(),
    );

    // 2. Persist for the banner. Best-effort: if the sink itself can't
    //    write (e.g. disk full, sink mutex also poisoned), we still
    //    propagate the original error — the stderr line above is the
    //    last-resort signal.
    if let Err(e) = sink.emit(code, location, params) {
        eprintln!(
            "[CRITICAL] {location}: ALSO failed to emit critical-notification row ({e}) — vault is in a degraded state but the banner row could not be persisted"
        );
    }

    Err(MutexPoisonError { code, location })
}

// Convenience for the very common case where the caller wraps the mutex in
// an `Arc<Mutex<T>>` (Tauri's state pattern).
pub fn lock_or_fail_arc<'a, T>(
    arc: &'a Arc<Mutex<T>>,
    code: CriticalFailureCode,
    location: &'static str,
    sink: &CriticalNotificationSink,
    params: serde_json::Value,
) -> Result<MutexGuard<'a, T>, MutexPoisonError> {
    lock_or_fail(arc.as_ref(), code, location, sink, params)
}
