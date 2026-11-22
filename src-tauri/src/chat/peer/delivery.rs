//! The delivery service of the chat with contacts (spec 072 research R3, R4, R6): its own Nostr
//! client per vault session. It listens for kind 1059 wraps to this device on the own and stored
//! contacts' Nostr-Relays, stores what passes the receiver checks, answers new messages with
//! batched delivery receipts and publishes the outbox with back-off. Since spec 095 every event
//! travels as a ratchet message on the session with the receiving device: sending in
//! `delivery_send.rs`, receiving in `delivery_receive.rs`.
//!
//! Contact relay changes are picked up on catch-up; own relay changes apply on catch-up as well.

use std::collections::{BTreeMap, HashSet};
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use haex_crdt::rusqlite::params;
use nostr::event::{Event, Kind};
use nostr::filter::Filter;
use nostr::types::Timestamp;
use nostr_sdk::client::{Client, ClientNotification};
use nostr_sdk::relay::RelayCapabilities;
use tokio::sync::{broadcast, mpsc};
use tokio_util::sync::CancellationToken;

use super::outbox::{self, Attempt, Entry, FailReason};
use super::receive;
use super::targets::{self, Target};
use super::wrap::BACKDATE_SECS;
use super::{send, session::Me};
use crate::error::{HaexError, Result};
use crate::storage::query::Query;
use crate::vault_gate::VaultDb;

/// How long connecting a Nostr-Relay may take before the attempt goes on without it.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// The outbox is looked at this often even without a kick.
const OUTBOX_TICK: Duration = Duration::from_secs(5);
/// Delivery receipts of a conversation are collected this long (research R6).
const RECEIPT_BATCH: Duration = Duration::from_secs(2);
/// The stored wraps are fetched again this often (`ponytail:` a fixed interval; upgrade: only
/// after the notification channel reported a gap).
const CATCH_UP: Duration = Duration::from_secs(10 * 60);
/// Notifications nostr-sdk keeps for a slow reader before it skips some.
const NOTIFICATION_CHANNEL: NonZeroUsize = match NonZeroUsize::new(16_384) {
    Some(size) => size,
    None => unreachable!(),
};
/// New messages the notifications may fall behind by before they skip some.
const ARRIVALS: usize = 256;
/// The milliseconds since the Unix epoch.
pub fn now_ms() -> i64 {
    crate::passwords::clock::unix_millis(std::time::SystemTime::now())
}

/// The time the delivery works with, in milliseconds since the Unix epoch: [`now_ms`] in the app, a
/// clock the tests move in theirs (spec 095 T003).
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// One vault session's delivery.
pub struct Delivery {
    db: VaultDb,
    me: Me,
    clock: Clock,
    /// The own relays and the stored contacts' relays used to receive wraps.
    read_relays: Mutex<Vec<String>>,
    client: Client,
    /// Delivery receipts still to queue: other vault → its event ids.
    receipts: Mutex<BTreeMap<String, Vec<String>>>,
    /// The vault closes: no further step starts.
    closing: CancellationToken,
    /// New messages from other vaults, for the notifications (FR-023).
    arrived: broadcast::Sender<receive::Stored>,
}

impl Delivery {
    /// Connects to the listening Nostr-Relays and subscribes to the wraps for this device;
    /// `closing` ends it with the vault session.
    pub async fn connect(
        db: VaultDb,
        me: Me,
        clock: Clock,
        read_relays: Vec<String>,
        closing: CancellationToken,
    ) -> Result<(
        Arc<Self>,
        mpsc::UnboundedReceiver<(String, Event)>,
        tokio::task::JoinHandle<()>,
    )> {
        crate::sync::presence::ensure_crypto_provider();
        let client = Client::builder()
            .notification_channel_size(NOTIFICATION_CHANNEL)
            .build();
        let mut active_read_relays = Vec::new();
        for url in &read_relays {
            match client.add_relay(url.as_str()).await {
                Ok(_) => active_read_relays.push(url.clone()),
                Err(error) => log::warn!("chat: Nostr-Relay {url} is not a valid URL: {error}"),
            }
        }
        client.connect().and_wait(CONNECT_TIMEOUT).await;
        let delivery = Arc::new(Self {
            db,
            me,
            clock,
            read_relays: Mutex::new(active_read_relays),
            client,
            receipts: Mutex::new(BTreeMap::new()),
            closing,
            arrived: broadcast::channel(ARRIVALS).0,
        });
        // Attach the notification reader before subscribing: relays may send stored wraps as soon
        // as the subscription is created, and Client::notifications() does not replay earlier ones.
        let (incoming, forward) = delivery.forward_wraps();
        // Without a reachable Nostr-Relay the outbox still runs and marks failures; the
        // subscription is made again on the next wake.
        if let Err(error) = delivery.subscribe().await {
            log::warn!("chat: not subscribed to the listening Nostr-Relays: {error}");
        }
        Ok((delivery, incoming, forward))
    }

    /// Starts forwarding live wraps before the first subscription is sent.
    fn forward_wraps(
        &self,
    ) -> (
        mpsc::UnboundedReceiver<(String, Event)>,
        tokio::task::JoinHandle<()>,
    ) {
        let (sender, receiver) = mpsc::unbounded_channel();
        let mut notifications = self.client.notifications();
        let forward = tokio::spawn(async move {
            use futures::StreamExt;

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

    /// Now, by this delivery's clock.
    fn now(&self) -> i64 {
        (self.clock)()
    }

    /// New messages from other vaults from now on.
    pub fn arrivals(&self) -> broadcast::Receiver<receive::Stored> {
        self.arrived.subscribe()
    }

    /// The wraps to this device since the oldest cursor of the listening Nostr-Relays, 48 h
    /// earlier for the backdated `created_at` (contracts/nostr-chat.md "Receiving").
    async fn filter(&self) -> Result<Filter> {
        let relays = self
            .read_relays
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let since = self
            .db
            .read(move |r| {
                let mut oldest: Option<i64> = None;
                for relay in &relays {
                    let cursor: Option<i64> = r.query_row(
                        "SELECT newest_created_at FROM chat_relay_cursors_no_sync WHERE relay = ?1",
                        params![relay],
                        |row| row.get(0),
                    )?;
                    let Some(cursor) = cursor else {
                        return Ok(None);
                    };
                    oldest = Some(oldest.map_or(cursor, |o| o.min(cursor)));
                }
                Ok(oldest)
            })
            .await?;
        let device = crate::sync::presence::public_key(&self.me.keys.device_pubkey)
            .map_err(|e| invalid(&e.to_string()))?;
        let mut filter = Filter::new().kind(Kind::GiftWrap).pubkey(device);
        if let Some(cursor) = since {
            let secs = u64::try_from(cursor)
                .unwrap_or(0)
                .saturating_sub(BACKDATE_SECS);
            filter = filter.since(Timestamp::from_secs(secs));
        }
        Ok(filter)
    }

    /// Subscribes to new wraps of [`Self::filter`] on the listening Nostr-Relays. The stored ones
    /// come only from [`Self::catch_up`], as one batch it orders before storing (spec 095 T022): a
    /// subscription would hand them out again, in the relay's order and mixed with new ones, and a
    /// session keeps only 40 skipped message keys per chain.
    async fn subscribe(&self) -> Result<()> {
        let filter = self.filter().await?.limit(0);
        let _ = self.client.unsubscribe_all().await;
        self.client
            .subscribe(filter)
            .await
            .map_err(|e| invalid(&e.to_string()))?;
        Ok(())
    }

    /// Fetches the stored wraps of [`Self::filter`] once more and stores the ones not seen yet: the
    /// net under a live event a Nostr-Relay or the notification channel dropped under load.
    async fn catch_up(&self) -> Result<()> {
        self.refresh_read_relays().await?;
        let filter = self.filter().await?;
        let events = self
            .client
            .fetch_events(filter)
            .timeout(CONNECT_TIMEOUT * 4)
            .await
            .map_err(|e| invalid(&e.to_string()))?;
        self.receive_batch(events.into_iter().map(|event| (event, None)).collect())
            .await
            .map(|_| ())
    }

    /// Tracks cards added or removed while the vault stayed open, so the fallback route can be
    /// received even when the contact's own card lists no Nostr-Relays.
    async fn refresh_read_relays(&self) -> Result<()> {
        let desired = self.db.read(|r| Ok(targets::listening_relays(r)?)).await?;
        let current = self
            .read_relays
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let added: Vec<String> = desired
            .iter()
            .filter(|url| !current.contains(url))
            .cloned()
            .collect();
        let removed: Vec<String> = current
            .iter()
            .filter(|url| !desired.contains(url))
            .cloned()
            .collect();
        let mut next = current;
        let mut changed = false;
        for url in &added {
            if let Err(error) = self.client.add_relay(url.as_str()).await {
                log::warn!("chat: Nostr-Relay {url} is not a valid URL: {error}");
                continue;
            }
            let _ = self
                .client
                .try_connect_relay(url.as_str(), CONNECT_TIMEOUT)
                .await;
            next.push(url.clone());
            changed = true;
        }
        for url in &removed {
            if let Err(error) = self.client.remove_relay(url.as_str()).force().await {
                log::debug!("chat: no longer listening to Nostr-Relay {url}: {error}");
            }
            next.retain(|known| known != url);
            changed = true;
        }
        if changed {
            *self
                .read_relays
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = next.clone();
            self.subscribe().await?;
        }
        Ok(())
    }

    /// New connections and a new subscription, after the app came back or the network changed;
    /// what arrived meanwhile is caught up, and the outbox is tried again at once.
    pub async fn reconnect(&self) -> Result<()> {
        self.refresh_read_relays().await?;
        self.client.disconnect().await;
        self.client.connect().and_wait(CONNECT_TIMEOUT).await;
        // A relay still closing from the disconnect ignores a connect as if it were connecting
        // and then stays without a connection; asking again brings it back.
        let deadline = tokio::time::Instant::now() + CONNECT_TIMEOUT;
        while !self.any_relay_connected().await && tokio::time::Instant::now() < deadline {
            self.client.connect().await;
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        self.subscribe().await?;
        self.catch_up_and_keep_record().await?;
        self.retry_now().await
    }

    /// Makes the waiting outbox rows due and publishes them.
    async fn retry_now(&self) -> Result<()> {
        let now = self.now();
        self.db
            .write(move |tx| Ok(outbox::due_now(tx, now)?))
            .await?;
        self.pump().await
    }

    /// Queues the collected delivery receipts, one per other vault, and publishes them.
    pub async fn flush_receipts(&self) -> Result<()> {
        let batch =
            std::mem::take(&mut *self.receipts.lock().unwrap_or_else(PoisonError::into_inner));
        if batch.is_empty() {
            return Ok(());
        }
        let me = self.me.clone();
        let now = self.now();
        self.db
            .write(move |tx| {
                for (peer, delivered) in batch {
                    send::receipt(tx, &me, &peer, delivered, Vec::new(), now)?;
                }
                Ok(())
            })
            .await?;
        self.pump().await
    }

    /// Publishes every due outbox row once; stops between rows when the vault closes.
    pub async fn pump(&self) -> Result<()> {
        let now = self.now();
        let due = self.db.read(move |r| Ok(outbox::due(r, now)?)).await?;
        let mut refreshed_devices = HashSet::new();
        for entry in due {
            if self.closing.is_cancelled() {
                break;
            }
            let entry = if entry.is_resend() {
                let me = self.me.clone();
                let Some(entry) = self
                    .db
                    .write(move |tx| Ok(outbox::start_resend(tx, &me, &entry)?))
                    .await?
                else {
                    continue;
                };
                self.refresh_quiet_records(&entry, &mut refreshed_devices)
                    .await?;
                entry
            } else {
                entry
            };
            let attempt = self.attempt(&entry).await?;
            let now = self.now();
            self.db
                .write(move |tx| Ok(outbox::record(tx, &entry, &attempt, now)?))
                .await?;
        }
        Ok(())
    }

    async fn attempt(&self, entry: &Entry) -> Result<Attempt> {
        let mut attempt = Attempt::default();
        for vault in &entry.pending {
            let me = self.me.clone();
            let for_vault = vault.clone();
            let target = self
                .db
                .read(move |r| Ok(targets::of(r, &me, &for_vault)?))
                .await?;
            match self.publish(entry, vault, &target).await {
                Published::Accepted => attempt.accepted.push(vault.clone()),
                Published::Failed(reason) => attempt.failed.push((vault.clone(), reason)),
                Published::NotYet {
                    error,
                    partial_accepted,
                    none_accepted,
                } => {
                    if partial_accepted {
                        attempt.partial_accepted.push(vault.clone());
                    }
                    if none_accepted {
                        attempt.unaccepted.push(vault.clone());
                    }
                    attempt.error = Some(error);
                }
            }
        }
        Ok(attempt)
    }

    async fn publish(&self, entry: &Entry, vault: &String, target: &Target) -> Published {
        if target.devices.is_empty() {
            // The own vault without other devices has nothing to do; a contact needs a device.
            return if *vault == self.me.vault_hex {
                Published::Accepted
            } else {
                Published::Failed(FailReason::NoDevices)
            };
        }
        if target.relays.is_empty() {
            return Published::Failed(FailReason::NoRelay);
        }
        self.use_relays(&target.relays).await;
        self.publish_to_devices(entry, target).await
    }

    /// Ensures the target relays are available for publishing and receiving.
    async fn use_relays(&self, relays: &[String]) {
        let mut read_relays = self
            .read_relays
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let new_relays: Vec<String> = relays
            .iter()
            .filter(|url| !read_relays.contains(url))
            .cloned()
            .collect();
        let mut added_for_reading = false;
        for url in new_relays {
            if let Err(error) = self
                .client
                .add_relay(url.as_str())
                .capabilities(RelayCapabilities::READ | RelayCapabilities::WRITE)
                .await
            {
                log::warn!("chat: Nostr-Relay {url} is not a valid URL: {error}");
                continue;
            }
            let _ = self
                .client
                .try_connect_relay(url.as_str(), CONNECT_TIMEOUT)
                .await;
            read_relays.push(url);
            added_for_reading = true;
        }
        if added_for_reading {
            *self
                .read_relays
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = read_relays;
            if let Err(error) = self.subscribe().await {
                log::warn!("chat: not subscribed after adding contact relays: {error}");
            }
        }
    }
}

/// What publishing to one vault did.
enum Published {
    Accepted,
    Failed(FailReason),
    /// Not done yet; `partial_accepted` separates relay acceptance from vault completion.
    NotYet {
        error: String,
        partial_accepted: bool,
        none_accepted: bool,
    },
}

fn invalid(reason: &str) -> HaexError {
    HaexError::InvalidInput {
        reason: format!("chat delivery: {reason}"),
    }
}

#[path = "delivery_run.rs"]
mod run;

#[path = "delivery_records.rs"]
mod records;

#[path = "delivery_send.rs"]
mod sending;

#[path = "delivery_receive.rs"]
mod receiving;

#[cfg(test)]
#[path = "delivery_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "forward_secrecy_tests.rs"]
mod forward_secrecy_tests;

#[cfg(test)]
#[path = "crash_tests.rs"]
mod crash_tests;
