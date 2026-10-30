//! A drive's password in the password manager (spec 064 FR-013, FR-016, research R7): an ordinary
//! entry of the user's whose URL is `haex:drive:<key>`, so it syncs to every own device. Read and
//! written as the user, as if the user had typed it; the password itself only ever leaves here as
//! a [`DriveSecret`].

use serde::Serialize;
use ts_rs::TS;

use super::backend::{DriveError, DriveSecret};
use crate::error::HolziError;
use crate::passwords::access::Caller;
use crate::passwords::ids::{fold, fold_for_search};
use crate::passwords::model::{ItemHeader, ItemInput, ItemPatch};
use crate::passwords::service::{Headers, PasswordsService};
use crate::patch::Patch;

/// The tag of the entries that hold a drive password; the one place of its name.
pub const TAG: &str = "haex-drive";

/// A password manager entry offered for a drive; never its password.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct DriveEntry {
    pub item_id: String,
    pub title: String,
    pub username: Option<String>,
    pub url: Option<String>,
}

impl From<ItemHeader> for DriveEntry {
    fn from(header: ItemHeader) -> Self {
        Self {
            item_id: header.id,
            title: header.title.unwrap_or_default(),
            username: header.username,
            url: header.url,
        }
    }
}

/// The address an entry carries for the drive with `key`.
pub fn entry_url(key: &str) -> String {
    format!("haex:drive:{}", key.to_lowercase())
}

/// The entries outside the trash with a password and exactly the drive's address.
pub async fn matching_entries(
    passwords: &PasswordsService,
    key: &str,
) -> Result<Vec<DriveEntry>, DriveError> {
    let url = entry_url(key);
    Ok(headers(passwords)
        .await?
        .into_iter()
        .filter(|header| header.has_password && header.url.as_deref() == Some(url.as_str()))
        .map(DriveEntry::from)
        .collect())
}

/// The entries the user may pick in the unlock dialog: every one with a password that fits
/// `query` in its title, user name or address, the drive entries first, then by title.
pub async fn candidates(
    passwords: &PasswordsService,
    query: &str,
) -> Result<Vec<DriveEntry>, DriveError> {
    let words: Vec<String> = fold_for_search(query)
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    let tag = fold(TAG);
    let mut found: Vec<(bool, DriveEntry)> = headers(passwords)
        .await?
        .into_iter()
        .filter(|header| header.has_password)
        .filter(|header| {
            let texts: Vec<String> = [&header.title, &header.username, &header.url]
                .into_iter()
                .flatten()
                .map(|text| fold_for_search(text))
                .collect();
            words
                .iter()
                .all(|word| texts.iter().any(|text| text.contains(word)))
        })
        .map(|header| {
            let tagged = header.tags.iter().any(|t| fold(&t.name) == tag);
            (tagged, DriveEntry::from(header))
        })
        .collect();
    found.sort_by_cached_key(|(tagged, entry)| (!tagged, entry.title.to_lowercase()));
    Ok(found.into_iter().map(|(_, entry)| entry).collect())
}

/// The password of the entry `item_id`.
pub async fn read_password(
    passwords: &PasswordsService,
    item_id: &str,
) -> Result<DriveSecret, DriveError> {
    let item = passwords
        .read_secret_item(&Caller::User, &[], item_id.to_owned())
        .await
        .map_err(from_passwords)?;
    item.password
        .filter(|password| !password.is_empty())
        .map(DriveSecret::new)
        .ok_or_else(|| DriveError::Failed("the entry has no password".to_owned()))
}

/// Saves a typed password as a new entry of the user's for the drive with `key`.
pub async fn remember_typed(
    passwords: &PasswordsService,
    key: &str,
    title: &str,
    secret: &DriveSecret,
) -> Result<(), DriveError> {
    let input = ItemInput {
        title: Some(title.trim().to_owned()),
        password: Some(secret.expose().to_owned()),
        url: Some(entry_url(key)),
        tags: vec![TAG.to_owned()],
        ..ItemInput::default()
    };
    passwords
        .create_item(&Caller::User, &[], input, None)
        .await
        .map(|_| ())
        .map_err(from_passwords)
}

/// Gives the entry `item_id` the drive's address and tag. One with another address stays as it is
/// ([`DriveError::UrlTaken`]) unless `replace`.
pub async fn link_entry(
    passwords: &PasswordsService,
    item_id: &str,
    key: &str,
    replace: bool,
) -> Result<(), DriveError> {
    let url = entry_url(key);
    let header = passwords
        .get_item(&Caller::User, item_id.to_owned())
        .await
        .map_err(from_passwords)?
        .header;
    match header.url.as_deref().filter(|current| !current.is_empty()) {
        Some(current) if current == url => return Ok(()),
        Some(_) if !replace => return Err(DriveError::UrlTaken),
        _ => {}
    }
    let patch = ItemPatch {
        url: Patch::Set(url),
        ..ItemPatch::default()
    };
    passwords
        .update_item(
            &Caller::User,
            &[],
            item_id.to_owned(),
            header.updated_at.unwrap_or_default(),
            patch,
        )
        .await
        .map_err(from_passwords)?;
    passwords
        .set_tags(
            &Caller::User,
            vec![item_id.to_owned()],
            vec![TAG.to_owned()],
            Vec::new(),
        )
        .await
        .map(|_| ())
        .map_err(from_passwords)
}

async fn headers(passwords: &PasswordsService) -> Result<Vec<ItemHeader>, DriveError> {
    match passwords
        .list_headers(&Caller::User, &[])
        .await
        .map_err(from_passwords)?
    {
        Headers::Items(headers) => Ok(headers),
        Headers::Agent(_) => Err(DriveError::Failed("not the user's list".to_owned())),
    }
}

fn from_passwords(error: HolziError) -> DriveError {
    match error {
        HolziError::NoActiveInstance | HolziError::VaultClosed => DriveError::NoVault,
        other => DriveError::Failed(other.to_string()),
    }
}

#[cfg(test)]
#[path = "unlock_tests.rs"]
mod tests;
