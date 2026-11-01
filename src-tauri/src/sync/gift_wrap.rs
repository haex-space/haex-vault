//! The three layers of NIP-59 (gift wrap, seal, unsigned rumor) for every holzi event that travels
//! over Nostr-Relays: presence and the link rendezvous (spec 024, ephemeral kind 21059) and the chat
//! with contacts (spec 072 research R1, stored kind 1059). One implementation; only the
//! [`WrapShape`] differs.

use nostr::event::{
    Event, EventBuilder, FinalizeEvent, FinalizeUnsignedEvent, Kind, Tag, UnsignedEvent,
};
use nostr::key::{Keys, PublicKey, SecretKey};
use nostr::nips::nip44;
use nostr::types::Timestamp;

use crate::sync::keys::random_bytes;
use crate::sync::presence::{nostr_secret, PresenceError, GIFT_WRAP_KIND, MAX_EVENT_BYTES};

/// How a gift wrap is shaped: its kind, how far back its `created_at` may be drawn, its
/// `expiration` and the largest serialized size. Presence and the link rendezvous use
/// [`WrapShape::PRESENCE`]; the chat with contacts (spec 072 research R1, R2) its own shape, so
/// there is one wrapping implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WrapShape {
    pub kind: Kind,
    /// `created_at` of the seal and the wrap is drawn from `now - backdate_secs ..= now` (NIP-59).
    pub backdate_secs: u64,
    /// An `expiration` tag (NIP-40) this many seconds after now.
    pub expires_after_secs: Option<u64>,
    pub max_bytes: usize,
}

impl WrapShape {
    /// Ephemeral wraps with the real time and the 16 KiB limit (contracts/nostr-events.md).
    pub const PRESENCE: WrapShape = WrapShape {
        kind: GIFT_WRAP_KIND,
        backdate_secs: 0,
        expires_after_secs: None,
        max_bytes: MAX_EVENT_BYTES,
    };
}

/// [`wrap_payload`] with the wrap's kind, timestamps, expiration and size limit of `shape`.
pub(crate) fn wrap_shaped(
    shape: WrapShape,
    inner_kind: Kind,
    sender_secret: &[u8; 32],
    payload: &str,
    recipient: &PublicKey,
) -> Result<Event, PresenceError> {
    let sender_secret = nostr_secret(sender_secret)?;
    let sender_keys = Keys::new(sender_secret.clone());

    let rumor: UnsignedEvent =
        EventBuilder::new(inner_kind, payload).finalize_unsigned(sender_keys.public_key());
    let rumor_json =
        serde_json::to_string(&rumor).map_err(|e| PresenceError::Malformed(e.to_string()))?;
    let sealed = nip44::encrypt(
        &sender_secret,
        recipient,
        rumor_json.as_bytes(),
        nip44::Version::V2,
    )
    .map_err(|e| PresenceError::Crypto(e.to_string()))?;
    let seal: Event = EventBuilder::new(Kind::Seal, sealed)
        .custom_created_at(drawn_created_at(shape.backdate_secs))
        .finalize(&sender_keys)
        .map_err(|e| PresenceError::Crypto(e.to_string()))?;

    let seal_json =
        serde_json::to_string(&seal).map_err(|e| PresenceError::Malformed(e.to_string()))?;
    let onetime = Keys::generate();
    let wrapped = nip44::encrypt(
        onetime.secret_key(),
        recipient,
        seal_json.as_bytes(),
        nip44::Version::V2,
    )
    .map_err(|e| PresenceError::Crypto(e.to_string()))?;
    let mut builder = EventBuilder::new(shape.kind, wrapped)
        .tag(Tag::public_key(*recipient))
        .custom_created_at(drawn_created_at(shape.backdate_secs));
    if let Some(after) = shape.expires_after_secs {
        builder = builder.tag(Tag::expiration(Timestamp::from_secs(
            Timestamp::now().as_secs() + after,
        )));
    }
    let event = builder
        .finalize(&onetime)
        .map_err(|e| PresenceError::Crypto(e.to_string()))?;
    if event.as_json().len() > shape.max_bytes {
        return Err(PresenceError::TooLarge);
    }
    Ok(event)
}

/// Now, or a moment drawn uniformly from the last `backdate_secs` seconds.
fn drawn_created_at(backdate_secs: u64) -> Timestamp {
    let now = Timestamp::now().as_secs();
    if backdate_secs == 0 {
        return Timestamp::from_secs(now);
    }
    let draw = u64::from_le_bytes(*random_bytes::<8>());
    Timestamp::from_secs(now - draw % (backdate_secs + 1))
}

/// [`unwrap_rumor`] with the size limit of the wrap's shape.
pub(crate) fn unwrap_shaped(
    gift_wrap: &Event,
    mb_sk: &SecretKey,
    max_bytes: usize,
) -> Result<([u8; 32], Kind, String), PresenceError> {
    if gift_wrap.as_json().len() > max_bytes {
        return Err(PresenceError::TooLarge);
    }
    let seal_json = nip44::decrypt_to_bytes(mb_sk, &gift_wrap.pubkey, &gift_wrap.content)
        .map_err(|e| PresenceError::Crypto(e.to_string()))?;
    let seal: Event =
        serde_json::from_slice(&seal_json).map_err(|e| PresenceError::Malformed(e.to_string()))?;
    seal.verify()
        .map_err(|e| PresenceError::Crypto(e.to_string()))?;
    if seal.kind != Kind::Seal {
        return Err(PresenceError::WrongKind);
    }

    let rumor_json = nip44::decrypt_to_bytes(mb_sk, &seal.pubkey, &seal.content)
        .map_err(|e| PresenceError::Crypto(e.to_string()))?;
    let rumor: UnsignedEvent =
        serde_json::from_slice(&rumor_json).map_err(|e| PresenceError::Malformed(e.to_string()))?;
    Ok((seal.pubkey.to_bytes(), rumor.kind, rumor.content))
}
