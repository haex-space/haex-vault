//! Receiving on device sessions (spec 095 T021, T022, contracts/nostr-chat-ratchet.md
//! "Receiving").
//!
//! A batch of wraps (one catch-up, or what waited in the live channel) is opened first without
//! writing anything: a wrap seen before, past its `expiration` or without one is left out, and the
//! rest is opened with the device key. The batch is then ordered per sender device (openings by
//! session id and counter, other messages by ratchet key and counter, read without decrypting) and
//! stored one wrap per write transaction in that order, so a session never skips more message keys
//! than the order of the Nostr-Relay forces. That transaction marks the wrap seen, moves the relay
//! cursor, decrypts, runs the spec 072 receiver checks and stores the rows: a crash leaves either
//! all of it or nothing.
//!
//! A chat event or receipt directly in a seal is the earlier format and is dropped (FR-016). A
//! session that this event opened is saved only when the event passed its checks; an event that
//! fails them leaves an existing session advanced (its message key is used) and a new one unsaved.
//! A keep-alive carries no card, so it counts only from a device on a stored card.

use std::collections::HashSet;
use std::sync::PoisonError;

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use nostr::event::{Event, Kind};
use vodozemac::olm::OlmMessage;

use super::Delivery;
use crate::chat::peer::envelope::{CHAT_KIND, RECEIPT_KIND};
use crate::chat::peer::ratchet::session::{self, Decrypt, Decrypted};
use crate::chat::peer::ratchet::wire::{Plaintext, RatchetRumor, KEEP_ALIVE};
use crate::chat::peer::ratchet::{RATCHET_KIND, RESET_KIND};
use crate::chat::peer::receive::{self, Stored};
use crate::chat::peer::session::Me;
use crate::chat::peer::targets;
use crate::chat::peer::verify::{self, Drop, Incoming};
use crate::chat::peer::wrap::{self, Opened};
use crate::error::Result;
use crate::storage::query::Query;
use crate::sync::keys::hex;

/// A wrap of a batch, opened without writing anything.
pub(crate) struct Prepared {
    wrap_id: String,
    /// Where it came from live; its cursor moves on.
    relay: Option<String>,
    created_at: i64,
    what: What,
    order: Order,
}

/// What an opened wrap holds.
enum What {
    /// Dropped before or while opening; the reason is logged.
    Dropped(&'static str),
    Ratchet {
        sender: [u8; 32],
        message: Box<OlmMessage>,
    },
    /// A request to open a new session; acted on with spec 095 T030.
    Reset,
}

/// Sender device, opening (0) or not (1), session id or ratchet key, counter, place in the batch.
/// Wraps that hold no ratchet message come first, in the order they came.
type Order = (u8, [u8; 32], u8, String, u64, usize);

impl Delivery {
    /// Stores a batch of received wraps (each with the relay it came from live, if any); answers
    /// the messages it stored. A wrap whose transaction failed stays unseen, so the next catch-up
    /// tries it again; the first such error is answered after the rest of the batch was stored.
    pub(super) async fn receive_batch(
        &self,
        events: Vec<(Event, Option<String>)>,
    ) -> Result<Vec<Stored>> {
        let ids: Vec<String> = events.iter().map(|(event, _)| event.id.to_hex()).collect();
        let mut skip = self
            .db
            .read(move |r| {
                let mut seen = HashSet::new();
                for id in ids {
                    let present: Option<i64> = r.query_row(
                        "SELECT 1 FROM chat_wraps_seen_no_sync WHERE wrap_id = ?1",
                        params![id],
                        |row| row.get(0),
                    )?;
                    if present.is_some() {
                        seen.insert(id);
                    }
                }
                Ok(seen)
            })
            .await?;
        let now = self.now();
        let mut batch = Vec::new();
        for (index, (event, relay)) in events.into_iter().enumerate() {
            // Seen before, or twice in this batch (from two Nostr-Relays).
            if skip.insert(event.id.to_hex()) {
                batch.push(prepare(
                    &event,
                    relay,
                    &self.me.keys.device_secret,
                    now,
                    index,
                ));
            }
        }
        batch.sort_by(|a, b| a.order.cmp(&b.order));
        let mut stored = Vec::new();
        let mut first_error = None;
        for prepared in batch {
            let (me, now) = (self.me.clone(), self.now());
            match self
                .db
                .write(move |tx| Ok(receive_one(tx, &me, prepared, now)?))
                .await
            {
                Ok(Some((one, event_id))) => {
                    if one.new && one.incoming {
                        self.receipts
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .entry(one.peer.clone())
                            .or_default()
                            .push(event_id);
                        // Nobody listening (tests, notifications not started) is fine.
                        let _ = self.arrived.send(one.clone());
                    }
                    stored.push(one);
                }
                Ok(None) => {}
                Err(error) => {
                    log::warn!("chat: a received wrap was not stored: {error}");
                    first_error.get_or_insert(error);
                }
            }
        }
        first_error.map_or(Ok(stored), Err)
    }
}

/// Opens `event` with `device_secret` at `now` (milliseconds), without writing anything.
pub(crate) fn prepare(
    event: &Event,
    relay: Option<String>,
    device_secret: &[u8; 32],
    now: i64,
    index: usize,
) -> Prepared {
    let what = open(event, device_secret, now);
    let order = match &what {
        What::Ratchet { sender, message } => match message.as_ref() {
            OlmMessage::PreKey(opening) => (
                1,
                *sender,
                0,
                opening.session_id(),
                opening.message().chain_index(),
                index,
            ),
            OlmMessage::Normal(message) => (
                1,
                *sender,
                1,
                message.ratchet_key().to_base64(),
                message.chain_index(),
                index,
            ),
        },
        What::Dropped(_) | What::Reset => (0, [0; 32], 0, String::new(), 0, index),
    };
    Prepared {
        wrap_id: event.id.to_hex(),
        relay,
        created_at: i64::try_from(event.created_at.as_secs()).unwrap_or(i64::MAX),
        what,
        order,
    }
}

fn open(event: &Event, device_secret: &[u8; 32], now: i64) -> What {
    let Some(expiration) = event.tags.expiration() else {
        return What::Dropped("noExpiration");
    };
    let expires_at = i64::try_from(expiration.as_secs())
        .unwrap_or(i64::MAX / 1000)
        .saturating_mul(1000);
    if expires_at < now {
        return What::Dropped("expired");
    }
    let Ok(opened) = wrap::unwrap(event, device_secret) else {
        return What::Dropped("undecryptable");
    };
    if opened.kind == RATCHET_KIND {
        return match RatchetRumor::decode(&opened.content).and_then(|rumor| rumor.message()) {
            Ok(message) => What::Ratchet {
                sender: opened.sender_device,
                message: Box::new(message),
            },
            Err(drop) => What::Dropped(drop.code()),
        };
    }
    if opened.kind == RESET_KIND {
        return What::Reset;
    }
    if opened.kind == CHAT_KIND || opened.kind == RECEIPT_KIND {
        return What::Dropped(Drop::EarlierFormat.code());
    }
    What::Dropped(Drop::UnknownKind.code())
}

/// Stores one prepared wrap in the caller's write transaction: marks it seen, moves the relay
/// cursor and, for a ratchet message, decrypts and stores the event; answers the message it stored
/// with its event id.
pub(crate) fn receive_one(
    tx: &mut CrdtTransaction<'_>,
    me: &Me,
    prepared: Prepared,
    now: i64,
) -> Result<Option<(Stored, String)>> {
    if !verify::first_sight(tx, &prepared.wrap_id, now)? {
        return Ok(None);
    }
    if let Some(relay) = &prepared.relay {
        tx.execute(
            "INSERT INTO chat_relay_cursors_no_sync (relay, newest_created_at) \
             VALUES (?1, ?2) ON CONFLICT (relay) DO UPDATE SET newest_created_at = \
             MAX(newest_created_at, excluded.newest_created_at)",
            params![relay, prepared.created_at],
        )?;
    }
    match prepared.what {
        What::Dropped(reason) => {
            log::info!("chat: dropped a wrap: {reason}");
            Ok(None)
        }
        What::Reset => {
            log::info!("chat: a reset is not acted on yet");
            Ok(None)
        }
        What::Ratchet { sender, message } => ratchet(tx, me, &sender, &message, now),
    }
}

fn ratchet(
    tx: &mut CrdtTransaction<'_>,
    me: &Me,
    sender: &[u8; 32],
    message: &OlmMessage,
    now: i64,
) -> Result<Option<(Stored, String)>> {
    let decrypted = match session::decrypt(tx, me, &hex(sender), message, now)? {
        Decrypt::Decrypted(decrypted) => *decrypted,
        Decrypt::Undecryptable => {
            log::info!("chat: a ratchet message did not decrypt");
            return Ok(None);
        }
    };
    let plaintext = match Plaintext::decode(decrypted.plaintext()) {
        Ok(plaintext) => plaintext,
        Err(drop) => return dropped(tx, me, decrypted, drop),
    };
    if plaintext.kind == KEEP_ALIVE {
        if targets::vault_of_device(tx, me, sender)?.is_none() {
            log::info!("chat: dropped a keep-alive: {}", Drop::UnknownDevice.code());
            return Ok(None);
        }
        decrypted.confirm(tx, me)?;
        return Ok(None);
    }
    let opened = Opened {
        sender_device: *sender,
        kind: Kind::from(plaintext.kind),
        content: plaintext.content,
    };
    let verified = match verify::check(tx, &me.vault, &opened)? {
        Ok(verified) => verified,
        Err(drop) => return dropped(tx, me, decrypted, drop),
    };
    let stored = match &verified.incoming {
        Incoming::Message(envelope) => {
            receive::message(tx, me.receiver(), &verified.sender_device, envelope, now)?
                .map(|stored| (stored, envelope.id.clone()))
        }
        Incoming::Receipt(receipt) => {
            receive::receipt(tx, &me.vault_hex, receipt, now)?;
            None
        }
    };
    decrypted.confirm(tx, me)?;
    Ok(stored)
}

/// A decrypted event failed a check: an existing session stays advanced, a new one is not saved.
fn dropped(
    tx: &mut CrdtTransaction<'_>,
    me: &Me,
    decrypted: Decrypted,
    drop: Drop,
) -> Result<Option<(Stored, String)>> {
    log::info!("chat: dropped a rumor: {}", drop.code());
    if !decrypted.opened() {
        decrypted.confirm(tx, me)?;
    }
    Ok(None)
}
