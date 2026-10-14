//! The SQL of the sync with a phone (spec 051, data-model.md): links, targets, issues, the state of
//! the run and the device-local list of raw contacts holzi linked. Only the run of this module
//! calls it.

use std::collections::BTreeMap;

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use serde::{Deserialize, Serialize};

use super::mapping::PhoneSide;
use crate::contacts::ids::{issue_id, link_id, target_id};
use crate::error::Result;
use crate::storage::query::Query;

/// A value of holzi as the last run left it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseValue {
    pub kind: String,
    pub label: String,
    pub value: String,
}

/// The state both sides agreed on after the last run (research R6): holzi's head and phone values,
/// this raw contact as holzi saw it, and which data row holds which holzi value.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Base {
    /// `updated_at` of the contact after the last run; another value means holzi changed it.
    pub holzi_updated_at: String,
    pub holzi_head: BTreeMap<String, String>,
    /// Value id → value; only kinds that go to a phone.
    pub holzi_values: BTreeMap<String, BaseValue>,
    pub phone: PhoneSide,
    /// Data id → holzi value id.
    pub mapping: BTreeMap<i64, String>,
}

/// A raw contact of this phone linked to a contact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub contact_id: String,
    pub raw_contact_id: i64,
    pub lookup_key: String,
    pub account_type: String,
    pub account_name: String,
    pub source_id: Option<String>,
    pub version: i64,
    pub read_only: bool,
    pub base: Base,
    pub photo_base: Option<String>,
}

/// The links of one phone.
pub fn links(q: &mut impl Query, device: &str) -> Result<Vec<Link>> {
    Ok(q.query_map(
        "SELECT contact_id, raw_contact_id, lookup_key, account_type, account_name, source_id, \
         version, read_only, base, photo_base FROM haex_contacts_phone_links \
         WHERE vault_device_uuid = ?1 ORDER BY raw_contact_id",
        params![device],
        |r| {
            let base: String = r.get(8)?;
            Ok(Link {
                contact_id: r.get(0)?,
                raw_contact_id: r.get(1)?,
                lookup_key: r.get(2)?,
                account_type: r.get(3)?,
                account_name: r.get(4)?,
                source_id: r.get(5)?,
                version: r.get(6)?,
                read_only: r.get::<_, i64>(7)? != 0,
                base: serde_json::from_str(&base).unwrap_or_default(),
                photo_base: r.get(9)?,
            })
        },
    )?)
}

/// Writes a link: an update when it exists, else an insert (a delete and an insert of the same id
/// in one transaction would let the delete win on other devices).
pub fn save_link(tx: &mut CrdtTransaction<'_>, device: &str, link: &Link) -> Result<()> {
    let id = link_id(device, link.raw_contact_id);
    let base = serde_json::to_string(&link.base).unwrap_or_default();
    let updated = tx.execute(
        "UPDATE haex_contacts_phone_links SET contact_id = ?2, lookup_key = ?3, account_type = ?4, \
         account_name = ?5, source_id = ?6, version = ?7, read_only = ?8, base = ?9, \
         photo_base = ?10 WHERE id = ?1",
        params![
            id,
            link.contact_id,
            link.lookup_key,
            link.account_type,
            link.account_name,
            link.source_id,
            link.version,
            i64::from(link.read_only),
            base,
            link.photo_base,
        ],
    )?;
    if updated == 0 {
        tx.execute(
            "INSERT INTO haex_contacts_phone_links (id, vault_device_uuid, contact_id, \
             raw_contact_id, lookup_key, account_type, account_name, source_id, version, \
             read_only, base, photo_base) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                id,
                device,
                link.contact_id,
                link.raw_contact_id,
                link.lookup_key,
                link.account_type,
                link.account_name,
                link.source_id,
                link.version,
                i64::from(link.read_only),
                base,
                link.photo_base,
            ],
        )?;
    }
    tx.execute(
        "INSERT OR REPLACE INTO haex_contacts_phone_known_no_sync (raw_contact_id, contact_id) \
         VALUES (?1, ?2)",
        params![link.raw_contact_id, link.contact_id],
    )?;
    Ok(())
}

/// Drops a link; with `forget` the raw contact is no longer known to this phone either, so a later
/// run takes it for a new contact.
pub fn drop_link(
    tx: &mut CrdtTransaction<'_>,
    device: &str,
    raw_contact_id: i64,
    forget: bool,
) -> Result<()> {
    tx.execute(
        "DELETE FROM haex_contacts_phone_links WHERE id = ?1",
        params![link_id(device, raw_contact_id)],
    )?;
    if forget {
        forget_raw(tx, raw_contact_id)?;
    }
    Ok(())
}

/// Removes a raw contact from the device-local list.
pub fn forget_raw(tx: &mut CrdtTransaction<'_>, raw_contact_id: i64) -> Result<()> {
    tx.execute(
        "DELETE FROM haex_contacts_phone_known_no_sync WHERE raw_contact_id = ?1",
        params![raw_contact_id],
    )?;
    Ok(())
}

/// The raw contacts this phone linked, with the contact each belonged to.
pub fn known(q: &mut impl Query) -> Result<BTreeMap<i64, String>> {
    Ok(q.query_map(
        "SELECT raw_contact_id, contact_id FROM haex_contacts_phone_known_no_sync",
        &[],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?
    .into_iter()
    .collect())
}

/// A sync target: the phone account a contact goes to on one phone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub contact_id: String,
    pub account_type: String,
    pub account_name: String,
}

/// The contacts with this phone as their target.
pub fn targets(q: &mut impl Query, device: &str) -> Result<Vec<Target>> {
    Ok(q.query_map(
        "SELECT contact_id, account_type, account_name FROM haex_contacts_targets \
         WHERE vault_device_uuid = ?1 ORDER BY contact_id",
        params![device],
        |r| {
            Ok(Target {
                contact_id: r.get(0)?,
                account_type: r.get(1)?,
                account_name: r.get(2)?,
            })
        },
    )?)
}

/// Makes this phone a target of a contact, in the given account.
pub fn set_target(tx: &mut CrdtTransaction<'_>, device: &str, target: &Target) -> Result<()> {
    let id = target_id(&target.contact_id, device);
    let updated = tx.execute(
        "UPDATE haex_contacts_targets SET account_type = ?2, account_name = ?3 WHERE id = ?1",
        params![id, target.account_type, target.account_name],
    )?;
    if updated == 0 {
        tx.execute(
            "INSERT INTO haex_contacts_targets (id, contact_id, vault_device_uuid, account_type, \
             account_name) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                id,
                target.contact_id,
                device,
                target.account_type,
                target.account_name
            ],
        )?;
    }
    Ok(())
}

/// Why a change did not reach the phone (FR-025).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsPhoneIssueReason"
)]
#[serde(rename_all = "snake_case")]
pub enum IssueReason {
    ReadOnly,
    AccountRemoved,
    Permission,
    Failed,
}

impl IssueReason {
    pub fn as_str(self) -> &'static str {
        match self {
            IssueReason::ReadOnly => "read_only",
            IssueReason::AccountRemoved => "account_removed",
            IssueReason::Permission => "permission",
            IssueReason::Failed => "failed",
        }
    }

    pub fn parse(text: &str) -> Self {
        match text {
            "read_only" => IssueReason::ReadOnly,
            "account_removed" => IssueReason::AccountRemoved,
            "permission" => IssueReason::Permission,
            _ => IssueReason::Failed,
        }
    }
}

/// Records why a contact's change did not reach this phone; one issue per contact and phone.
pub fn set_issue(
    tx: &mut CrdtTransaction<'_>,
    device: &str,
    contact_id: &str,
    reason: IssueReason,
    detail: Option<&str>,
    now: &str,
) -> Result<()> {
    let id = issue_id(device, contact_id);
    let updated = tx.execute(
        "UPDATE haex_contacts_phone_issues SET reason = ?2, detail = ?3, at = ?4 WHERE id = ?1",
        params![id, reason.as_str(), detail, now],
    )?;
    if updated == 0 {
        tx.execute(
            "INSERT INTO haex_contacts_phone_issues (id, vault_device_uuid, contact_id, reason, \
             detail, at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, device, contact_id, reason.as_str(), detail, now],
        )?;
    }
    Ok(())
}

/// Clears the issue of a contact once a change reached the phone.
pub fn clear_issue(tx: &mut CrdtTransaction<'_>, device: &str, contact_id: &str) -> Result<()> {
    tx.execute(
        "DELETE FROM haex_contacts_phone_issues WHERE id = ?1",
        params![issue_id(device, contact_id)],
    )?;
    Ok(())
}

/// An open issue of this phone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../src/types/bindings/",
    rename = "ContactsPhoneIssue"
)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub contact_id: String,
    pub reason: IssueReason,
    pub at: String,
}

pub fn issues(q: &mut impl Query, device: &str) -> Result<Vec<Issue>> {
    Ok(q.query_map(
        "SELECT contact_id, reason, at FROM haex_contacts_phone_issues \
         WHERE vault_device_uuid = ?1 ORDER BY at DESC",
        params![device],
        |r| {
            Ok(Issue {
                contact_id: r.get(0)?,
                reason: IssueReason::parse(&r.get::<_, String>(1)?),
                at: r.get(2)?,
            })
        },
    )?)
}

/// The state of the sync on one phone (data-model.md); no row while it is off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRow {
    pub state: String,
    pub paused_reason: Option<String>,
    pub last_run_at: Option<String>,
    pub counts: Option<String>,
}

pub fn run_row(q: &mut impl Query, device: &str) -> Result<Option<RunRow>> {
    Ok(q.query_row(
        "SELECT state, paused_reason, last_run_at, counts FROM haex_contacts_phone_runs \
         WHERE vault_device_uuid = ?1",
        params![device],
        |r| {
            Ok(RunRow {
                state: r.get(0)?,
                paused_reason: r.get(1)?,
                last_run_at: r.get(2)?,
                counts: r.get(3)?,
            })
        },
    )?)
}

pub fn save_run_row(tx: &mut CrdtTransaction<'_>, device: &str, row: &RunRow) -> Result<()> {
    let updated = tx.execute(
        "UPDATE haex_contacts_phone_runs SET state = ?2, paused_reason = ?3, last_run_at = ?4, \
         counts = ?5 WHERE vault_device_uuid = ?1",
        params![
            device,
            row.state,
            row.paused_reason,
            row.last_run_at,
            row.counts
        ],
    )?;
    if updated == 0 {
        tx.execute(
            "INSERT INTO haex_contacts_phone_runs (vault_device_uuid, state, paused_reason, \
             last_run_at, counts) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                device,
                row.state,
                row.paused_reason,
                row.last_run_at,
                row.counts
            ],
        )?;
    }
    Ok(())
}

pub fn delete_run_row(tx: &mut CrdtTransaction<'_>, device: &str) -> Result<()> {
    tx.execute(
        "DELETE FROM haex_contacts_phone_runs WHERE vault_device_uuid = ?1",
        params![device],
    )?;
    Ok(())
}

/// `updated_at` of every contact.
pub fn updated_at(q: &mut impl Query) -> Result<BTreeMap<String, String>> {
    Ok(
        q.query_map("SELECT id, updated_at FROM haex_contacts", &[], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?
        .into_iter()
        .collect(),
    )
}

/// When holzi last changed the columns of a row, as Unix milliseconds of the newest of their HLC
/// times (the row's time when a column has none); `None` without a time.
pub fn changed_millis(
    q: &mut impl Query,
    table: &str,
    id: &str,
    columns: &[&str],
) -> Result<Option<i64>> {
    let row: Option<(Option<String>, Option<String>)> = q.query_row(
        &format!("SELECT haex_hlc_no_sync, haex_column_hlcs_no_sync FROM {table} WHERE id = ?1"),
        params![id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let Some((row_hlc, column_hlcs)) = row else {
        return Ok(None);
    };
    let by_column: BTreeMap<String, String> = column_hlcs
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default();
    let newest = columns
        .iter()
        .filter_map(|column| by_column.get(*column).or(row_hlc.as_ref()))
        .filter_map(|hlc| hlc_millis(hlc))
        .max();
    Ok(newest.or_else(|| row_hlc.as_deref().and_then(hlc_millis)))
}

/// The physical time of an HLC timestamp (`<NTP64>/<node>`) in Unix milliseconds.
pub fn hlc_millis(hlc: &str) -> Option<i64> {
    let time = hlc.split('/').next()?.parse::<u64>().ok()?;
    let since_epoch = haex_crdt::uhlc::NTP64(time).to_duration();
    i64::try_from(since_epoch.as_millis()).ok()
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
