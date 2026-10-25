//! What this vault knows of the shares its contacts gave it (spec 056 data-model.md, FR-030, FR-031):
//! one row per share, written from the answer of the device that serves it. An answer replaces only
//! what that device said before, so the shares of a contact's other devices stay.

use haex_crdt::rusqlite::{params, Row};
use haex_crdt::CrdtTransaction;
use uuid::Uuid;

use super::{FileShareItemKind, FileShareLevel};
use crate::error::Result;
use crate::files::peer::wire::WireShare;
use crate::storage::query::Query;

/// Namespace of received share ids: `UUIDv5(NS, owner vault "\n" share id)`.
const NS_RECEIVED: Uuid = Uuid::from_u128(0x6a0e_93c2_17d4_4b58_a3f1_5c82_e09b_d164);

const COLUMNS: &str = "id, owner_vault_pubkey, share_id, name, item_kind, level, \
                       owner_device_pubkey, owner_endpoint_id, last_reached_at";

/// A share a contact gave this vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivedShare {
    pub id: String,
    /// The contact's vault, 64 lowercase hex characters.
    pub owner_vault_pubkey: String,
    /// The share's id at its owner, which requests name.
    pub share_id: String,
    pub name: String,
    pub item_kind: FileShareItemKind,
    pub level: FileShareLevel,
    /// The device that serves it (hex), and its network id (hex).
    pub owner_device_pubkey: String,
    pub owner_endpoint_id: String,
    pub last_reached_at: Option<String>,
}

/// Who answered: the contact's vault and the serving device.
#[derive(Debug, Clone, Copy)]
pub struct Answerer<'a> {
    pub vault: &'a str,
    pub device: &'a str,
    pub endpoint: &'a str,
}

/// The id of the received share `share_id` of `owner_vault`.
pub fn received_id(owner_vault: &str, share_id: &str) -> String {
    Uuid::new_v5(
        &NS_RECEIVED,
        format!("{owner_vault}\n{share_id}").as_bytes(),
    )
    .to_string()
}

fn of_row(r: &Row<'_>) -> haex_crdt::rusqlite::Result<Option<ReceivedShare>> {
    let kind = FileShareItemKind::parse(&r.get::<_, String>(4)?);
    let level = FileShareLevel::parse(&r.get::<_, String>(5)?);
    let (Some(item_kind), Some(level)) = (kind, level) else {
        // A value a newer version wrote: left out until this one knows it.
        return Ok(None);
    };
    Ok(Some(ReceivedShare {
        id: r.get(0)?,
        owner_vault_pubkey: r.get(1)?,
        share_id: r.get(2)?,
        name: r.get(3)?,
        item_kind,
        level,
        owner_device_pubkey: r.get(6)?,
        owner_endpoint_id: r.get(7)?,
        last_reached_at: r.get(8)?,
    }))
}

/// The received share `id`.
pub fn get(q: &mut impl Query, id: &str) -> Result<Option<ReceivedShare>> {
    Ok(q.query_row(
        &format!("SELECT {COLUMNS} FROM haex_file_shares_received WHERE id = ?1"),
        &[&id],
        of_row,
    )?
    .flatten())
}

/// The shares `owner_vault` gave this vault, by name.
pub fn of_vault(q: &mut impl Query, owner_vault: &str) -> Result<Vec<ReceivedShare>> {
    Ok(q.query_map(
        &format!(
            "SELECT {COLUMNS} FROM haex_file_shares_received WHERE owner_vault_pubkey = ?1 \
             ORDER BY name, id"
        ),
        &[&owner_vault],
        of_row,
    )?
    .into_iter()
    .flatten()
    .collect())
}

/// Writes the answer of `from` (FR-030): its shares are created or updated, and those it served
/// before but no longer names are gone (FR-031); other devices' shares stay.
pub fn apply_answer(
    tx: &mut CrdtTransaction<'_>,
    from: Answerer<'_>,
    shares: &[WireShare],
    now: &str,
) -> Result<()> {
    let before: Vec<ReceivedShare> = of_vault(tx, from.vault)?
        .into_iter()
        .filter(|row| row.owner_device_pubkey == from.device)
        .collect();
    for share in shares {
        let id = received_id(from.vault, &share.id);
        let exists = get(tx, &id)?.is_some();
        if exists {
            tx.execute(
                "UPDATE haex_file_shares_received SET name = ?2, item_kind = ?3, level = ?4, \
                 owner_device_pubkey = ?5, owner_endpoint_id = ?6, last_reached_at = ?7 \
                 WHERE id = ?1",
                params![
                    id,
                    share.name,
                    share.item_kind.as_str(),
                    share.level.as_str(),
                    from.device,
                    from.endpoint,
                    now
                ],
            )?;
        } else {
            tx.execute(
                &format!(
                    "INSERT INTO haex_file_shares_received ({COLUMNS}) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
                ),
                params![
                    id,
                    from.vault,
                    share.id,
                    share.name,
                    share.item_kind.as_str(),
                    share.level.as_str(),
                    from.device,
                    from.endpoint,
                    now
                ],
            )?;
        }
    }
    for gone in before
        .iter()
        .filter(|row| !shares.iter().any(|share| share.id == row.share_id))
    {
        tx.execute(
            "DELETE FROM haex_file_shares_received WHERE id = ?1",
            params![gone.id],
        )?;
    }
    Ok(())
}

/// The vaults this vault holds received shares of.
pub fn owners(q: &mut impl Query) -> Result<Vec<String>> {
    Ok(q.query_map(
        "SELECT DISTINCT owner_vault_pubkey FROM haex_file_shares_received",
        &[],
        |r| r.get::<_, String>(0),
    )?)
}

/// Forgets every share `owner_vault` gave this vault (FR-023).
pub fn forget_vault(tx: &mut CrdtTransaction<'_>, owner_vault: &str) -> Result<()> {
    tx.execute(
        "DELETE FROM haex_file_shares_received WHERE owner_vault_pubkey = ?1",
        params![owner_vault],
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "received_tests.rs"]
mod tests;
