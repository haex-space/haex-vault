//! Server-side waiter registry for permission-prompt resolution.
//!
//! The external bridge (`external_bridge::server::process`) runs requests to
//! completion inside a single async task per connection — there is no
//! request/response round-trip the client can use to retry after a prompt is
//! shown. So when a permission check returns `PermissionPromptRequired`, the
//! bridge itself registers a waiter here (keyed by `(principal_id,
//! resource_type, action)` — deliberately WITHOUT `target`, since the user
//! can edit the prompted target before saving, see `permission-prompt.vue`'s
//! editable target field) and blocks on it. Once the frontend resolves the
//! prompt, `notify_extension_permission_decision` wakes the waiter; the
//! bridge then performs an authoritative re-check rather than trusting the
//! wake signal blindly (the re-check is what actually enforces the decision).

use std::collections::HashMap;
use tokio::sync::{oneshot, Mutex};

/// `(principal_id, resource_type, action)`.
pub type PromptKey = (String, String, String);

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
    pub async fn register(&self, key: PromptKey) -> oneshot::Receiver<()> {
        let (tx, rx) = oneshot::channel();
        let mut waiters = self.waiters.lock().await;
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

    fn key(s: &str) -> PromptKey {
        (
            "client-1".to_string(),
            "extensionApi".to_string(),
            s.to_string(),
        )
    }

    #[tokio::test]
    async fn wake_resolves_a_registered_waiter() {
        let waiters = PermissionPromptWaiters::new();
        let rx = waiters.register(key("call")).await;
        waiters.wake(&key("call")).await;
        assert!(rx.await.is_ok());
    }

    #[tokio::test]
    async fn wake_resolves_multiple_waiters_for_the_same_key() {
        let waiters = PermissionPromptWaiters::new();
        let rx1 = waiters.register(key("call")).await;
        let rx2 = waiters.register(key("call")).await;
        waiters.wake(&key("call")).await;
        assert!(rx1.await.is_ok());
        assert!(rx2.await.is_ok());
    }

    #[tokio::test]
    async fn wake_does_not_affect_a_different_key() {
        let waiters = PermissionPromptWaiters::new();
        let rx = waiters.register(key("call")).await;
        waiters.wake(&key("other-action")).await;
        // The sender for "call" is still pending — dropping `waiters` (out of
        // scope at test end) would close it, so assert it's not yet resolved
        // via try_recv instead of awaiting indefinitely.
        let mut rx = rx;
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn wake_without_a_registered_waiter_is_a_no_op() {
        let waiters = PermissionPromptWaiters::new();
        // Must not panic even though nothing registered for this key.
        waiters.wake(&key("nothing-registered")).await;
    }
}
