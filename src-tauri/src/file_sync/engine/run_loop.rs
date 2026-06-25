use std::sync::Arc;
use std::time::Duration;

use serde_json::Value as JsonValue;
use tokio_util::sync::CancellationToken;

use crate::database::DbConnection;

use super::super::provider::SyncProvider;
use super::super::types::{DeleteMode, SyncDirection};

use super::emit::{emit_sync_result, write_sync_log_entry};
use super::error::SyncEngineError;
use super::execute::execute_sync;

// ---------------------------------------------------------------------------
// Periodic sync loop
// ---------------------------------------------------------------------------

/// Base wait after the first failed sync cycle. Subsequent failures double
/// this duration (exponential backoff) up to `MAX_RETRY_INTERVAL`.
const INITIAL_RETRY: Duration = Duration::from_secs(30);

/// Hard cap on the retry interval. Reached after ~6 consecutive failures.
const MAX_RETRY_INTERVAL: Duration = Duration::from_secs(30 * 60);

/// After this many consecutive failures the rule is auto-disabled so it
/// stops hammering a broken target. The user has to re-enable it manually
/// after fixing the underlying issue.
const MAX_CONSECUTIVE_FAILURES: u32 = 20;

/// 30s * 2^(failures-1), capped at MAX_RETRY_INTERVAL.
fn backoff_duration(consecutive_failures: u32) -> Duration {
    if consecutive_failures == 0 {
        return INITIAL_RETRY;
    }
    let shift = (consecutive_failures - 1).min(10);
    let secs = INITIAL_RETRY.as_secs().saturating_mul(1u64 << shift);
    Duration::from_secs(secs.min(MAX_RETRY_INTERVAL.as_secs()))
}

/// Persist `enabled = false` on a sync rule, tear down its runtime
/// state (SyncManager registration + file watchers) and notify the frontend.
async fn auto_disable_rule(app: &tauri::AppHandle, rule_id: &str, failures: u32, last_error: &str) {
    use tauri::{Emitter, Manager};
    let state = app.state::<crate::AppState>();
    {
        let hlc = match state.lock_or_fail(
            &state.hlc,
            crate::critical::CriticalFailureCode::HlcMutexPoisoned,
            "file_sync::engine::auto_disable_rule",
            // Surface the failing rule_id in the banner row so an operator
            // looking at `haex_critical_notifications_no_sync` can correlate
            // the poison to a specific user-visible sync rule.
            serde_json::json!({ "rule_id": rule_id }),
        ) {
            Ok(g) => g,
            Err(_) => return,
        };

        let sql = "UPDATE haex_sync_rules SET enabled = 0 WHERE id = ?1".to_string();
        let params = vec![JsonValue::String(rule_id.to_string())];

        if let Err(e) = crate::database::core::execute_with_crdt(sql, params, &state.db, &hlc) {
            eprintln!("[FileSyncEngine] Failed to persist auto-pause for rule {rule_id}: {e}");
        }
    }

    // Unregister from SyncManager so `is_running` reflects reality, and stop
    // any file watchers that were started alongside this rule. Without this,
    // the loop exits but the runtime state stays as a zombie entry.
    {
        let mut manager = state.sync_manager.lock().await;
        manager.stop(rule_id);
    }
    let _ = state.file_watcher.unwatch(rule_id);
    let _ = state.file_watcher.unwatch(&format!("{}_source", rule_id));

    write_sync_log_entry(
        app,
        rule_id,
        "error",
        "autoPaused",
        serde_json::json!({ "failures": failures }),
        Some(last_error),
    );

    let _ = app.emit_to(
        "main",
        "file-sync:auto-paused",
        serde_json::json!({
            "ruleId": rule_id,
            "consecutiveFailures": failures,
            "lastError": last_error,
        }),
    );
    crate::crdt::notify_dirty_tables_changed(&app);
}

/// Run periodic sync for a rule. Cancellable via `CancellationToken`.
///
/// The optional `trigger_receiver` allows external events (e.g. file watcher)
/// to interrupt the sleep timer and trigger an immediate sync cycle.
///
/// On failure, exponential backoff is applied: 30s, 60s, 120s, … up to
/// `MAX_RETRY_INTERVAL`. The counter resets on the first successful cycle
/// so transient failures still self-heal quickly.
pub async fn run_sync_loop(
    source: Arc<dyn SyncProvider>,
    target: Arc<dyn SyncProvider>,
    direction: SyncDirection,
    delete_mode: DeleteMode,
    rule_id: String,
    interval: Duration,
    cancel: CancellationToken,
    mut trigger_receiver: tokio::sync::mpsc::Receiver<()>,
    db: DbConnection,
    app_handle: tauri::AppHandle,
) {
    // Run initial sync immediately
    eprintln!("[FileSyncEngine] Rule {} initial sync starting", rule_id);
    let result = execute_sync(
        source.clone(),
        target.clone(),
        direction,
        delete_mode,
        &rule_id,
        &db,
        Some(app_handle.clone()),
        Some(cancel.clone()),
    )
    .await;
    eprintln!(
        "[FileSyncEngine] Rule {} initial sync done: {:?}",
        rule_id,
        result.as_ref().map(|r| r.files_downloaded)
    );

    // Two independent counters:
    // - `consecutive_failures` drives the exponential backoff (any failure
    //   slows the retry cadence, including transient outages).
    // - `pause_failures` is the subset that counts toward auto-pause.
    //   Source/target-unavailable errors are excluded because the remote
    //   side may equally be a phone, peer, or cloud bucket that goes
    //   offline temporarily — the rule must keep pinging and resume on
    //   reconnect instead of disabling itself.
    fn is_unavailable(e: &SyncEngineError) -> bool {
        matches!(
            e,
            SyncEngineError::SourceUnavailable(_) | SyncEngineError::TargetUnavailable(_)
        )
    }
    let initial_is_unavail = result.as_ref().err().map(is_unavailable).unwrap_or(false);
    let mut consecutive_failures: u32 = if result.is_err() { 1 } else { 0 };
    let mut pause_failures: u32 = if result.is_err() && !initial_is_unavail {
        1
    } else {
        0
    };
    let mut next_wait = if consecutive_failures > 0 {
        let w = backoff_duration(consecutive_failures);
        eprintln!(
            "[FileSyncEngine] Rule {} failed (attempt {}), next retry in {}s",
            rule_id,
            consecutive_failures,
            w.as_secs()
        );
        w
    } else {
        interval
    };
    // Last error message, used when auto-pausing the rule.
    let mut last_error_text: String = result
        .as_ref()
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    // Marker used by the trigger arm to honour the backoff window: it skips
    // any trigger that fires before the next allowed attempt.
    let mut next_attempt_at: std::time::Instant = std::time::Instant::now() + next_wait;
    emit_sync_result(&app_handle, &rule_id, &result);

    // Stop immediately if the very first sync already exhausted the budget
    // (only realistic with MAX = 1, but keeps the invariant clean).
    if pause_failures >= MAX_CONSECUTIVE_FAILURES {
        eprintln!(
            "[FileSyncEngine] Rule {} auto-paused after {} consecutive failures",
            rule_id, pause_failures
        );
        auto_disable_rule(&app_handle, &rule_id, pause_failures, &last_error_text).await;
        return;
    }

    // Manual mode (interval = 0): only sync on trigger, no periodic timer
    let use_timer = !interval.is_zero();

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                eprintln!("[FileSyncEngine] Rule {} cancelled", rule_id);
                break;
            }
            _ = tokio::time::sleep(next_wait), if use_timer => {
                let result = execute_sync(
                    source.clone(),
                    target.clone(),
                    direction,
                    delete_mode,
                    &rule_id,
                    &db,
                    Some(app_handle.clone()),
                    Some(cancel.clone()),
                )
                .await;
                if let Err(ref e) = result {
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    if !is_unavailable(e) {
                        pause_failures = pause_failures.saturating_add(1);
                    }
                    last_error_text = e.to_string();
                    next_wait = backoff_duration(consecutive_failures);
                    eprintln!(
                        "[FileSyncEngine] Rule {} failed (attempt {}), next retry in {}s",
                        rule_id,
                        consecutive_failures,
                        next_wait.as_secs()
                    );
                } else {
                    if consecutive_failures > 0 {
                        eprintln!(
                            "[FileSyncEngine] Rule {} recovered after {} failures",
                            rule_id, consecutive_failures
                        );
                    }
                    consecutive_failures = 0;
                    pause_failures = 0;
                    last_error_text.clear();
                    next_wait = interval;
                }
                next_attempt_at = std::time::Instant::now() + next_wait;
                emit_sync_result(&app_handle, &rule_id, &result);

                if pause_failures >= MAX_CONSECUTIVE_FAILURES {
                    eprintln!(
                        "[FileSyncEngine] Rule {} auto-paused after {} consecutive failures",
                        rule_id, pause_failures
                    );
                    auto_disable_rule(
                        &app_handle,
                        &rule_id,
                        pause_failures,
                        &last_error_text,
                    )
                    .await;
                    break;
                }
            }
            msg = trigger_receiver.recv() => {
                if msg.is_none() {
                    // All senders dropped — stop the loop cleanly
                    break;
                }
                // Drain any additional pending triggers to avoid redundant syncs
                while trigger_receiver.try_recv().is_ok() {}

                // While the backoff window is open, ignore file-watcher triggers
                // — otherwise filesystem activity bypasses the retry slowdown
                // and we hammer the failing target. Triggers reaching us after
                // the backoff has elapsed proceed normally.
                if consecutive_failures > 0 && std::time::Instant::now() < next_attempt_at {
                    let remaining = next_attempt_at - std::time::Instant::now();
                    eprintln!(
                        "[FileSyncEngine] Rule {} trigger suppressed during backoff (~{}s left)",
                        rule_id,
                        remaining.as_secs()
                    );
                    continue;
                }

                let result = execute_sync(
                    source.clone(),
                    target.clone(),
                    direction,
                    delete_mode,
                    &rule_id,
                    &db,
                    Some(app_handle.clone()),
                    Some(cancel.clone()),
                )
                .await;
                if let Err(ref e) = result {
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    if !is_unavailable(e) {
                        pause_failures = pause_failures.saturating_add(1);
                    }
                    last_error_text = e.to_string();
                    next_wait = backoff_duration(consecutive_failures);
                } else {
                    consecutive_failures = 0;
                    pause_failures = 0;
                    last_error_text.clear();
                    next_wait = interval;
                }
                next_attempt_at = std::time::Instant::now() + next_wait;
                emit_sync_result(&app_handle, &rule_id, &result);

                if pause_failures >= MAX_CONSECUTIVE_FAILURES {
                    eprintln!(
                        "[FileSyncEngine] Rule {} auto-paused after {} consecutive failures",
                        rule_id, pause_failures
                    );
                    auto_disable_rule(
                        &app_handle,
                        &rule_id,
                        pause_failures,
                        &last_error_text,
                    )
                    .await;
                    break;
                }
            }
        }
    }
}
