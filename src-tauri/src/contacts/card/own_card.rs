//! The vault's own Visitenkarte (spec 051 FR-009, FR-010, research R12; spec 060). Only a main device
//! holds the vault secret and signs one. A main device keeps the newest card in the synced
//! `haex_contacts_own_card` (made when the device list or the Nostr-Relays change); a share of the
//! own contact signs a fresh card with only the devices and relays the person chose and stores
//! nothing (spec 060 FR-008, FR-016).

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;

use super::format::{self, CardDevice, ContactCard, SignedCard, VERSION};
use crate::contacts::model::Head;
use crate::contacts::share::name::name_for;
use crate::contacts::{ids, store as contacts_store};
use crate::error::{HolziError, Result};
use crate::passwords::clock;
use crate::storage::query::Query;
use crate::sync::{device_list, keys, presence, servers};

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
}

/// The stored own card, decoded.
pub(crate) fn stored(q: &mut impl Query) -> Result<Option<format::SignedCard>> {
    let bytes: Option<Vec<u8>> = q.query_row(
        "SELECT card FROM haex_contacts_own_card WHERE id = 1",
        &[],
        |r| r.get(0),
    )?;
    Ok(bytes.and_then(|bytes| format::from_bytes(bytes).ok()))
}

/// The stored own card as `holzi:card:` text, for tests that hand it to another vault.
#[cfg(test)]
pub(crate) fn stored_text(q: &mut impl Query) -> Result<Option<String>> {
    Ok(stored(q)?.map(|card| card.text()))
}

/// What a card of this vault can name: the identity, the effective device list's generation, the
/// devices and the Nostr-Relays, as they are now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Addresses {
    pub vault: [u8; 32],
    pub list_generation: u64,
    pub devices: Vec<CardDevice>,
    /// The `vault_device_uuid` of each device, in the same order.
    pub device_uuids: Vec<String>,
    pub nostr_relays: Vec<String>,
}

/// The addresses a card of this vault carries; `None` while the vault has no identity or no valid
/// device list yet.
pub fn addresses(q: &mut impl Query) -> Result<Option<Addresses>> {
    let Some(vault) = keys::vault_pubkey(q)? else {
        return Ok(None);
    };
    let rows = device_list::load_all(q)?;
    let valid = device_list::valid_lists(&rows, &vault);
    let Some(list) = device_list::effective(&valid) else {
        return Ok(None);
    };
    let relays: BTreeMap<[u8; 32], String> = presence::load_all(q)?
        .into_iter()
        .filter_map(|seen| {
            let relay = seen.endpoint_addr?.relay_urls().next()?.to_string();
            Some((seen.device_pubkey, relay))
        })
        .collect();
    let devices = list
        .list
        .devices
        .iter()
        .map(|device| CardDevice {
            device_pubkey: device.device_pubkey,
            endpoint_id: device.endpoint_id,
            iroh_relay: relays.get(&device.device_pubkey).cloned(),
        })
        .collect();
    let mut nostr_relays = servers::read(q)?.effective_nostr_relays();
    nostr_relays.truncate(format::MAX_RELAYS);
    Ok(Some(Addresses {
        vault,
        list_generation: list.list.generation,
        devices,
        device_uuids: list
            .list
            .devices
            .iter()
            .map(|device| device.vault_device_uuid.to_string())
            .collect(),
        nostr_relays,
    }))
}

fn invalid(reason: &str) -> HolziError {
    HolziError::ShareInvalid {
        reason: reason.to_owned(),
    }
}

/// The card of this vault with the devices and relays named, or with all of them for `None`. A
/// device or relay the vault does not have is [`HolziError::ShareInvalid`]. The card carries no
/// values; its `name` is the name of a share of the own contact (spec 060 FR-011). `None` while
/// the vault has no addresses yet.
pub fn build_card(
    q: &mut impl Query,
    devices: Option<&[[u8; 32]]>,
    relays: Option<&[String]>,
) -> Result<Option<ContactCard>> {
    let Some(known) = addresses(q)? else {
        return Ok(None);
    };
    let chosen_devices = match devices {
        None => known.devices.clone(),
        Some(wanted) => wanted
            .iter()
            .map(|key| {
                known
                    .devices
                    .iter()
                    .find(|device| &device.device_pubkey == key)
                    .cloned()
                    .ok_or_else(|| invalid("a device is not on the device list"))
            })
            .collect::<Result<Vec<_>>>()?,
    };
    let chosen_relays = match relays {
        None => known.nostr_relays.clone(),
        Some(wanted) => wanted
            .iter()
            .map(|url| {
                known
                    .nostr_relays
                    .contains(url)
                    .then(|| url.clone())
                    .ok_or_else(|| invalid("a relay is not one of the vault's"))
            })
            .collect::<Result<Vec<_>>>()?,
    };
    // The own contact is there in every vault; a missing one must not stop a card.
    let head = match contacts_store::load(q, ids::SELF_CONTACT_ID) {
        Ok(contact) => contact.head,
        Err(HolziError::ContactsNotFound) => Head::default(),
        Err(error) => return Err(error),
    };
    let (name, _) = name_for(&head, Some(known.vault));
    Ok(Some(ContactCard {
        version: VERSION,
        vault: known.vault,
        issued_at: 0,
        list_generation: known.list_generation,
        devices: chosen_devices,
        nostr_relays: chosen_relays,
        name,
        values: Vec::new(),
    }))
}

/// A card for one share: signed now with only the devices and relays chosen, never stored, and
/// without the 2,048 characters of a QR code of its own (the limit is that of the whole share). A
/// linked device has no vault secret and gives `None`.
pub fn build_for_share(
    q: &mut impl Query,
    devices: &[[u8; 32]],
    relays: &[String],
) -> Result<Option<SignedCard>> {
    let Some(secret) = keys::vault_secret(q)? else {
        return Ok(None);
    };
    let Some(mut card) = build_card(q, Some(devices), Some(relays))? else {
        return Ok(None);
    };
    // A later `issued_at` than any card this vault made before, so a receiver never takes it for
    // an older one (spec 051 FR-012).
    let previous = stored(q)?.map_or(0, |card| card.card.issued_at);
    card.issued_at = unix_seconds().max(previous + 1);
    let (signed, _) = format::sign_with(card, &secret, None)?;
    Ok(Some(signed))
}

/// Makes and stores a new own card, on a main device; a linked device does nothing.
pub fn refresh(tx: &mut CrdtTransaction<'_>) -> Result<()> {
    let Some(secret) = keys::vault_secret(tx)? else {
        return Ok(());
    };
    let Some(mut card) = build_card(tx, None, None)? else {
        return Ok(());
    };
    // A newer card replaces an older one only with a later `issued_at` (FR-012).
    let previous = stored(tx)?.map_or(0, |card| card.card.issued_at);
    let issued_at = unix_seconds().max(previous + 1);
    card.issued_at = issued_at;
    let (signed, _omitted) = format::sign(card, &secret)?;
    let at = clock::format_millis(i64::try_from(issued_at.saturating_mul(1000)).unwrap_or(0));
    let updated = tx.execute(
        "UPDATE haex_contacts_own_card SET card = ?1, issued_at = ?2 WHERE id = 1",
        params![signed.bytes, at],
    )?;
    if updated == 0 {
        tx.execute(
            "INSERT INTO haex_contacts_own_card (id, card, issued_at) VALUES (1, ?1, ?2)",
            params![signed.bytes, at],
        )?;
    }
    Ok(())
}

/// [`refresh`] for the places that sign a device list or change the relays: a card that cannot be
/// made is logged and never stops the change it follows.
pub fn refresh_quietly(tx: &mut CrdtTransaction<'_>) {
    if let Err(error) = refresh(tx) {
        log::warn!("contacts: the own business card was not renewed: {error}");
    }
}

#[cfg(test)]
#[path = "own_card_tests.rs"]
mod tests;
