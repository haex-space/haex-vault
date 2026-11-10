//! The receiver checks of a chat rumor (spec 072 research R5, contracts/nostr-chat.md "Receiver
//! checks"), in their order: the carried card is valid and signed by `from`, the sealing device is
//! on it, the card is not older than the stored one (a newer one replaces it, FR-020), the sender is
//! not blocked, and the event is for this vault. Whether the wrap was seen already is checked before
//! opening it ([`first_sight`]).
//!
//! Any failure drops the rumor without a receipt; the reason is logged without content. Rumors from
//! the own vault come from the own other devices: their sealing device must be on the own current
//! device list. A message of a vault that is no contact (no stored card) passes and becomes a
//! request (US5); a receipt of such a vault is dropped.

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;

use super::envelope::{Envelope, Receipt, To, CHAT_KIND, MAX_ATTACHMENTS, RECEIPT_KIND, VERSION};
use super::wrap::Opened;
use crate::contacts::card::format::{self, SignedCard};
use crate::contacts::card::{own_card, received};
use crate::error::Result;
use crate::sync::keys::hex;
use crate::sync::presence::decode_hex;

/// Seen wraps are kept this long: after it a Nostr-Relay drops the wrap (`expiration`).
pub const SEEN_KEEP_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// Why a rumor was dropped; logged as its code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drop {
    Malformed,
    UnknownKind,
    UnknownVersion,
    BadCard,
    WrongSender,
    UnknownDevice,
    OlderCard,
    Blocked,
    UnknownSender,
    NotForUs,
    TooManyAttachments,
}

impl Drop {
    pub fn code(self) -> &'static str {
        match self {
            Self::Malformed => "malformed",
            Self::UnknownKind => "unknownKind",
            Self::UnknownVersion => "unknownVersion",
            Self::BadCard => "badCard",
            Self::WrongSender => "wrongSender",
            Self::UnknownDevice => "unknownDevice",
            Self::OlderCard => "olderCard",
            Self::Blocked => "blocked",
            Self::UnknownSender => "unknownSender",
            Self::NotForUs => "notForUs",
            Self::TooManyAttachments => "tooManyAttachments",
        }
    }
}

/// What a verified rumor is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incoming {
    Message(Envelope),
    Receipt(Receipt),
}

/// A rumor that passed every check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verified {
    /// The sealing device key, hex.
    pub sender_device: String,
    /// The sender's vault, hex.
    pub from: String,
    pub incoming: Incoming,
}

/// Records the wrap `wrap_id` as seen; `false` when it was seen before (then it is not opened again).
pub fn first_sight(tx: &mut CrdtTransaction<'_>, wrap_id: &str, now: i64) -> Result<bool> {
    let present: Option<i64> = tx.query_row(
        "SELECT 1 FROM chat_wraps_seen_no_sync WHERE wrap_id = ?1",
        params![wrap_id],
        |row| row.get(0),
    )?;
    if present.is_some() {
        return Ok(false);
    }
    tx.execute(
        "INSERT INTO chat_wraps_seen_no_sync (wrap_id, seen_at) VALUES (?1, ?2)",
        params![wrap_id, now],
    )?;
    Ok(true)
}

/// Forgets seen wraps older than [`SEEN_KEEP_MS`].
pub fn prune_seen(tx: &mut CrdtTransaction<'_>, now: i64) -> Result<()> {
    tx.execute(
        "DELETE FROM chat_wraps_seen_no_sync WHERE seen_at < ?1",
        params![now - SEEN_KEEP_MS],
    )?;
    Ok(())
}

/// Runs the receiver checks on an opened wrap for the vault `own`.
pub fn check(
    tx: &mut CrdtTransaction<'_>,
    own: &[u8; 32],
    opened: &Opened,
) -> Result<std::result::Result<Verified, Drop>> {
    let (from, card_text, incoming) = match decode(opened) {
        Ok(decoded) => decoded,
        Err(drop) => return Ok(Err(drop)),
    };
    let Ok(card) = format::parse(&card_text) else {
        return Ok(Err(Drop::BadCard));
    };
    let Some(from_bytes) = vault_bytes(&from) else {
        return Ok(Err(Drop::Malformed));
    };
    if card.card.vault != from_bytes {
        return Ok(Err(Drop::WrongSender));
    }
    let own_hex = hex(own);
    let verdict = if from_bytes == *own {
        check_own(tx, opened, &incoming)?
    } else {
        check_peer(tx, &own_hex, &from_bytes, &card, opened, &incoming)?
    };
    Ok(verdict.map(|()| Verified {
        sender_device: hex(&opened.sender_device),
        from,
        incoming,
    }))
}

fn decode(opened: &Opened) -> std::result::Result<(String, String, Incoming), Drop> {
    let version = |json: &serde_json::Value| json.get("v").and_then(serde_json::Value::as_u64);
    let json: serde_json::Value =
        serde_json::from_str(&opened.content).map_err(|_| Drop::Malformed)?;
    if version(&json) != Some(u64::from(VERSION)) {
        return Err(Drop::UnknownVersion);
    }
    if opened.kind == CHAT_KIND {
        let envelope: Envelope = serde_json::from_value(json).map_err(|_| Drop::Malformed)?;
        if matches!(&envelope.body, super::envelope::Body::Message(body) if body.attachments.len() > MAX_ATTACHMENTS)
        {
            return Err(Drop::TooManyAttachments);
        }
        Ok((
            envelope.from.clone(),
            envelope.card.clone(),
            Incoming::Message(envelope),
        ))
    } else if opened.kind == RECEIPT_KIND {
        let receipt: Receipt = serde_json::from_value(json).map_err(|_| Drop::Malformed)?;
        Ok((
            receipt.from.clone(),
            receipt.card.clone(),
            Incoming::Receipt(receipt),
        ))
    } else {
        Err(Drop::UnknownKind)
    }
}

/// A rumor of the own vault: from a device on the own current list, about a direct conversation
/// with another vault. Receipts never come from the own vault.
fn check_own(
    tx: &mut CrdtTransaction<'_>,
    opened: &Opened,
    incoming: &Incoming,
) -> Result<std::result::Result<(), Drop>> {
    let Some(addresses) = own_card::addresses(tx)? else {
        return Ok(Err(Drop::UnknownDevice));
    };
    if !addresses
        .devices
        .iter()
        .any(|device| device.device_pubkey == opened.sender_device)
    {
        return Ok(Err(Drop::UnknownDevice));
    }
    Ok(match incoming {
        Incoming::Message(Envelope {
            to: To::Direct(peer),
            ..
        }) if *peer != hex(&addresses.vault) => Ok(()),
        _ => Err(Drop::NotForUs),
    })
}

fn check_peer(
    tx: &mut CrdtTransaction<'_>,
    own_hex: &str,
    from: &[u8; 32],
    card: &SignedCard,
    opened: &Opened,
    incoming: &Incoming,
) -> Result<std::result::Result<(), Drop>> {
    if !card
        .card
        .devices
        .iter()
        .any(|device| device.device_pubkey == opened.sender_device)
    {
        return Ok(Err(Drop::UnknownDevice));
    }
    if received::is_blocked(tx, from)? {
        return Ok(Err(Drop::Blocked));
    }
    let stored = received::stored(tx, from)?;
    let carried = (card.card.list_generation, card.card.issued_at);
    if let Some((_, stored)) = &stored {
        if carried < (stored.card.list_generation, stored.card.issued_at) {
            return Ok(Err(Drop::OlderCard));
        }
    }
    let for_us = match incoming {
        Incoming::Message(envelope) => envelope.to == To::Direct(own_hex.to_owned()),
        // A receipt answers an own message, which only ever goes to a stored card.
        Incoming::Receipt(_) if stored.is_none() => return Ok(Err(Drop::UnknownSender)),
        Incoming::Receipt(receipt) => receipt.thread == To::Direct(own_hex.to_owned()),
    };
    if !for_us {
        return Ok(Err(Drop::NotForUs));
    }
    // A message of a vault without a stored card becomes a request (US5); its card stays on the
    // member row until the user accepts it.
    if let Some((contact_id, stored)) = stored {
        if carried > (stored.card.list_generation, stored.card.issued_at) {
            received::replace(tx, &contact_id, card)?;
        }
    }
    Ok(Ok(()))
}

pub(super) fn vault_bytes(text: &str) -> Option<[u8; 32]> {
    decode_hex(text).ok()?.try_into().ok()
}

#[cfg(test)]
#[path = "verify_tests.rs"]
mod tests;
