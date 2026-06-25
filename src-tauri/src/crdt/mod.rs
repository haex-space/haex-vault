pub mod cleanup;
pub mod commands;
pub mod hlc;
pub mod insert_transformer;
//pub mod query_transformer;
pub mod scanner;
pub mod transformer;
pub mod trigger;

#[cfg(test)]
mod hlc_node_tests;
#[cfg(test)]
mod scanner_origin_tests;

use tauri::{AppHandle, Emitter};

/// Notify the main window that the set of CRDT-dirty tables may have changed.
///
/// Every call site that writes through `execute_with_crdt` (and a couple that
/// mutate `haex_*` tables through other paths) used to emit the
/// `EVENT_CRDT_DIRTY_TABLES_CHANGED` Tauri event inline. Centralising the emit
/// here ensures:
/// - the event name and target window (`"main"`) stay in one place — extension
///   webviews must not observe this event (data leak surface);
/// - fire-and-forget semantics (the discarded `Result`) are consistent, so a
///   single missed `let _ =` cannot turn into an accidental `?` propagation.
///
/// Call this AFTER a successful CRDT-tracked mutation. Errors from `emit_to`
/// are intentionally swallowed: the sync orchestrator also reads dirty tables
/// directly from the database on its own cadence, so a dropped event is a
/// latency hit, not a correctness hit.
pub fn notify_dirty_tables_changed(app: &AppHandle) {
    let _ = app.emit_to(
        "main",
        crate::event_names::EVENT_CRDT_DIRTY_TABLES_CHANGED,
        (),
    );
}
