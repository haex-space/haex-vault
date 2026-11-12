//! Entries (spec 034, US1): the overview without secrets, the detail with flags, creating and the
//! partial update with the conflict check (research R7 and R15). Reads take any [`Query`], writes
//! a [`CrdtTransaction`].
//!
//! An update writes only the columns that really change: haex-crdt merges per column, so two
//! devices that change different fields of one entry both keep their change (FR-037). Writes use
//! check-then-write instead of `ON CONFLICT DO UPDATE` (see `storage/preferences.rs`).
//!
//! Maintainability exception (spaex 500-LoC rule): reads, create and update of an entry share the
//! column set and the TOTP rules, so they have stayed in one file. Concrete split plan, if this
//! grows further: move the update path (`Stored`, `update_item`, `apply_otp`, `write_parts`,
//! `replace_key_values`) into `items_update.rs`, leaving the reads and `create_item` here.

use std::collections::{HashMap, HashSet};

use haex_crdt::rusqlite::{params, ToSql};
use haex_crdt::CrdtTransaction;
use uuid::Uuid;

use super::access::{sees_owned, Caller, ItemState};
use super::kind::{EntryType, Kinds};
use super::model::{
    AgentHeader, AttachmentView, GroupRow, ItemDetail, ItemHeader, ItemInput, ItemPatch,
    KeyValuePatch, KeyValueView, Overview, Patch, TagRef, TagRow,
};
use super::sets::Sets;
use super::totp::{self, otp_state, OtpParams};
use super::{clock, passkeys, snapshots, tags};
use crate::error::{HolziError, Result};
use crate::storage::query::Query;

fn conflict(reason: &str) -> HolziError {
    HolziError::PasswordsConflict {
        reason: reason.to_string(),
    }
}

fn invalid(reason: &str) -> HolziError {
    HolziError::InvalidInput {
        reason: reason.to_string(),
    }
}

type HeaderRow = (String, ItemHeader);

/// The headers (all, or the one of `only`) with tags and counts attached. Never selects a note, a
/// secret or a binary.
pub(super) fn load_headers(q: &mut impl Query, only: Option<&str>) -> Result<Vec<ItemHeader>> {
    let filter = if only.is_some() {
        "WHERE d.id = ?1"
    } else {
        ""
    };
    let sql = format!(
        "SELECT d.id, d.title, d.username, d.url, d.icon, d.color, d.expires_at, \
                d.created_at, d.updated_at, \
                CASE WHEN d.password IS NOT NULL AND d.password <> '' THEN 1 ELSE 0 END, \
                CASE WHEN d.otp_secret IS NOT NULL AND trim(d.otp_secret) <> '' THEN 1 ELSE 0 END, \
                CASE WHEN g.id IS NULL THEN NULL ELSE gi.group_id END, \
                gi.trashed_from_group_id, d.owner, d.kind, \
                pd.pass_kind, pd.start_date, pd.start_at, pd.start_zone, pd.parent_id \
         FROM haex_passwords_item_details d \
         LEFT JOIN haex_passwords_pass_details pd ON pd.item_id = d.id \
         LEFT JOIN haex_passwords_group_items gi ON gi.item_id = d.id \
         LEFT JOIN haex_passwords_groups g ON g.id = gi.group_id \
         {filter} ORDER BY d.created_at, d.id"
    );
    let owned = only.map(str::to_owned);
    let bound: Vec<&dyn ToSql> = owned.iter().map(|id| id as &dyn ToSql).collect();
    let rows: Vec<HeaderRow> = q.query_map(&sql, &bound, |r| {
        let id: String = r.get(0)?;
        Ok((
            id.clone(),
            ItemHeader {
                id,
                title: r.get(1)?,
                username: r.get(2)?,
                url: r.get(3)?,
                icon: r.get(4)?,
                color: r.get(5)?,
                expires_at: r.get(6)?,
                created_at: r.get(7)?,
                updated_at: r.get(8)?,
                has_password: r.get::<_, i64>(9)? != 0,
                has_totp: r.get::<_, i64>(10)? != 0,
                group_id: r.get(11)?,
                trashed_from_group_id: r.get(12)?,
                owner: r.get(13)?,
                kind: EntryType::parse(&r.get::<_, String>(14)?),
                pass_kind: r.get(15)?,
                start_date: r.get(16)?,
                start_at: r.get(17)?,
                start_zone: r.get(18)?,
                parent_id: r.get(19)?,
                tags: Vec::new(),
                passkey_count: 0,
                attachment_count: 0,
            },
        ))
    })?;
    let mut headers: Vec<ItemHeader> = rows.into_iter().map(|(_, h)| h).collect();
    // A pass whose detail row is missing (it has not arrived by sync yet) reads as `other`
    // (spec 092, data-model.md); the next write of the pass repairs the row.
    for header in headers
        .iter_mut()
        .filter(|h| h.kind.is_pass() && h.pass_kind.is_none())
    {
        header.pass_kind = Some("other".to_owned());
    }
    attach_tags_and_counts(q, &mut headers, only)?;
    Ok(headers)
}

fn attach_tags_and_counts(
    q: &mut impl Query,
    headers: &mut [ItemHeader],
    only: Option<&str>,
) -> Result<()> {
    let owned = only.map(str::to_owned);
    let bound: Vec<&dyn ToSql> = owned.iter().map(|id| id as &dyn ToSql).collect();
    let (tag_filter, item_filter) = if only.is_some() {
        ("WHERE it.item_id = ?1", "AND item_id = ?1")
    } else {
        ("", "")
    };
    let tag_rows: Vec<(String, TagRef)> = q.query_map(
        &format!(
            "SELECT it.item_id, t.id, t.name, t.color FROM haex_passwords_item_tags it \
             JOIN haex_passwords_tags t ON t.id = it.tag_id {tag_filter} \
             ORDER BY t.name COLLATE NOCASE, t.id"
        ),
        &bound,
        |r| {
            Ok((
                r.get(0)?,
                TagRef {
                    id: r.get(1)?,
                    name: r.get(2)?,
                    color: r.get(3)?,
                },
            ))
        },
    )?;
    let mut by_item: HashMap<String, Vec<TagRef>> = HashMap::new();
    for (item, tag) in tag_rows {
        by_item.entry(item).or_default().push(tag);
    }
    let passkeys: HashMap<String, u32> = q
        .query_map(
            &format!(
                "SELECT item_id, COUNT(*) FROM haex_passwords_passkeys \
                 WHERE item_id IS NOT NULL {item_filter} GROUP BY item_id"
            ),
            &bound,
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?)),
        )?
        .into_iter()
        .collect();
    let attachments: HashMap<String, u32> = q
        .query_map(
            &format!(
                "SELECT item_id, COUNT(*) FROM haex_passwords_item_binaries \
                 WHERE 1 = 1 {item_filter} GROUP BY item_id"
            ),
            &bound,
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?)),
        )?
        .into_iter()
        .collect();
    for header in headers {
        header.tags = by_item.remove(&header.id).unwrap_or_default();
        header.passkey_count = passkeys.get(&header.id).copied().unwrap_or(0);
        header.attachment_count = attachments.get(&header.id).copied().unwrap_or(0);
    }
    Ok(())
}

/// Every entry (the trash included), every folder and every tag with its count: the user's
/// overview. No secret, note or binary is selected.
pub fn load_overview(q: &mut impl Query) -> Result<Overview> {
    let headers = load_headers(q, None)?;
    let groups = q.query_map(
        "SELECT id, name, description, icon, color, sort_order, parent_id, trashed_from_parent_id \
         FROM haex_passwords_groups ORDER BY COALESCE(sort_order, 0), name COLLATE NOCASE, id",
        &[],
        |r| {
            Ok(GroupRow {
                id: r.get(0)?,
                name: r.get(1)?,
                description: r.get(2)?,
                icon: r.get(3)?,
                color: r.get(4)?,
                sort_order: r.get(5)?,
                parent_id: r.get(6)?,
                trashed_from_parent_id: r.get(7)?,
            })
        },
    )?;
    let tags = q.query_map(
        "SELECT t.id, t.name, t.color, COUNT(it.id) FROM haex_passwords_tags t \
         LEFT JOIN haex_passwords_item_tags it ON it.tag_id = t.id \
         GROUP BY t.id, t.name, t.color ORDER BY t.name COLLATE NOCASE, t.id",
        &[],
        |r| {
            Ok(TagRow {
                id: r.get(0)?,
                name: r.get(1)?,
                color: r.get(2)?,
                item_count: r.get(3)?,
            })
        },
    )?;
    Ok(Overview {
        headers,
        groups,
        tags,
    })
}

struct OtpColumns {
    note: Option<String>,
    autofill_aliases: Option<String>,
    secret: Option<String>,
    digits: Option<i64>,
    period: Option<i64>,
    algorithm: Option<String>,
}

/// The entry with flags instead of secrets (the custom values excepted, see `KeyValueView`), or
/// `None` when it does not exist.
pub fn get_item(q: &mut impl Query, id: &str) -> Result<Option<ItemDetail>> {
    let Some(header) = load_headers(q, Some(id))?.into_iter().next() else {
        return Ok(None);
    };
    let Some(cols) = q.query_row(
        "SELECT note, autofill_aliases, otp_secret, otp_digits, otp_period, otp_algorithm \
         FROM haex_passwords_item_details WHERE id = ?1",
        params![id],
        |r| {
            Ok(OtpColumns {
                note: r.get(0)?,
                autofill_aliases: r.get(1)?,
                secret: r.get(2)?,
                digits: r.get(3)?,
                period: r.get(4)?,
                algorithm: r.get(5)?,
            })
        },
    )?
    else {
        return Ok(None);
    };
    let key_values = q.query_map(
        "SELECT id, key, value, CASE WHEN value IS NOT NULL AND value <> '' THEN 1 ELSE 0 END \
         FROM haex_passwords_item_key_values WHERE item_id = ?1 ORDER BY rowid",
        params![id],
        |r| {
            Ok(KeyValueView {
                id: r.get(0)?,
                key: r.get(1)?,
                value: r.get(2)?,
                has_value: r.get::<_, i64>(3)? != 0,
            })
        },
    )?;
    let attachments = q.query_map(
        "SELECT ib.id, ib.file_name, b.size, ib.binary_hash \
         FROM haex_passwords_item_binaries ib \
         JOIN haex_passwords_binaries b ON b.hash = ib.binary_hash \
         WHERE ib.item_id = ?1 ORDER BY ib.rowid",
        params![id],
        |r| {
            Ok(AttachmentView {
                id: r.get(0)?,
                file_name: r.get(1)?,
                size: r.get::<_, i64>(2)?.max(0) as u64,
                binary_hash: r.get(3)?,
            })
        },
    )?;
    let has_secret = cols.secret.as_deref().is_some_and(|s| !s.trim().is_empty());
    let otp_state = otp_state(
        cols.secret.as_deref(),
        cols.digits,
        cols.period,
        cols.algorithm.as_deref(),
    );
    Ok(Some(ItemDetail {
        header,
        note: cols.note,
        autofill_aliases: cols.autofill_aliases,
        otp_digits: cols.digits.and_then(|d| u32::try_from(d).ok()),
        otp_period: cols.period.and_then(|p| u32::try_from(p).ok()),
        otp_algorithm: cols.algorithm,
        has_otp_secret: has_secret,
        otp_state,
        key_values,
        attachments,
        passkeys: passkeys::list_for_item(q, id)?,
        references: Default::default(),
    }))
}

/// The parameters of the stored TOTP of an entry: `PasswordsNotFound` for a missing entry,
/// `InvalidInput` for a missing or invalid secret, digits, period or algorithm.
pub fn otp_params_of(q: &mut impl Query, id: &str) -> Result<OtpParams> {
    let cols = q
        .query_row(
            "SELECT otp_secret, otp_digits, otp_period, otp_algorithm \
             FROM haex_passwords_item_details WHERE id = ?1",
            params![id],
            |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, Option<i64>>(1)?,
                    r.get::<_, Option<i64>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            },
        )?
        .ok_or(HolziError::PasswordsNotFound)?;
    let secret = cols.0.unwrap_or_default();
    totp::stored_params(&secret, cols.1, cols.2, cols.3.as_deref())
}

/// Creates an entry with its tags, custom fields and folder in the caller's transaction. A secret
/// that is not valid is refused (FR-003); a missing title is fine (FR-001).
pub fn create_item(
    tx: &mut CrdtTransaction<'_>,
    input: &ItemInput,
    group_id: Option<&str>,
) -> Result<String> {
    create_item_as(tx, input, group_id, &EntryType::Login, |_, _| Ok(()))
}

/// [`create_item`] for an entry of any type: the row is written with `kind`, and `details` writes
/// what belongs to the type (a pass writes its detail row and codes) before the first state is
/// taken, so the state already holds it (spec 092, FR-032).
pub fn create_item_as(
    tx: &mut CrdtTransaction<'_>,
    input: &ItemInput,
    group_id: Option<&str>,
    kind: &EntryType,
    details: impl FnOnce(&mut CrdtTransaction<'_>, &str) -> Result<()>,
) -> Result<String> {
    let otp = match input.otp_secret.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(text) => Some(totp::resolve(
            text,
            input.otp_digits.map(i64::from),
            input.otp_period.map(i64::from),
            input.otp_algorithm.as_deref(),
        )?),
        None => None,
    };
    if let Some(group) = group_id {
        let exists = tx
            .query_row(
                "SELECT COUNT(*) FROM haex_passwords_groups WHERE id = ?1",
                params![group],
                |r| r.get::<_, i64>(0),
            )?
            .unwrap_or(0);
        if exists == 0 {
            return Err(invalid("group"));
        }
    }
    super::groups::check_place(tx, kind, group_id)?;
    let id = Uuid::new_v4().to_string();
    let now = clock::now();
    let (secret, digits, period, algorithm) = match &otp {
        Some(p) => (
            Some(p.normalised_secret().to_string()),
            i64::from(p.digits),
            i64::from(p.period),
            p.algorithm.as_str(),
        ),
        None => (None, 6, 30, "SHA1"),
    };
    tx.execute(
        "INSERT INTO haex_passwords_item_details \
         (id, title, username, password, note, icon, color, url, otp_secret, otp_digits, \
          otp_period, otp_algorithm, expires_at, autofill_aliases, created_at, updated_at, kind) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15, ?16)",
        params![
            id,
            input.title,
            input.username,
            input.password,
            input.note,
            input.icon,
            input.color,
            input.url,
            secret,
            digits,
            period,
            algorithm,
            input.expires_at,
            input.autofill_aliases,
            now,
            kind.as_text(),
        ],
    )?;
    tags::set_item_tags(tx, &id, &input.tags)?;
    for field in &input.key_values {
        if field.key.trim().is_empty() {
            continue;
        }
        tx.execute(
            "INSERT INTO haex_passwords_item_key_values (id, item_id, key, value, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![Uuid::new_v4().to_string(), id, field.key, field.value, now],
        )?;
    }
    if let Some(group) = group_id {
        tx.execute(
            "INSERT INTO haex_passwords_group_items (item_id, group_id) VALUES (?1, ?2)",
            params![id, group],
        )?;
    }
    details(tx, &id)?;
    snapshots::take_snapshot(tx, &id)?;
    Ok(id)
}

/// The columns of an entry as an update compares them.
struct Stored {
    title: Option<String>,
    username: Option<String>,
    password: Option<String>,
    note: Option<String>,
    url: Option<String>,
    icon: Option<String>,
    color: Option<String>,
    expires_at: Option<String>,
    autofill_aliases: Option<String>,
    secret: Option<String>,
    digits: Option<i64>,
    period: Option<i64>,
    algorithm: Option<String>,
    updated_at: Option<String>,
}

/// Applies a partial update to an entry if `expected_updated_at` is still its token, and returns
/// the new token. A missing entry is `PasswordsConflict { deleted }`, a changed one `{ changed }`.
/// The TOTP is validated only when the patch touches it, so an entry with an invalid stored secret
/// stays editable (FR-003).
pub fn update_item(
    tx: &mut CrdtTransaction<'_>,
    id: &str,
    expected_updated_at: &str,
    patch: &ItemPatch,
) -> Result<String> {
    update_item_with(tx, id, expected_updated_at, patch, |_, _| Ok(()))
}

/// [`update_item`] for an entry whose type has more to write: `details` runs after the shared
/// columns and before the state is taken (spec 092, a pass writes its detail row and codes).
pub fn update_item_with(
    tx: &mut CrdtTransaction<'_>,
    id: &str,
    expected_updated_at: &str,
    patch: &ItemPatch,
    details: impl FnOnce(&mut CrdtTransaction<'_>, &str) -> Result<()>,
) -> Result<String> {
    let stored = tx
        .query_row(
            "SELECT title, username, password, note, url, icon, color, expires_at, \
                    autofill_aliases, otp_secret, otp_digits, otp_period, otp_algorithm, \
                    updated_at FROM haex_passwords_item_details WHERE id = ?1",
            params![id],
            |r| {
                Ok(Stored {
                    title: r.get(0)?,
                    username: r.get(1)?,
                    password: r.get(2)?,
                    note: r.get(3)?,
                    url: r.get(4)?,
                    icon: r.get(5)?,
                    color: r.get(6)?,
                    expires_at: r.get(7)?,
                    autofill_aliases: r.get(8)?,
                    secret: r.get(9)?,
                    digits: r.get(10)?,
                    period: r.get(11)?,
                    algorithm: r.get(12)?,
                    updated_at: r.get(13)?,
                })
            },
        )?
        .ok_or_else(|| conflict("deleted"))?;
    if stored.updated_at.as_deref() != Some(expected_updated_at) {
        return Err(conflict("changed"));
    }

    let mut sets = Sets::default();
    sets.text("title", &stored.title, &patch.title);
    sets.text("username", &stored.username, &patch.username);
    sets.text("password", &stored.password, &patch.password);
    sets.text("note", &stored.note, &patch.note);
    sets.text("url", &stored.url, &patch.url);
    sets.text("icon", &stored.icon, &patch.icon);
    sets.text("color", &stored.color, &patch.color);
    sets.text("expires_at", &stored.expires_at, &patch.expires_at);
    sets.text(
        "autofill_aliases",
        &stored.autofill_aliases,
        &patch.autofill_aliases,
    );
    apply_otp(&mut sets, &stored, patch)?;

    // ponytail: the token is the update time in milliseconds, made strictly increasing per entry
    // (ceiling: two devices writing within one millisecond are told apart by sync, not by this
    // check; upgrade path: compare the row HLC).
    let new_token = clock::now_after(stored.updated_at.as_deref());
    sets.push("updated_at", new_token.clone());
    sets.execute(tx, "haex_passwords_item_details", "id", id)?;

    if let Some(names) = &patch.tags {
        tags::set_item_tags(tx, id, names)?;
    }
    if let Some(fields) = &patch.key_values {
        replace_key_values(tx, id, fields, &new_token)?;
    }
    details(tx, id)?;
    snapshots::take_snapshot(tx, id)?;
    Ok(new_token)
}

/// The OTP part of an update: validated and written only when the patch touches a TOTP field.
fn apply_otp(sets: &mut Sets, stored: &Stored, patch: &ItemPatch) -> Result<()> {
    let touched = !matches!(
        (
            &patch.otp_secret,
            &patch.otp_digits,
            &patch.otp_period,
            &patch.otp_algorithm
        ),
        (Patch::Keep, Patch::Keep, Patch::Keep, Patch::Keep)
    );
    if !touched {
        return Ok(());
    }
    let number = |patch: &Patch<u32>, current: Option<i64>, fresh_secret: bool| match patch {
        Patch::Set(v) => Some(i64::from(*v)),
        Patch::Clear => None,
        Patch::Keep => {
            if fresh_secret {
                None
            } else {
                current
            }
        }
    };
    let fresh_secret = matches!(patch.otp_secret, Patch::Set(_));
    let digits = number(&patch.otp_digits, stored.digits, fresh_secret);
    let period = number(&patch.otp_period, stored.period, fresh_secret);
    let algorithm = match &patch.otp_algorithm {
        Patch::Set(v) => Some(v.clone()),
        Patch::Clear => None,
        Patch::Keep if fresh_secret => None,
        Patch::Keep => stored.algorithm.clone(),
    };
    let secret_text = match &patch.otp_secret {
        Patch::Set(text) => Some(text.clone()),
        Patch::Clear => None,
        Patch::Keep => stored.secret.clone(),
    };
    let Some(text) = secret_text.filter(|t| !t.trim().is_empty()) else {
        // No secret: the parts are only checked for their ranges.
        let (d, p, a) = totp::validate_parts(digits, period, algorithm.as_deref())?;
        if stored.secret.is_some() {
            sets.push("otp_secret", Option::<String>::None);
        }
        write_parts(sets, stored, d, p, a.as_str());
        return Ok(());
    };
    let params = totp::resolve(&text, digits, period, algorithm.as_deref())?;
    let normalised = params.normalised_secret().to_string();
    if stored.secret.as_deref() != Some(normalised.as_str()) {
        sets.push("otp_secret", normalised);
    }
    write_parts(
        sets,
        stored,
        params.digits,
        params.period,
        params.algorithm.as_str(),
    );
    Ok(())
}

fn write_parts(sets: &mut Sets, stored: &Stored, digits: u32, period: u32, algorithm: &str) {
    if stored.digits != Some(i64::from(digits)) {
        sets.push("otp_digits", i64::from(digits));
    }
    if stored.period != Some(i64::from(period)) {
        sets.push("otp_period", i64::from(period));
    }
    if stored.algorithm.as_deref() != Some(algorithm) {
        sets.push("otp_algorithm", algorithm.to_string());
    }
}

/// Makes the custom fields of an entry the given list: rows with an empty key are dropped, a field
/// with an `id` keeps its row (a missing `value` keeps the stored one), others are created, stored
/// fields that are not listed are deleted.
fn replace_key_values(
    tx: &mut CrdtTransaction<'_>,
    item_id: &str,
    fields: &[KeyValuePatch],
    now: &str,
) -> Result<()> {
    let existing: Vec<(String, Option<String>, Option<String>)> = tx.query_map(
        "SELECT id, key, value FROM haex_passwords_item_key_values WHERE item_id = ?1",
        params![item_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let mut kept = Vec::new();
    for field in fields.iter().filter(|f| !f.key.trim().is_empty()) {
        let stored = field
            .id
            .as_deref()
            .and_then(|id| existing.iter().find(|(stored_id, _, _)| stored_id == id));
        match stored {
            Some((stored_id, stored_key, stored_value)) => {
                kept.push(stored_id.clone());
                let mut sets = Sets::default();
                if stored_key.as_deref() != Some(field.key.as_str()) {
                    sets.push("key", field.key.clone());
                }
                if let Some(value) = &field.value {
                    if stored_value.as_deref() != Some(value.as_str()) {
                        sets.push("value", value.clone());
                    }
                }
                if sets.is_empty() {
                    continue;
                }
                sets.push("updated_at", now.to_string());
                sets.execute(tx, "haex_passwords_item_key_values", "id", stored_id)?;
            }
            None => {
                tx.execute(
                    "INSERT INTO haex_passwords_item_key_values (id, item_id, key, value, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![Uuid::new_v4().to_string(), item_id, field.key, field.value, now],
                )?;
            }
        }
    }
    for (id, _, _) in existing.iter().filter(|(id, _, _)| !kept.contains(id)) {
        tx.execute(
            "DELETE FROM haex_passwords_item_key_values WHERE id = ?1",
            params![id],
        )?;
    }
    Ok(())
}

/// What the access rules need about a stored entry, owned; [`StoredState::view`] lends it as an
/// [`ItemState`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredState {
    pub tags: Vec<String>,
    /// Its folder is the trash or below it.
    pub in_trash: bool,
    /// The holzi function it belongs to (rule Z14), `None` for the user's own.
    pub owner: Option<String>,
    /// Whether it is a login; a pass and an entry of an unknown type are for the user's own window
    /// alone (spec 092, FR-024).
    pub login: bool,
}

impl StoredState {
    pub fn view(&self) -> ItemState<'_> {
        ItemState {
            tags: &self.tags,
            in_trash: self.in_trash,
            owner: self.owner.as_deref(),
            login: self.login,
        }
    }
}

/// The state the access rules need of an entry; `None` for a missing entry.
pub fn item_state(q: &mut impl Query, id: &str) -> Result<Option<StoredState>> {
    let Some((owner, kind)) = q.query_row(
        "SELECT owner, kind FROM haex_passwords_item_details WHERE id = ?1",
        params![id],
        |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, String>(1)?)),
    )?
    else {
        return Ok(None);
    };
    let in_trash = q
        .query_row(
            "WITH RECURSIVE up(id, parent_id) AS ( \
               SELECT g.id, g.parent_id FROM haex_passwords_group_items gi \
               JOIN haex_passwords_groups g ON g.id = gi.group_id WHERE gi.item_id = ?1 \
               UNION \
               SELECT p.id, p.parent_id FROM haex_passwords_groups p JOIN up ON p.id = up.parent_id) \
             SELECT COUNT(*) FROM up WHERE id = ?2",
            params![id, super::TRASH_GROUP_ID],
            |r| r.get::<_, i64>(0),
        )?
        .unwrap_or(0)
        > 0;
    Ok(Some(StoredState {
        tags: tags::names_of_item(q, id)?,
        in_trash,
        owner,
        login: EntryType::parse(&kind).is_login(),
    }))
}

/// The ids of the entries in the trash: those whose folder is the trash or lies below it.
pub fn trashed_item_ids(q: &mut impl Query) -> Result<HashSet<String>> {
    Ok(q.query_map(
        "WITH RECURSIVE trash_groups(id) AS ( \
           SELECT ?1 \
           UNION \
           SELECT g.id FROM haex_passwords_groups g JOIN trash_groups t ON g.parent_id = t.id) \
         SELECT gi.item_id FROM haex_passwords_group_items gi \
         JOIN trash_groups tg ON tg.id = gi.group_id",
        params![super::TRASH_GROUP_ID],
        |r| r.get::<_, String>(0),
    )?
    .into_iter()
    .collect())
}

/// The headers a caller from outside may see: entries that are not in the trash and whose tags
/// meet the scope (Z4, Z13). Only logins (spec 092, FR-024): a card or a ticket is no credential,
/// and an entry of a type this build does not know is not offered either.
pub fn headers_in_scope(
    q: &mut impl Query,
    scope: &super::access::Scope,
    caller: &Caller,
) -> Result<Vec<ItemHeader>> {
    headers_in_scope_of(q, scope, caller, Kinds::Logins)
}

/// [`headers_in_scope`] for the asked types (spec 092): the reader says it wants passes or both.
pub fn headers_in_scope_of(
    q: &mut impl Query,
    scope: &super::access::Scope,
    caller: &Caller,
    kinds: Kinds,
) -> Result<Vec<ItemHeader>> {
    let trashed = trashed_item_ids(q)?;
    Ok(load_headers(q, None)?
        .into_iter()
        .filter(|h| kinds.admits(&h.kind))
        .filter(|h| !trashed.contains(&h.id))
        .filter(|h| sees_owned(caller, h.owner.as_deref()))
        .filter(|h| scope.covers(&h.tags.iter().map(|t| t.name.clone()).collect::<Vec<_>>()))
        .collect())
}

/// What the built-in agent may see of every entry of the asked types that is not in the trash
/// (FR-027): title, tag names, folder name and whether a TOTP exists; for a pass its kind, start and
/// end, never a code.
pub fn agent_headers(q: &mut impl Query, kinds: Kinds) -> Result<Vec<AgentHeader>> {
    let trashed = trashed_item_ids(q)?;
    let folders: HashMap<String, Option<String>> = q
        .query_map("SELECT id, name FROM haex_passwords_groups", &[], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })?
        .into_iter()
        .collect();
    Ok(load_headers(q, None)?
        .into_iter()
        .filter(|h| kinds.admits(&h.kind))
        .filter(|h| !trashed.contains(&h.id))
        .filter(|h| h.owner.is_none())
        .map(|h| AgentHeader {
            folder: h
                .group_id
                .as_ref()
                .and_then(|g| folders.get(g).cloned().flatten()),
            tags: h.tags.iter().map(|t| t.name.clone()).collect(),
            has_totp: h.has_totp,
            title: h.title,
            id: h.id,
            kind: h.kind,
            pass_kind: h.pass_kind,
            start_date: h.start_date,
            start_at: h.start_at,
            start_zone: h.start_zone,
            expires_at: h.expires_at,
        })
        .collect())
}
