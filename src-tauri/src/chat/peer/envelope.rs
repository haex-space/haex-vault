//! The JSON inside a chat rumor (spec 072 contracts/nostr-chat.md, research R2, R5): the envelope of
//! kind 24120 and the receipt of kind 24121. Both carry the sender's current Visitenkarte so every
//! receiver knows the sender's devices and Nostr-Relays without a lookup.
//!
//! This slice knows the envelope type `message` with text; attachments (US3) and group types (US4)
//! join with their slices. A rumor of an unknown type does not decode and is dropped by the
//! receiver, like one of an unknown `v`.

use nostr::event::Kind;
use serde::{Deserialize, Serialize};

use crate::error::{HolziError, Result};
use crate::sync::keys::random_bytes;

/// The rumor kind of chat events.
pub const CHAT_KIND: Kind = Kind::Custom(24120);
/// The rumor kind of receipts.
pub const RECEIPT_KIND: Kind = Kind::Custom(24121);
/// The one envelope version this build reads and writes.
pub const VERSION: u8 = 1;
/// The longest text of a message, in characters (research R2).
pub const MAX_TEXT_CHARS: usize = 4_000;
/// The largest rumor content in bytes: two NIP-44 layers (padding, base64) grow it to about 60 KiB
/// (research R2). Sending also wraps once on trial, since escaping can grow a rumor further.
pub const MAX_RUMOR_BYTES: usize = 24 * 1024;

/// Whom an event is for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum To {
    /// A direct conversation: the other vault, hex. For a receipt: the original sender's vault.
    Direct(String),
    /// A group conversation (US4).
    Group(String),
}

/// The content of a message (contracts/nostr-chat.md `type: "message"`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageBody {
    pub text: String,
}

/// What an envelope carries, by its `type`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "body")]
pub enum Body {
    #[serde(rename = "message")]
    Message(MessageBody),
}

/// A chat event (kind 24120).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    pub v: u8,
    /// 128 bits, hex, random: names the event in message ids and receipts (research R4).
    pub id: String,
    #[serde(flatten)]
    pub body: Body,
    /// The sender's time, milliseconds.
    pub at: i64,
    /// The sender's vault identity, hex.
    pub from: String,
    /// The sender's signed Visitenkarte, `holzi:card:` text.
    pub card: String,
    pub to: To,
}

/// A receipt (kind 24121): which events of the original sender arrived or were read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    pub v: u8,
    /// The receiver's vault identity, hex.
    pub from: String,
    pub card: String,
    pub thread: To,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub delivered: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub read: Vec<String>,
}

/// A fresh event id: 128 random bits, hex.
pub fn new_event_id() -> String {
    crate::sync::keys::hex(&*random_bytes::<16>())
}

/// The text of a message, trimmed, refused when empty or over [`MAX_TEXT_CHARS`] (research R2).
pub fn checked_text(text: &str) -> Result<&str> {
    let text = text.trim();
    if text.is_empty() {
        return Err(HolziError::InvalidInput {
            reason: "a message needs a text".into(),
        });
    }
    if text.chars().count() > MAX_TEXT_CHARS {
        return Err(too_large());
    }
    Ok(text)
}

/// A text message from `from` (hex) to the direct conversation with `to` (hex), checked against
/// the limits of research R2.
pub fn message(text: &str, from: &str, card: &str, to: To, at: i64) -> Result<Envelope> {
    let text = checked_text(text)?;
    Ok(Envelope {
        v: VERSION,
        id: new_event_id(),
        body: Body::Message(MessageBody {
            text: text.to_owned(),
        }),
        at,
        from: from.to_owned(),
        card: card.to_owned(),
        to,
    })
}

/// The JSON of a rumor's content, refused when it is over [`MAX_RUMOR_BYTES`].
pub fn encode<T: Serialize>(value: &T) -> Result<String> {
    let json = serde_json::to_string(value).map_err(|e| HolziError::InvalidInput {
        reason: format!("the chat event does not encode: {e}"),
    })?;
    if json.len() > MAX_RUMOR_BYTES {
        return Err(too_large());
    }
    Ok(json)
}

/// "chat.error.tooLarge" (FR-012).
pub fn too_large() -> HolziError {
    HolziError::InvalidInput {
        reason: "chat.error.tooLarge".into(),
    }
}
