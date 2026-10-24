//! Managing file shares from the window (spec 056 US2, contracts/tauri-commands.md): creating one on
//! this device, changing its name and grants from any own device, taking grants away and deleting
//! it. Grants go only to vaults whose card is stored on a contact and that are not blocked (FR-016);
//! a new share is always one of this device's own files (FR-015, FR-018).

use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::store::{self, FileShareGrantRow, FileShareRow, NewFileShare};
use super::{FileShareItemKind, FileShareLevel, FileShareSourceState};
use crate::contacts::card::received;
use crate::files::local::{resolve, OwnPlaces};
use crate::files::peer::connect::key_of;
use crate::files::{FilesError, FilesErrorCode, SourceRef};
use crate::passwords::clock;
use crate::storage::query::Query;
use crate::vault_gate::VaultDb;

/// What the window sees of a share's state (FR-022, FR-026).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum FileShareState {
    Active,
    SourceMissing,
    SourceUnavailable,
    /// The serving device has not seen the latest change yet.
    Pending,
}

/// A contact's grant in a share.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct FileShareGrant {
    pub vault_pubkey: String,
    /// `None` when no contact carries the vault's card any more.
    pub contact_id: Option<String>,
    pub contact_name: Option<String>,
    pub level: FileShareLevel,
}

/// The device that serves a share.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct FileShareDevice {
    /// Its `vault_device_uuid`.
    pub id: String,
    pub name: Option<String>,
    pub is_current: bool,
}

/// A file share for the window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct FileShare {
    pub id: String,
    pub name: String,
    pub source: SourceRef,
    pub path: String,
    pub item_kind: FileShareItemKind,
    pub serving_device: FileShareDevice,
    pub grants: Vec<FileShareGrant>,
    pub state: FileShareState,
}

/// One grant asked for in [`FileShareSaveArgs`].
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct FileShareGrantInput {
    pub vault_pubkey: String,
    pub level: FileShareLevel,
}

/// Creates a share (`share_id` absent) or changes one; the grants replace the share's grants.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct FileShareSaveArgs {
    #[ts(optional)]
    pub share_id: Option<String>,
    pub source: SourceRef,
    pub path: String,
    pub name: String,
    pub grants: Vec<FileShareGrantInput>,
}

/// This device and its own places, as managing shares needs them.
pub struct Here<'a> {
    pub db: &'a VaultDb,
    /// This device's `vault_device_uuid`.
    pub device: &'a str,
    pub own: &'a OwnPlaces,
}

fn failed(error: crate::error::HolziError) -> FilesError {
    log::warn!("files: a share could not be read or written: {error}");
    FilesError::new(FilesErrorCode::Unsupported, "the share could not be saved")
}

fn not_found(what: &str) -> FilesError {
    FilesError::new(FilesErrorCode::NotFound, what)
}

/// Every share of the vault (FR-020).
pub async fn list(here: &Here<'_>) -> Result<Vec<FileShare>, FilesError> {
    let device = here.device.to_owned();
    here.db
        .read(move |q| {
            let rows = store::all_shares(q)?;
            Ok(rows
                .into_iter()
                .map(|row| view(q, &device, row))
                .collect::<crate::error::Result<Vec<_>>>()?)
        })
        .await
        .map_err(failed)
}

/// The share of `path` in `source` served by this device, if one exists (FR-017).
pub async fn of(
    here: &Here<'_>,
    source: SourceRef,
    path: String,
) -> Result<Option<FileShare>, FilesError> {
    let device = here.device.to_owned();
    let own = here.own.clone();
    let source_for_path = source.clone();
    let path_for_resolution = path.clone();
    let resolved = tauri::async_runtime::spawn_blocking(move || {
        root_path(&own, &source_for_path, &path_for_resolution)
    })
    .await
    .map_err(|error| {
        FilesError::new(
            FilesErrorCode::Unsupported,
            format!("share path lookup: {error}"),
        )
    })?;
    let path = resolved.map_or(path, |(path, _)| path);
    here.db
        .read(
            move |q| match store::share_at(q, &device, &source, &path)? {
                Some(row) => Ok(Some(view(q, &device, row)?)),
                None => Ok(None),
            },
        )
        .await
        .map_err(failed)
}

/// The shares that grant `contact_id`'s vault anything.
pub async fn of_contact(here: &Here<'_>, contact_id: String) -> Result<Vec<FileShare>, FilesError> {
    let device = here.device.to_owned();
    here.db
        .read(move |q| {
            let Some(card) = received::stored_of_contact(q, &contact_id)? else {
                return Ok(Vec::new());
            };
            let vault = crate::sync::keys::hex(&card.card.vault);
            let mut shares = Vec::new();
            for row in store::all_shares(q)? {
                if store::grant_for(q, &row.id, &vault)?.is_some() {
                    shares.push(view(q, &device, row)?);
                }
            }
            Ok(shares)
        })
        .await
        .map_err(failed)
}

/// Creates or changes a share as `args` say; its view afterwards.
pub async fn save(here: &Here<'_>, args: FileShareSaveArgs) -> Result<FileShare, FilesError> {
    let device = here.device.to_owned();
    let new = match &args.share_id {
        Some(_) => None,
        None => {
            let own = here.own.clone();
            let source = args.source.clone();
            let path = args.path.clone();
            Some(
                tauri::async_runtime::spawn_blocking(move || root_path(&own, &source, &path))
                    .await
                    .map_err(|error| {
                        FilesError::new(
                            FilesErrorCode::Unsupported,
                            format!("share path lookup: {error}"),
                        )
                    })??,
            )
        }
    };
    for grant in &args.grants {
        store::check_vault(&grant.vault_pubkey)
            .map_err(|_| FilesError::new(FilesErrorCode::InvalidName, "not a vault"))?;
    }
    here.db
        .write(move |tx| {
            for grant in &args.grants {
                if !may_receive(tx, &grant.vault_pubkey)? {
                    return Ok(Err(not_found("no stored card for this vault")));
                }
            }
            let now = clock::now();
            let id = match (&args.share_id, new) {
                (Some(id), _) => {
                    if store::get_share(tx, id)?.is_none() {
                        return Ok(Err(not_found("no such share")));
                    }
                    store::rename_share(tx, id, &args.name, &now)?;
                    id.clone()
                }
                (None, Some((path, item_kind))) => store::create_share(
                    tx,
                    &NewFileShare {
                        vault_device_uuid: &device,
                        source: &args.source,
                        path: &path,
                        item_kind,
                        name: &args.name,
                    },
                    &now,
                )?,
                (None, None) => return Ok(Err(not_found("no such share"))),
            };
            let before = store::grants_of(tx, &id)?;
            for gone in before.iter().filter(|old| {
                !args
                    .grants
                    .iter()
                    .any(|g| g.vault_pubkey == old.vault_pubkey)
            }) {
                store::delete_grant(tx, &id, &gone.vault_pubkey, &now)?;
            }
            for grant in &args.grants {
                let same = before
                    .iter()
                    .any(|old| old.vault_pubkey == grant.vault_pubkey && old.level == grant.level);
                if !same {
                    store::save_grant(tx, &id, &grant.vault_pubkey, grant.level, &now)?;
                }
            }
            served_here(tx, &device, &id)?;
            let Some(row) = store::get_share(tx, &id)? else {
                return Ok(Err(not_found("no such share")));
            };
            Ok(Ok(view(tx, &device, row)?))
        })
        .await
        .map_err(failed)?
}

/// Takes `vault_pubkey`'s grant in `share_id` away (FR-024).
pub async fn revoke(
    here: &Here<'_>,
    share_id: String,
    vault_pubkey: String,
) -> Result<FileShare, FilesError> {
    let device = here.device.to_owned();
    here.db
        .write(move |tx| {
            if store::get_share(tx, &share_id)?.is_none() {
                return Ok(Err(not_found("no such share")));
            }
            store::delete_grant(tx, &share_id, &vault_pubkey, &clock::now())?;
            served_here(tx, &device, &share_id)?;
            let Some(row) = store::get_share(tx, &share_id)? else {
                return Ok(Err(not_found("no such share")));
            };
            Ok(Ok(view(tx, &device, row)?))
        })
        .await
        .map_err(failed)?
}

/// Deletes `share_id` with its grants.
pub async fn delete(here: &Here<'_>, share_id: String) -> Result<(), FilesError> {
    here.db
        .write(move |tx| Ok(store::delete_share(tx, &share_id)?))
        .await
        .map_err(failed)
}

/// The stored path and kind of a new share's root: a folder or file of this device, never in
/// holzi's own places (FR-007, FR-015).
fn root_path(
    own: &OwnPlaces,
    source: &SourceRef,
    path: &str,
) -> Result<(String, FileShareItemKind), FilesError> {
    if *source != SourceRef::Device {
        // Shares of a storage or an encrypted folder come with their own delivery.
        return Err(FilesError::new(
            FilesErrorCode::Unsupported,
            "only folders and files of this device can be shared yet",
        ));
    }
    let real = resolve(Path::new(path))?;
    if own.contains(&real) {
        return Err(FilesError::new(
            FilesErrorCode::HolziOwned,
            "holzi's own data is never shared",
        ));
    }
    let meta = std::fs::metadata(&real).map_err(|_| not_found("not found"))?;
    let kind = if meta.is_dir() {
        FileShareItemKind::Folder
    } else {
        FileShareItemKind::File
    };
    Ok((real.to_string_lossy().into_owned(), kind))
}

/// Whether `vault_pubkey` may get a grant: its card is stored and it is not blocked (FR-016).
fn may_receive(q: &mut impl Query, vault_pubkey: &str) -> crate::error::Result<bool> {
    let Some(vault) = key_of(vault_pubkey) else {
        return Ok(false);
    };
    Ok(!received::is_blocked(q, &vault)? && received::stored(q, &vault)?.is_some())
}

/// A change made on the serving device itself is seen there at once (FR-026).
fn served_here(
    tx: &mut haex_crdt::CrdtTransaction<'_>,
    device: &str,
    share: &str,
) -> crate::error::Result<()> {
    if let Some(row) = store::get_share(tx, share)? {
        if row.vault_device_uuid == device {
            store::set_served(tx, share, &row.changed_at)?;
        }
    }
    Ok(())
}

fn view(q: &mut impl Query, device: &str, row: FileShareRow) -> crate::error::Result<FileShare> {
    let grants = store::grants_of(q, &row.id)?
        .into_iter()
        .map(|grant| grant_view(q, grant))
        .collect::<crate::error::Result<Vec<_>>>()?;
    let name: Option<String> = q
        .query_row(
            "SELECT alias FROM known_devices WHERE vault_device_uuid = ?1",
            &[&row.vault_device_uuid],
            |r| r.get::<_, Option<String>>(0),
        )?
        .flatten();
    let state = if row.is_pending() {
        FileShareState::Pending
    } else {
        match row.source_state {
            FileShareSourceState::Ok => FileShareState::Active,
            FileShareSourceState::Missing => FileShareState::SourceMissing,
            FileShareSourceState::Unavailable => FileShareState::SourceUnavailable,
        }
    };
    Ok(FileShare {
        serving_device: FileShareDevice {
            is_current: row.vault_device_uuid == device,
            id: row.vault_device_uuid,
            name,
        },
        id: row.id,
        name: row.name,
        source: row.source,
        path: row.path,
        item_kind: row.item_kind,
        grants,
        state,
    })
}

/// A grant with the contact that carries the vault's card, named as the card names it.
fn grant_view(
    q: &mut impl Query,
    grant: FileShareGrantRow,
) -> crate::error::Result<FileShareGrant> {
    let stored = match key_of(&grant.vault_pubkey) {
        Some(vault) => received::stored(q, &vault)?,
        None => None,
    };
    let (contact_id, contact_name) = match stored {
        Some((contact_id, card)) => (Some(contact_id), Some(card.card.name)),
        None => (None, None),
    };
    Ok(FileShareGrant {
        vault_pubkey: grant.vault_pubkey,
        contact_id,
        contact_name,
        level: grant.level,
    })
}

#[cfg(test)]
#[path = "manage_tests.rs"]
mod tests;
