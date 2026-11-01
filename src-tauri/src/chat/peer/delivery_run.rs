//! The loop of the delivery service (spec 072 research R3): one task per vault session that
//! receives, publishes, batches receipts and catches up, until the vault closes.

use std::sync::Arc;

use futures::StreamExt;
use nostr::event::{Event, Kind};
use nostr_sdk::client::ClientNotification;
use tokio::sync::mpsc;

use super::{now_ms, Delivery, CATCH_UP, OUTBOX_TICK, RECEIPT_BATCH};
use crate::chat::peer::session::Session;
use crate::chat::peer::verify;

impl Delivery {
    /// Reads the client's notifications in a task of their own and hands the wraps on without a
    /// limit: nostr-sdk skips notifications for a reader that falls behind, and this service's
    /// loop also publishes.
    fn forward_wraps(
        &self,
    ) -> (
        mpsc::UnboundedReceiver<(String, Event)>,
        tokio::task::JoinHandle<()>,
    ) {
        let (sender, receiver) = mpsc::unbounded_channel();
        let mut notifications = self.client.notifications();
        let forward = tokio::spawn(async move {
            while let Some(notification) = notifications.next().await {
                if let ClientNotification::Event {
                    relay_url, event, ..
                } = notification
                {
                    if event.kind == Kind::GiftWrap
                        && sender.send((relay_url.to_string(), *event)).is_err()
                    {
                        return;
                    }
                }
            }
        });
        (receiver, forward)
    }

    /// Runs until the vault closes: receives, publishes, sends receipts.
    pub async fn run(self: Arc<Self>, session: Arc<Session>) {
        let token = self.closing.clone();
        let (mut incoming, forward) = self.forward_wraps();
        let mut outbox_tick = tokio::time::interval(OUTBOX_TICK);
        let mut receipt_tick = tokio::time::interval(RECEIPT_BATCH);
        let mut catch_up_tick =
            tokio::time::interval_at(tokio::time::Instant::now() + CATCH_UP, CATCH_UP);
        let prune_now = now_ms();
        if let Err(error) = self
            .db
            .write(move |tx| Ok(verify::prune_seen(tx, prune_now)?))
            .await
        {
            log::warn!("chat: seen wraps not pruned: {error}");
        }
        if let Err(error) = self.retry_now().await {
            log::warn!("chat: the outbox was not tried at the start: {error}");
        }
        loop {
            let result = tokio::select! {
                () = token.cancelled() => break,
                () = session.kick.notified() => self.pump().await,
                _ = outbox_tick.tick() => self.pump().await,
                _ = receipt_tick.tick() => self.flush_receipts().await,
                _ = catch_up_tick.tick() => self.catch_up().await,
                () = session.wake.notified() => self.reconnect().await,
                received = incoming.recv() => {
                    let Some((relay, event)) = received else { break };
                    self.receive(event, Some(relay)).await.map(|_| ())
                }
            };
            if let Err(error) = result {
                log::warn!("chat: delivery step failed: {error}");
            }
        }
        forward.abort();
        if let Err(error) = self.flush_receipts().await {
            log::warn!("chat: receipts not queued at the close: {error}");
        }
        self.client.shutdown().await;
    }
}
