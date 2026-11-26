//! Queuing what this device sends (spec 072 US2, research R4, R6): a text message becomes its row,
//! a delivery row per recipient and an outbox row for the recipient and the own other devices (a
//! note: no delivery row and an outbox row for the own other devices only);
//! receipts become outbox rows to the original sender. The delivery service publishes them.

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use uuid::Uuid;

use super::envelope::{self, Answer, Attachment, Receipt, To, CHAT_KIND, RECEIPT_KIND, VERSION};
use super::outbox::{self, Rumor};
use super::ratchet::wire::Plaintext;
use super::receive::{self, MessageRow};
use super::session::Me;
use super::store::{self, ConversationKind};
use super::{ids, settings, targets};
use crate::contacts::card::{own_card, received};
use crate::error::{HaexError, Result};
use crate::storage::query::Query;

/// The own card as `haex:card:` text: every rumor carries it (research R5).
pub fn own_card_text(tx: &mut impl Query) -> Result<String> {
    own_card::stored(tx)?
        .map(|card| card.text())
        .ok_or_else(|| HaexError::NotFound {
            name: "own business card".into(),
        })
}

/// The JSON of an event or receipt of `kind`, refused with "chat.error.tooLarge" when the plaintext
/// of the ratchet message that carries it would be over its limit (spec 095). An opening message
/// with the largest plaintext still wraps within the size a Nostr-Relay accepts, so only the
/// plaintext counts; the event's quotes are escaped once more inside it.
fn queued_content<T: serde::Serialize>(kind: nostr::event::Kind, event: &T) -> Result<String> {
    let content = envelope::encode(event)?;
    Plaintext {
        kind: kind.as_u16(),
        content: content.clone(),
    }
    .encode()?;
    Ok(content)
}

/// Whether `text` is sent to `thread_id` as a message of its own (research R2): when its envelope
/// with the own card fits one wrap. A text that does not fit goes as a file with a preview instead
/// (`files::transfers::ChatFiles::send_long_text`). A note that reaches no other device (see
/// [`note_is_wrapped`]) has no wrap, so it always fits.
pub fn fits_inline(
    q: &mut impl Query,
    me: &Me,
    thread_id: Uuid,
    text: &str,
    now: i64,
) -> Result<bool> {
    let to = match target(q, thread_id)? {
        Target::Direct(peer) => To::Direct(peer),
        Target::Note if note_is_wrapped(q, me)? => To::Note(me.vault_hex.clone()),
        Target::Note => return Ok(true),
    };
    let card = own_card_text(q)?;
    let envelope = envelope::message(text, Vec::new(), &me.vault_hex, &card, to, now)?;
    match queued_content(CHAT_KIND, &envelope) {
        Ok(_) => Ok(true),
        Err(HaexError::InvalidInput { reason }) if reason == "chat.error.tooLarge" => Ok(false),
        Err(error) => Err(error),
    }
}

/// Where a message of a conversation goes.
enum Target {
    /// To the other vault of an active direct conversation (hex).
    Direct(String),
    /// The own other devices (FR-002b): a note goes as a wrap like a message, and also reaches them
    /// through the sync of the vault.
    Note,
}

fn target(q: &mut impl Query, thread_id: Uuid) -> Result<Target> {
    match store::kind_of(q, thread_id)? {
        Some(ConversationKind::Note) => return Ok(Target::Note),
        Some(ConversationKind::Direct) => {}
        _ => {
            return Err(HaexError::NotFound {
                name: "direct conversation".into(),
            })
        }
    }
    let row: Option<(Option<String>, String)> = q.query_row(
        "SELECT peer_vault, state FROM chat_threads WHERE id = ?1",
        params![thread_id.to_string()],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    match row {
        Some((Some(peer), state)) if state == "active" => Ok(Target::Direct(peer)),
        _ => Err(HaexError::InvalidInput {
            reason: "the conversation takes no messages".into(),
        }),
    }
}

/// The vault that takes the messages of `thread_id`: the other vault of an active direct
/// conversation, the own vault for the notes. Files are checked against it before they are uploaded.
pub fn recipient(q: &mut impl Query, me: &Me, thread_id: Uuid) -> Result<String> {
    Ok(match target(q, thread_id)? {
        Target::Direct(peer) => peer,
        Target::Note => me.vault_hex.clone(),
    })
}

/// Whether a note is published at all: there is another own device and an own Nostr-Relay to reach
/// it. Without them the note stays in the vault, and the sync brings it to a device that joins
/// later (FR-002b).
fn note_is_wrapped(q: &mut impl Query, me: &Me) -> Result<bool> {
    let own = targets::of(q, me, &me.vault_hex)?;
    Ok(!own.devices.is_empty() && !own.relays.is_empty())
}

/// Whether the own message with `event_id` in `thread_id` was queued already (a retry).
pub fn sent_already(
    q: &mut impl Query,
    me: &Me,
    thread_id: Uuid,
    event_id: &str,
) -> Result<Option<Uuid>> {
    let message_id = ids::message_id(thread_id, &me.vault_hex, event_id);
    let present: Option<i64> = q.query_row(
        "SELECT 1 FROM chat_peer_messages WHERE id = ?1",
        params![message_id.to_string()],
        |row| row.get(0),
    )?;
    Ok(present.map(|_| message_id))
}

/// Queues `text` and the uploaded `attachments` to the direct conversation `thread_id`, or stores a
/// text as a note; answers the message id. A retry with the same `event_id` (from the command's
/// idempotency key) answers the first message.
pub fn message(
    tx: &mut CrdtTransaction<'_>,
    me: &Me,
    thread_id: Uuid,
    event_id: &str,
    text: &str,
    attachments: Vec<Attachment>,
    now: i64,
) -> Result<Uuid> {
    let target = target(tx, thread_id)?;
    if let Some(message_id) = sent_already(tx, me, thread_id, event_id)? {
        return Ok(message_id);
    }
    let message_id = ids::message_id(thread_id, &me.vault_hex, event_id);
    let (to, peer) = match target {
        Target::Direct(peer) => (To::Direct(peer.clone()), peer),
        Target::Note => (To::Note(me.vault_hex.clone()), me.vault_hex.clone()),
    };
    let note = matches!(to, To::Note(_));
    if note && !note_is_wrapped(tx, me)? {
        let text = envelope::checked_body(text, attachments.len())?;
        store_own(tx, me, message_id, thread_id, event_id, text, now)?;
        receive::insert_attachments(tx, message_id, &attachments)?;
        return Ok(message_id);
    }
    let card = own_card_text(tx)?;
    let mut envelope = envelope::message(text, attachments, &me.vault_hex, &card, to, now)?;
    envelope.id = event_id.to_owned();
    let content = queued_content(CHAT_KIND, &envelope)?;
    let envelope::Body::Message(body) = &envelope.body;
    store_own(tx, me, message_id, thread_id, event_id, &body.text, now)?;
    receive::insert_attachments(tx, message_id, &body.attachments)?;
    // A note has no delivery state, no receipt to wait for and no other recipient.
    let (pending, awaiting) = if note {
        (vec![me.vault_hex.clone()], Vec::new())
    } else {
        receive::ensure_delivery(tx, message_id, &peer)?;
        (vec![peer.clone(), me.vault_hex.clone()], vec![peer.clone()])
    };
    outbox::enqueue(
        tx,
        event_id,
        &thread_id.to_string(),
        Some(&message_id.to_string()),
        &Rumor {
            kind: CHAT_KIND.as_u16(),
            content,
        },
        &pending,
        &awaiting,
        now,
    )?;
    // The other vault answered that it takes no messages from this one (spec 099 FR-010): the
    // message is kept and waits, parked, for its answer "accepted".
    if super::gate::blocked_by_peer(tx, &me.vault_hex, &peer)? {
        outbox::park_one(tx, event_id, &message_id.to_string(), &peer, now)?;
    }
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
    enqueue_receipt(tx, me, peer, delivered, read, None, now)
}

/// Queues the answer of this vault to `peer` (spec 099 FR-007, FR-009, FR-014, FR-016): a receipt
/// with `answer` and no events. A blocked answer is the one rumor that goes to a blocked vault
/// (research R5, [`super::targets::of_entry`]).
pub fn notice(
    tx: &mut CrdtTransaction<'_>,
    me: &Me,
    peer: &str,
    answer: Answer,
    sequence: i64,
    decision_hlc: &str,
    now: i64,
) -> Result<()> {
    enqueue_receipt(
        tx,
        me,
        peer,
        Vec::new(),
        Vec::new(),
        Some((answer, sequence, decision_hlc.to_owned())),
        now,
    )
}

fn enqueue_receipt(
    tx: &mut CrdtTransaction<'_>,
    me: &Me,
    peer: &str,
    delivered: Vec<String>,
    read: Vec<String>,
    answer: Option<(Answer, i64, String)>,
    now: i64,
) -> Result<()> {
    let (answer, answer_sequence, answer_decision_hlc) = answer
        .map(|(answer, sequence, decision_hlc)| (Some(answer), Some(sequence), Some(decision_hlc)))
        .unwrap_or((None, None, None));
    let rumor = Receipt {
        v: VERSION,
        from: me.vault_hex.clone(),
        card: own_card_text(tx)?,
        thread: To::Direct(peer.to_owned()),
        delivered,
        read,
        answer,
        answer_sequence,
        answer_decision_hlc,
    };
    let thread_id = ids::direct_thread_id(&me.vault_hex, peer);
    outbox::enqueue(
        tx,
        &envelope::new_event_id(),
        &thread_id.to_string(),
        None,
        &Rumor {
            kind: RECEIPT_KIND.as_u16(),
            content: queued_content(RECEIPT_KIND, &rumor)?,
        },
        &[peer.to_owned()],
        &[],
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
        return Err(HaexError::NotFound {
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
