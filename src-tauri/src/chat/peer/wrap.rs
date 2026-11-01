//! Gift wraps of chat events (spec 072 research R1, R2, contracts/nostr-chat.md "Gift wrap"): a rumor
//! sealed by the sender's device key and wrapped with a one-time key to **one device key** of a
//! recipient, as a stored kind 1059 event with a backdated `created_at` and a 30-day `expiration`.
//! The layers are the ones presence builds ([`crate::sync::gift_wrap::wrap_shaped`]); only the shape
//! differs.

use nostr::event::{Event, Kind};
use nostr::key::SecretKey;

use crate::error::{HolziError, Result};
use crate::sync::gift_wrap::{self, WrapShape};
use crate::sync::presence::{self, PresenceError};

/// How far back a wrap's `created_at` is drawn (NIP-59).
pub const BACKDATE_SECS: u64 = 48 * 60 * 60;
/// When Nostr-Relays may drop a wrap: after the pickup window.
pub const EXPIRES_AFTER_SECS: u64 = 30 * 24 * 60 * 60;
/// The largest wrap a common Nostr-Relay accepts with room to spare (research R2).
pub const MAX_WRAP_BYTES: usize = 60 * 1024;

/// The shape of every chat wrap.
pub(crate) const CHAT_WRAP: WrapShape = WrapShape {
    kind: Kind::GiftWrap,
    backdate_secs: BACKDATE_SECS,
    expires_after_secs: Some(EXPIRES_AFTER_SECS),
    max_bytes: MAX_WRAP_BYTES,
};

/// An opened wrap: the device key that sealed it, the rumor's kind and content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    pub sender_device: [u8; 32],
    pub kind: Kind,
    pub content: String,
}

/// Wraps the rumor `kind`/`content`, sealed by `sender_secret`, to the device key `recipient`.
pub fn wrap(
    kind: Kind,
    content: &str,
    sender_secret: &[u8; 32],
    recipient: &[u8; 32],
) -> Result<Event> {
    let recipient = presence::public_key(recipient).map_err(failure)?;
    gift_wrap::wrap_shaped(CHAT_WRAP, kind, sender_secret, content, &recipient).map_err(failure)
}

/// Opens a wrap addressed to this device with its secret.
pub fn unwrap(event: &Event, device_secret: &[u8; 32]) -> Result<Opened> {
    let secret = SecretKey::from_slice(device_secret).map_err(|_| HolziError::InvalidInput {
        reason: "the device secret is not a key".into(),
    })?;
    let (sender_device, kind, content) =
        gift_wrap::unwrap_shaped(event, &secret, MAX_WRAP_BYTES).map_err(failure)?;
    Ok(Opened {
        sender_device,
        kind,
        content,
    })
}

fn failure(error: PresenceError) -> HolziError {
    match error {
        PresenceError::TooLarge => super::envelope::too_large(),
        other => HolziError::InvalidInput {
            reason: format!("chat wrap: {other}"),
        },
    }
}

#[cfg(test)]
#[path = "wrap_tests.rs"]
mod tests;
