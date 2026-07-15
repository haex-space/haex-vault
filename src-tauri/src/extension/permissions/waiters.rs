//! Server-side waiter registry for permission-prompt resolution.
//!
//! The external bridge (`external_bridge::server::process`) runs requests to
//! completion inside a single async task per connection — there is no
//! request/response round-trip the client can use to retry after a prompt is
//! shown. So when a permission check returns `PermissionPromptRequired`, the
//! bridge itself registers a waiter here (keyed by `(principal_id,
//! resource_type, action, target)` — `target` is the ORIGINAL prompt target,
//! which `notify_extension_permission_decision` round-trips unchanged, so
//! concurrent prompts for different targets never wake each other) and blocks
//! on it. Once the frontend resolves the prompt,
//! `notify_extension_permission_decision` wakes the waiter; the bridge then
//! performs an authoritative re-check rather than trusting the wake signal
//! blindly (the re-check is what actually enforces the decision).

use std::collections::HashMap;
use tokio::sync::{oneshot, Mutex};

/// `(principal_id, resource_type, action, target)`.
pub type PromptKey = (String, String, String, String);

#[derive(Default)]
pub struct PermissionPromptWaiters {
    waiters: Mutex<HashMap<PromptKey, Vec<oneshot::Sender<()>>>>,
}

impl PermissionPromptWaiters {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a new waiter for `key`, returning the receiver half.
    /// Multiple concurrent callers for the same key each get their own
    /// sender in the `Vec` and are all woken together.
    ///
    /// Also prunes senders whose receiver was dropped (timed-out or
    /// abandoned prompts) — without this, keys that are never woken would
    /// accumulate dead senders for the lifetime of the process.
    pub async fn register(&self, key: PromptKey) -> oneshot::Receiver<()> {
        let (tx, rx) = oneshot::channel();
        let mut waiters = self.waiters.lock().await;
        waiters.retain(|_, senders| {
            senders.retain(|tx| !tx.is_closed());
            !senders.is_empty()
        });
        waiters.entry(key).or_default().push(tx);
        rx
    }

    /// Wakes (and removes) all waiters registered for `key`. Called after a
    /// permission-prompt decision is resolved — grant OR deny, so a denial
    /// wakes waiters immediately instead of leaving them to idle until their
    /// timeout.
    pub async fn wake(&self, key: &PromptKey) {
        let mut waiters = self.waiters.lock().await;
        if let Some(senders) = waiters.remove(key) {
            for tx in senders {
                let _ = tx.send(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::oneshot::error::TryRecvError;

    fn key(s: &str) -> PromptKey {
        (
            "client-1".to_string(),
            "extensionApi".to_string(),
            "call".to_string(),
            s.to_string(),
        )
    }

    #[tokio::test]
    async fn wake_resolves_a_registered_waiter() {
        let waiters = PermissionPromptWaiters::new();
        let rx = waiters.register(key("pk::ext::getItems")).await;
        waiters.wake(&key("pk::ext::getItems")).await;
        assert!(rx.await.is_ok());
    }

    #[tokio::test]
    async fn wake_resolves_multiple_waiters_for_the_same_key() {
        let waiters = PermissionPromptWaiters::new();
        let rx1 = waiters.register(key("pk::ext::getItems")).await;
        let rx2 = waiters.register(key("pk::ext::getItems")).await;
        waiters.wake(&key("pk::ext::getItems")).await;
        assert!(rx1.await.is_ok());
        assert!(rx2.await.is_ok());
    }

    #[tokio::test]
    async fn wake_does_not_affect_a_different_key() {
        let waiters = PermissionPromptWaiters::new();
        let rx = waiters.register(key("pk::ext::getItems")).await;
        waiters.wake(&key("pk::ext::other-target")).await;
        // The sender for the original key must still be alive and pending —
        // `Empty` proves the channel is open but unresolved (a `Closed` error
        // would mean the sender was wrongly dropped/woken).
        let mut rx = rx;
        assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[tokio::test]
    async fn wake_without_a_registered_waiter_is_a_no_op() {
        let waiters = PermissionPromptWaiters::new();
        // Must not panic even though nothing registered for this key.
        waiters.wake(&key("nothing-registered")).await;
    }

    #[tokio::test]
    async fn register_prunes_senders_whose_receiver_was_dropped() {
        let waiters = PermissionPromptWaiters::new();
        let rx = waiters.register(key("abandoned")).await;
        drop(rx); // simulate a timed-out/abandoned prompt

        // The next register (any key) prunes the dead sender.
        let _rx2 = waiters.register(key("other")).await;

        let map = waiters.waiters.lock().await;
        assert!(!map.contains_key(&key("abandoned")));
        assert_eq!(map.get(&key("other")).map(Vec::len), Some(1));
    }
}
