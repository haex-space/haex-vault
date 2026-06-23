//! Sync engine — orchestration, execution, and periodic loop.
//!
//! Ties together providers, diff computation, and database state tracking.
//!
//! ## Mutex poisoning in progress-tracking locks
//!
//! Many `Mutex`/`RwLock` accesses in this file (`speed_tracker`,
//! `file_progress`, `byte_progress`, `last_emit` timestamps) use
//! `unwrap_or_else(|e| e.into_inner())`. These guard *UI progress state* —
//! transient counters and timestamps used to feed the sync-status emitter.
//! A poison there results in a momentarily wrong byte counter; the next
//! `add()` overwrites with fresh data. There is no durable state behind
//! these locks and no CRDT involvement, so a banner row would be misleading.
//!
//! HLC and DB-mutating paths in this file (e.g. `update_last_synced_at`,
//! `auto_disable_rule`) DO use `lock_or_fail` and surface a banner row.

mod conflict;
mod emit;
mod error;
mod execute;
mod run_loop;
mod speed;
mod state;

#[cfg(test)]
mod tests;

pub use error::SyncEngineError;
pub use execute::execute_sync;
pub use run_loop::run_sync_loop;
pub use state::{
    clear_sync_state, load_sync_state, mark_deleted, upsert_sync_state, SyncStateEntry,
};
