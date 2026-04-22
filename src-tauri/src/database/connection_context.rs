// src-tauri/src/database/connection_context.rs

use crate::crdt::hlc::{HlcError, HlcService};
use std::sync::{Arc, Mutex};
use uhlc::Timestamp;

/// Per-connection state for transaction-scoped CRDT operations.
///
/// Holds the HLC timestamp that is shared across all writes inside a single
/// SQLite transaction. The slot is reset by the connection's commit_hook and
/// rollback_hook so every new transaction (explicit or auto-commit) starts
/// with a fresh value.
///
/// A `write_pending` flag guards the cache so a stray read-only
/// `SELECT current_hlc()` cannot poison the HLC of a later write transaction:
/// the cache is only reused when the update_hook has observed at least one
/// row-level INSERT/UPDATE/DELETE in the current transaction.
#[derive(Clone)]
pub struct ConnectionContext {
    tx_hlc_slot: Arc<Mutex<Option<Timestamp>>>,
    write_pending: Arc<Mutex<bool>>,
}

impl ConnectionContext {
    pub fn new() -> Self {
        ConnectionContext {
            tx_hlc_slot: Arc::new(Mutex::new(None)),
            write_pending: Arc::new(Mutex::new(false)),
        }
    }

    /// Returns the HLC for the current transaction. When no write has been
    /// observed yet, every call draws a fresh timestamp — read-only probes
    /// (`SELECT current_hlc()`) therefore never pin a value that a later
    /// write transaction could inherit. Once `mark_write_pending` fires
    /// from the update_hook, subsequent calls within the same transaction
    /// return the first cached value until commit or rollback.
    pub fn current_or_new_tx_hlc(
        &self,
        hlc_service: &HlcService,
    ) -> Result<Timestamp, HlcError> {
        let writes = *self
            .write_pending
            .lock()
            .map_err(|_| HlcError::MutexPoisoned)?;
        let mut slot = self
            .tx_hlc_slot
            .lock()
            .map_err(|_| HlcError::MutexPoisoned)?;
        if writes {
            if let Some(existing) = slot.as_ref() {
                return Ok(*existing);
            }
        }
        let ts = hlc_service.new_timestamp()?;
        *slot = Some(ts);
        Ok(ts)
    }

    /// Signals that a row-level write happened in the current transaction.
    /// Called from the connection's `update_hook` on every INSERT/UPDATE/DELETE
    /// so the next `current_or_new_tx_hlc` call can safely treat the cached
    /// slot as transaction-scoped instead of a stale read-only probe.
    pub fn mark_write_pending(&self) {
        if let Ok(mut w) = self.write_pending.lock() {
            *w = true;
        }
    }

    /// Clears the slot. Called from commit_hook and rollback_hook — must never
    /// panic, so a poisoned mutex is silently ignored (the slot is unusable
    /// anyway and any further `current_or_new_tx_hlc` call will surface the
    /// error).
    pub fn reset_tx_slot(&self) {
        if let Ok(mut slot) = self.tx_hlc_slot.lock() {
            *slot = None;
        }
        if let Ok(mut w) = self.write_pending.lock() {
            *w = false;
        }
    }
}

impl Default for ConnectionContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_share_timestamp_once_write_pending_is_set() {
        // Mirrors the production sequence: update_hook flips write_pending on
        // the first row change, after which every `current_or_new_tx_hlc`
        // call returns the same cached timestamp.
        let hlc = HlcService::new_for_testing("test-device-1");
        let ctx = ConnectionContext::new();

        let first = ctx.current_or_new_tx_hlc(&hlc).expect("first hlc");
        ctx.mark_write_pending();
        let second = ctx.current_or_new_tx_hlc(&hlc).expect("second hlc");

        assert_eq!(
            first, second,
            "once write_pending is set, repeated calls must return the cached timestamp"
        );
    }

    #[test]
    fn readonly_calls_do_not_pin_a_stale_slot() {
        // Without a write_pending signal, every call must draw a fresh
        // timestamp — otherwise a bare `SELECT current_hlc()` could dictate
        // the HLC of the next write transaction.
        let hlc = HlcService::new_for_testing("test-device-readonly");
        let ctx = ConnectionContext::new();

        let a = ctx.current_or_new_tx_hlc(&hlc).expect("first hlc");
        let b = ctx.current_or_new_tx_hlc(&hlc).expect("second hlc");

        assert_ne!(
            a, b,
            "read-only invocations must not reuse a cached slot that no write has claimed"
        );
    }

    #[test]
    fn reset_produces_fresh_timestamp() {
        let hlc = HlcService::new_for_testing("test-device-2");
        let ctx = ConnectionContext::new();

        let first = ctx.current_or_new_tx_hlc(&hlc).expect("first hlc");
        ctx.mark_write_pending();
        let pinned = ctx.current_or_new_tx_hlc(&hlc).expect("pinned hlc");
        assert_eq!(first, pinned, "write_pending must pin the first value");

        ctx.reset_tx_slot();
        let fresh = ctx.current_or_new_tx_hlc(&hlc).expect("post-reset hlc");

        assert_ne!(first, fresh, "after reset a new timestamp must be produced");
    }
}
