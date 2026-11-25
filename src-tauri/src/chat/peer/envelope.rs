//! The JSON inside a chat rumor (spec 072 contracts/nostr-chat.md, research R2, R5): the envelope of
//! kind 24120 and the receipt of kind 24121. Both carry the sender's current Visitenkarte so every
//! receiver knows the sender's devices and Nostr-Relays without a lookup.
//!
//! This build knows the envelope type `message` with text and file attachments (US3); contact
//! attachments (US6) and group types (US4) join with their slices. A rumor of an unknown type does not decode and is dropped by the
//! receiver, like one of an unknown `v`.

use nostr::event::Kind;
use serde::{Deserialize, Serialize};

use crate::error::{HaexError, Result};
use crate::sync::keys::random_bytes;

/// The rumor kind of chat events.
pub const CHAT_KIND: Kind = Kind::Custom(24120);
/// The rumor kind of receipts.
pub const RECEIPT_KIND: Kind = Kind::Custom(24121);
/// The one envelope version this build reads and writes.
pub const VERSION: u8 = 1;
/// The longest text of a message or note, in characters (research R2, amended 2026-11-20). What
/// does not fit one wrap travels as an `inline` file (see [`FileAttachment::inline`]).
pub const MAX_TEXT_CHARS: usize = 1_000_000;
/// The preview of a text that travels as a file: its first characters, in the message's `text`.
pub const PREVIEW_CHARS: usize = 1_000;
/// The largest file a receiver honours as the full text of a message (4 characters of 4 bytes
/// each at [`MAX_TEXT_CHARS`] fit).
pub const MAX_INLINE_FILE_BYTES: u64 = 4 * 1024 * 1024;
/// The mime type of the file that carries a long text.
pub const INLINE_MIME: &str = "text/plain";
/// The most attachments of one message (contracts/nostr-chat.md).
pub const MAX_ATTACHMENTS: usize = 10;

/// Whom an event is for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum To {
    /// A direct conversation: the other vault, hex. For a receipt: the original sender's vault.
    Direct(String),
    /// A group conversation (US4).
    Group(String),
    /// The notes of the sender: the own vault, hex (FR-002b). Only the own devices accept it.
    Note(String),
}

/// The content of a message (contracts/nostr-chat.md `type: "message"`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageBody {
    /// May be empty when the message has attachments.
    #[serde(default)]
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attachment>,
}

/// One attachment of a message, by its `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Attachment {
    File(FileAttachment),
}

/// A file: a copy on a Blossom-Server, encrypted with `blossom.key` (research R11, R12).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileAttachment {
    pub name: String,
    pub mime: String,
    /// Plaintext bytes.
    pub size: u64,
    /// Plaintext SHA-256, hex.
    pub sha256: String,
    pub blossom: BlobRef,
    /// The file is the full text of the message and `text` only its preview (research R2). A
    /// receiver honours it only for [`INLINE_MIME`] up to [`MAX_INLINE_FILE_BYTES`]; absent means
    /// an ordinary file.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub inline: bool,
}

/// Where the encrypted copy is and how to open it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobRef {
    pub url: String,
    /// Ciphertext SHA-256, hex: the blob's name on the server.
    pub sha256: String,
    /// The AES-256 key, hex.
    pub key: String,
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
    /// The sender's signed Visitenkarte, `haex:card:` text.
    pub card: String,
    pub to: To,
}

/// The receiving vault's answer to a sender (spec 099 contracts/nostr-chat-gate.md): it may write
/// now, or it may not (a declined request or a blocked contact; the sender does not learn which).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Answer {
    Accepted,
    Blocked,
}

impl Answer {
    /// The value stored in `chat_gate_notices_no_sync.answer`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Blocked => "blocked",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "accepted" => Some(Self::Accepted),
            "blocked" => Some(Self::Blocked),
            _ => None,
        }
    }
}

/// A receipt (kind 24121): which events of the original sender arrived or were read, or with
/// `answer` the receiving vault's answer to it (spec 099).
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<Answer>,
}

/// A fresh event id: 128 random bits, hex.
pub fn new_event_id() -> String {
    crate::sync::keys::hex(&*random_bytes::<16>())
}

/// The text of a message, trimmed, refused when empty or over [`MAX_TEXT_CHARS`] (research R2).
pub fn checked_text(text: &str) -> Result<&str> {
    let text = text.trim();
    if text.is_empty() {
        return Err(HaexError::InvalidInput {
            reason: "a message needs a text".into(),
        });
    }
    // Every character is at least one byte: only a text this long can be over the limit.
    if text.len() > MAX_TEXT_CHARS && text.chars().count() > MAX_TEXT_CHARS {
        return Err(too_long());
    }
    Ok(text)
}

/// The first characters of `text` for its message when the text itself travels as a file: at most
/// [`PREVIEW_CHARS`] with the closing `…`, cut at the last whitespace before the limit (at the limit
/// when there is none). A text that is short enough stays whole.
pub fn preview(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= PREVIEW_CHARS {
        return text.to_owned();
    }
    let end = text
        .char_indices()
        .nth(PREVIEW_CHARS - 1)
        .map_or(text.len(), |(at, _)| at);
    let window = &text[..end];
    let cut = window
        .rfind(char::is_whitespace)
        .filter(|at| *at > 0)
        .map_or(window, |at| &window[..at]);
    format!("{}…", cut.trim_end())
}

/// The text of a message with `attachments`: trimmed, may be empty only when there are some (at
/// most [`MAX_ATTACHMENTS`]).
pub fn checked_body(text: &str, attachments: usize) -> Result<&str> {
    if attachments > MAX_ATTACHMENTS {
        return Err(HaexError::InvalidInput {
            reason: "chat.file.tooMany".into(),
        });
    }
    if attachments > 0 && text.trim().is_empty() {
        return Ok("");
    }
    checked_text(text)
}

/// A message from `from` (hex) to the direct conversation with `to` (hex), checked against the
/// limits of research R2.
pub fn message(
    text: &str,
    attachments: Vec<Attachment>,
    from: &str,
    card: &str,
    to: To,
    at: i64,
) -> Result<Envelope> {
    let text = checked_body(text, attachments.len())?;
    Ok(Envelope {
        v: VERSION,
        id: new_event_id(),
        body: Body::Message(MessageBody {
            text: text.to_owned(),
            attachments,
        }),
        at,
        from: from.to_owned(),
        card: card.to_owned(),
        to,
    })
}

/// The JSON of an event or receipt. Its size is checked as the plaintext of the ratchet message
/// that carries it (spec 095, `ratchet::wire::Plaintext::encode`).
pub fn encode<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|e| HaexError::InvalidInput {
        reason: format!("the chat event does not encode: {e}"),
    })
}

/// "chat.error.tooLong": a text over [`MAX_TEXT_CHARS`] (FR-012).
pub fn too_long() -> HaexError {
    HaexError::InvalidInput {
        reason: "chat.error.tooLong".into(),
    }
}

/// "chat.error.tooLarge": what does not fit one wrap (FR-012).
pub fn too_large() -> HaexError {
    HaexError::InvalidInput {
        reason: "chat.error.tooLarge".into(),
    }
}

#[cfg(test)]
#[path = "envelope_tests.rs"]
mod tests;
