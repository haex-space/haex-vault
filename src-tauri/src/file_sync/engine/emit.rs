use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value as JsonValue;

use super::super::types::SyncResult;
use super::error::SyncEngineError;

/// Get the current Unix timestamp in seconds.
fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ---------------------------------------------------------------------------
// Update lastSyncedAt via CRDT (propagates to other devices)
// ---------------------------------------------------------------------------

pub(super) fn update_last_synced_at(app: &tauri::AppHandle, rule_id: &str) {
    use tauri::Manager;
    let state = app.state::<crate::AppState>();
    // Phase 2: route HLC poison through `AppState::lock_or_fail` so the
    // user sees a banner via `haex_critical_notifications_no_sync`
    // instead of a silent skip + stderr-only log. Function returns ()
    // so we can't propagate the Err — but the banner row is persisted
    // regardless, and skipping the last-synced-at update is the
    // correct fallback (the alternative would be writing a CRDT row
    // with a corrupted HLC).
    let hlc = match state.lock_or_fail(
        &state.hlc,
        crate::critical::CriticalFailureCode::HlcMutexPoisoned,
        "file_sync::engine::update_last_synced_at",
        serde_json::json!({"rule_id": rule_id}),
    ) {
        Ok(g) => g,
        Err(_) => return,
    };
    let now = unix_now();

    let sql = "UPDATE haex_sync_rules SET last_synced_at = ?1 WHERE id = ?2".to_string();
    let params = vec![
        JsonValue::Number(serde_json::Number::from(now)),
        JsonValue::String(rule_id.to_string()),
    ];

    if let Err(e) = crate::database::core::execute_with_crdt(sql, params, &state.db, &hlc) {
        eprintln!("[FileSyncEngine] Failed to update lastSyncedAt for rule {rule_id}: {e}");
    }

    // Notify frontend that CRDT dirty tables changed (triggers sync push)
    crate::crdt::notify_dirty_tables_changed(&app);
}

// ---------------------------------------------------------------------------
// Event emission
// ---------------------------------------------------------------------------

/// Persist a sync log entry into the CRDT-synced `haex_logs` table.
///
/// `source = "file-sync"`, and the rule ID is stored in the `metadata` JSON
/// (`{ "ruleId": <id> }`) — NOT in `extension_id`, because that column has a
/// FK on `haex_extensions(id)` and sync rules are not extensions, so the
/// INSERT would fail with `FOREIGN KEY constraint failed` on every cycle.
///
/// `message` is encoded as JSON `{ code, params?, raw? }` — a stable
/// machine-readable shape so the frontend can localize the rendered string per
/// device locale. Persisting a pre-rendered locale-specific string here would
/// freeze that locale into the CRDT row forever (it gets replicated to every
/// peer regardless of their locale).
pub(super) fn write_sync_log_entry(
    app: &tauri::AppHandle,
    rule_id: &str,
    level: &str,
    code: &str,
    params: serde_json::Value,
    raw: Option<&str>,
) {
    use tauri::Manager;
    let state = app.state::<crate::AppState>();
    let device_id = state
        .context
        .lock()
        .map(|ctx| ctx.device_id.clone())
        .unwrap_or_default();
    let mut message = serde_json::json!({ "code": code, "params": params });
    if let Some(r) = raw {
        message["raw"] = serde_json::Value::String(r.to_string());
    }
    let metadata = serde_json::json!({ "ruleId": rule_id });
    if let Err(e) = crate::logging::insert_log(
        &state,
        level,
        "file-sync",
        None,
        &message.to_string(),
        Some(metadata),
        &device_id,
    ) {
        eprintln!("[FileSyncEngine] Failed to persist sync log for rule {rule_id}: {e}");
    }
}

pub(super) fn emit_sync_result(
    app: &tauri::AppHandle,
    rule_id: &str,
    result: &Result<SyncResult, SyncEngineError>,
) {
    use tauri::Emitter;
    // emit_to(label, …) keeps these UI-only events out of extension webviews.
    match result {
        Ok(r) => {
            update_last_synced_at(app, rule_id);
            // Per-file transfer failures are collected into r.errors instead of
            // surfacing as Err — without this branch a cycle where every
            // transfer failed (counters all 0, errors populated) would leave
            // no trace in the persistent log.
            if !r.errors.is_empty() {
                let raw = r.errors.join("; ");
                write_sync_log_entry(
                    app,
                    rule_id,
                    "error",
                    "syncCompletedWithErrors",
                    serde_json::json!({ "errorCount": r.errors.len() }),
                    Some(&raw),
                );
            } else if r.files_downloaded > 0
                || r.files_deleted > 0
                || r.directories_created > 0
                || r.conflicts_resolved > 0
            {
                // Only log non-trivial cycles so the persistent log doesn't fill up
                // with empty no-op syncs — mirrors the in-memory append logic in
                // the frontend store. All non-zero counters are persisted so
                // delete-only / mkdir-only / conflict-only cycles don't render
                // as "0 files / 0 bytes" in the history.
                write_sync_log_entry(
                    app,
                    rule_id,
                    "info",
                    "syncSuccess",
                    serde_json::json!({
                        "filesDownloaded": r.files_downloaded,
                        "filesDeleted": r.files_deleted,
                        "directoriesCreated": r.directories_created,
                        "conflictsResolved": r.conflicts_resolved,
                        "bytesTransferred": r.bytes_transferred,
                    }),
                    None,
                );
            }
            let _ = app.emit_to(
                "main",
                "file-sync:complete",
                serde_json::json!({ "ruleId": rule_id, "result": r }),
            );
        }
        Err(e) => {
            // Cancellation is a user-initiated control-flow signal, not a
            // sync failure — persisting it as `syncFailed` would pollute the
            // CRDT log on every stop/disable. The frontend already removes
            // the in-flight state when a cancel emits, so skipping here
            // leaves no orphaned UI artifacts either.
            if matches!(e, SyncEngineError::Cancelled) {
                return;
            }
            let raw = e.to_string();
            // Source/target unavailability (peer offline, network blip,
            // bucket unreachable) is expected — the loop retries with
            // backoff and the rule must not auto-pause. Persisting
            // "syncFailed" every cycle would spam the CRDT log and bounce
            // the frontend on every retry. The runtime event is still
            // emitted so the UI can show a transient state.
            let unavailable_side: Option<&'static str> = match e {
                SyncEngineError::SourceUnavailable(_) => Some("source"),
                SyncEngineError::TargetUnavailable(_) => Some("target"),
                _ => None,
            };
            if unavailable_side.is_none() {
                // Genuine error (DB failure, provider crash, …). Render the
                // raw text verbatim — it already includes whatever
                // provider-specific detail the user needs to debug.
                write_sync_log_entry(
                    app,
                    rule_id,
                    "error",
                    "syncFailed",
                    serde_json::json!({}),
                    Some(&raw),
                );
            }
            let _ = app.emit_to(
                "main",
                "file-sync:error",
                serde_json::json!({
                    "ruleId": rule_id,
                    "error": raw,
                    "unavailable": unavailable_side,
                }),
            );
        }
    }
}
