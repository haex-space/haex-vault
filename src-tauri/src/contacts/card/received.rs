//! Visitenkarten read in, as rows of `haex_contacts_cards` (spec 051 FR-008, FR-012, FR-013): one
//! card per vault identity, on one contact; its devices and relays are the contact's confirmed
//! holzi addresses. Blocked vaults (`haex_contacts_blocked_vaults`) live here too.

use std::collections::BTreeSet;

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use serde::Serialize;
use ts_rs::TS;

use super::format::{self, CardDevice, SignedCard};
use crate::error::Result;
use crate::passwords::clock;
use crate::storage::query::Query;
use crate::sync::keys::hex;

/// The confirmed holzi addresses of a contact, from its Visitenkarte (FR-008, FR-009).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ContactCardInfo {
    /// The vault identity, hex.
    pub vault_pubkey: String,
    /// The vault identity as `npub1…`.
    pub npub: String,
    /// The name the card carries.
    pub name: String,
    pub issued_at: String,
    pub devices: Vec<ContactCardDevice>,
    pub nostr_relays: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/types/bindings/")]
#[serde(rename_all = "camelCase")]
pub struct ContactCardDevice {
    pub device_pubkey: String,
    /// The iroh endpoint id, hex.
    pub endpoint_id: String,
    pub iroh_relay: Option<String>,
}

/// `npub1…` of a vault identity.
pub fn npub(vault: &[u8; 32]) -> String {
    use nostr::nips::nip19::ToBech32;
    nostr::key::PublicKey::from_slice(vault)
        .ok()
        .and_then(|key| key.to_bech32().ok())
        .unwrap_or_else(|| hex(vault))
}

/// When a card was made, as the window shows times.
pub fn issued_text(card: &SignedCard) -> String {
    clock::format_millis(i64::try_from(card.card.issued_at.saturating_mul(1000)).unwrap_or(0))
}

pub fn info(card: &SignedCard) -> ContactCardInfo {
    ContactCardInfo {
        vault_pubkey: hex(&card.card.vault),
        npub: npub(&card.card.vault),
        name: card.card.name.clone(),
        issued_at: issued_text(card),
        devices: card
            .card
            .devices
            .iter()
            .map(|device| ContactCardDevice {
                device_pubkey: hex(&device.device_pubkey),
                endpoint_id: hex(&device.endpoint_id),
                iroh_relay: device.iroh_relay.clone(),
            })
            .collect(),
        nostr_relays: card.card.nostr_relays.clone(),
    }
}

/// A row of `haex_contacts_cards` as the database holds it: key, vault, contact and card bytes.
type RawRow = (String, String, String, Vec<u8>);

const SELECT_ROWS: &str = "SELECT hash, vault_pubkey, contact_id, card FROM haex_contacts_cards";

fn raw_row(r: &haex_crdt::rusqlite::Row<'_>) -> haex_crdt::rusqlite::Result<RawRow> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
}

/// A stored card is trusted only when its row fits its content (spec 060 FR-025a): the format, the
/// limits and the signature hold, the key is the hash of the signed content and the vault of the row
/// is the vault the card names. A row that does not fit is skipped and logged, so a card of one
/// vault can never be listed under another.
fn verified(row: RawRow) -> Option<(String, SignedCard)> {
    let (hash, vault, contact_id, bytes) = row;
    let card = match format::from_bytes(bytes) {
        Ok(card) => card,
        Err(error) => {
            log::warn!("contacts: a stored card is not valid and is ignored: {error}");
            return None;
        }
    };
    if card.hash() != hash || hex(&card.card.vault) != vault {
        log::warn!("contacts: a stored card does not fit its row and is ignored");
        return None;
    }
    Some((contact_id, card))
}

/// Newest first: the highest device list generation, then the later `issued_at`, then the smaller
/// key, so every device picks the same card.
fn recency(card: &SignedCard) -> (u64, u64, std::cmp::Reverse<String>) {
    (
        card.card.list_generation,
        card.card.issued_at,
        std::cmp::Reverse(card.hash()),
    )
}

fn newest_first(mut cards: Vec<(String, SignedCard)>) -> Vec<(String, SignedCard)> {
    cards.sort_by_key(|(_, card)| std::cmp::Reverse(recency(card)));
    cards
}

/// The verified cards of a vault, newest first, each with its contact.
fn cards_of_vault(q: &mut impl Query, vault: &[u8; 32]) -> Result<Vec<(String, SignedCard)>> {
    let rows = q.query_map(
        &format!("{SELECT_ROWS} WHERE vault_pubkey = ?1"),
        params![hex(vault)],
        raw_row,
    )?;
    Ok(newest_first(
        rows.into_iter().filter_map(verified).collect(),
    ))
}

/// The newest verified card of a contact, if it has one.
pub fn stored_of_contact(q: &mut impl Query, contact_id: &str) -> Result<Option<SignedCard>> {
    let rows = q.query_map(
        &format!("{SELECT_ROWS} WHERE contact_id = ?1"),
        params![contact_id],
        raw_row,
    )?;
    Ok(
        newest_first(rows.into_iter().filter_map(verified).collect())
            .into_iter()
            .next()
            .map(|(_, card)| card),
    )
}

/// The holzi addresses of a vault: the devices and Nostr-Relays of all its stored cards together,
/// each once. A device keeps the iroh relay of the newest card that names it. Added addresses only
/// serve to dial; the newest card alone sets the threshold for a device list (`foreign_trust`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Addresses {
    pub devices: Vec<CardDevice>,
    pub nostr_relays: Vec<String>,
}

fn addresses_of(cards: &[(String, SignedCard)]) -> Addresses {
    let mut found = Addresses::default();
    for (_, card) in cards {
        for device in &card.card.devices {
            if !found
                .devices
                .iter()
                .any(|known| known.endpoint_id == device.endpoint_id)
            {
                found.devices.push(device.clone());
            }
        }
        for relay in &card.card.nostr_relays {
            if !found.nostr_relays.contains(relay) {
                found.nostr_relays.push(relay.clone());
            }
        }
    }
    found
}

/// The addresses of a vault from all its stored cards (empty without a card).
pub fn addresses(q: &mut impl Query, vault: &[u8; 32]) -> Result<Addresses> {
    Ok(addresses_of(&cards_of_vault(q, vault)?))
}

/// The card of a contact for the window: the newest card, with the addresses of all its cards.
pub fn of(q: &mut impl Query, contact_id: &str) -> Result<Option<ContactCardInfo>> {
    let Some(newest) = stored_of_contact(q, contact_id)? else {
        return Ok(None);
    };
    let all = addresses(q, &newest.card.vault)?;
    let mut shown = info(&newest);
    shown.devices = all
        .devices
        .iter()
        .map(|device| ContactCardDevice {
            device_pubkey: hex(&device.device_pubkey),
            endpoint_id: hex(&device.endpoint_id),
            iroh_relay: device.iroh_relay.clone(),
        })
        .collect();
    shown.nostr_relays = all.nostr_relays;
    Ok(Some(shown))
}

/// The contacts that have a card.
pub fn with_cards(q: &mut impl Query) -> Result<BTreeSet<String>> {
    Ok(q.query_map(SELECT_ROWS, &[], raw_row)?
        .into_iter()
        .filter_map(verified)
        .map(|(contact_id, _)| contact_id)
        .collect())
}

/// The keys of the cards stored on a contact, sorted; they tell whether its addresses changed.
pub fn hashes_of_contact(q: &mut impl Query, contact_id: &str) -> Result<Vec<String>> {
    Ok(q.query_map(
        "SELECT hash FROM haex_contacts_cards WHERE contact_id = ?1 ORDER BY hash",
        params![contact_id],
        |r| r.get(0),
    )?)
}

/// The newest stored card of a vault: the contact it is on and the card.
pub fn stored(q: &mut impl Query, vault: &[u8; 32]) -> Result<Option<(String, SignedCard)>> {
    Ok(cards_of_vault(q, vault)?.into_iter().next())
}

/// Stores a card on a contact next to the cards the vault already has. The key is the hash of the
/// signed content, so storing the same card again is no change; answers whether a row was added.
pub fn save(tx: &mut CrdtTransaction<'_>, contact_id: &str, card: &SignedCard) -> Result<bool> {
    let hash = card.hash();
    let present: Option<i64> = tx.query_row(
        "SELECT 1 FROM haex_contacts_cards WHERE hash = ?1",
        params![hash],
        |r| r.get(0),
    )?;
    if present.is_some() {
        return Ok(false);
    }
    tx.execute(
        "INSERT INTO haex_contacts_cards (hash, vault_pubkey, contact_id, card, issued_at, \
         received_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            hash,
            hex(&card.card.vault),
            contact_id,
            card.bytes,
            issued_text(card),
            clock::now()
        ],
    )?;
    Ok(true)
}

/// Removes the stored cards of a vault.
pub fn delete_for_vault(tx: &mut CrdtTransaction<'_>, vault: &[u8; 32]) -> Result<()> {
    tx.execute(
        "DELETE FROM haex_contacts_cards WHERE vault_pubkey = ?1",
        params![hex(vault)],
    )?;
    Ok(())
}

/// Stores a card on a contact in place of the vault's earlier cards (spec 051 FR-012: a newer card
/// replaces the confirmed addresses). Only a plain business card is read this way; a share adds.
pub fn replace(tx: &mut CrdtTransaction<'_>, contact_id: &str, card: &SignedCard) -> Result<()> {
    delete_for_vault(tx, &card.card.vault)?;
    save(tx, contact_id, card)?;
    Ok(())
}

/// A blocked vault (FR-039).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsBlockedVault"
)]
#[serde(rename_all = "camelCase")]
pub struct BlockedVault {
    pub vault_pubkey: String,
    pub npub: String,
    pub blocked_at: String,
}

pub fn blocked(q: &mut impl Query) -> Result<Vec<BlockedVault>> {
    let rows: Vec<(String, String)> = q.query_map(
        "SELECT vault_pubkey, blocked_at FROM haex_contacts_blocked_vaults ORDER BY blocked_at",
        &[],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(rows
        .into_iter()
        .map(|(vault_pubkey, blocked_at)| {
            let bytes = data_encoding::HEXLOWER.decode(vault_pubkey.as_bytes()).ok();
            let npub = bytes
                .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
                .map_or_else(|| vault_pubkey.clone(), |key| npub(&key));
            BlockedVault {
                vault_pubkey,
                npub,
                blocked_at,
            }
        })
        .collect())
}

pub fn is_blocked(q: &mut impl Query, vault: &[u8; 32]) -> Result<bool> {
    let found: Option<i64> = q.query_row(
        "SELECT 1 FROM haex_contacts_blocked_vaults WHERE vault_pubkey = ?1",
        params![hex(vault)],
        |r| r.get(0),
    )?;
    Ok(found.is_some())
}

pub fn block(tx: &mut CrdtTransaction<'_>, vault_pubkey: &str) -> Result<()> {
    let present: Option<i64> = tx.query_row(
        "SELECT 1 FROM haex_contacts_blocked_vaults WHERE vault_pubkey = ?1",
        params![vault_pubkey],
        |r| r.get(0),
    )?;
    if present.is_none() {
        tx.execute(
            "INSERT INTO haex_contacts_blocked_vaults (vault_pubkey, blocked_at) VALUES (?1, ?2)",
            params![vault_pubkey, clock::now()],
        )?;
    }
    Ok(())
}

pub fn unblock(tx: &mut CrdtTransaction<'_>, vault_pubkey: &str) -> Result<()> {
    tx.execute(
        "DELETE FROM haex_contacts_blocked_vaults WHERE vault_pubkey = ?1",
        params![vault_pubkey],
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "received_tests.rs"]
mod tests;
