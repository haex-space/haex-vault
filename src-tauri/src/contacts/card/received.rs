//! Visitenkarten read in, as rows of `haex_contacts_cards` (spec 051 FR-008, FR-012, FR-013): one
//! card per vault identity, on one contact; its devices and relays are the contact's confirmed
//! holzi addresses. Blocked vaults (`haex_contacts_blocked_vaults`) live here too.

use std::collections::BTreeSet;

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use serde::Serialize;
use ts_rs::TS;

use super::format::{self, SignedCard};
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

/// The card of a contact, if it has one.
pub fn of(q: &mut impl Query, contact_id: &str) -> Result<Option<ContactCardInfo>> {
    let bytes: Option<Vec<u8>> = q.query_row(
        "SELECT card FROM haex_contacts_cards WHERE contact_id = ?1",
        params![contact_id],
        |r| r.get(0),
    )?;
    Ok(bytes
        .and_then(|bytes| format::from_bytes(bytes).ok())
        .map(|card| info(&card)))
}

/// The contacts that have a card.
pub fn with_cards(q: &mut impl Query) -> Result<BTreeSet<String>> {
    Ok(
        q.query_map("SELECT contact_id FROM haex_contacts_cards", &[], |r| {
            r.get(0)
        })?
        .into_iter()
        .collect(),
    )
}

/// The stored card of a vault: the contact it is on and the card.
pub fn stored(q: &mut impl Query, vault: &[u8; 32]) -> Result<Option<(String, SignedCard)>> {
    let row: Option<(String, Vec<u8>)> = q.query_row(
        "SELECT contact_id, card FROM haex_contacts_cards WHERE vault_pubkey = ?1",
        params![hex(vault)],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(row.and_then(|(contact, bytes)| Some((contact, format::from_bytes(bytes).ok()?))))
}

/// Stores a card on a contact, replacing the vault's earlier card.
pub fn save(tx: &mut CrdtTransaction<'_>, contact_id: &str, card: &SignedCard) -> Result<()> {
    let (vault, issued, now) = (hex(&card.card.vault), issued_text(card), clock::now());
    let updated = tx.execute(
        "UPDATE haex_contacts_cards SET contact_id = ?2, card = ?3, issued_at = ?4, \
         received_at = ?5 WHERE vault_pubkey = ?1",
        params![vault, contact_id, card.bytes, issued, now],
    )?;
    if updated == 0 {
        tx.execute(
            "INSERT INTO haex_contacts_cards (vault_pubkey, contact_id, card, issued_at, \
             received_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![vault, contact_id, card.bytes, issued, now],
        )?;
    }
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
