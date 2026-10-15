//! Sync targets and issues of contacts as rows (spec 051 FR-018b to FR-018d, FR-025,
//! data-model.md): which phones a contact goes to, and what did not reach one. The service and the
//! run of the sync with the phone both write targets through here.
//!
//! A removed target tells its phone what to do by what stays: with "delete" the phone's links of
//! the contact stay, and the phone deletes the raw contacts it finds linked without a target; with
//! "keep" the links go in the same transaction, and the phone leaves the raw contacts alone.

use std::collections::BTreeMap;

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;

use super::ids::{issue_id, target_id};
use super::model::{ContactIssue, ContactTarget};
use super::phone::IssueReason;
use crate::error::Result;
use crate::storage::query::Query;

/// Makes a phone a target of a contact, in the given account; an existing target changes its
/// account (raw contacts already written stay where they are, FR-018).
pub fn set(
    tx: &mut CrdtTransaction<'_>,
    contact_id: &str,
    device: &str,
    account: (&str, &str),
) -> Result<()> {
    let id = target_id(contact_id, device);
    let updated = tx.execute(
        "UPDATE haex_contacts_targets SET account_type = ?2, account_name = ?3 WHERE id = ?1",
        params![id, account.0, account.1],
    )?;
    if updated == 0 {
        tx.execute(
            "INSERT INTO haex_contacts_targets (id, contact_id, vault_device_uuid, account_type, \
             account_name) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, contact_id, device, account.0, account.1],
        )?;
    }
    Ok(())
}

/// Removes a phone as a target of a contact (FR-018d). With `keep` the phone only stops syncing
/// the contact: its links and its issue go too.
pub fn remove(
    tx: &mut CrdtTransaction<'_>,
    contact_id: &str,
    device: &str,
    keep: bool,
) -> Result<()> {
    tx.execute(
        "DELETE FROM haex_contacts_targets WHERE id = ?1",
        params![target_id(contact_id, device)],
    )?;
    if keep {
        tx.execute(
            "DELETE FROM haex_contacts_phone_links WHERE vault_device_uuid = ?1 AND contact_id = ?2",
            params![device, contact_id],
        )?;
        tx.execute(
            "DELETE FROM haex_contacts_phone_issues WHERE id = ?1",
            params![issue_id(device, contact_id)],
        )?;
    }
    Ok(())
}

/// The targets of a contact.
pub fn of(q: &mut impl Query, contact_id: &str) -> Result<Vec<ContactTarget>> {
    Ok(q.query_map(
        "SELECT vault_device_uuid, account_type, account_name FROM haex_contacts_targets \
         WHERE contact_id = ?1 ORDER BY vault_device_uuid",
        params![contact_id],
        |r| {
            Ok(ContactTarget {
                device: r.get(0)?,
                account_type: r.get(1)?,
                account_name: r.get(2)?,
            })
        },
    )?)
}

/// The open issues of a contact, on every phone.
pub fn issues_of(q: &mut impl Query, contact_id: &str) -> Result<Vec<ContactIssue>> {
    Ok(q.query_map(
        "SELECT vault_device_uuid, reason, at FROM haex_contacts_phone_issues \
         WHERE contact_id = ?1 ORDER BY vault_device_uuid",
        params![contact_id],
        |r| {
            Ok(ContactIssue {
                device: r.get(0)?,
                reason: IssueReason::parse(&r.get::<_, String>(1)?),
                at: r.get(2)?,
            })
        },
    )?)
}

/// What the list shows of targets and issues: the phones of each contact, and one issue reason
/// of each contact that has one.
pub struct Marks {
    pub targets: BTreeMap<String, Vec<String>>,
    pub issues: BTreeMap<String, IssueReason>,
}

pub fn marks(q: &mut impl Query) -> Result<Marks> {
    let mut targets: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (contact, device) in q.query_map(
        "SELECT contact_id, vault_device_uuid FROM haex_contacts_targets \
         ORDER BY contact_id, vault_device_uuid",
        &[],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
    )? {
        targets.entry(contact).or_default().push(device);
    }
    let mut issues = BTreeMap::new();
    for (contact, reason) in q.query_map(
        "SELECT contact_id, reason FROM haex_contacts_phone_issues ORDER BY contact_id, at",
        &[],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
    )? {
        issues
            .entry(contact)
            .or_insert_with(|| IssueReason::parse(&reason));
    }
    Ok(Marks { targets, issues })
}
