//! Chat key records (spec 095 contracts/nostr-chat-ratchet.md "Chat key record", data-model.md):
//! a replaceable Nostr event of kind 10120, signed by a device key, that offers the device's ratchet
//! identity key and current start key, so another device can open a session with it while it is
//! offline.
//!
//! `until` is the time the start key was made plus 30 days: publishing the same start key again
//! keeps it, so a start key is never offered longer than the device keeps it. A reader accepts a
//! record only from a device a stored card lists, only while `until` has not passed by its own
//! clock, and never one older than the record it holds, so a Nostr-Relay cannot hand back an
//! earlier start key.

use haex_crdt::CrdtTransaction;
use nostr::event::{Event, EventBuilder, FinalizeEvent, Tag};
use nostr::key::Keys;
use nostr::types::Timestamp;
use serde::{Deserialize, Serialize};
use vodozemac::olm::Account;
use vodozemac::Curve25519PublicKey;

use super::store::{self, RecordRow};
use super::{account, KEY_RECORD_KIND, START_KEY_RENEW_MS};
use crate::chat::peer::session::Me;
use crate::chat::peer::targets;
use crate::error::{HaexError, Result};
use crate::sync::keys::hex;
use crate::sync::presence;

/// The version of the record's content.
pub const VERSION: u8 = 1;

/// The content of a record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Content {
    v: u8,
    /// The ratchet identity key (Curve25519, base64).
    identity: String,
    /// The current start key (Curve25519, base64).
    start: String,
    /// Seconds since the Unix epoch.
    until: u64,
}

/// Why a record was refused; logged as its code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    WrongKind,
    BadSignature,
    Malformed,
    UnknownVersion,
    /// No stored card lists the signing device (also a removed device or a blocked vault).
    UnknownDevice,
    Expired,
    /// Older than the record this device holds for that device.
    Older,
}

impl Refused {
    pub fn code(self) -> &'static str {
        match self {
            Self::WrongKind => "wrongKind",
            Self::BadSignature => "badSignature",
            Self::Malformed => "malformed",
            Self::UnknownVersion => "unknownVersion",
            Self::UnknownDevice => "unknownDevice",
            Self::Expired => "expired",
            Self::Older => "older",
        }
    }
}

/// The own record at `now` (milliseconds): the keys of `account`, valid until
/// [`START_KEY_RENEW_MS`] after `start_key_made_at`, signed by this device's key.
pub fn own_event(me: &Me, account: &Account, start_key_made_at: i64, now: i64) -> Result<Event> {
    let keys = account::own_keys(account)?;
    let until = secs(start_key_made_at + START_KEY_RENEW_MS);
    let content = serde_json::to_string(&Content {
        v: VERSION,
        identity: keys.identity.to_base64(),
        start: keys.start.to_base64(),
        until,
    })
    .map_err(|e| invalid(&e.to_string()))?;
    let signer =
        presence::nostr_secret(&me.keys.device_secret).map_err(|e| invalid(&e.to_string()))?;
    EventBuilder::new(KEY_RECORD_KIND, content)
        .tag(Tag::expiration(Timestamp::from_secs(until)))
        .custom_created_at(Timestamp::from_secs(secs(now)))
        .finalize(&Keys::new(signer))
        .map_err(|e| invalid(&e.to_string()))
}

/// Checks a record fetched from a Nostr-Relay at `now` (milliseconds) and stores it for its device.
/// A record with another ratchet identity key than the stored one means the device started anew:
/// the sessions of this installation with it are deleted. An accepted record ends a reset's request
/// to fetch it again (`refetch_record`).
pub fn accept(
    tx: &mut CrdtTransaction<'_>,
    me: &Me,
    event: &Event,
    now: i64,
) -> Result<std::result::Result<RecordRow, Refused>> {
    if event.kind != KEY_RECORD_KIND {
        return Ok(Err(Refused::WrongKind));
    }
    if event.verify().is_err() {
        return Ok(Err(Refused::BadSignature));
    }
    let content = match decode(&event.content) {
        Ok(content) => content,
        Err(refused) => return Ok(Err(refused)),
    };
    let device = event.pubkey.to_bytes();
    if targets::vault_of_device(tx, me, &device)?.is_none() {
        return Ok(Err(Refused::UnknownDevice));
    }
    let valid_until = millis(content.until);
    if valid_until <= now {
        return Ok(Err(Refused::Expired));
    }
    let peer_device = hex(&device);
    let row = RecordRow {
        peer_device: peer_device.clone(),
        identity_key: content.identity,
        start_key: content.start,
        made_at: millis(event.created_at.as_secs()),
        valid_until,
    };
    if let Some(stored) = store::record(tx, &peer_device)? {
        if row.made_at < stored.made_at {
            return Ok(Err(Refused::Older));
        }
        if stored.identity_key != row.identity_key {
            store::delete_sessions(tx, me.installation, &peer_device)?;
        }
    }
    store::put_record(tx, &row)?;
    // A reset asked for this fresh record (spec 095 T030).
    let mut peer = store::peer(tx, me.installation, &peer_device)?;
    if peer.refetch_record {
        peer.refetch_record = false;
        store::put_peer(tx, me.installation, &peer_device, &peer)?;
    }
    Ok(Ok(row))
}

fn decode(text: &str) -> std::result::Result<Content, Refused> {
    let json: serde_json::Value = serde_json::from_str(text).map_err(|_| Refused::Malformed)?;
    if json.get("v").and_then(serde_json::Value::as_u64) != Some(u64::from(VERSION)) {
        return Err(Refused::UnknownVersion);
    }
    let content: Content = serde_json::from_value(json).map_err(|_| Refused::Malformed)?;
    for key in [&content.identity, &content.start] {
        Curve25519PublicKey::from_base64(key).map_err(|_| Refused::Malformed)?;
    }
    Ok(content)
}

fn secs(millis: i64) -> u64 {
    u64::try_from(millis / 1000).unwrap_or(0)
}

fn millis(secs: u64) -> i64 {
    i64::try_from(secs)
        .unwrap_or(i64::MAX / 1000)
        .saturating_mul(1000)
}

fn invalid(reason: &str) -> HaexError {
    HaexError::InvalidInput {
        reason: format!("chat key record: {reason}"),
    }
}

#[cfg(test)]
#[path = "record_tests.rs"]
mod tests;
