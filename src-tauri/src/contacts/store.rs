//! The SQL of the address book (spec 051, data-model.md). Only [`super::service`] calls it; the
//! access checks happen there. Writes touch only the rows they name: a value row is updated in
//! place and never deleted and inserted again, so a value of a kind this version does not know
//! survives every edit (FR-007, research R3).

use std::collections::{BTreeMap, BTreeSet};

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use uuid::Uuid;

use super::ids::member_id;
use super::kinds::{self, EMAIL, PHONE};
use super::model::{Contact, ContactHeader, Group, Head, Origin, OriginKind, Value};
use super::targets;
use crate::error::{HolziError, Result};
use crate::storage::query::Query;

/// The head columns in table order, matching [`Head`].
pub const HEAD_COLUMNS: [&str; 10] = [
    "display_name",
    "given_name",
    "family_name",
    "middle_name",
    "name_prefix",
    "name_suffix",
    "nickname",
    "organization",
    "job_title",
    "note",
];

const SELECT_CONTACT: &str = "SELECT id, display_name, given_name, family_name, middle_name, \
     name_prefix, name_suffix, nickname, organization, job_title, note, photo, origin_kind, \
     origin_device, origin_caller, origin_account, created_at, updated_at FROM haex_contacts";

struct ContactRow {
    id: String,
    head: Head,
    photo: Option<String>,
    origin: Origin,
    created_at: String,
    updated_at: String,
}

fn contact_row(r: &haex_crdt::rusqlite::Row<'_>) -> haex_crdt::rusqlite::Result<ContactRow> {
    let origin_kind: String = r.get(12)?;
    Ok(ContactRow {
        id: r.get(0)?,
        head: Head {
            display_name: r.get(1)?,
            given_name: r.get(2)?,
            family_name: r.get(3)?,
            middle_name: r.get(4)?,
            name_prefix: r.get(5)?,
            name_suffix: r.get(6)?,
            nickname: r.get(7)?,
            organization: r.get(8)?,
            job_title: r.get(9)?,
            note: r.get(10)?,
        },
        photo: r.get(11)?,
        origin: Origin {
            // A row of a newer version with an origin this one does not know reads as holzi.
            kind: OriginKind::parse(&origin_kind).unwrap_or(OriginKind::Holzi),
            device: r.get(13)?,
            caller: r.get(14)?,
            account: r.get(15)?,
        },
        created_at: r.get(16)?,
        updated_at: r.get(17)?,
    })
}

fn present(text: &Option<String>) -> Option<&str> {
    text.as_deref().map(str::trim).filter(|t| !t.is_empty())
}

/// The name the list shows: the stored display name, else the name parts, else the nickname, the
/// organization, the first phone number or e-mail address.
pub fn derived_name(head: &Head, phones: &[String], emails: &[String]) -> String {
    if let Some(name) = present(&head.display_name) {
        return name.to_owned();
    }
    let parts: Vec<&str> = [
        &head.name_prefix,
        &head.given_name,
        &head.middle_name,
        &head.family_name,
        &head.name_suffix,
    ]
    .into_iter()
    .filter_map(present)
    .collect();
    if !parts.is_empty() {
        return parts.join(" ");
    }
    present(&head.nickname)
        .or_else(|| present(&head.organization))
        .or_else(|| phones.first().map(String::as_str))
        .or_else(|| emails.first().map(String::as_str))
        .unwrap_or_default()
        .to_owned()
}

/// Whether a contact has anything to be found by: a name part, an organization, a phone number
/// or an e-mail address.
pub fn has_identity(head: &Head, phones: usize, emails: usize) -> bool {
    [
        &head.display_name,
        &head.given_name,
        &head.family_name,
        &head.middle_name,
        &head.name_prefix,
        &head.name_suffix,
        &head.nickname,
        &head.organization,
    ]
    .into_iter()
    .any(|field| present(field).is_some())
        || phones + emails > 0
}

/// Every value of a contact, ordered by kind and position.
pub fn values_of(q: &mut impl Query, contact_id: &str) -> Result<Vec<Value>> {
    Ok(q.query_map(
        "SELECT id, kind, label, value, position FROM haex_contacts_values \
         WHERE contact_id = ?1 ORDER BY kind, position, id",
        params![contact_id],
        |r| {
            let kind: String = r.get(1)?;
            Ok(Value {
                id: r.get(0)?,
                known: kinds::is_known(&kind),
                kind,
                label: r.get(2)?,
                value: r.get(3)?,
                position: r.get(4)?,
            })
        },
    )?)
}

/// The groups of a contact.
pub fn groups_of(q: &mut impl Query, contact_id: &str) -> Result<Vec<Group>> {
    Ok(q.query_map(
        "SELECT g.id, g.name FROM haex_contacts_groups g \
         JOIN haex_contacts_group_members m ON m.group_id = g.id \
         WHERE m.contact_id = ?1 ORDER BY g.name, g.id",
        params![contact_id],
        |r| {
            Ok(Group {
                id: r.get(0)?,
                name: r.get(1)?,
            })
        },
    )?)
}

/// The group ids of a contact, for the access checks; `None` when the contact does not exist.
pub fn group_ids_if_exists(q: &mut impl Query, contact_id: &str) -> Result<Option<Vec<String>>> {
    let exists = q
        .query_row(
            "SELECT COUNT(*) FROM haex_contacts WHERE id = ?1",
            params![contact_id],
            |r| r.get::<_, i64>(0),
        )?
        .unwrap_or(0)
        > 0;
    if !exists {
        return Ok(None);
    }
    Ok(Some(
        groups_of(q, contact_id)?
            .into_iter()
            .map(|group| group.id)
            .collect(),
    ))
}

/// One contact with its values and groups.
pub fn load(q: &mut impl Query, contact_id: &str) -> Result<Contact> {
    let row = q
        .query_row(
            &format!("{SELECT_CONTACT} WHERE id = ?1"),
            params![contact_id],
            contact_row,
        )?
        .ok_or(HolziError::ContactsNotFound)?;
    let values = values_of(q, contact_id)?;
    let texts = |kind: &str| -> Vec<String> {
        values
            .iter()
            .filter(|value| value.kind == kind)
            .map(|value| value.value.clone())
            .collect()
    };
    let name = derived_name(&row.head, &texts(PHONE), &texts(EMAIL));
    Ok(Contact {
        id: row.id,
        name,
        head: row.head,
        photo: row.photo,
        origin: row.origin,
        groups: groups_of(q, contact_id)?,
        targets: targets::of(q, contact_id)?,
        issues: targets::issues_of(q, contact_id)?,
        values,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// The headers of every contact: three queries, put together here.
pub fn headers(q: &mut impl Query) -> Result<Vec<ContactHeader>> {
    let rows = q.query_map(SELECT_CONTACT, &[], contact_row)?;
    let mut texts: BTreeMap<String, (Vec<String>, Vec<String>)> = BTreeMap::new();
    for (contact_id, kind, value) in q.query_map(
        "SELECT contact_id, kind, value FROM haex_contacts_values \
         WHERE kind IN ('phone', 'email') ORDER BY contact_id, kind, position, id",
        &[],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get(2)?)),
    )? {
        let entry = texts.entry(contact_id).or_default();
        if kind == PHONE {
            entry.0.push(value);
        } else {
            entry.1.push(value);
        }
    }
    let mut memberships: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (contact_id, group_id) in q.query_map(
        "SELECT contact_id, group_id FROM haex_contacts_group_members ORDER BY group_id",
        &[],
        |r| Ok((r.get::<_, String>(0)?, r.get(1)?)),
    )? {
        memberships.entry(contact_id).or_default().push(group_id);
    }
    let mut marks = targets::marks(q)?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let (phones, emails) = texts.remove(&row.id).unwrap_or_default();
            ContactHeader {
                name: derived_name(&row.head, &phones, &emails),
                groups: memberships.remove(&row.id).unwrap_or_default(),
                given_name: row.head.given_name,
                family_name: row.head.family_name,
                organization: row.head.organization,
                phones,
                emails,
                photo: row.photo,
                origin_kind: row.origin.kind,
                targets: marks.targets.remove(&row.id).unwrap_or_default(),
                issue: marks.issues.remove(&row.id),
                id: row.id,
            }
        })
        .collect())
}

/// Inserts a new contact row.
pub fn insert(
    tx: &mut CrdtTransaction<'_>,
    id: &str,
    head: &Head,
    origin: &Origin,
    now: &str,
) -> Result<()> {
    tx.execute(
        "INSERT INTO haex_contacts (id, display_name, given_name, family_name, middle_name, \
         name_prefix, name_suffix, nickname, organization, job_title, note, origin_kind, \
         origin_device, origin_caller, origin_account, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?16)",
        params![
            id,
            head.display_name,
            head.given_name,
            head.family_name,
            head.middle_name,
            head.name_prefix,
            head.name_suffix,
            head.nickname,
            head.organization,
            head.job_title,
            head.note,
            origin.kind.as_str(),
            origin.device,
            origin.caller,
            origin.account,
            now,
        ],
    )?;
    Ok(())
}

/// Sets one head column (`column` must be one of [`HEAD_COLUMNS`]).
pub fn set_head(
    tx: &mut CrdtTransaction<'_>,
    contact_id: &str,
    column: &str,
    value: Option<&str>,
) -> Result<()> {
    if !HEAD_COLUMNS.contains(&column) {
        return Err(HolziError::ContactsInvalid {
            reason: format!("unknown field {column}"),
        });
    }
    tx.execute(
        &format!("UPDATE haex_contacts SET {column} = ?1 WHERE id = ?2"),
        params![value, contact_id],
    )?;
    Ok(())
}

/// Sets the photo of a contact.
pub fn set_photo(tx: &mut CrdtTransaction<'_>, contact_id: &str, hash: Option<&str>) -> Result<()> {
    tx.execute(
        "UPDATE haex_contacts SET photo = ?1 WHERE id = ?2",
        params![hash, contact_id],
    )?;
    Ok(())
}

/// Stamps the time of the last change.
pub fn touch(tx: &mut CrdtTransaction<'_>, contact_id: &str, now: &str) -> Result<()> {
    tx.execute(
        "UPDATE haex_contacts SET updated_at = ?1 WHERE id = ?2",
        params![now, contact_id],
    )?;
    Ok(())
}

/// The next free position for a value of `kind` of a contact.
pub fn next_position(q: &mut impl Query, contact_id: &str, kind: &str) -> Result<i64> {
    Ok(q.query_row(
        "SELECT COALESCE(MAX(position) + 1, 0) FROM haex_contacts_values \
         WHERE contact_id = ?1 AND kind = ?2",
        params![contact_id, kind],
        |r| r.get::<_, i64>(0),
    )?
    .unwrap_or(0))
}

/// Inserts a value and returns its id.
pub fn insert_value(
    tx: &mut CrdtTransaction<'_>,
    contact_id: &str,
    kind: &str,
    label: &str,
    value: &str,
    position: i64,
) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    insert_value_as(tx, &id, contact_id, (kind, label, value), position)?;
    Ok(id)
}

/// Inserts a value under an id the caller chose (the sync with a phone maps rows to it first).
pub fn insert_value_as(
    tx: &mut CrdtTransaction<'_>,
    id: &str,
    contact_id: &str,
    (kind, label, value): (&str, &str, &str),
    position: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO haex_contacts_values (id, contact_id, kind, label, value, position) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, contact_id, kind, label, value, position],
    )?;
    Ok(())
}

/// The contact and kind of a stored value; `None` when it does not exist.
pub fn value_owner(q: &mut impl Query, value_id: &str) -> Result<Option<(String, String)>> {
    Ok(q.query_row(
        "SELECT contact_id, kind FROM haex_contacts_values WHERE id = ?1",
        params![value_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?)
}

/// Changes a stored value in place.
pub fn update_value(
    tx: &mut CrdtTransaction<'_>,
    value_id: &str,
    label: &str,
    value: &str,
    position: Option<i64>,
) -> Result<()> {
    tx.execute(
        "UPDATE haex_contacts_values SET label = ?1, value = ?2, \
         position = COALESCE(?3, position) WHERE id = ?4",
        params![label, value, position, value_id],
    )?;
    Ok(())
}

pub fn delete_value(tx: &mut CrdtTransaction<'_>, value_id: &str) -> Result<()> {
    tx.execute(
        "DELETE FROM haex_contacts_values WHERE id = ?1",
        params![value_id],
    )?;
    Ok(())
}

/// Puts a contact into exactly these groups: memberships not named go, missing ones are added. A
/// change stamps the contact, so a phone that syncs it sees the change (T096).
pub fn set_groups(
    tx: &mut CrdtTransaction<'_>,
    contact_id: &str,
    groups: &BTreeSet<String>,
) -> Result<()> {
    let current: BTreeSet<String> = groups_of(tx, contact_id)?
        .into_iter()
        .map(|group| group.id)
        .collect();
    if &current != groups {
        touch(tx, contact_id, &crate::passwords::clock::now())?;
    }
    for group in current.difference(groups) {
        tx.execute(
            "DELETE FROM haex_contacts_group_members WHERE id = ?1",
            params![member_id(group, contact_id)],
        )?;
    }
    for group in groups.difference(&current) {
        tx.execute(
            "INSERT INTO haex_contacts_group_members (id, group_id, contact_id) \
             VALUES (?1, ?2, ?3)",
            params![member_id(group, contact_id), group, contact_id],
        )?;
    }
    Ok(())
}

/// Fails with `ContactsInvalid` unless every group id exists.
pub fn check_groups_exist(q: &mut impl Query, groups: &BTreeSet<String>) -> Result<()> {
    for group in groups {
        let exists = q
            .query_row(
                "SELECT COUNT(*) FROM haex_contacts_groups WHERE id = ?1",
                params![group],
                |r| r.get::<_, i64>(0),
            )?
            .unwrap_or(0)
            > 0;
        if !exists {
            return Err(HolziError::ContactsInvalid {
                reason: "unknown group".to_owned(),
            });
        }
    }
    Ok(())
}

/// The photo hash of a contact.
pub fn photo_of(q: &mut impl Query, contact_id: &str) -> Result<Option<String>> {
    Ok(q.query_row(
        "SELECT photo FROM haex_contacts WHERE id = ?1",
        params![contact_id],
        |r| r.get::<_, Option<String>>(0),
    )?
    .flatten())
}

/// Deletes a contact; its values, cards, memberships, targets, links and issues go with it.
pub fn delete(tx: &mut CrdtTransaction<'_>, contact_id: &str) -> Result<()> {
    tx.execute(
        "DELETE FROM haex_contacts WHERE id = ?1",
        params![contact_id],
    )?;
    Ok(())
}

/// Every group.
pub fn all_groups(q: &mut impl Query) -> Result<Vec<Group>> {
    Ok(q.query_map(
        "SELECT id, name FROM haex_contacts_groups ORDER BY name, id",
        &[],
        |r| {
            Ok(Group {
                id: r.get(0)?,
                name: r.get(1)?,
            })
        },
    )?)
}
