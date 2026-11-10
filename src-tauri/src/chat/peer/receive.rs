//! A verified rumor becomes rows (spec 072 research R4, R6): a message lands on the row its derived
//! id names, whichever way it arrived first (a wrap to this device, one to another own device, the
//! own-device sync), and a receipt sets the delivery states of the own messages once.

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use uuid::Uuid;

use super::envelope::{Attachment, Body, Envelope, Receipt, To};
use super::verify::vault_bytes;
use super::{ids, store};
use crate::contacts::card::{format, received};
use crate::error::Result;
use crate::sync::presence::decode_hex;

/// How far a sender's clock may run ahead before the arrival time orders the message (R4).
pub const CLOCK_SLACK_MS: i64 = 5 * 60 * 1000;

/// A message this device stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    pub thread_id: Uuid,
    pub message_id: Uuid,
    /// The other vault of the direct conversation, hex.
    pub peer: String,
    /// `false` when the row was there already.
    pub new: bool,
    /// The message came from the other vault (not from an own other device).
    pub incoming: bool,
    /// The message has attachments for this device to download.
    pub attachments: bool,
}

/// Who receives: the own vault (hex) and this device's `vault_device_uuid`.
#[derive(Debug, Clone, Copy)]
pub struct Receiver<'a> {
    pub vault: &'a str,
    pub device_uuid: &'a str,
}

/// Stores a verified message of `sender_device` (hex) in its direct conversation, opened when
/// there is none yet. An own message from another own device also gets its delivery row, so the
/// receipts that reach this device find it. `None` for an envelope without a direct `to`.
pub fn message(
    tx: &mut CrdtTransaction<'_>,
    me: Receiver<'_>,
    sender_device: &str,
    envelope: &Envelope,
    now: i64,
) -> Result<Option<Stored>> {
    let To::Direct(to) = &envelope.to else {
        return Ok(None);
    };
    let incoming = envelope.from != me.vault;
    let peer = if incoming { &envelope.from } else { to };
    let thread_id = store::open_direct(tx, me.vault, peer, now)?;
    if incoming {
        sort_request(tx, thread_id, envelope)?;
    }
    let message_id = ids::message_id(thread_id, &envelope.from, &envelope.id);
    let mut stored = Stored {
        thread_id,
        message_id,
        peer: peer.clone(),
        new: false,
        incoming,
        attachments: false,
    };
    let present: Option<i64> = tx.query_row(
        "SELECT 1 FROM chat_peer_messages WHERE id = ?1",
        params![message_id.to_string()],
        |row| row.get(0),
    )?;
    if present.is_some() {
        return Ok(Some(stored));
    }
    let Body::Message(body) = &envelope.body;
    stored.attachments = !body.attachments.is_empty();
    insert_message(
        tx,
        &MessageRow {
            id: message_id,
            thread_id,
            event_id: &envelope.id,
            sender_vault: &envelope.from,
            sender_device,
            text: &body.text,
            sent_at: envelope.at,
            received_at: now,
            inserted_by: me.device_uuid,
        },
    )?;
    insert_attachments(tx, message_id, &body.attachments)?;
    if !incoming {
        ensure_delivery(tx, message_id, peer)?;
    }
    stored.new = true;
    Ok(Some(stored))
}

/// A message of a vault without a stored card makes its conversation a request with the carried
/// card (US5); one of a confirmed contact ends a request that is still open on this device, for the
/// case its card arrived through the sync before the conversation did.
fn sort_request(tx: &mut CrdtTransaction<'_>, thread_id: Uuid, envelope: &Envelope) -> Result<()> {
    let Some(from) = vault_bytes(&envelope.from) else {
        return Ok(());
    };
    if received::stored(tx, &from)?.is_some() {
        store::accept_requests_of(tx, &envelope.from)?;
        return Ok(());
    }
    // The receiver checks parsed this card already; one that does not parse keeps no request card.
    if let Ok(card) = format::parse(&envelope.card) {
        store::mark_request(tx, thread_id, &envelope.from, &card)?;
    }
    Ok(())
}

/// The columns of a new message row.
pub struct MessageRow<'a> {
    pub id: Uuid,
    pub thread_id: Uuid,
    pub event_id: &'a str,
    pub sender_vault: &'a str,
    pub sender_device: &'a str,
    pub text: &'a str,
    pub sent_at: i64,
    pub received_at: i64,
    pub inserted_by: &'a str,
}

/// Inserts a text message; `sort_at` follows research R4.
pub fn insert_message(tx: &mut CrdtTransaction<'_>, row: &MessageRow<'_>) -> Result<()> {
    let sort_at = row.sent_at.min(row.received_at + CLOCK_SLACK_MS);
    tx.execute(
        "INSERT INTO chat_peer_messages (id, thread_id, event_id, sender_vault, sender_device, \
           kind, text, sent_at, received_at, sort_at, inserted_by) \
         VALUES (?1, ?2, ?3, ?4, ?5, 'text', ?6, ?7, ?8, ?9, ?10)",
        params![
            row.id.to_string(),
            row.thread_id.to_string(),
            row.event_id,
            row.sender_vault,
            row.sender_device,
            row.text,
            row.sent_at,
            row.received_at,
            sort_at,
            row.inserted_by
        ],
    )?;
    Ok(())
}

/// The rows of a message's attachments; their ids follow from the message id and the index, so
/// every device writes the same rows (research R4).
pub fn insert_attachments(
    tx: &mut CrdtTransaction<'_>,
    message_id: Uuid,
    attachments: &[Attachment],
) -> Result<()> {
    for (index, attachment) in attachments.iter().enumerate() {
        let Attachment::File(file) = attachment;
        let key = decode_hex(&file.blossom.key)
            .ok()
            .filter(|key| key.len() == 32);
        tx.execute(
            "INSERT INTO chat_attachments (id, message_id, kind, name, mime, size, sha256, \
               blossom_url, blob_sha256, blob_key) \
             VALUES (?1, ?2, 'file', ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                ids::attachment_id(message_id, index).to_string(),
                message_id.to_string(),
                file.name,
                file.mime,
                i64::try_from(file.size).unwrap_or(i64::MAX),
                file.sha256.to_ascii_lowercase(),
                file.blossom.url,
                file.blossom.sha256.to_ascii_lowercase(),
                key
            ],
        )?;
    }
    Ok(())
}

/// The delivery row of an own message to `recipient`, created when missing.
pub fn ensure_delivery(
    tx: &mut CrdtTransaction<'_>,
    message_id: Uuid,
    recipient: &str,
) -> Result<()> {
    let id = ids::delivery_id(message_id, recipient).to_string();
    let present: Option<i64> = tx.query_row(
        "SELECT 1 FROM chat_deliveries WHERE id = ?1",
        params![id],
        |row| row.get(0),
    )?;
    if present.is_none() {
        tx.execute(
            "INSERT INTO chat_deliveries (id, message_id, recipient_vault) VALUES (?1, ?2, ?3)",
            params![id, message_id.to_string(), recipient],
        )?;
    }
    Ok(())
}

/// Applies a verified receipt of `receipt.from` to the own messages it names: `delivered_at`, and
/// for read ones also `read_at`, each set once. Answers how many states changed.
pub fn receipt(
    tx: &mut CrdtTransaction<'_>,
    own: &str,
    receipt: &Receipt,
    now: i64,
) -> Result<usize> {
    let To::Direct(_) = receipt.thread else {
        return Ok(0);
    };
    let thread_id = ids::direct_thread_id(own, &receipt.from);
    let mut changed = 0;
    let read = receipt.read.iter().map(|id| (id, true));
    for (event_id, is_read) in receipt.delivered.iter().map(|id| (id, false)).chain(read) {
        let message_id = ids::message_id(thread_id, own, event_id);
        let delivery = ids::delivery_id(message_id, &receipt.from).to_string();
        let columns: &[&str] = if is_read {
            &["delivered_at", "read_at"]
        } else {
            &["delivered_at"]
        };
        for column in columns {
            changed += tx.execute(
                &format!(
                    "UPDATE chat_deliveries SET {column} = ?2 WHERE id = ?1 AND {column} IS NULL"
                ),
                params![delivery, now],
            )?;
        }
    }
    Ok(changed)
}

#[cfg(test)]
#[path = "receive_tests.rs"]
mod tests;
