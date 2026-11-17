//! Where a rumor for a vault goes (spec 072 contracts/nostr-chat.md "Publishing", FR-016, FR-082):
//! one wrap per device on the vault's stored cards, to the Nostr-Relays on those cards or, when
//! they name none, to the own Nostr-Relays (US2 scenario 8). The own vault stands for the own other
//! devices: the current own device list without this device and without removed devices, reached
//! over the own Nostr-Relays.

use uuid::Uuid;

use super::session::Me;
use super::verify::vault_bytes;
use super::{ids, store};
use crate::contacts::card::format::CardDevice;
use crate::contacts::card::{own_card, received};
use crate::error::Result;
use crate::storage::query::Query;
use crate::sync::keys::hex;

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
    // Nothing of any kind goes to a blocked vault, also no receipt queued before the block (FR-042).
    if received::is_blocked(q, &bytes)? {
        return Ok(Target::default());
    }
    let mut addresses = received::addresses(q, &bytes)?;
    if addresses.devices.is_empty() {
        // A request: the sender is no contact, so its receipts go to the card it carried (US5
        // scenario 6: it sees _delivered_).
        let thread_id = ids::direct_thread_id(&me.vault_hex, vault);
        if let Some(card) = store::request_card(q, thread_id)? {
            addresses = received::Addresses {
                devices: card.card.devices,
                nostr_relays: card.card.nostr_relays,
            };
        }
    }
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

/// The vault (hex) on whose stored card `device` stands as a device (spec 095): the own vault for
/// the own other devices on the current list, a contact's vault for a device on its stored cards,
/// or the vault of an open request for a device on the card it carried. `None` for this device,
/// for a device no card lists (also a removed one) and for a blocked vault. The devices are the
/// ones [`of`] sends to.
pub fn vault_of_device(q: &mut impl Query, me: &Me, device: &[u8; 32]) -> Result<Option<String>> {
    if *device == me.keys.device_pubkey {
        return Ok(None);
    }
    let lists =
        |devices: &[CardDevice]| devices.iter().any(|listed| listed.device_pubkey == *device);
    if let Some(own) = own_card::addresses(q)? {
        if lists(&own.devices) {
            return Ok(Some(me.vault_hex.clone()));
        }
    }
    for contact in received::with_cards(q)? {
        let Some(card) = received::stored_of_contact(q, &contact)? else {
            continue;
        };
        let vault = card.card.vault;
        if lists(&received::addresses(q, &vault)?.devices) {
            return Ok(if received::is_blocked(q, &vault)? {
                None
            } else {
                Some(hex(&vault))
            });
        }
    }
    let requests: Vec<String> = q.query_map(
        "SELECT id FROM chat_threads WHERE kind = 'direct' AND state = 'request'",
        &[],
        |row| row.get(0),
    )?;
    for thread in requests {
        let Ok(thread) = Uuid::parse_str(&thread) else {
            continue;
        };
        let Some(card) = store::request_card(q, thread)? else {
            continue;
        };
        if lists(&card.card.devices) {
            return Ok(if received::is_blocked(q, &card.card.vault)? {
                None
            } else {
                Some(hex(&card.card.vault))
            });
        }
    }
    Ok(None)
}

#[cfg(test)]
#[path = "targets_tests.rs"]
mod tests;
