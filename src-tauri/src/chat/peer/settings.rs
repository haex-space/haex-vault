//! The settings of the chat with contacts (spec 072 FR-023, FR-024, FR-036, FR-038): read receipts
//! for the whole vault (reciprocal: off sends none and shows none), notifications per device, the
//! Blossom-Server for files (whole vault; a public server until the user picks another or none) and how large a file may
//! be to download by itself (per device). A missing or unreadable value reads as the default.

use haex_crdt::CrdtTransaction;
use uuid::Uuid;

use crate::error::Result;
use crate::storage::preferences::{self, PrefScope};
use crate::storage::query::Query;

/// Read receipts, for the whole vault.
pub const KEY_READ_RECEIPTS: &str = "chat.receipts.read";
/// Notifications of new messages, per device.
pub const KEY_NOTIFICATIONS: &str = "chat.notifications";
/// The Blossom-Server address for files, for the whole vault. Missing = [`DEFAULT_BLOSSOM_SERVER`];
/// empty = the user chose none (FR-038).
pub const KEY_BLOSSOM_SERVER: &str = "chat.blossom.server";
/// The server of a vault that never chose one, so files work right away. It took an encrypted 20 MB
/// blob anonymously and honoured `DELETE` (spec 072 research R12, tested 2026-11-25).
pub const DEFAULT_BLOSSOM_SERVER: &str = "https://nostr.download";
/// Files up to this many bytes download by themselves, per device (FR-036).
pub const KEY_AUTO_LIMIT: &str = "chat.download.autoLimit";
/// The id of the password-manager entry that holds the Blossom upload key, for the whole vault;
/// missing or empty = none (FR-081). Written only by `files::upload_key`.
pub const KEY_BLOSSOM_KEY_ENTRY: &str = "chat.blossom.keyEntry";
/// Whether strangers may leave a request, for the whole vault; missing = yes (spec 099 FR-012).
/// Written only by `requests::set_allowed`, which removes the open requests with it.
pub const KEY_ALLOW_STRANGERS: &str = "chat.requests.allowStrangers";
/// The default of [`KEY_AUTO_LIMIT`]: 25 MB.
pub const DEFAULT_AUTO_LIMIT: u64 = 25_000_000;

fn flag(q: &mut impl Query, scope: PrefScope, key: &str) -> Result<bool> {
    Ok(preferences::parse_bool(preferences::get(q, scope, key)?.as_deref()).unwrap_or(true))
}

fn set_flag(tx: &mut CrdtTransaction<'_>, scope: PrefScope, key: &str, on: bool) -> Result<()> {
    preferences::insert_or_update(tx, scope, key, if on { "true" } else { "false" })?;
    Ok(())
}

pub fn read_receipts(q: &mut impl Query) -> Result<bool> {
    flag(q, PrefScope::Vault, KEY_READ_RECEIPTS)
}

pub fn set_read_receipts(tx: &mut CrdtTransaction<'_>, on: bool) -> Result<()> {
    set_flag(tx, PrefScope::Vault, KEY_READ_RECEIPTS, on)
}

pub fn notifications(q: &mut impl Query, device: Uuid) -> Result<bool> {
    flag(q, PrefScope::Device(device), KEY_NOTIFICATIONS)
}

pub fn set_notifications(tx: &mut CrdtTransaction<'_>, device: Uuid, on: bool) -> Result<()> {
    set_flag(tx, PrefScope::Device(device), KEY_NOTIFICATIONS, on)
}

/// Whether a stranger's message may leave a request (spec 099 FR-012).
pub fn allow_strangers(q: &mut impl Query) -> Result<bool> {
    flag(q, PrefScope::Vault, KEY_ALLOW_STRANGERS)
}

pub fn set_allow_strangers(tx: &mut CrdtTransaction<'_>, on: bool) -> Result<()> {
    set_flag(tx, PrefScope::Vault, KEY_ALLOW_STRANGERS, on)
}

/// The Blossom-Server address: the default while the user never chose, `None` once they chose none.
pub fn blossom_server(q: &mut impl Query) -> Result<Option<String>> {
    let Some(stored) = preferences::get(q, PrefScope::Vault, KEY_BLOSSOM_SERVER)? else {
        return Ok(Some(DEFAULT_BLOSSOM_SERVER.to_owned()));
    };
    Ok(Some(stored.trim().to_owned()).filter(|address| !address.is_empty()))
}

pub fn set_blossom_server(tx: &mut CrdtTransaction<'_>, address: Option<&str>) -> Result<()> {
    preferences::insert_or_update(
        tx,
        PrefScope::Vault,
        KEY_BLOSSOM_SERVER,
        address.unwrap_or_default().trim(),
    )?;
    Ok(())
}

/// The id of the entry that holds the Blossom upload key, `None` when none is set.
pub fn blossom_key_entry(q: &mut impl Query) -> Result<Option<String>> {
    Ok(
        preferences::get(q, PrefScope::Vault, KEY_BLOSSOM_KEY_ENTRY)?
            .map(|id| id.trim().to_owned())
            .filter(|id| !id.is_empty()),
    )
}

/// The largest file this device downloads by itself, in bytes.
pub fn auto_limit(q: &mut impl Query, device: Uuid) -> Result<u64> {
    Ok(
        preferences::get(q, PrefScope::Device(device), KEY_AUTO_LIMIT)?
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(DEFAULT_AUTO_LIMIT),
    )
}

pub fn set_auto_limit(tx: &mut CrdtTransaction<'_>, device: Uuid, bytes: u64) -> Result<()> {
    preferences::insert_or_update(
        tx,
        PrefScope::Device(device),
        KEY_AUTO_LIMIT,
        &bytes.to_string(),
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
