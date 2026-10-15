//! The vault's own Visitenkarte (spec 051 FR-009, FR-010, research R12). Only a main device holds
//! the vault secret and signs it; the newest card lies in the synced `haex_contacts_own_card`, so a
//! linked device shows it too. A main device makes a new card when the device list changes (each
//! place that signs a list calls [`refresh`] in the same transaction), when the Nostr-Relays
//! change and when the person changes what the card carries.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::format::{self, CardDevice, CardValue, ContactCard, VERSION};
use crate::contacts::kinds;
use crate::contacts::store as contacts_store;
use crate::error::{HolziError, Result};
use crate::passwords::clock;
use crate::storage::preferences::{self, PrefScope};
use crate::storage::query::Query;
use crate::sync::{device_list, keys, presence, servers};

/// The vault setting with the id of the person's own contact.
pub const PREF_OWN_CONTACT: &str = "contacts.card.own_contact";
/// The vault setting with the ids of the own contact's values the card carries (JSON list).
pub const PREF_VALUES: &str = "contacts.card.values";

/// The own card as the window shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsOwnCard"
)]
#[serde(rename_all = "camelCase")]
pub struct OwnCard {
    /// The card as text; `None` before a main device made one.
    pub text: Option<String>,
    pub qr_svg: Option<String>,
    pub issued_at: Option<String>,
    /// This device holds the vault secret and makes the card.
    pub can_sign: bool,
    /// Chosen values that did not fit the card.
    pub omitted: Vec<String>,
    /// The person's own contact and the values the card carries.
    pub own_contact: Option<String>,
    pub value_ids: Vec<String>,
}

/// What the card carries, as the person chose.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    pub own_contact: Option<String>,
    pub value_ids: Vec<String>,
}

pub fn choice(q: &mut impl Query) -> Result<Choice> {
    let own_contact = preferences::get(q, PrefScope::Vault, PREF_OWN_CONTACT)?;
    let value_ids = preferences::get(q, PrefScope::Vault, PREF_VALUES)?
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();
    Ok(Choice {
        own_contact,
        value_ids,
    })
}

pub fn set_choice(tx: &mut CrdtTransaction<'_>, choice: &Choice) -> Result<()> {
    match &choice.own_contact {
        Some(id) => {
            preferences::insert_or_update(tx, PrefScope::Vault, PREF_OWN_CONTACT, id)?;
        }
        None => {
            preferences::delete(tx, PrefScope::Vault, PREF_OWN_CONTACT)?;
        }
    }
    let json = serde_json::to_string(&choice.value_ids).unwrap_or_default();
    preferences::insert_or_update(tx, PrefScope::Vault, PREF_VALUES, &json)?;
    Ok(())
}

/// Name and values of the own contact for the card; empty without one.
fn own_part(q: &mut impl Query, choice: &Choice) -> Result<(String, Vec<CardValue>)> {
    let Some(id) = &choice.own_contact else {
        return Ok((String::new(), Vec::new()));
    };
    let contact = match contacts_store::load(q, id) {
        Ok(contact) => contact,
        Err(HolziError::ContactsNotFound) => return Ok((String::new(), Vec::new())),
        Err(error) => return Err(error),
    };
    let values = choice
        .value_ids
        .iter()
        .filter_map(|wanted| contact.values.iter().find(|value| &value.id == wanted))
        .filter(|value| kinds::is_known(&value.kind) && !kinds::is_holzi_address(&value.kind))
        .map(|value| CardValue {
            kind: value.kind.clone(),
            label: value.label.clone(),
            value: value.value.clone(),
        })
        .collect();
    Ok((contact.name, values))
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
}

/// The stored own card, decoded.
fn stored(q: &mut impl Query) -> Result<Option<format::SignedCard>> {
    let bytes: Option<Vec<u8>> = q.query_row(
        "SELECT card FROM haex_contacts_own_card WHERE id = 1",
        &[],
        |r| r.get(0),
    )?;
    Ok(bytes.and_then(|bytes| format::from_bytes(bytes).ok()))
}

/// Makes and stores a new own card, on a main device; a linked device does nothing.
pub fn refresh(tx: &mut CrdtTransaction<'_>) -> Result<()> {
    let Some(secret) = keys::vault_secret(tx)? else {
        return Ok(());
    };
    let Some(vault) = keys::vault_pubkey(tx)? else {
        return Ok(());
    };
    let rows = device_list::load_all(tx)?;
    let valid = device_list::valid_lists(&rows, &vault);
    let Some(list) = device_list::effective(&valid) else {
        return Ok(());
    };
    let relays: BTreeMap<[u8; 32], String> = presence::load_all(tx)?
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
    let mut nostr_relays = servers::read(tx)?.effective_nostr_relays();
    nostr_relays.truncate(format::MAX_RELAYS);
    let chosen = choice(tx)?;
    let (name, values) = own_part(tx, &chosen)?;
    // A newer card replaces an older one only with a later `issued_at` (FR-012).
    let previous = stored(tx)?.map_or(0, |card| card.card.issued_at);
    let issued_at = unix_seconds().max(previous + 1);
    let card = ContactCard {
        version: VERSION,
        vault,
        issued_at,
        list_generation: list.list.generation,
        devices,
        nostr_relays,
        name,
        values,
    };
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

/// The own card as the window shows it.
pub fn read(q: &mut impl Query) -> Result<OwnCard> {
    let can_sign = keys::vault_secret(q)?.is_some();
    let choice = choice(q)?;
    let card = stored(q)?;
    let (_, chosen) = own_part(q, &choice)?;
    let (text, qr_svg, issued_at, omitted) = match card {
        Some(card) => {
            let text = card.text();
            let omitted = chosen
                .into_iter()
                .filter(|value| !card.card.values.contains(value))
                .map(|value| value.value)
                .collect();
            let at = clock::format_millis(
                i64::try_from(card.card.issued_at.saturating_mul(1000)).unwrap_or(0),
            );
            (
                Some(text.clone()),
                Some(format::qr_svg(&text)?),
                Some(at),
                omitted,
            )
        }
        None => (None, None, None, Vec::new()),
    };
    Ok(OwnCard {
        text,
        qr_svg,
        issued_at,
        can_sign,
        omitted,
        own_contact: choice.own_contact,
        value_ids: choice.value_ids,
    })
}

#[cfg(test)]
#[path = "own_card_tests.rs"]
mod tests;
