//! The two layers of a ratchet message (spec 095 contracts/nostr-chat-ratchet.md "`chat.ratchet`"):
//! the rumor `{ "v": 1, "type": 0 | 1, "body": "<base64>" }` carrying an Olm message, and its
//! plaintext `{ "kind": 24120 | 24121 | 0, "content": "..." }` carrying the spec 072 event, the
//! receipt or a keep-alive.
//!
//! Decoding answers the [`Drop`] reason the receiving side logs; the content is never part of it.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use vodozemac::olm::OlmMessage;
use zeroize::Zeroizing;

use super::MAX_PLAINTEXT_BYTES;
use crate::chat::peer::envelope::{self, CHAT_KIND, RECEIPT_KIND};
use crate::chat::peer::verify::Drop;
use crate::error::{HaexError, Result};

/// The version of both layers.
pub const VERSION: u8 = 1;
/// An opening message (Olm pre-key message).
pub const OPENING: u8 = 0;
/// A message on an existing session.
pub const NORMAL: u8 = 1;
/// The plaintext kind of a keep-alive.
pub const KEEP_ALIVE: u16 = 0;

/// The content of a `chat.ratchet` rumor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RatchetRumor {
    pub v: u8,
    #[serde(rename = "type")]
    pub message_type: u8,
    /// Base64 of the Olm message bytes.
    pub body: String,
}

impl RatchetRumor {
    /// The rumor of an Olm message.
    pub fn of(message: &OlmMessage) -> Self {
        let (message_type, bytes) = message.to_parts();
        Self {
            v: VERSION,
            message_type: if message_type == 0 { OPENING } else { NORMAL },
            body: STANDARD.encode(bytes),
        }
    }

    /// Whether it opens a session.
    pub fn is_opening(&self) -> bool {
        self.message_type == OPENING
    }

    /// The Olm message it carries.
    pub fn message(&self) -> std::result::Result<OlmMessage, Drop> {
        let bytes = STANDARD.decode(&self.body).map_err(|_| Drop::Malformed)?;
        OlmMessage::from_parts(usize::from(self.message_type), &bytes).map_err(|_| Drop::Malformed)
    }

    pub fn encode(&self) -> Result<String> {
        serde_json::to_string(self).map_err(|e| HaexError::InvalidInput {
            reason: format!("chat ratchet rumor: {e}"),
        })
    }

    /// Reads a rumor's content; refuses another version or type.
    pub fn decode(text: &str) -> std::result::Result<Self, Drop> {
        let json: serde_json::Value = serde_json::from_str(text).map_err(|_| Drop::Malformed)?;
        if json.get("v").and_then(serde_json::Value::as_u64) != Some(u64::from(VERSION)) {
            return Err(Drop::UnknownVersion);
        }
        let rumor: Self = serde_json::from_value(json).map_err(|_| Drop::Malformed)?;
        if rumor.message_type != OPENING && rumor.message_type != NORMAL {
            return Err(Drop::Malformed);
        }
        Ok(rumor)
    }
}

/// The content of a `chat.reset` rumor.
pub fn reset_content() -> String {
    format!("{{\"v\":{VERSION}}}")
}

/// Checks a `chat.reset` rumor's content: `{ "v": 1 }` and nothing else.
pub fn check_reset(text: &str) -> std::result::Result<(), Drop> {
    let json: serde_json::Value = serde_json::from_str(text).map_err(|_| Drop::Malformed)?;
    let object = json.as_object().ok_or(Drop::Malformed)?;
    if object.get("v").and_then(serde_json::Value::as_u64) != Some(u64::from(VERSION)) {
        return Err(Drop::UnknownVersion);
    }
    if object.len() != 1 {
        return Err(Drop::Malformed);
    }
    Ok(())
}

/// What a ratchet message encrypts.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plaintext {
    /// 24120 (chat event), 24121 (receipt) or 0 (keep-alive).
    pub kind: u16,
    /// The JSON of the spec 072 event or receipt; empty for a keep-alive.
    pub content: String,
}

impl std::fmt::Debug for Plaintext {
    /// The kind only: the content is a message.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Plaintext")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl Plaintext {
    /// A keep-alive: moves the session forward and is never shown.
    pub fn keep_alive() -> Self {
        Self {
            kind: KEEP_ALIVE,
            content: String::new(),
        }
    }

    /// The bytes to encrypt; refuses an unknown kind and more than [`MAX_PLAINTEXT_BYTES`]
    /// ("chat.error.tooLarge").
    pub fn encode(&self) -> Result<Zeroizing<String>> {
        if !known_kind(self.kind) {
            return Err(HaexError::InvalidInput {
                reason: format!("chat ratchet plaintext: unknown kind {}", self.kind),
            });
        }
        let json =
            Zeroizing::new(
                serde_json::to_string(self).map_err(|e| HaexError::InvalidInput {
                    reason: format!("chat ratchet plaintext: {e}"),
                })?,
            );
        if json.len() > MAX_PLAINTEXT_BYTES {
            return Err(envelope::too_large());
        }
        Ok(json)
    }

    /// Reads decrypted bytes; refuses an unknown kind.
    pub fn decode(bytes: &[u8]) -> std::result::Result<Self, Drop> {
        if bytes.len() > MAX_PLAINTEXT_BYTES {
            return Err(Drop::TooLarge);
        }
        let plaintext: Self = serde_json::from_slice(bytes).map_err(|_| Drop::Malformed)?;
        if !known_kind(plaintext.kind) {
            return Err(Drop::UnknownKind);
        }
        Ok(plaintext)
    }
}

fn known_kind(kind: u16) -> bool {
    kind == CHAT_KIND.as_u16() || kind == RECEIPT_KIND.as_u16() || kind == KEEP_ALIVE
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;
