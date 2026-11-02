//! The settings of the chat with contacts (spec 072 FR-023, FR-024, FR-036, FR-038): read receipts
//! for the whole vault (reciprocal: off sends none and shows none), notifications per device, the
//! Blossom-Server for files (whole vault; none until the user picks one) and how large a file may
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
/// The Blossom-Server address for files, for the whole vault; empty or missing = none (FR-038).
pub const KEY_BLOSSOM_SERVER: &str = "chat.blossom.server";
/// Files up to this many bytes download by themselves, per device (FR-036).
pub const KEY_AUTO_LIMIT: &str = "chat.download.autoLimit";
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

/// The Blossom-Server address, `None` when the user chose none.
pub fn blossom_server(q: &mut impl Query) -> Result<Option<String>> {
    Ok(preferences::get(q, PrefScope::Vault, KEY_BLOSSOM_SERVER)?
        .map(|address| address.trim().to_owned())
        .filter(|address| !address.is_empty()))
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
