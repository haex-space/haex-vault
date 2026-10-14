//! What a run must not do on its own (spec 051 FR-016, FR-026, research R9): delete more than 10
//! contacts that are also more than 10 % of the contacts linked on this phone, in either direction,
//! or take an account that left the phone for deleted contacts.

use std::collections::BTreeSet;

use tauri_plugin_holzi_android::contacts::{PhoneAccount, RawIndex};

use super::store::Link;

/// More deletions than this need the person's decision, if they are also more than 10 %.
pub const MASS_DELETE_COUNT: usize = 10;

/// Whether `deletions` out of `linked` contacts need the person's decision.
pub fn is_mass_delete(deletions: usize, linked: usize) -> bool {
    deletions > MASS_DELETE_COUNT && deletions * 10 > linked
}

/// The accounts (type, name) whose raw contacts are all gone while the phone no longer lists the
/// account: the account was removed, its contacts were not deleted (FR-026).
pub fn vanished_accounts(
    gone: &[Link],
    index: &[RawIndex],
    accounts: &[PhoneAccount],
) -> BTreeSet<(String, String)> {
    let listed: BTreeSet<(&str, &str)> = accounts
        .iter()
        .map(|account| (account.account_type.as_str(), account.name.as_str()))
        .collect();
    let present: BTreeSet<(&str, &str)> = index
        .iter()
        .filter(|raw| !raw.deleted)
        .map(|raw| (raw.account_type.as_str(), raw.account_name.as_str()))
        .collect();
    gone.iter()
        .map(|link| (link.account_type.as_str(), link.account_name.as_str()))
        .filter(|account| !listed.contains(account) && !present.contains(account))
        .map(|(kind, name)| (kind.to_owned(), name.to_owned()))
        .collect()
}

#[cfg(test)]
#[path = "guard_tests.rs"]
mod tests;
