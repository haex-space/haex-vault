//! The delivery service of the chat with contacts (spec 072 research R3, R4, R6): its own Nostr
//! client per vault session. It listens for kind 1059 wraps to this device on the own and stored
//! contacts' Nostr-Relays, stores what passes the receiver checks, answers new messages with
//! batched delivery receipts and publishes the outbox with back-off.
//!
//! Contact relay changes are picked up on catch-up; own relay changes apply on catch-up as well.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use haex_crdt::rusqlite::params;
use nostr::event::{Event, Kind};
use nostr::filter::Filter;
use nostr::types::Timestamp;
use nostr_sdk::client::Client;
use nostr_sdk::relay::RelayCapabilities;
use tokio_util::sync::CancellationToken;

use super::outbox::{self, Attempt, Entry, FailReason};
use super::receive;
use super::targets::{self, Target};
use super::verify::{self, Incoming};
use super::wrap::{self, BACKDATE_SECS};
use super::{send, session::Me};
use crate::error::{HolziError, Result};
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
/// The milliseconds since the Unix epoch.
pub fn now_ms() -> i64 {
    crate::passwords::clock::unix_millis(std::time::SystemTime::now())
}

/// One vault session's delivery.
pub struct Delivery {
    db: VaultDb,
    me: Me,
    /// The own relays and the stored contacts' relays used to receive wraps.
    read_relays: Mutex<Vec<String>>,
    client: Client,
    /// Delivery receipts still to queue: other vault → its event ids.
    receipts: Mutex<BTreeMap<String, Vec<String>>>,
    /// The vault closes: no further step starts.
    closing: CancellationToken,
}

impl Delivery {
    /// Connects to the listening Nostr-Relays and subscribes to the wraps for this device;
    /// `closing` ends it with the vault session.
    pub async fn connect(
        db: VaultDb,
        me: Me,
        read_relays: Vec<String>,
        closing: CancellationToken,
    ) -> Result<Arc<Self>> {
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
            read_relays: Mutex::new(active_read_relays),
            client,
            receipts: Mutex::new(BTreeMap::new()),
            closing,
        });
        // Without a reachable Nostr-Relay the outbox still runs and marks failures; the
        // subscription is made again on the next wake.
        if let Err(error) = delivery.subscribe().await {
            log::warn!("chat: not subscribed to the listening Nostr-Relays: {error}");
        }
        Ok(delivery)
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

    /// Subscribes to [`Self::filter`] on the listening Nostr-Relays.
    async fn subscribe(&self) -> Result<()> {
        let filter = self.filter().await?;
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
        for event in events {
            self.receive(event, None).await?;
        }
        Ok(())
    }

    /// Tracks cards added or removed while the vault stayed open, so the fallback route can be
    /// received even when the contact's own card lists no Nostr-Relays.
    async fn refresh_read_relays(&self) -> Result<()> {
        let desired = self
            .db
            .read(|r| Ok(targets::listening_relays(r)?))
            .await?;
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
    /// the outbox is tried again at once.
    pub async fn reconnect(&self) -> Result<()> {
        self.refresh_read_relays().await?;
        self.client.disconnect().await;
        self.client.connect().and_wait(CONNECT_TIMEOUT).await;
        self.subscribe().await?;
        self.retry_now().await
    }

    /// Makes the waiting outbox rows due and publishes them.
    async fn retry_now(&self) -> Result<()> {
        let now = now_ms();
        self.db
            .write(move |tx| Ok(outbox::due_now(tx, now)?))
            .await?;
        self.pump().await
    }

    /// Stores a received wrap; answers the message it stored, if any. `relay` is where it came
    /// from live; its cursor moves on.
    pub async fn receive(
        &self,
        event: Event,
        relay: Option<String>,
    ) -> Result<Option<receive::Stored>> {
        let wrap_id = event.id.to_hex();
        let seen = self
            .db
            .read(move |r| {
                Ok(r.query_row(
                    "SELECT 1 FROM chat_wraps_seen_no_sync WHERE wrap_id = ?1",
                    params![wrap_id],
                    |row| row.get::<_, i64>(0),
                )?
                .is_some())
            })
            .await?;
        if seen {
            return Ok(None);
        }
        let me = self.me.clone();
        let now = now_ms();
        let stored = self
            .db
            .write(move |tx| {
                if !verify::first_sight(tx, &event.id.to_hex(), now)? {
                    return Ok(None);
                }
                if let Some(relay) = relay {
                    let created = i64::try_from(event.created_at.as_secs()).unwrap_or(i64::MAX);
                    tx.execute(
                        "INSERT INTO chat_relay_cursors_no_sync (relay, newest_created_at) \
                         VALUES (?1, ?2) ON CONFLICT (relay) DO UPDATE SET newest_created_at = \
                         MAX(newest_created_at, excluded.newest_created_at)",
                        params![relay, created],
                    )?;
                }
                let Ok(opened) = wrap::unwrap(&event, &me.keys.device_secret) else {
                    log::info!("chat: dropped a wrap: undecryptable");
                    return Ok(None);
                };
                let verified = match verify::check(tx, &me.vault, &opened)? {
                    Ok(verified) => verified,
                    Err(drop) => {
                        log::info!("chat: dropped a rumor: {}", drop.code());
                        return Ok(None);
                    }
                };
                Ok(match &verified.incoming {
                    Incoming::Message(envelope) => {
                        receive::message(tx, me.receiver(), &verified.sender_device, envelope, now)?
                            .map(|stored| (stored, envelope.id.clone()))
                    }
                    Incoming::Receipt(receipt) => {
                        receive::receipt(tx, &me.vault_hex, receipt, now)?;
                        None
                    }
                })
            })
            .await?;
        let Some((stored, event_id)) = stored else {
            return Ok(None);
        };
        if stored.new && stored.incoming {
            self.receipts
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(stored.peer.clone())
                .or_default()
                .push(event_id);
        }
        Ok(Some(stored))
    }

    /// Queues the collected delivery receipts, one per other vault, and publishes them.
    pub async fn flush_receipts(&self) -> Result<()> {
        let batch =
            std::mem::take(&mut *self.receipts.lock().unwrap_or_else(PoisonError::into_inner));
        if batch.is_empty() {
            return Ok(());
        }
        let me = self.me.clone();
        let now = now_ms();
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
        let now = now_ms();
        let due = self.db.read(move |r| Ok(outbox::due(r, now)?)).await?;
        for entry in due {
            if self.closing.is_cancelled() {
                break;
            }
            let attempt = self.attempt(&entry).await?;
            let now = now_ms();
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
                Published::NotYet(error) => attempt.error = Some(error),
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
        let kind = Kind::from(entry.rumor.kind);
        let mut accepted = false;
        let mut error = String::from("no Nostr-Relay accepted");
        for device in &target.devices {
            let event = match wrap::wrap(
                kind,
                &entry.rumor.content,
                &self.me.keys.device_secret,
                device,
            ) {
                Ok(event) => event,
                Err(_) => return Published::Failed(FailReason::TooLarge),
            };
            match self
                .client
                .send_event(&event)
                .to(target.relays.iter().map(String::as_str))
                .await
            {
                Ok(output) if !output.success.is_empty() => accepted = true,
                Ok(output) => {
                    if let Some(reason) = output.failed.values().next() {
                        error = reason.clone();
                    }
                }
                Err(e) => error = e.to_string(),
            }
        }
        if accepted {
            Published::Accepted
        } else {
            Published::NotYet(error)
        }
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
                .capabilities(RelayCapabilities::READ_WRITE)
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
    NotYet(String),
}

fn invalid(reason: &str) -> HolziError {
    HolziError::InvalidInput {
        reason: format!("chat delivery: {reason}"),
    }
}

#[path = "delivery_run.rs"]
mod run;

#[cfg(test)]
#[path = "delivery_tests.rs"]
mod tests;
