//! Whether the sync with this phone is on, its target account and its state (spec 051 FR-015,
//! FR-015a, FR-018, FR-027, data-model.md). On/off and the target account are device settings in
//! `preferences`; the state is the row of `haex_contacts_phone_runs`, which exists only while the
//! sync is on (switching off deletes it and keeps the links).

use std::collections::BTreeSet;
use std::path::Path;

use haex_crdt::CrdtTransaction;
use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

use super::run::{ACTIVE, PAUSED, REVIEW};
use super::store::{self, Issue, RunRow};
use super::{Account, Counts, MassDelete, Paused};
use crate::error::Result;
use crate::storage::preferences::{self, PrefScope};
use crate::storage::query::Query;

/// The device setting that switches the sync on; missing means off.
pub const PREF_ENABLED: &str = "contacts.phone.enabled";
/// The device setting with the account new contacts go to on this phone (JSON of [`Account`]).
pub const PREF_TARGET: &str = "contacts.phone.target_account";
/// The device setting with the phone's accounts as the last run saw them (JSON of [`Account`]s),
/// so another device can offer them as targets.
pub const PREF_ACCOUNTS: &str = "contacts.phone.accounts";

/// The state of the sync as the window shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsPhoneState"
)]
#[serde(rename_all = "camelCase")]
pub enum PhoneState {
    Off,
    /// Switched on; the overview of the first run waits for the person.
    Review,
    Active,
    Paused,
}

/// Everything the settings show of the sync on this phone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsPhoneStatus"
)]
#[serde(rename_all = "camelCase")]
pub struct PhoneStatus {
    pub state: PhoneState,
    pub paused: Option<Paused>,
    pub last_run_at: Option<String>,
    pub counts: Option<Counts>,
    pub mass_delete: Option<MassDelete>,
    pub issues: Vec<Issue>,
    pub target: Option<Account>,
}

fn device_scope(device: &str) -> Result<PrefScope> {
    let uuid =
        Uuid::parse_str(device).map_err(|error| crate::error::HolziError::ContactsInvalid {
            reason: format!("device: {error}"),
        })?;
    Ok(PrefScope::Device(uuid))
}

/// Whether the sync is on for this phone.
pub fn enabled(q: &mut impl Query, device: &str) -> Result<bool> {
    let value = preferences::get(q, device_scope(device)?, PREF_ENABLED)?;
    Ok(preferences::parse_bool(value.as_deref()).unwrap_or(false))
}

/// The target account chosen for this phone.
pub fn target(q: &mut impl Query, device: &str) -> Result<Option<Account>> {
    let value = preferences::get(q, device_scope(device)?, PREF_TARGET)?;
    Ok(value.and_then(|json| serde_json::from_str(&json).ok()))
}

pub fn set_target(tx: &mut CrdtTransaction<'_>, device: &str, account: &Account) -> Result<()> {
    let json = serde_json::to_string(account).unwrap_or_default();
    preferences::insert_or_update(tx, device_scope(device)?, PREF_TARGET, &json)?;
    Ok(())
}

/// The phone's accounts as its last run saw them.
pub fn accounts(q: &mut impl Query, device: &str) -> Result<Vec<Account>> {
    let value = preferences::get(q, device_scope(device)?, PREF_ACCOUNTS)?;
    Ok(value
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default())
}

/// Saves the phone's accounts when they differ from the saved ones.
pub fn set_accounts(tx: &mut CrdtTransaction<'_>, device: &str, list: &[Account]) -> Result<()> {
    if accounts(tx, device)? == list {
        return Ok(());
    }
    let json = serde_json::to_string(list).unwrap_or_default();
    preferences::insert_or_update(tx, device_scope(device)?, PREF_ACCOUNTS, &json)?;
    Ok(())
}

/// A phone of the vault with the sync on, as a target for contacts (FR-018c).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsPhoneSummary"
)]
#[serde(rename_all = "camelCase")]
pub struct PhoneSummary {
    /// The phone's `vault_device_uuid`.
    pub device: String,
    /// The phone's name in the vault.
    pub name: Option<String>,
    /// The account new contacts go to there.
    pub target: Option<Account>,
    /// The phone's accounts as its last run saw them.
    pub accounts: Vec<Account>,
}

/// The phones of the vault with the sync on.
pub fn phones(q: &mut impl Query) -> Result<Vec<PhoneSummary>> {
    let mut out = Vec::new();
    for row in preferences::list_by_key(q, PREF_ENABLED)? {
        let PrefScope::Device(uuid) = row.scope else {
            continue;
        };
        if preferences::parse_bool(row.value.as_deref()) != Some(true) {
            continue;
        }
        let device = uuid.to_string();
        let name: Option<Option<String>> = q.query_row(
            "SELECT alias FROM known_devices WHERE vault_device_uuid = ?1",
            haex_crdt::rusqlite::params![device],
            |r| r.get(0),
        )?;
        out.push(PhoneSummary {
            name: name.flatten(),
            target: target(q, &device)?,
            accounts: accounts(q, &device)?,
            device,
        });
    }
    Ok(out)
}

/// Whether a device is a phone of the vault with the sync on.
pub fn is_phone(q: &mut impl Query, device: &str) -> Result<bool> {
    match device_scope(device) {
        Ok(_) => enabled(q, device),
        Err(_) => Ok(false),
    }
}

/// Switches the sync on; a phone without a state starts at the overview, a paused one stays.
pub fn enable(tx: &mut CrdtTransaction<'_>, device: &str) -> Result<()> {
    preferences::insert_or_update(tx, device_scope(device)?, PREF_ENABLED, "true")?;
    if store::run_row(tx, device)?.is_none() {
        let row = RunRow {
            state: REVIEW.to_owned(),
            paused_reason: None,
            last_run_at: None,
            counts: None,
        };
        store::save_run_row(tx, device, &row)?;
    }
    Ok(())
}

/// Switches the sync off; nothing is deleted on either side (FR-026), the links stay.
pub fn disable(tx: &mut CrdtTransaction<'_>, device: &str) -> Result<()> {
    preferences::insert_or_update(tx, device_scope(device)?, PREF_ENABLED, "false")?;
    store::delete_run_row(tx, device)
}

pub fn status(q: &mut impl Query, device: &str) -> Result<PhoneStatus> {
    let on = enabled(q, device)?;
    let row = if on { store::run_row(q, device)? } else { None };
    let paused = row
        .as_ref()
        .and_then(|row| row.paused_reason.as_deref())
        .and_then(Paused::parse);
    let state = match row.as_ref().map(|row| row.state.as_str()) {
        None => PhoneState::Off,
        Some(REVIEW) => PhoneState::Review,
        Some(PAUSED) => PhoneState::Paused,
        Some(ACTIVE) | Some(_) => PhoneState::Active,
    };
    let counts_json = row.as_ref().and_then(|row| row.counts.clone());
    let (counts, mass_delete) = if paused == Some(Paused::MassDelete) {
        (
            None,
            counts_json.and_then(|json| serde_json::from_str(&json).ok()),
        )
    } else {
        (
            counts_json.and_then(|json| serde_json::from_str(&json).ok()),
            None,
        )
    };
    Ok(PhoneStatus {
        state,
        paused,
        last_run_at: row.and_then(|row| row.last_run_at),
        counts,
        mass_delete,
        issues: store::issues(q, device)?,
        target: target(q, device)?,
    })
}

/// The account suggested as the target: the phone's default, else the first writable account of
/// a sync adapter, else the local account.
pub fn suggested(accounts: &[Account]) -> Option<&Account> {
    accounts
        .iter()
        .find(|account| account.writable && account.is_default)
        .or_else(|| {
            accounts
                .iter()
                .find(|account| account.writable && !account.account_type.is_empty())
        })
        .or_else(|| accounts.iter().find(|account| account.writable))
}

/// The file in the app's private storage that lists the vaults syncing with this phone (FR-027):
/// a vault cannot see another one, the app's storage outlives both.
pub const VAULTS_FILE: &str = "contacts-phone-vaults.json";

fn read_vaults(path: &Path) -> BTreeSet<String> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Whether another vault already syncs with this phone's address book.
pub fn other_vault(path: &Path, vault: &str) -> bool {
    read_vaults(path).iter().any(|other| other != vault)
}

/// Adds or removes a vault from the list; a failure is logged, the list is only a hint.
pub fn mark_vault(path: &Path, vault: &str, syncing: bool) {
    let mut vaults = read_vaults(path);
    let changed = if syncing {
        vaults.insert(vault.to_owned())
    } else {
        vaults.remove(vault)
    };
    if !changed {
        return;
    }
    let written = serde_json::to_vec(&vaults)
        .map_err(|error| error.to_string())
        .and_then(|bytes| std::fs::write(path, bytes).map_err(|error| error.to_string()));
    if let Err(error) = written {
        log::warn!("contacts: the list of syncing vaults was not written: {error}");
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
