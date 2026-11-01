//! The settings of the chat with contacts (spec 072 FR-023, FR-024): read receipts for the whole
//! vault (reciprocal: off sends none and shows none) and notifications per device. A missing or
//! unreadable value reads as the default, which is on for both.

use haex_crdt::CrdtTransaction;
use uuid::Uuid;

use crate::error::Result;
use crate::storage::preferences::{self, PrefScope};
use crate::storage::query::Query;

/// Read receipts, for the whole vault.
pub const KEY_READ_RECEIPTS: &str = "chat.receipts.read";
/// Notifications of new messages, per device.
pub const KEY_NOTIFICATIONS: &str = "chat.notifications";

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

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
