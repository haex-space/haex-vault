//! Sessions of this installation with peer devices (data-model.md "Device session",
//! contracts/nostr-chat-ratchet.md "Sending", "Receiving", "Limits").
//!
//! Every operation loads the pickle inside the caller's write transaction and stores the advanced
//! one in the same transaction; no session is held in memory between operations. The vault's
//! write transactions run one after another, so that transaction is the per-session lock of the
//! data model: two operations on one session never start from the same state.
//!
//! Sending uses the retained session with the smallest session id, so both devices pick the same one
//! once they know the same sessions. Receiving accepts any retained session. A decrypted message is
//! saved only when the caller confirms it after its own checks ([`Decrypted::confirm`]).
//!
//! `renewed_at` (FR-022): when this device made the newest ratchet key that the peer has answered
//! with a fresh ratchet key of its own. It is found without counting events: after a message
//! decrypted, a throwaway copy of the session encrypts once; if that copy would send with another
//! ratchet key than the one this device last sent with, the peer has answered that key with a new
//! one. A delayed message on an older chain does not advance the session and moves nothing.

use haex_crdt::CrdtTransaction;
use vodozemac::olm::{OlmMessage, Session, SessionConfig, SessionPickle};
use vodozemac::Curve25519PublicKey;
use zeroize::Zeroizing;

use super::store::{self, SessionRow};
use super::wire::RatchetRumor;
use super::{account, reset, MAX_SESSIONS};
use crate::chat::peer::session::Me;
use crate::chat::peer::targets;
use crate::error::{HaexError, Result};
use crate::sync::presence::decode_hex;

/// The result of [`decrypt`].
#[derive(Debug)]
pub enum Decrypt {
    Decrypted(Box<Decrypted>),
    /// No retained session decrypts it and it opens none: a used or skipped-too-far message key, a
    /// session this device lost, or an opening for a start key it no longer holds.
    Undecryptable,
}

/// A decrypted message whose session is not saved yet.
pub struct Decrypted {
    plaintext: Zeroizing<Vec<u8>>,
    row: SessionRow,
    opened: bool,
    previous_pickle: Option<Zeroizing<String>>,
}

impl std::fmt::Debug for Decrypted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Decrypted")
            .field("session_id", &self.row.session_id)
            .field("opened", &self.opened)
            .finish_non_exhaustive()
    }
}

impl Decrypted {
    pub fn plaintext(&self) -> &[u8] {
        &self.plaintext
    }

    /// The message opened a new session on this device.
    pub fn opened(&self) -> bool {
        self.opened
    }

    pub fn session_id(&self) -> &str {
        &self.row.session_id
    }

    /// Saves the advanced session, once the caller's checks passed. A new session from an opening
    /// that carries another ratchet identity key than the retained sessions replaces them (the
    /// device started anew); beyond [`MAX_SESSIONS`] the least recently used goes.
    pub fn confirm(self, tx: &mut CrdtTransaction<'_>, me: &Me) -> Result<()> {
        let installation = me.installation;
        let current = store::sessions(tx, installation, &self.row.peer_device)?;
        let unchanged = match &self.previous_pickle {
            Some(previous) => current.iter().any(|row| {
                row.session_id == self.row.session_id && row.session.as_str() == previous.as_str()
            }),
            None => current
                .iter()
                .all(|row| row.session_id != self.row.session_id),
        };
        if !unchanged {
            return Err(HaexError::InvalidInput {
                reason: "chat ratchet session changed while decrypting".to_owned(),
            });
        }
        if self.opened {
            for other in current {
                if other.peer_identity_key != self.row.peer_identity_key {
                    store::delete_session(tx, installation, &other.peer_device, &other.session_id)?;
                }
            }
        }
        store::put_session(tx, installation, &self.row)?;
        if self.opened {
            keep_at_most(tx, me, &self.row.peer_device, &self.row.session_id)?;
        }
        reset::recovered(tx, installation, &self.row.peer_device, self.row.created_at)
    }
}

/// Opens a session with `peer_device` (hex) from its record's ratchet identity key and start key;
/// answers the session id. The first messages on it are opening messages.
pub fn open_outbound(
    tx: &mut CrdtTransaction<'_>,
    me: &Me,
    peer_device: &str,
    identity: Curve25519PublicKey,
    start: Curve25519PublicKey,
    now: i64,
) -> Result<String> {
    let account = account::load_or_create(tx, me.installation, now)?;
    let session = account
        .create_outbound_session(SessionConfig::version_1(), identity, start)
        .map_err(|_| broken("opening a session"))?;
    let mut probe = Session::from_pickle(session.pickle());
    let own_ratchet_key = ratchet_key_of(
        &probe
            .encrypt([])
            .map_err(|_| broken("probing the initial ratchet key"))?,
    )
    .to_base64();
    let row = SessionRow {
        peer_device: peer_device.to_owned(),
        session_id: session.session_id(),
        session: pickle(&session)?,
        peer_identity_key: identity.to_base64(),
        created_at: now,
        last_received_at: None,
        last_sent_at: None,
        own_ratchet_key: Some(own_ratchet_key),
        own_key_since: Some(now),
        renewed_at: None,
    };
    store::put_session(tx, me.installation, &row)?;
    keep_at_most(tx, me, peer_device, &row.session_id)?;
    Ok(row.session_id)
}

/// Encrypts `plaintext` for `peer_device` on the session to send on and saves the session; `None`
/// without a session.
pub fn encrypt(
    tx: &mut CrdtTransaction<'_>,
    me: &Me,
    peer_device: &str,
    plaintext: &[u8],
    now: i64,
) -> Result<Option<RatchetRumor>> {
    // `sessions` answers them by session id, so the first is the one to send on.
    let Some(mut row) = store::sessions(tx, me.installation, peer_device)?
        .into_iter()
        .next()
    else {
        return Ok(None);
    };
    let mut session = unpickle(&row.session)?;
    let message = session
        .encrypt(plaintext)
        .map_err(|_| broken("encrypting"))?;
    let ratchet_key = ratchet_key_of(&message).to_base64();
    if row.own_ratchet_key.as_deref() != Some(ratchet_key.as_str()) {
        row.own_ratchet_key = Some(ratchet_key);
        row.own_key_since = Some(now);
    }
    row.session = pickle(&session)?;
    row.last_sent_at = Some(now);
    store::put_session(tx, me.installation, &row)?;
    Ok(Some(RatchetRumor::of(&message)))
}

/// Decrypts a ratchet message from `peer_device`; nothing is saved before
/// [`Decrypted::confirm`].
pub fn decrypt(
    tx: &mut CrdtTransaction<'_>,
    me: &Me,
    peer_device: &str,
    message: &OlmMessage,
    now: i64,
) -> Result<Decrypt> {
    let retained = store::sessions(tx, me.installation, peer_device)?;
    let opening = match message {
        OlmMessage::PreKey(pre_key) => {
            let id = pre_key.session_id();
            if let Some(row) = retained.into_iter().find(|row| row.session_id == id) {
                return Ok(try_session(row, message, now)
                    .map_or(Decrypt::Undecryptable, |found| {
                        Decrypt::Decrypted(Box::new(found))
                    }));
            }
            pre_key
        }
        OlmMessage::Normal(_) => {
            return Ok(retained
                .into_iter()
                .find_map(|row| try_session(row, message, now))
                .map_or(Decrypt::Undecryptable, |found| {
                    Decrypt::Decrypted(Box::new(found))
                }));
        }
    };
    let Some((mut account, _)) = account::load(tx, me.installation)? else {
        return Ok(Decrypt::Undecryptable);
    };
    let Ok(created) =
        account.create_inbound_session(SessionConfig::version_1(), opening.identity_key(), opening)
    else {
        return Ok(Decrypt::Undecryptable);
    };
    let row = SessionRow {
        peer_device: peer_device.to_owned(),
        session_id: created.session.session_id(),
        session: pickle(&created.session)?,
        peer_identity_key: opening.identity_key().to_base64(),
        created_at: now,
        last_received_at: Some(now),
        last_sent_at: None,
        own_ratchet_key: None,
        own_key_since: None,
        renewed_at: None,
    };
    Ok(Decrypt::Decrypted(Box::new(Decrypted {
        plaintext: Zeroizing::new(created.plaintext),
        row,
        opened: true,
        previous_pickle: None,
    })))
}

/// Decrypts `message` on a fresh copy of `row`'s session; the advanced row when it decrypted.
fn try_session(mut row: SessionRow, message: &OlmMessage, now: i64) -> Option<Decrypted> {
    let previous_pickle = row.session.clone();
    let mut session = unpickle(&row.session).ok()?;
    let plaintext = Zeroizing::new(session.decrypt(message).ok()?);
    if let (Some(own_key), Some(own_since)) = (&row.own_ratchet_key, row.own_key_since) {
        let mut probe = Session::from_pickle(session.pickle());
        let next_key = probe
            .encrypt([])
            .ok()
            .map(|next| ratchet_key_of(&next).to_base64());
        if next_key.is_some_and(|next| next != *own_key) {
            row.renewed_at = Some(own_since);
        }
    }
    row.session = pickle(&session).ok()?;
    row.last_received_at = Some(now);
    Some(Decrypted {
        plaintext,
        row,
        opened: false,
        previous_pickle: Some(previous_pickle),
    })
}

/// Deletes the sessions, the peer state and the stored record of every peer device that
/// [`targets::addressed_vault`] no longer answers for; answers how many devices went.
pub fn forget_unaddressed(tx: &mut CrdtTransaction<'_>, me: &Me) -> Result<usize> {
    let mut forgotten = 0;
    for device in store::known_devices(tx, me.installation)? {
        let bytes: Option<[u8; 32]> = decode_hex(&device)
            .ok()
            .and_then(|bytes| bytes.try_into().ok());
        let addressed = match bytes {
            Some(bytes) => targets::addressed_vault(tx, me, &bytes)?.is_some(),
            None => false,
        };
        if !addressed {
            store::forget_device(tx, me.installation, &device)?;
            forgotten += 1;
        }
    }
    Ok(forgotten)
}

/// Deletes sessions with `peer_device` beyond [`MAX_SESSIONS`], never `keep`: the least recently
/// used first; on a tie the larger session id, so the session to send on stays.
fn keep_at_most(
    tx: &mut CrdtTransaction<'_>,
    me: &Me,
    peer_device: &str,
    keep: &str,
) -> Result<()> {
    let mut rows = store::sessions(tx, me.installation, peer_device)?;
    while rows.len() > MAX_SESSIONS {
        let Some(index) = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.session_id != keep)
            .min_by(|(_, a), (_, b)| {
                a.last_used()
                    .cmp(&b.last_used())
                    .then_with(|| b.session_id.cmp(&a.session_id))
            })
            .map(|(index, _)| index)
        else {
            break;
        };
        let gone = rows.remove(index);
        store::delete_session(tx, me.installation, peer_device, &gone.session_id)?;
    }
    Ok(())
}

fn ratchet_key_of(message: &OlmMessage) -> Curve25519PublicKey {
    match message {
        OlmMessage::Normal(message) => message.ratchet_key(),
        OlmMessage::PreKey(pre_key) => pre_key.message().ratchet_key(),
    }
}

fn pickle(session: &Session) -> Result<Zeroizing<String>> {
    serde_json::to_string(&session.pickle())
        .map(Zeroizing::new)
        .map_err(|_| broken("pickling"))
}

fn unpickle(text: &str) -> Result<Session> {
    let pickle: SessionPickle = serde_json::from_str(text).map_err(|_| broken("reading"))?;
    Ok(Session::from_pickle(pickle))
}

/// The error of a session that does not open, encrypt or (un)pickle; never key material.
fn broken(what: &str) -> HaexError {
    HaexError::InvalidInput {
        reason: format!("chat ratchet session: {what} failed"),
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
