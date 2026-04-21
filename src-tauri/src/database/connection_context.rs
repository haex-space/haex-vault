// src-tauri/src/database/connection_context.rs

use crate::crdt::hlc::{HlcError, HlcService};
use std::sync::{Arc, Mutex};
use uhlc::Timestamp;

/// Per-connection state for transaction-scoped CRDT operations.
///
/// Holds the HLC timestamp that is shared across all writes inside a single
/// SQLite transaction. The slot is reset by the connection's commit_hook and
/// rollback_hook so every new transaction (explicit or auto-commit) starts with
/// a fresh value.
#[derive(Clone)]
pub struct ConnectionContext {
    tx_hlc_slot: Arc<Mutex<Option<Timestamp>>>,
}

impl ConnectionContext {
    pub fn new() -> Self {
        ConnectionContext {
            tx_hlc_slot: Arc::new(Mutex::new(None)),
        }
    }

    /// Returns the HLC for the current transaction. When the slot is empty a
    /// new timestamp is drawn from the HlcService and cached until the slot
    /// is reset by commit or rollback.
    pub fn current_or_new_tx_hlc(
        &self,
        hlc_service: &HlcService,
    ) -> Result<Timestamp, HlcError> {
        let mut slot = self
            .tx_hlc_slot
            .lock()
            .map_err(|_| HlcError::MutexPoisoned)?;
        if let Some(existing) = slot.as_ref() {
            return Ok(*existing);
        }
        let ts = hlc_service.new_timestamp()?;
        *slot = Some(ts);
        Ok(ts)
    }

    /// Clears the slot. Called from commit_hook and rollback_hook — must never
    /// panic, so a poisoned mutex is silently ignored (the slot is unusable
    /// anyway and any further `current_or_new_tx_hlc` call will surface the
    /// error).
    pub fn reset_tx_slot(&self) {
        if let Ok(mut slot) = self.tx_hlc_slot.lock() {
            *slot = None;
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
    fn same_tx_returns_same_timestamp() {
        let hlc = HlcService::new_for_testing("test-device-1");
        let ctx = ConnectionContext::new();

        let a = ctx.current_or_new_tx_hlc(&hlc).expect("first hlc");
        let b = ctx.current_or_new_tx_hlc(&hlc).expect("second hlc");

        assert_eq!(a, b, "repeated calls inside the same tx must return the same timestamp");
    }

    #[test]
    fn reset_produces_fresh_timestamp() {
        let hlc = HlcService::new_for_testing("test-device-2");
        let ctx = ConnectionContext::new();

        let first = ctx.current_or_new_tx_hlc(&hlc).expect("first hlc");
        ctx.reset_tx_slot();
        let second = ctx.current_or_new_tx_hlc(&hlc).expect("second hlc");

        assert_ne!(first, second, "after reset a new timestamp must be produced");
    }
}
