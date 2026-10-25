//! The iroh home relay each device of the vault is reached at, as the vault's own Visitenkarte
//! names it (spec 051 FR-009, FR-010; spec 057 research R5).
//!
//! The card takes a device's relay from `device_presence_no_sync`, where a device is recorded when
//! its presence meeting is heard. A device never hears itself, so the issuer's own relay is
//! recorded here from its own endpoint instead, and a relay that changed renews the card, for
//! every device of the vault alike: a vault of one device is no special case, since further
//! devices can join later.

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use iroh::{EndpointAddr, TransportAddr};

use crate::contacts::card::own_card;
use crate::sync::presence::record_seen;
use crate::sync::replica::Replica;

/// The home relay `addr` names, as the card writes it.
fn relay_of(addr: &EndpointAddr) -> Option<String> {
    addr.relay_urls().next().map(ToString::to_string)
}

/// The endpoint address stored for `device`.
fn stored_addr(
    tx: &mut CrdtTransaction<'_>,
    device: &[u8; 32],
) -> haex_crdt::Result<Option<EndpointAddr>> {
    let stored: Option<Vec<u8>> = tx
        .query_row(
            "SELECT endpoint_addr FROM device_presence_no_sync WHERE device_pubkey = ?1",
            params![device.as_slice()],
            |r| r.get::<_, Option<Vec<u8>>>(0),
        )?
        .flatten();
    Ok(stored.and_then(|bytes| postcard::from_bytes::<EndpointAddr>(&bytes).ok()))
}

/// [`record_seen`] for a device of the vault, and a new own card when `addr` names a home relay
/// other than the stored one. Returns whether the card was renewed (on a main device; a linked
/// device cannot sign).
pub(crate) fn record_device(
    tx: &mut CrdtTransaction<'_>,
    device: &[u8; 32],
    last_seen_ms: u64,
    addr: Option<&EndpointAddr>,
) -> haex_crdt::Result<bool> {
    let named = addr.and_then(relay_of);
    let stored = stored_addr(tx, device)?;
    let before = stored.as_ref().and_then(relay_of);
    // A presence meeting may contain fresh direct addresses without a relay. Keep the last
    // known relay in that address, and keep the complete last address when no address was named.
    let preserved = match addr {
        Some(addr) if named.is_none() => stored.as_ref().and_then(|old| {
            old.relay_urls().next().map(|relay| {
                EndpointAddr::from_parts(
                    addr.id,
                    addr.addrs
                        .iter()
                        .cloned()
                        .chain([TransportAddr::Relay(relay.clone())]),
                )
            })
        }),
        None => stored.clone(),
        _ => None,
    };
    record_seen(tx, device, last_seen_ms, preserved.as_ref().or(addr))?;
    if named.is_none() || named == before {
        return Ok(false);
    }
    // Keep the presence write and the card renewal in one transaction. If card creation fails,
    // rolling back the relay write leaves the next meeting able to retry the renewal.
    own_card::refresh(tx)?;
    Ok(true)
}

/// Records this device's own address once its endpoint has reached a home relay. An address
/// without one is left alone, so a relay that is down for a moment does not take the relay off the
/// card. Returns whether the home relay changed, which renewed the card; the caller then tells the
/// sync sessions, because this write does not go through `VaultDb`.
pub(crate) fn note_own(
    replica: &Replica,
    own: &[u8; 32],
    addr: &EndpointAddr,
) -> haex_crdt::Result<bool> {
    if relay_of(addr).is_none() {
        return Ok(false);
    }
    // Never "seen": the device's own row carries only its address, not an online time.
    replica
        .db()
        .write(|tx| record_device(tx, own, 0, Some(addr)))
}

#[cfg(test)]
#[path = "home_relay_tests.rs"]
mod tests;
