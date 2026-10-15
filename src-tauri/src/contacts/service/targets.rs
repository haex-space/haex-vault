//! Sync target methods of the service (spec 051 FR-018b to FR-018d): the phones of the vault that
//! can be targets, and setting and removing targets for contacts. Only the user decides where a
//! contact goes; a phone with the sync off is never offered and never becomes a new target, but
//! a target it already has stays until the user removes it.

use std::collections::BTreeSet;

use haex_crdt::rusqlite::params;
use serde::Deserialize;
use ts_rs::TS;

use super::{Caller, ContactsService};
use crate::contacts::access::require_user;
use crate::contacts::model::TargetInput;
use crate::contacts::phone::state::{self, PhoneSummary};
use crate::contacts::phone::Account;
use crate::contacts::{store, targets};
use crate::error::{HolziError, Result};
use crate::storage::query::Query;

/// Which contacts a change of targets is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    Contacts(Vec<String>),
    /// Every contact of a group, as it is now.
    Group(String),
}

/// What a phone does with a contact that stops having it as a target (FR-018d).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsOnPhone"
)]
#[serde(rename_all = "snake_case")]
pub enum OnPhone {
    /// The phone deletes the contact.
    Delete,
    /// The contact stays on the phone and is no longer synced there.
    Keep,
}

fn invalid(reason: &str) -> HolziError {
    HolziError::ContactsInvalid {
        reason: reason.to_owned(),
    }
}

/// The account a contact goes into on a phone: the one asked for, which must be a writable
/// account the phone reported, else the phone's target account.
pub(super) fn account_on(
    q: &mut impl Query,
    device: &str,
    account: Option<&Account>,
) -> Result<(String, String)> {
    if !state::is_phone(q, device)? {
        return Err(invalid("the device is no phone with the sync on"));
    }
    let chosen = match account {
        Some(account) => {
            let known = state::accounts(q, device)?;
            let offered = known.is_empty()
                || known.iter().any(|other| {
                    other.writable
                        && other.account_type == account.account_type
                        && other.name == account.name
                });
            if !offered {
                return Err(invalid("the phone has no such writable account"));
            }
            account.clone()
        }
        None => {
            state::target(q, device)?.ok_or_else(|| invalid("the phone has no target account"))?
        }
    };
    Ok((chosen.account_type, chosen.name))
}

/// Writes the targets of a new contact (FR-018c).
pub(super) fn write_new(
    tx: &mut haex_crdt::CrdtTransaction<'_>,
    contact_id: &str,
    inputs: &[TargetInput],
) -> Result<()> {
    for input in inputs {
        let account = account_on(tx, &input.device, input.account.as_ref())?;
        targets::set(tx, contact_id, &input.device, (&account.0, &account.1))?;
    }
    Ok(())
}

fn members(q: &mut impl Query, group_id: &str) -> Result<Vec<String>> {
    store::check_groups_exist(q, &BTreeSet::from([group_id.to_owned()]))?;
    Ok(q.query_map(
        "SELECT contact_id FROM haex_contacts_group_members WHERE group_id = ?1 \
         ORDER BY contact_id",
        params![group_id],
        |r| r.get(0),
    )?)
}

impl ContactsService {
    /// The phones of the vault with the sync on: the targets a contact can get.
    pub async fn phones(&self, caller: &Caller) -> Result<Vec<PhoneSummary>> {
        require_user(caller)?;
        self.db()
            .read(|q| state::phones(q).map_err(Into::into))
            .await
    }

    /// Makes a phone a target of the selected contacts, in the given account or the phone's
    /// target account (FR-018c). A contact that has the target already moves to that account for
    /// what is written from now on.
    pub async fn targets_set(
        &self,
        caller: &Caller,
        selection: Selection,
        device: String,
        account: Option<Account>,
    ) -> Result<()> {
        require_user(caller)?;
        self.db()
            .write(move |tx| {
                let account = account_on(tx, &device, account.as_ref())?;
                let ids = match selection {
                    Selection::Contacts(ids) => ids,
                    Selection::Group(group) => members(tx, &group)?,
                };
                for id in &ids {
                    store::group_ids_if_exists(tx, id)?.ok_or(HolziError::ContactsNotFound)?;
                    targets::set(tx, id, &device, (&account.0, &account.1))?;
                }
                Ok(())
            })
            .await
    }

    /// Removes a phone as a target of contacts (FR-018d): the phone deletes them or keeps them
    /// unsynced. Works for a phone with the sync off too.
    pub async fn targets_remove(
        &self,
        caller: &Caller,
        contact_ids: Vec<String>,
        device: String,
        on_phone: OnPhone,
    ) -> Result<()> {
        require_user(caller)?;
        self.db()
            .write(move |tx| {
                for id in &contact_ids {
                    targets::remove(tx, id, &device, on_phone == OnPhone::Keep)?;
                }
                Ok(())
            })
            .await
    }
}

#[cfg(test)]
#[path = "targets_tests.rs"]
mod tests;
