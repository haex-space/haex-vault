//! The Blossom upload key (spec 072 FR-081, research R12): the credential of a Blossom-Server that
//! only takes files from registered Nostr keys. It is an entry of the password manager, never kept
//! elsewhere, modelled on the login of a marketplace (`marketplace::credentials`): the vault setting
//! `chat.blossom.keyEntry` holds only the id of the entry, whose password field is the `nsec`.
//!
//! An entry this module creates belongs to `chat`, so rule Z14 of spec 034 hides it from every
//! caller but the user and this module; one the user chose stays theirs and only gets the tag.
//! [`remove`] never deletes an entry of the user. With a key set, uploads sign with it instead of
//! a fresh key per file; the blob row then keeps the entry's id instead of a secret.

use haex_crdt::CrdtTransaction;
use nostr::key::Keys;
use nostr::nips::nip19::ToBech32;
use serde::Serialize;
use ts_rs::TS;

use crate::chat::peer::settings;
use crate::error::{HaexError, Result};
use crate::passwords::access::{Caller, Grant, GrantAction, Scope};
use crate::passwords::model::{ItemInput, SecretItem};
use crate::passwords::service::PasswordsService;
use crate::passwords::{tags, usage::EntryUsage};
use crate::remote_storage::credentials::{candidates_in, entry_for, state_in_with};
use crate::remote_storage::model::{CredentialsCandidate, CredentialsEntry};
use crate::remote_storage::CredentialsState;
use crate::storage::preferences::{self, PrefScope};
use crate::storage::query::Query;
use crate::vault_gate::VaultDb;

/// The haex-vault function that owns the entry this module creates and the name the delete
/// warning of the password manager shows (`passwords.owners.chat`).
pub const FEATURE: &str = "chat";

/// The caller this module reads and writes the password manager as.
pub const CALLER: Caller = Caller::Internal { feature: FEATURE };

/// The tag of the entry that holds the upload key; the one place of its name.
pub const TAG: &str = "haex-blossom";

/// Where the key stands for this vault on this device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum UploadKeyStatus {
    /// None is set: each file is signed with a fresh key.
    None,
    /// The entry is here and holds a key.
    Present,
    /// The entry was deleted, or no longer holds a key.
    Missing,
    /// The entry has not arrived through the sync yet.
    Syncing,
}

/// What the settings show of the upload key; no secret leaves the backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct ChatUploadKey {
    pub status: UploadKeyStatus,
    /// The public key to register at the server (`npub1…`), when the entry is here.
    pub npub: Option<String>,
    pub entry: Option<CredentialsEntry>,
}

/// What an upload signs with.
pub enum Resolved {
    /// No key is set.
    Unset,
    /// The key and the id of its entry.
    Ready(Keys, String),
    Missing,
    Syncing,
}

/// The fixed grant of this module.
fn grants() -> [Grant; 1] {
    [Grant::new(GrantAction::ReadWrite, Scope::All)]
}

/// The key an entry holds: its password as an `nsec` or 64 hex digits; `None` when it holds
/// another value.
pub fn keys_of(item: &SecretItem) -> Option<Keys> {
    let secret = item.password.as_deref()?.trim();
    if secret.is_empty() {
        return None;
    }
    Keys::parse(secret).ok()
}

/// The id of the entry the setting names, if any.
pub async fn entry_id(db: &VaultDb) -> Result<Option<String>> {
    db.read(|q| Ok(settings::blossom_key_entry(q)?)).await
}

/// The key uploads sign with: the entry's, a fresh one's absence, or why the entry cannot be read.
pub async fn resolve(passwords: &PasswordsService, db: &VaultDb) -> Result<Resolved> {
    match entry_id(db).await? {
        Some(id) => resolve_entry(passwords, db, id).await,
        None => Ok(Resolved::Unset),
    }
}

async fn resolve_entry(passwords: &PasswordsService, db: &VaultDb, id: String) -> Result<Resolved> {
    match passwords
        .read_secret_item(&CALLER, &grants(), id.clone())
        .await
    {
        Ok(item) => Ok(keys_of(&item).map_or(Resolved::Missing, |keys| Resolved::Ready(keys, id))),
        Err(HaexError::PasswordsNotFound) => Ok(match state(db, &id).await? {
            CredentialsState::Syncing => Resolved::Syncing,
            _ => Resolved::Missing,
        }),
        // A damaged or unreadable entry makes the key unavailable; keep the cause out of the UI.
        Err(error) => {
            log::warn!("chat: the upload key entry {id} could not be read: {error}");
            Ok(Resolved::Missing)
        }
    }
}

/// Whether the entry `item_id` is here with a key, deleted or without one, or not yet arrived.
pub async fn state(db: &VaultDb, item_id: &str) -> Result<CredentialsState> {
    let item_id = item_id.to_owned();
    db.read(move |q| state_in_with(q, &item_id, |item| keys_of(item).is_some()).map_err(Into::into))
        .await
}

/// What the settings show: the status, the public key when the entry is here, its title.
pub async fn describe(passwords: &PasswordsService, db: &VaultDb) -> Result<ChatUploadKey> {
    let Some(id) = entry_id(db).await? else {
        return Ok(ChatUploadKey {
            status: UploadKeyStatus::None,
            npub: None,
            entry: None,
        });
    };
    let entry_id = id.clone();
    let entry = db
        .read(move |q| entry_for(q, FEATURE, &entry_id).map_err(Into::into))
        .await?;
    let (status, npub) = match resolve_entry(passwords, db, id).await? {
        Resolved::Ready(keys, _) => (UploadKeyStatus::Present, Some(npub_of(&keys)?)),
        Resolved::Syncing => (UploadKeyStatus::Syncing, None),
        Resolved::Missing | Resolved::Unset => (UploadKeyStatus::Missing, None),
    };
    Ok(ChatUploadKey {
        status,
        npub,
        entry,
    })
}

fn npub_of(keys: &Keys) -> Result<String> {
    keys.public_key()
        .to_bech32()
        .map_err(|error| HaexError::InvalidInput {
            reason: format!("chat upload key: {error}"),
        })
}

/// Makes a new key, stores it as an entry "Blossom-Upload-Schlüssel" of the password manager and
/// sets it as the upload key; refused while one is set. Returns the entry's id.
pub async fn create(db: &VaultDb) -> Result<String> {
    let keys = Keys::generate();
    let secret = keys
        .secret_key()
        .to_bech32()
        .map_err(|error| HaexError::InvalidInput {
            reason: format!("chat upload key: {error}"),
        })?;
    let input = ItemInput {
        title: Some("Blossom-Upload-Schlüssel".to_owned()),
        username: Some(npub_of(&keys)?),
        password: Some(secret),
        tags: vec![TAG.to_owned()],
        ..ItemInput::default()
    };
    db.write(move |tx| {
        refuse_when_set(tx)?;
        let id = PasswordsService::create_owned_item_in(tx, &CALLER, &input)?;
        set(tx, &id)?;
        Ok(id)
    })
    .await
}

/// Sets an entry of the user, one that holds a key, as the upload key; it gets the tag and keeps
/// its owner and shares. Refused while one is set.
pub async fn choose(db: &VaultDb, item_id: &str) -> Result<()> {
    let id = item_id.to_owned();
    db.write(move |tx| {
        refuse_when_set(tx)?;
        let item = PasswordsService::read_secret_item_in(tx, &CALLER, &grants(), &id)?;
        if keys_of(&item).is_none() {
            return Err(HaexError::InvalidInput {
                reason: "chat.blossom.keyEntry: the entry holds no key".to_owned(),
            }
            .into());
        }
        tags::bulk_set(tx, std::slice::from_ref(&id), &[TAG.to_owned()], &[])?;
        Ok(set(tx, &id)?)
    })
    .await
}

/// Unsets the upload key. Keep the entry for pending blob deletions on this or another device.
pub async fn remove(db: &VaultDb) -> Result<()> {
    db.write(|tx| {
        preferences::delete(tx, PrefScope::Vault, settings::KEY_BLOSSOM_KEY_ENTRY)?;
        Ok(())
    })
    .await
}

/// The entries that may be chosen: the user's own or created by this module, holding a key.
pub async fn candidates(db: &VaultDb) -> Result<Vec<CredentialsCandidate>> {
    db.read(|q| {
        candidates_in(
            q,
            FEATURE,
            TAG,
            |header| header.has_password,
            |item| keys_of(item).is_some(),
        )
        .map_err(Into::into)
    })
    .await
}

fn refuse_when_set(q: &mut impl Query) -> Result<()> {
    if settings::blossom_key_entry(q)?.is_some() {
        return Err(HaexError::InvalidInput {
            reason: "chat.blossom.keyEntry: an upload key is set already".to_owned(),
        });
    }
    Ok(())
}

fn set(tx: &mut CrdtTransaction<'_>, id: &str) -> Result<()> {
    preferences::insert_or_update(tx, PrefScope::Vault, settings::KEY_BLOSSOM_KEY_ENTRY, id)?;
    Ok(())
}

/// Tells the password manager that the chat uses an entry (spec 034 FR-034), so deleting the upload
/// key there warns first. It reads the vault that is open when it is asked.
pub struct ChatUsage<F> {
    database: F,
}

impl<F> ChatUsage<F>
where
    F: Fn() -> Option<VaultDb> + Send + Sync,
{
    /// Creates a usage check that obtains the active vault from `database` on each request.
    pub fn new(database: F) -> Self {
        Self { database }
    }
}

impl<F> EntryUsage for ChatUsage<F>
where
    F: Fn() -> Option<VaultDb> + Send + Sync,
{
    fn feature(&self) -> &'static str {
        FEATURE
    }

    /// Whether the setting or any local blob names the entry.
    fn uses(&self, item_id: &str) -> bool {
        let Some(db) = (self.database)() else {
            return false;
        };
        db.read_blocking(|q| {
            if settings::blossom_key_entry(q)?.as_deref() == Some(item_id) {
                return Ok(true);
            }
            Ok(q.query_row(
                "SELECT EXISTS(SELECT 1 FROM chat_blobs_no_sync WHERE key_entry_id = ?1)",
                haex_crdt::rusqlite::params![item_id],
                |row| row.get(0),
            )?
            .unwrap_or(false))
        })
        .unwrap_or(false)
    }
}

#[cfg(test)]
#[path = "upload_key_tests.rs"]
mod tests;
