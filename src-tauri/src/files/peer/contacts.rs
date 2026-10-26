//! A contact's shares in the file browser (spec 056 US2, FR-028–FR-031): asking every device on the
//! contact's stored card at once, writing each answer as it comes, and the contacts with what is
//! known of their shares for the sidebar. Only a person's action asks (expanding a contact).

use std::time::Duration;

use futures::future::join_all;
use serde::Serialize;
use ts_rs::TS;

use super::connect::Connections;
use super::wire::{Caller, Op, Reply, WireShare};
use crate::contacts::card::format::SignedCard;
use crate::contacts::card::received as cards;
use crate::files::share::received::{self, Answerer, ReceivedShare};
use crate::files::share::{FileShareItemKind, FileShareLevel, SHARE_PREFIX};
use crate::files::{FilesError, FilesErrorCode};
use crate::passwords::clock;
use crate::sync::keys::hex;
use crate::sync::presence::endpoint_addr_of;
use crate::vault_gate::VaultDb;

/// How long a contact's devices get to answer (FR-008: then "nicht erreichbar").
pub const ANSWER_LIMIT: Duration = Duration::from_secs(10);

/// A share a contact gave this vault, for the sidebar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct FilesReceivedShare {
    /// The storage id `share:<id>`.
    pub storage_id: String,
    pub name: String,
    pub item_kind: FileShareItemKind,
    pub level: FileShareLevel,
    /// Whether its device answered this session.
    pub reachable: bool,
}

/// A contact with a stored card and the shares it gave this vault.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct FilesContactShares {
    pub contact_id: String,
    /// The name the card carries.
    pub contact_name: String,
    pub vault_pubkey: String,
    pub shares: Vec<FilesReceivedShare>,
}

fn failed(error: crate::error::HolziError) -> FilesError {
    log::warn!("files: the shares of a contact could not be read: {error}");
    FilesError::new(
        FilesErrorCode::Unsupported,
        "the contact's shares could not be read",
    )
}

fn view(
    contact_id: String,
    card: &SignedCard,
    rows: Vec<ReceivedShare>,
    links: Option<&Connections>,
) -> FilesContactShares {
    FilesContactShares {
        contact_id,
        contact_name: card.card.name.clone(),
        vault_pubkey: hex(&card.card.vault),
        shares: rows
            .into_iter()
            .map(|row| FilesReceivedShare {
                reachable: links.is_some_and(|links| links.reaches(&row.owner_endpoint_id)),
                storage_id: format!("{SHARE_PREFIX}{}", row.id),
                name: row.name,
                item_kind: row.item_kind,
                level: row.level,
            })
            .collect(),
    }
}

/// Every contact with a stored card that is not blocked, with the shares known of it (FR-031).
pub async fn known(
    db: &VaultDb,
    links: Option<&Connections>,
) -> Result<Vec<FilesContactShares>, FilesError> {
    let found = db
        .read(|q| {
            let mut found = Vec::new();
            for contact_id in cards::with_cards(q)? {
                let Some(card) = cards::stored_of_contact(q, &contact_id)? else {
                    continue;
                };
                if cards::is_blocked(q, &card.card.vault)? {
                    continue;
                }
                let rows = received::of_vault(q, &hex(&card.card.vault))?;
                found.push((contact_id, card, rows));
            }
            Ok(found)
        })
        .await
        .map_err(failed)?;
    let mut contacts: Vec<FilesContactShares> = found
        .into_iter()
        .map(|(contact_id, card, rows)| view(contact_id, &card, rows, links))
        .collect();
    contacts.sort_by_cached_key(|contact| contact.contact_name.to_lowercase());
    Ok(contacts)
}

/// Asks every device on `contact_id`'s card for its shares, writes each answer (FR-030) and returns
/// what is known afterwards; a device that does not answer keeps its shares, unreachable.
pub async fn query(
    db: &VaultDb,
    links: &Connections,
    contact_id: String,
) -> Result<FilesContactShares, FilesError> {
    let id = contact_id.clone();
    let (card, addresses) = db
        .read(move |q| {
            let Some(card) = cards::stored_of_contact(q, &id)? else {
                return Ok(None);
            };
            if cards::is_blocked(q, &card.card.vault)? {
                return Ok(None);
            }
            // The devices of all stored cards of the vault, not of one (spec 060).
            let addresses = cards::addresses(q, &card.card.vault)?;
            Ok(Some((card, addresses)))
        })
        .await
        .map_err(failed)?
        .ok_or_else(|| FilesError::new(FilesErrorCode::NotFound, "no stored card"))?;
    let vault = card.card.vault;
    let asks = addresses.devices.iter().map(|device| async move {
        let addr = endpoint_addr_of(&device.endpoint_id, device.iroh_relay.as_deref(), &[]).ok()?;
        let asked = async {
            let link = links.link_to_contact(vault, addr).await.ok()?;
            let (reply, _) = link.ask(Caller::User, Op::Shares).await.ok()?;
            match reply {
                Reply::Shares(shares) => Some((device.device_pubkey, device.endpoint_id, shares)),
                _ => None,
            }
        };
        tokio::time::timeout(ANSWER_LIMIT, asked)
            .await
            .ok()
            .flatten()
    });
    let answers: Vec<([u8; 32], [u8; 32], Vec<WireShare>)> =
        join_all(asks).await.into_iter().flatten().collect();
    let owner = hex(&vault);
    db.write(move |tx| {
        let now = clock::now();
        for (device, endpoint, shares) in &answers {
            let (device, endpoint) = (hex(device), hex(endpoint));
            let from = Answerer {
                vault: &owner,
                device: &device,
                endpoint: &endpoint,
            };
            received::apply_answer(tx, from, shares, &now)?;
        }
        Ok(received::of_vault(tx, &owner)?)
    })
    .await
    .map_err(failed)
    .map(|rows| view(contact_id, &card, rows, Some(links)))
}

/// The id at its owner of the received share `storage_id`, which its requests name.
pub async fn share_of(db: &VaultDb, storage_id: &str) -> Result<String, FilesError> {
    let id = storage_id
        .strip_prefix(SHARE_PREFIX)
        .ok_or_else(|| FilesError::new(FilesErrorCode::NotFound, "not a received share"))?
        .to_owned();
    db.read(move |q| Ok(received::get(q, &id)?))
        .await
        .map_err(failed)?
        .map(|row| row.share_id)
        .ok_or_else(|| FilesError::new(FilesErrorCode::NotFound, "no such share"))
}
