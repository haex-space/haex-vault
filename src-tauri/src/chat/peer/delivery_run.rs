//! The loop of the delivery service (spec 072 research R3): one task per vault session that
//! receives, publishes, batches receipts and catches up, until the vault closes. It also sends the
//! keep-alives of spec 095 (T038): after the first catch-up, then every [`KEEP_ALIVE_CHECK`].

use std::sync::Arc;

use nostr::event::Event;

use super::{Delivery, CATCH_UP, KEEP_ALIVE_CHECK, OUTBOX_TICK, RECEIPT_BATCH};
use crate::chat::peer::session::Session;
use crate::chat::peer::verify;

impl Delivery {
    /// Runs until the vault closes: receives wraps, publishes outbox entries and sends receipts.
    pub async fn run(
        self: Arc<Self>,
        session: Arc<Session>,
        mut incoming: tokio::sync::mpsc::UnboundedReceiver<(String, Event)>,
        forward: tokio::task::JoinHandle<()>,
    ) {
        let token = self.closing.clone();
        let mut outbox_tick = tokio::time::interval(OUTBOX_TICK);
        let mut receipt_tick = tokio::time::interval(RECEIPT_BATCH);
        let mut catch_up_tick =
            tokio::time::interval_at(tokio::time::Instant::now() + CATCH_UP, CATCH_UP);
        let mut keep_alive_tick = tokio::time::interval_at(
            tokio::time::Instant::now() + KEEP_ALIVE_CHECK,
            KEEP_ALIVE_CHECK,
        );
        let prune_now = self.now();
        if let Err(error) = self
            .db
            .write(move |tx| Ok(verify::prune_seen(tx, prune_now)?))
            .await
        {
            log::warn!("chat: seen wraps not pruned: {error}");
        }
        // One catch-up at the start of the vault session, after which the chat key record is kept
        // current (spec 095 T013). Live wraps wait meanwhile, so the stored ones are stored first,
        // as one ordered batch.
        if let Err(error) = self.catch_up_and_keep_record().await {
            log::warn!("chat: no catch-up at the start: {error}");
        }
        wake_files(&session);
        if let Err(error) = self.send_keep_alives().await {
            log::warn!("chat: no keep-alives at the start: {error}");
        }
        if let Err(error) = self.retry_now().await {
            log::warn!("chat: the outbox was not tried at the start: {error}");
        }
        loop {
            let result = tokio::select! {
                () = token.cancelled() => break,
                () = session.kick.notified() => self.pump().await,
                _ = outbox_tick.tick() => {
                    if self.catch_up_owed() {
                        if let Err(error) = self.catch_up_and_keep_record().await {
                            log::warn!("chat: an owed catch-up failed: {error}");
                        }
                        wake_files(&session);
                    }
                    self.pump().await
                }
                _ = receipt_tick.tick() => self.flush_receipts().await,
                _ = catch_up_tick.tick() => {
                    let caught = self.catch_up_and_keep_record().await;
                    // Files of messages the catch-up stored, if any (US3).
                    wake_files(&session);
                    caught
                }
                _ = keep_alive_tick.tick() => self.send_keep_alives().await.map(|_| ()),
                () = session.wake.notified() => self.reconnect().await,
                received = incoming.recv() => {
                    let Some(first) = received else { break };
                    // Everything waiting is one batch, ordered before it is stored (spec 095 T022).
                    let mut batch = vec![first];
                    while let Ok(next) = incoming.try_recv() {
                        batch.push(next);
                    }
                    let batch = batch
                        .into_iter()
                        .map(|(relay, event)| (event, Some(relay)))
                        .collect();
                    self.receive_batch(batch).await.map(|stored| {
                        if stored.iter().any(|stored| stored.new && stored.attachments) {
                            wake_files(&session);
                        }
                    })
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

/// Tells this device's file downloads to look for new ones.
fn wake_files(session: &Session) {
    if let Some(files) = session.files.get() {
        files.wake.notify_one();
    }
}
