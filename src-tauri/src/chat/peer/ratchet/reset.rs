//! Resets (spec 095 T029, contracts/nostr-chat-ratchet.md "`chat.reset`", "Receiving" steps 2 and
//! 6): when a ratchet message from a known device does not decrypt, this device drops every session
//! with it and asks it for a new one; the device that accepts the request drops its sessions too,
//! fetches the other's record again before it opens a new session, and encrypts again what still
//! waits for the other's vault. Both directions happen at most once per device and
//! [`RESET_EVERY_MS`], and only for a device on a stored card (contact, open request or own vault).
//!
//! `recovering_since` marks the time between a reset and the first event decrypted on a session
//! made after it ([`recovered`]).

use haex_crdt::CrdtTransaction;
use uuid::Uuid;

use super::store;
use super::RESET_EVERY_MS;
use crate::chat::peer::session::Me;
use crate::chat::peer::{outbox, targets};
use crate::error::Result;
use crate::sync::keys::hex;

/// A ratchet message from `peer_device` did not decrypt: when the device is on a stored card and no
/// reset went to it within [`RESET_EVERY_MS`], drops every session with it, notes the reset and
/// answers `true`: the caller sends the reset.
pub fn undecryptable(
    tx: &mut CrdtTransaction<'_>,
    me: &Me,
    peer_device: &[u8; 32],
    now: i64,
) -> Result<bool> {
    if targets::vault_of_device(tx, me, peer_device)?.is_none() {
        return Ok(false);
    }
    let device = hex(peer_device);
    let mut peer = store::peer(tx, me.installation, &device)?;
    if recent(peer.reset_sent_at, now) {
        return Ok(false);
    }
    store::delete_sessions(tx, me.installation, &device)?;
    peer.reset_sent_at = Some(now);
    peer.recovering_since = Some(now);
    store::put_peer(tx, me.installation, &device, &peer)?;
    Ok(true)
}

/// The reset to `peer_device` reached no Nostr-Relay: the next undecryptable event may send one.
pub fn not_sent(tx: &mut CrdtTransaction<'_>, me: &Me, peer_device: &[u8; 32]) -> Result<()> {
    let device = hex(peer_device);
    let mut peer = store::peer(tx, me.installation, &device)?;
    peer.reset_sent_at = None;
    store::put_peer(tx, me.installation, &device, &peer)
}

/// A reset from `peer_device` arrived: when the device is on a stored card and no reset from it was
/// accepted within [`RESET_EVERY_MS`], drops every session with it, asks for its record to be
/// fetched again, and has the outbox encrypt again what awaits its vault; answers whether it was
/// accepted.
pub fn received(
    tx: &mut CrdtTransaction<'_>,
    me: &Me,
    peer_device: &[u8; 32],
    now: i64,
) -> Result<bool> {
    let Some(vault) = targets::vault_of_device(tx, me, peer_device)? else {
        return Ok(false);
    };
    let device = hex(peer_device);
    let mut peer = store::peer(tx, me.installation, &device)?;
    if recent(peer.reset_received_at, now) {
        return Ok(false);
    }
    store::delete_sessions(tx, me.installation, &device)?;
    peer.reset_received_at = Some(now);
    peer.recovering_since = Some(now);
    peer.refetch_record = true;
    store::put_peer(tx, me.installation, &device, &peer)?;
    outbox::reseal_for_device(tx, &device, &vault, now)?;
    Ok(true)
}

/// An event decrypted on a session of `peer_device` (hex) created at `session_created_at`: a
/// recovery that began no later than that session is over.
pub fn recovered(
    tx: &mut CrdtTransaction<'_>,
    installation: Uuid,
    peer_device: &str,
    session_created_at: i64,
) -> Result<()> {
    let mut peer = store::peer(tx, installation, peer_device)?;
    if peer
        .recovering_since
        .is_some_and(|since| session_created_at >= since)
    {
        peer.recovering_since = None;
        store::put_peer(tx, installation, peer_device, &peer)?;
    }
    Ok(())
}

fn recent(at: Option<i64>, now: i64) -> bool {
    at.is_some_and(|at| now - at < RESET_EVERY_MS)
}

#[cfg(test)]
#[path = "reset_tests.rs"]
mod tests;
