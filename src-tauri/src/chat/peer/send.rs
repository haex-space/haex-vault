//! Queuing what this device sends (spec 072 US2, research R4, R6): a text message becomes its row,
//! a delivery row per recipient and an outbox row for the recipient and the own other devices;
//! receipts become outbox rows to the original sender. The delivery service publishes them.

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use uuid::Uuid;

use super::envelope::{self, Receipt, To, CHAT_KIND, RECEIPT_KIND, VERSION};
use super::outbox::{self, Rumor};
use super::receive::{self, MessageRow};
use super::session::Me;
use super::store::{self, ConversationKind};
use super::{ids, settings};
use crate::contacts::card::{own_card, received};
use crate::error::{HolziError, Result};

/// The own card as `holzi:card:` text: every rumor carries it (research R5).
pub fn own_card_text(tx: &mut CrdtTransaction<'_>) -> Result<String> {
    own_card::stored(tx)?
        .map(|card| card.text())
        .ok_or_else(|| HolziError::NotFound {
            name: "own business card".into(),
        })
}

/// Where a message of a conversation goes.
enum Target {
    /// To the other vault of an active direct conversation (hex).
    Direct(String),
    /// Nowhere: a note stays in the vault and reaches the own devices through the sync (FR-002b).
    Note,
}

fn target(tx: &mut CrdtTransaction<'_>, thread_id: Uuid) -> Result<Target> {
    match store::kind_of(tx, thread_id)? {
        Some(ConversationKind::Note) => return Ok(Target::Note),
        Some(ConversationKind::Direct) => {}
        _ => {
            return Err(HolziError::NotFound {
                name: "direct conversation".into(),
            })
        }
    }
    let row: Option<(Option<String>, String)> = tx.query_row(
        "SELECT peer_vault, state FROM chat_threads WHERE id = ?1",
        params![thread_id.to_string()],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    match row {
        Some((Some(peer), state)) if state == "active" => Ok(Target::Direct(peer)),
        _ => Err(HolziError::InvalidInput {
            reason: "the conversation takes no messages".into(),
        }),
    }
}

/// Queues `text` to the direct conversation `thread_id`, or stores it as a note; answers the message
/// id. A retry with the same `event_id` (from the command's idempotency key) answers the first
/// message.
pub fn message(
    tx: &mut CrdtTransaction<'_>,
    me: &Me,
    thread_id: Uuid,
    event_id: &str,
    text: &str,
    now: i64,
) -> Result<Uuid> {
    let target = target(tx, thread_id)?;
    let message_id = ids::message_id(thread_id, &me.vault_hex, event_id);
    let present: Option<i64> = tx.query_row(
        "SELECT 1 FROM chat_peer_messages WHERE id = ?1",
        params![message_id.to_string()],
        |row| row.get(0),
    )?;
    if present.is_some() {
        return Ok(message_id);
    }
    let Target::Direct(peer) = target else {
        let text = envelope::checked_text(text)?;
        store_own(tx, me, message_id, thread_id, event_id, text, now)?;
        return Ok(message_id);
    };
    let card = own_card_text(tx)?;
    let mut envelope =
        envelope::message(text, &me.vault_hex, &card, To::Direct(peer.clone()), now)?;
    envelope.id = event_id.to_owned();
    let content = envelope::encode(&envelope)?;
    // A trial wrap to this device: what the Nostr-Relays will see must fit (research R2).
    super::wrap::wrap(
        CHAT_KIND,
        &content,
        &me.keys.device_secret,
        &me.keys.device_pubkey,
    )?;
    let envelope::Body::Message(body) = &envelope.body;
    store_own(tx, me, message_id, thread_id, event_id, &body.text, now)?;
    receive::ensure_delivery(tx, message_id, &peer)?;
    outbox::enqueue(
        tx,
        event_id,
        &thread_id.to_string(),
        Some(&message_id.to_string()),
        &Rumor {
            kind: CHAT_KIND.as_u16(),
            content,
        },
        &[peer, me.vault_hex.clone()],
        now,
    )?;
    Ok(message_id)
}

/// The own message row, and the conversation's activity time.
fn store_own(
    tx: &mut CrdtTransaction<'_>,
    me: &Me,
    id: Uuid,
    thread_id: Uuid,
    event_id: &str,
    text: &str,
    now: i64,
) -> Result<()> {
    receive::insert_message(
        tx,
        &MessageRow {
            id,
            thread_id,
            event_id,
            sender_vault: &me.vault_hex,
            sender_device: &me.device_hex(),
            text,
            sent_at: now,
            received_at: now,
            inserted_by: &me.device_uuid,
        },
    )?;
    tx.execute(
        "UPDATE chat_threads SET updated_at = ?2 WHERE id = ?1",
        params![thread_id.to_string(), now],
    )?;
    Ok(())
}

/// Queues a receipt to `peer` for its events `delivered` and `read`; nothing when both are empty or
/// `peer` is blocked (FR-042).
pub fn receipt(
    tx: &mut CrdtTransaction<'_>,
    me: &Me,
    peer: &str,
    delivered: Vec<String>,
    read: Vec<String>,
    now: i64,
) -> Result<()> {
    if delivered.is_empty() && read.is_empty() {
        return Ok(());
    }
    if let Some(bytes) = super::verify::vault_bytes(peer) {
        if received::is_blocked(tx, &bytes)? {
            return Ok(());
        }
    }
    let rumor = Receipt {
        v: VERSION,
        from: me.vault_hex.clone(),
        card: own_card_text(tx)?,
        thread: To::Direct(peer.to_owned()),
        delivered,
        read,
    };
    let thread_id = ids::direct_thread_id(&me.vault_hex, peer);
    outbox::enqueue(
        tx,
        &envelope::new_event_id(),
        &thread_id.to_string(),
        None,
        &Rumor {
            kind: RECEIPT_KIND.as_u16(),
            content: envelope::encode(&rumor)?,
        },
        &[peer.to_owned()],
        now,
    )
}

/// Marks the conversation read up to `upto` (FR-022) and, with read receipts on, queues one for the
/// other vault's messages that became read (FR-024). Answers whether `last_read_at` moved.
pub fn read_up_to(
    tx: &mut CrdtTransaction<'_>,
    me: &Me,
    thread_id: Uuid,
    upto: i64,
    now: i64,
) -> Result<bool> {
    let row: Option<(Option<i64>, String, String, Option<String>)> = tx.query_row(
        "SELECT last_read_at, kind, state, peer_vault FROM chat_threads WHERE id = ?1",
        params![thread_id.to_string()],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    let Some((last_read, kind, state, peer)) = row else {
        return Err(HolziError::NotFound {
            name: "conversation".into(),
        });
    };
    let last_read = last_read.unwrap_or(0);
    if upto <= last_read {
        return Ok(false);
    }
    tx.execute(
        "UPDATE chat_threads SET last_read_at = ?2 WHERE id = ?1",
        params![thread_id.to_string(), upto],
    )?;
    let Some(peer) = peer.filter(|_| kind == "direct" && state == "active") else {
        return Ok(true);
    };
    if !settings::read_receipts(tx)? {
        return Ok(true);
    }
    let read = tx.query_map(
        "SELECT event_id FROM chat_peer_messages WHERE thread_id = ?1 AND sender_vault = ?2 \
           AND sort_at > ?3 AND sort_at <= ?4 ORDER BY sort_at, id",
        params![thread_id.to_string(), peer, last_read, upto],
        |row| row.get::<_, String>(0),
    )?;
    receipt(tx, me, &peer, Vec::new(), read, now)?;
    Ok(true)
}

#[cfg(test)]
#[path = "send_tests.rs"]
mod tests;
