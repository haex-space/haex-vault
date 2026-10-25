//! The presence loop of an open vault (spec 024, FR-007, FR-008, FR-010,
//! FR-044, FR-045, contracts/nostr-events.md).
//!
//! [`run`] connects to the Nostr relays, listens on today's and yesterday's
//! mailbox and sends what this device has to say there:
//!
//! - a listed device with at least one peer sends its presence (kind 24100);
//! - a device the list does not name yet, a copy of a linked device's vault
//!   file, sends a signed request to be admitted (kind 24101,
//!   [`crate::sync::admission`]) until a main device admits it;
//! - a removed device, and a device whose list names only itself, send
//!   nothing.
//!
//! What arrives is checked before anything else happens: a presence meeting
//! from a listed device is recorded and dialed; one from an unknown device
//! that claims a newer device list is dialed once, to fetch that list (a copy
//! of a main device enrolled itself, FR-007); a request from an unknown
//! device is kept for the main devices to decide on.
//!
//! Maintainability exception (spaex 500-LoC rule): publishing, subscribing and handling incoming
//! presence share the mailbox keys and the roster of one loop, so they have stayed together.
//! Concrete split plan, if this grows further: move the handling of incoming events
//! (`handle_incoming`, `note_newer_list`, `handle_request`, `record_listed`, `Roster`,
//! `read_roster`) into `presence_incoming.rs`, leaving `run` and the subscription here.

use std::collections::HashSet;

use nostr::event::Event;
use nostr::key::PublicKey;

use crate::sync::admission::{self, ADMISSION_KIND};
use crate::sync::home_relay;
use crate::sync::keys::DeviceKeys;
use crate::sync::presence::{
    build, mailbox_keys, now_ms, unwrap_rumor, wrap_payload, PresenceContent, PresenceError,
    PRESENCE_KIND,
};
use iroh::RelayUrl;

/// How often this device republishes its own presence, absent an address
/// change (contracts/nostr-events.md).
const PUBLISH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);
/// How long presence waits for its relays before it goes on without them
/// (Constitution VII: a slow or dead relay never blocks the others). The
/// relays keep connecting, and reconnecting, in the background.
const RELAY_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
/// A request to be admitted that has waited this long gets a new time, so a
/// copy left running does not let its request go stale (R20: 30 days).
const REQUEST_RENEWAL_MS: u64 = 7 * 24 * 60 * 60 * 1000;
// ponytail: listens on the mailboxes of the 4 newest lower key generations (and of up to 4 higher
// ones skipped for a removed recipient). Ceiling: a device that missed more key changes than that is
// not heard and has to be dialed from a stored address or linked again. Upgrade path: raise it; each
// key adds two `p` values to the one subscription.
/// How many other content keys presence also listens with on either side of the current one.
const OTHER_MAILBOXES: usize = 4;

/// Runs presence for as long as this device's session lasts: connects to
/// `relay_urls`, subscribes to today's and yesterday's mailbox, publishes
/// this device's own reachability on the contract's cadence — and also
/// right away whenever `changed` fires, so a device list this session just
/// issued (linking, removing) does not wait out the rest of the 60 s tick
/// — and feeds every fresh, authenticated meeting it receives into `node`'s
/// address book and `device_presence_no_sync`, then wakes `wakeups.reconnect`
/// so the caller's reconnect loop dials it (dialing here would stall presence
/// for as long as an offline device's dial takes). Returns only if the relay
/// client itself ends; the caller races this against its own cancellation.
pub async fn run(
    node: &crate::sync::endpoint::SyncNode,
    replica: &crate::sync::replica::Replica,
    keys: &DeviceKeys,
    vault: [u8; 32],
    relay_urls: Vec<String>,
    mut changed: tokio::sync::watch::Receiver<u64>,
    wakeups: &crate::sync::resume::Wakeups,
) {
    use futures::StreamExt;

    crate::sync::presence::ensure_crypto_provider();
    let client = nostr_sdk::client::Client::new();
    for url in &relay_urls {
        if let Err(error) = client.add_relay(url.as_str()).await {
            log::warn!("sync: presence relay {url} is not a valid URL: {error}");
        }
    }
    // Unlike `try_connect_relay`, `connect` keeps a relay that is down right
    // now (e.g. the device opened the vault offline) reconnecting in the
    // background, and each reconnect renews the subscription (FR-010).
    client.connect().and_wait(RELAY_CONNECT_TIMEOUT).await;

    let mut notifications = client.notifications();
    let mut publish_tick = tokio::time::interval(PUBLISH_INTERVAL);
    // The relay usually connects after the first presence went out, which then names only direct
    // addresses: a device behind a NAT (a phone) is reachable only through its relay, so a new
    // relay is told at once instead of on the next tick (spec 043).
    let mut addr_changes = Box::pin(node.addr_changes());
    let mut announced_relay = node.addr().relay_urls().next().cloned();
    note_own_address(node, replica, keys);
    let mut subscribed: Option<Subscription> = None;
    let mut asking_since = now_ms();

    loop {
        let day = crate::sync::presence::day_tag_now();
        refresh_subscription(&client, replica, vault, keys, day, &mut subscribed).await;
        if now_ms().saturating_sub(asking_since) > REQUEST_RENEWAL_MS {
            asking_since = now_ms();
        }

        tokio::select! {
            _ = publish_tick.tick() => {
                if let Err(error) =
                    publish_own(&client, node, replica, keys, vault, day, asking_since).await
                {
                    log::warn!("sync: publishing presence failed: {error}");
                }
            }
            result = changed.changed() => {
                if result.is_err() {
                    return;
                }
                refresh_subscription(&client, replica, vault, keys, day, &mut subscribed).await;
                if let Err(error) =
                    publish_own(&client, node, replica, keys, vault, day, asking_since).await
                {
                    log::warn!("sync: publishing presence failed: {error}");
                }
            }
            // The app came back or the network changed (spec 043, research R5): the relay sockets
            // may be dead without having noticed. New connections, a new subscription, and this
            // device's presence at once, so the others dial it now.
            _ = wakeups.relays.notified() => {
                client.disconnect().await;
                client.connect().and_wait(RELAY_CONNECT_TIMEOUT).await;
                subscribed = None;
                refresh_subscription(&client, replica, vault, keys, day, &mut subscribed).await;
                if let Err(error) =
                    publish_own(&client, node, replica, keys, vault, day, asking_since).await
                {
                    log::warn!("sync: publishing presence failed: {error}");
                }
            }
            Some(addr) = addr_changes.next() => {
                let relay = addr.relay_urls().next().cloned();
                if relay != announced_relay {
                    announced_relay = relay;
                    note_own_address(node, replica, keys);
                    if let Err(error) =
                        publish_own(&client, node, replica, keys, vault, day, asking_since).await
                    {
                        log::warn!("sync: publishing presence failed: {error}");
                    }
                }
            }
            notification = notifications.next() => {
                let Some(notification) = notification else { break };
                if let nostr_sdk::client::ClientNotification::Event { event, .. } = notification {
                    if handle_incoming(node, replica, keys, vault, day, &event).await {
                        wakeups.reconnect.notify_one();
                    }
                }
            }
        }
    }
}

/// What this device listens to: the day and the content keys whose mailboxes it is subscribed to, the
/// current key first.
type Subscription = (u32, Vec<[u8; 32]>);

/// Brings the subscription in line with the day and the keys this device holds now.
async fn refresh_subscription(
    client: &nostr_sdk::client::Client,
    replica: &crate::sync::replica::Replica,
    vault: [u8; 32],
    keys: &DeviceKeys,
    day: u32,
    subscribed: &mut Option<Subscription>,
) {
    let wanted = match held_mailbox_keys(replica, vault, &keys.device_pubkey) {
        Ok(held) => held.map(|held| (day, held)),
        Err(error) => {
            log::warn!("sync: presence mailbox could not be read: {error}");
            None
        }
    };
    if *subscribed == wanted {
        return;
    }
    match resubscribe(client, replica, vault, keys, day).await {
        Ok(held) => *subscribed = held.map(|held| (day, held)),
        Err(error) => {
            log::warn!("sync: presence subscription failed, retrying next tick: {error}");
        }
    }
}

/// Subscribes to today's and yesterday's mailbox of every content key this device holds, replacing any
/// earlier subscription (contracts/nostr-events.md: no `since`, renewed on day rollover). Returns the
/// keys, the current one first.
///
/// Not only the current key: a device that was away while the key was replaced still sits at the old
/// mailbox and only learns the new key by syncing, which it can only do once someone has found it
/// there. Listening on the older mailboxes too lets the others find it; it is what they dial.
async fn resubscribe(
    client: &nostr_sdk::client::Client,
    replica: &crate::sync::replica::Replica,
    vault: [u8; 32],
    keys: &DeviceKeys,
    day: u32,
) -> Result<Option<Vec<[u8; 32]>>, PresenceError> {
    let Some(roster) = read_roster(replica, vault, &keys.device_pubkey)? else {
        let _ = client.unsubscribe_all().await;
        return Ok(None);
    };
    let content_keys = roster.listening_keys();
    let mut mailboxes = Vec::with_capacity(content_keys.len() * 2);
    for key in &content_keys {
        for day in [day, day.saturating_sub(1)] {
            mailboxes.push(mailbox_keys(key, day)?.1);
        }
    }
    let _ = client.unsubscribe_all().await;
    let filter = nostr::filter::Filter::new()
        .kind(crate::sync::presence::GIFT_WRAP_KIND)
        .pubkeys(mailboxes);
    client
        .subscribe(filter)
        .await
        .map_err(|e| PresenceError::Crypto(e.to_string()))?;
    Ok(Some(content_keys))
}

/// The content keys whose mailboxes this device listens to, the current one first. The current key can
/// change when a list removes a device that held the newest key, so list changes must also refresh
/// presence subscriptions, not only republish presence.
fn held_mailbox_keys(
    replica: &crate::sync::replica::Replica,
    vault: [u8; 32],
    own: &[u8; 32],
) -> Result<Option<Vec<[u8; 32]>>, PresenceError> {
    Ok(read_roster(replica, vault, own)?.map(|roster| roster.listening_keys()))
}

/// Publishes what this device has to say (see the module docs): presence,
/// or a request to be admitted, or nothing. `asking_since` is the time of
/// this device's request.
async fn publish_own(
    client: &nostr_sdk::client::Client,
    node: &crate::sync::endpoint::SyncNode,
    replica: &crate::sync::replica::Replica,
    keys: &DeviceKeys,
    vault: [u8; 32],
    day: u32,
    asking_since: u64,
) -> Result<(), PresenceError> {
    let Some(roster) = read_roster(replica, vault, &keys.device_pubkey)? else {
        return Ok(());
    };
    let (_, mb_pk) = mailbox_keys(&roster.content_key, day)?;
    let event = match roster.standing {
        Standing::Removed => return Ok(()),
        // FR-007: a one-device vault only listens.
        Standing::Listed if !roster.has_peers() => return Ok(()),
        Standing::Listed => {
            let addr = node.addr();
            let content = PresenceContent::own(
                keys.device_pubkey,
                keys.endpoint_id,
                addr.relay_urls().next().map(RelayUrl::to_string),
                addr.ip_addrs().copied().collect(),
                roster.list_generation,
            );
            build(keys, &content, &mb_pk)?
        }
        // A device that is being linked is named by a list within moments.
        Standing::Unlisted if roster.pending_link => return Ok(()),
        Standing::Unlisted => request_event(node, replica, keys, &mb_pk, asking_since)?,
    };
    client
        .send_event(&event)
        .await
        .map_err(|e| PresenceError::Crypto(e.to_string()))?;
    Ok(())
}

/// The gift-wrapped request of this device to be admitted (FR-044).
fn request_event(
    node: &crate::sync::endpoint::SyncNode,
    replica: &crate::sync::replica::Replica,
    keys: &DeviceKeys,
    mb_pk: &PublicKey,
    asking_since: u64,
) -> Result<Event, PresenceError> {
    let uuid = replica.db().device_id();
    let malformed = |e: String| PresenceError::Malformed(e);
    let name = crate::storage::query::read(replica.db(), |r| admission::own_name(r, uuid))
        .map_err(|e| malformed(e.to_string()))?;
    let request = admission::Request::sign(keys, uuid, name, asking_since)
        .map_err(|e| malformed(e.to_string()))?;
    let json = serde_json::to_string(&admission::Content::new(&request, &node.addr()))
        .map_err(|e| malformed(e.to_string()))?;
    wrap_payload(ADMISSION_KIND, &keys.device_secret, &json, mb_pk)
}

/// Opens and validates a received meeting, then acts on it (contracts/
/// nostr-events.md's receiver checks): fresh, signed by a device the
/// current effective list still names with that same endpoint, and (except
/// for a still-unlisted copy checking a newer list) not from a stranger.
/// Returns whether it recorded one, i.e. whether reconnect has something
/// new to dial.
pub(crate) async fn handle_incoming(
    node: &crate::sync::endpoint::SyncNode,
    replica: &crate::sync::replica::Replica,
    keys: &DeviceKeys,
    vault: [u8; 32],
    day: u32,
    event: &Event,
) -> bool {
    let roster = match read_roster(replica, vault, &keys.device_pubkey) {
        Ok(Some(roster)) => roster,
        Ok(None) => return false,
        Err(error) => {
            log::warn!("sync: presence could not load the current state: {error}");
            return false;
        }
    };
    // The current key's mailboxes first; another one is a device that was away while the key
    // changed, or has not heard of a removal yet.
    let opened = roster
        .listening_keys()
        .iter()
        .enumerate()
        .find_map(|(index, content_key)| {
            [day, day.saturating_sub(1)].into_iter().find_map(|tag| {
                let (mb_sk, _) = mailbox_keys(content_key, tag).ok()?;
                unwrap_rumor(event, &mb_sk)
                    .ok()
                    .map(|opened| (index > 0, opened))
            })
        });
    let Some((other, (sender, kind, payload))) = opened else {
        return false;
    };
    // A removed device still holds the other keys and could speak there under a fresh device key:
    // only a listed device's meeting of a device the list names is heard in another mailbox, never
    // a request to be admitted or a claim of a newer list.
    if other && (roster.standing != Standing::Listed || !roster.names(&sender)) {
        return false;
    }
    if kind == ADMISSION_KIND {
        return handle_request(node, replica, keys, &roster, sender, &payload).await;
    }
    if kind != PRESENCE_KIND {
        return false;
    }
    let Ok(content) = serde_json::from_str::<PresenceContent>(&payload) else {
        return false;
    };
    if content.device != sender || !content.is_fresh(now_ms()) {
        return false;
    }
    // The relay delivers this device's own meetings back to it, since it
    // subscribes to the same mailbox it publishes to. One from this key at
    // another endpoint is a copy of this installation (FR-030).
    if sender == keys.device_pubkey {
        if content.endpoint != keys.endpoint_id {
            node.flag(sender, crate::sync::problems::Problem::Duplicate)
                .await;
        }
        return false;
    }
    let Some(listed_endpoint) = roster
        .effective_devices
        .iter()
        .find_map(|(device, endpoint)| (*device == sender).then_some(*endpoint))
    else {
        return note_newer_list(node, &roster, sender, &content).await;
    };
    // A listed key speaking from another endpoint, or sharing its device id
    // with another key, means two installations act as one device (R14,
    // FR-030): sync with it stops until that ends.
    if content.endpoint != listed_endpoint || roster.conflicting.contains(&sender) {
        node.flag(sender, crate::sync::problems::Problem::Duplicate)
            .await;
        return false;
    }
    record_listed(node, replica, sender, content.endpoint_addr().ok()).await
}

/// A meeting of a device this device's list does not name. It leads to no
/// connection, except one to fetch a newer list: a copy of a main device
/// enrolled itself and names itself on a list of a higher generation
/// (FR-007). Only a listed device acts on it, and never for a device the
/// list removed. Returns whether reconnect has an address to try.
async fn note_newer_list(
    node: &crate::sync::endpoint::SyncNode,
    roster: &Roster,
    sender: [u8; 32],
    content: &PresenceContent,
) -> bool {
    if roster.standing != Standing::Listed
        || roster.settled.contains(&sender)
        || content.list_generation <= roster.list_generation
    {
        return false;
    }
    match content.endpoint_addr() {
        Ok(addr) => node.note_candidate(sender, addr),
        Err(_) => false,
    }
}

/// A request of a copy to be admitted (FR-045). A listed device keeps a
/// valid request from a device the list neither names nor removed, so it
/// reaches the main devices with the next sync. A request from a device the
/// list names by now (it was admitted and has not heard yet) is presence: it
/// says where to dial it. Returns whether reconnect has an address to try.
async fn handle_request(
    node: &crate::sync::endpoint::SyncNode,
    replica: &crate::sync::replica::Replica,
    keys: &DeviceKeys,
    roster: &Roster,
    sender: [u8; 32],
    payload: &str,
) -> bool {
    if sender == keys.device_pubkey || roster.standing != Standing::Listed {
        return false;
    }
    let Ok(content) = serde_json::from_str::<admission::Content>(payload) else {
        return false;
    };
    let Some(request) = content.request(&sender, now_ms()) else {
        return false;
    };
    if let Some((_, listed_endpoint)) = roster
        .effective_devices
        .iter()
        .find(|(device, _)| *device == sender)
    {
        if *listed_endpoint != request.endpoint || roster.conflicting.contains(&sender) {
            return false;
        }
        return record_listed(node, replica, sender, content.endpoint_addr()).await;
    }
    if roster.settled.contains(&sender) {
        return false;
    }
    let now = now_ms();
    let settled = roster.settled.clone();
    let kept = replica.db().write(|tx| {
        let changed = admission::store(tx, &request)?;
        if changed {
            admission::sweep(tx, now, &settled)?;
        }
        Ok(changed)
    });
    match kept {
        Ok(true) => {
            // This write does not go through `VaultDb`, so it never reaches
            // the gate's sync-notify signal: tell the sessions and the view.
            node.local_changed();
            node.announce_devices_changed();
        }
        Ok(false) => {}
        Err(error) => log::warn!("sync: keeping an admission request failed: {error}"),
    }
    false
}

/// Records that a listed device was heard of just now, at `addr`, so
/// reconnect can dial it. Returns whether it recorded.
async fn record_listed(
    node: &crate::sync::endpoint::SyncNode,
    replica: &crate::sync::replica::Replica,
    sender: [u8; 32],
    addr: Option<iroh::EndpointAddr>,
) -> bool {
    if let Some(addr) = &addr {
        node.note_presence(addr.clone()).await;
    }
    let write_result = replica
        .db()
        .write(|tx| home_relay::record_device(tx, &sender, now_ms(), addr.as_ref()));
    match write_result {
        // A renewed own card is a change the sessions have to carry on.
        Ok(card_renewed) => {
            if card_renewed {
                node.local_changed();
            }
        }
        Err(error) => {
            log::warn!("sync: recording presence for a device failed: {error}");
            return false;
        }
    }
    // This write does not go through `VaultDb`, so it never reaches the
    // gate's sync-notify signal (spec 024, FR-010): the caller nudges
    // reconnect directly instead of leaving it to its own periodic tick.
    true
}

/// Records this device's own address, so the own card names its home relay as it does for every
/// other device; the sessions are told when that renewed the card.
fn note_own_address(
    node: &crate::sync::endpoint::SyncNode,
    replica: &crate::sync::replica::Replica,
    keys: &DeviceKeys,
) {
    match home_relay::note_own(replica, &keys.device_pubkey, &node.addr()) {
        Ok(true) => node.local_changed(),
        Ok(false) => {}
        Err(error) => log::warn!("sync: recording this device's own address failed: {error}"),
    }
}

/// Where this device stands on the effective list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Standing {
    /// The list names it.
    Listed,
    /// Neither named nor removed: a copy of a linked device's vault file, or
    /// a device in the middle of a link.
    Unlisted,
    /// The list removed it (FR-027).
    Removed,
}

/// This device's current presence-relevant state: the content key it holds
/// (R9: skipping a generation wrapped for a removed device), the effective
/// list's generation, and the device keys that list currently names.
pub(crate) struct Roster {
    content_key: [u8; 32],
    /// Other keys this device holds (content_keys::listening_keys); presence only listens with them.
    other_keys: Vec<[u8; 32]>,
    list_generation: u64,
    /// Each device the list names, with the endpoint it names for it.
    effective_devices: Vec<([u8; 32], [u8; 32])>,
    /// Keys that share a device id with another key.
    conflicting: HashSet<[u8; 32]>,
    /// Devices the effective list names or removed.
    settled: HashSet<[u8; 32]>,
    standing: Standing,
    /// This device has a link record: it is being linked right now.
    pending_link: bool,
}

impl Roster {
    /// The keys whose mailboxes presence listens on: the current one first.
    fn listening_keys(&self) -> Vec<[u8; 32]> {
        std::iter::once(self.content_key)
            .chain(self.other_keys.iter().copied())
            .collect()
    }

    /// Whether the effective list names `device`.
    fn names(&self, device: &[u8; 32]) -> bool {
        self.effective_devices
            .iter()
            .any(|(listed, _)| listed == device)
    }

    /// Whether the effective list names any device besides this one.
    pub(crate) fn has_peers(&self) -> bool {
        self.effective_devices.len() > 1
    }
}

/// Reads the [`Roster`] from storage; `None` while this device holds no
/// content key or no valid device list yet.
pub(crate) fn read_roster(
    replica: &crate::sync::replica::Replica,
    vault: [u8; 32],
    own: &[u8; 32],
) -> Result<Option<Roster>, PresenceError> {
    crate::storage::query::read(replica.db(), |r| {
        let vault_pubkey = crate::sync::keys::vault_pubkey(r)?.unwrap_or(vault);
        let rows = crate::sync::device_list::load_all(r)?;
        let valid = crate::sync::device_list::valid_lists(&rows, &vault_pubkey);
        let Some(effective) = crate::sync::device_list::effective(&valid) else {
            return Ok(None);
        };
        let removed: Vec<_> = effective
            .list
            .removed
            .iter()
            .map(|removed| removed.device_pubkey)
            .collect();
        // Only the effective list's removals count (FR-043): a same-generation fork that lost
        // the tie-break may remove the remaining main device, whose keys would then all look
        // unsafe.
        let Some(key) =
            crate::sync::content_keys::current_key_for_list(r, &removed, Some(&effective.hash))?
        else {
            return Ok(None);
        };
        let standing = if effective.list.removes(own) {
            Standing::Removed
        } else if effective.list.device(own).is_some() {
            Standing::Listed
        } else {
            Standing::Unlisted
        };
        let other_keys = crate::sync::content_keys::listening_keys(r, &key, OTHER_MAILBOXES)?
            .into_iter()
            .map(|key| *key)
            .collect();
        Ok(Some(Roster {
            content_key: *key.key,
            other_keys,
            list_generation: effective.list.generation,
            effective_devices: effective
                .list
                .devices
                .iter()
                .map(|d| (d.device_pubkey, d.endpoint_id))
                .collect(),
            conflicting: crate::sync::device_list::uuid_conflicts(&valid),
            settled: admission::settled(effective),
            standing,
            pending_link: !crate::sync::link::pending::load_all(r)?.is_empty(),
        }))
    })
    .map_err(|e| PresenceError::Malformed(e.to_string()))
}

#[cfg(test)]
#[path = "presence_loop_relay_tests.rs"]
mod relay_tests;
#[cfg(test)]
#[path = "presence_loop_request_tests.rs"]
mod request_tests;
#[cfg(test)]
#[path = "presence_loop_tests.rs"]
mod tests;
