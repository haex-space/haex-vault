//! Where a rumor for a vault goes (spec 072 contracts/nostr-chat.md "Publishing", FR-016, FR-082):
//! one wrap per device on the vault's stored cards, to the Nostr-Relays on those cards or, when
//! they name none, to the own Nostr-Relays (US2 scenario 8). The own vault stands for the own other
//! devices: the current own device list without this device and without removed devices, reached
//! over the own Nostr-Relays.

use super::session::Me;
use super::verify::vault_bytes;
use crate::contacts::card::{own_card, received};
use crate::error::Result;
use crate::storage::query::Query;

/// The devices and Nostr-Relays of one recipient vault.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Target {
    pub devices: Vec<[u8; 32]>,
    pub relays: Vec<String>,
    /// The vault's cards name no Nostr-Relay, so the own ones stand in (shown as a hint).
    pub own_relays_instead: bool,
}

/// The Nostr-Relays on this device's own card.
pub fn own_relays(q: &mut impl Query) -> Result<Vec<String>> {
    Ok(own_card::addresses(q)?
        .map(|addresses| addresses.nostr_relays)
        .unwrap_or_default())
}

/// Relays this vault reads for incoming wraps: its own, plus the stored contacts' relays. The
/// latter are needed when a sender falls back to its own relays because this card names none.
pub fn listening_relays(q: &mut impl Query) -> Result<Vec<String>> {
    let mut relays = own_relays(q)?;
    for contact in received::with_cards(q)? {
        let Some(card) = received::stored_of_contact(q, &contact)? else {
            continue;
        };
        for relay in received::addresses(q, &card.card.vault)?.nostr_relays {
            if !relays.contains(&relay) {
                relays.push(relay);
            }
        }
    }
    Ok(relays)
}

/// The target of `vault` (hex) for `me`.
pub fn of(q: &mut impl Query, me: &Me, vault: &str) -> Result<Target> {
    if vault == me.vault_hex {
        let Some(addresses) = own_card::addresses(q)? else {
            return Ok(Target::default());
        };
        return Ok(Target {
            devices: addresses
                .devices
                .iter()
                .map(|device| device.device_pubkey)
                .filter(|device| *device != me.keys.device_pubkey)
                .collect(),
            relays: addresses.nostr_relays,
            own_relays_instead: false,
        });
    }
    let Some(bytes) = vault_bytes(vault) else {
        return Ok(Target::default());
    };
    let addresses = received::addresses(q, &bytes)?;
    let devices = addresses
        .devices
        .iter()
        .map(|device| device.device_pubkey)
        .collect();
    if addresses.nostr_relays.is_empty() {
        return Ok(Target {
            devices,
            relays: own_relays(q)?,
            own_relays_instead: true,
        });
    }
    Ok(Target {
        devices,
        relays: addresses.nostr_relays,
        own_relays_instead: false,
    })
}
