//! The history of an entry (spec 034, US4, FR-017, FR-018, research R5). After every change the
//! **new** state is saved as a self-contained JSON document (`SnapshotData`, `version: 1`) together
//! with the links to its attachments; the state before stays as the earlier row. Nothing is written
//! when nothing changed. Restoring a state makes it the current one and adds a state of its own, so
//! no state is ever lost. A state does not hold the folder or the passkeys.
//!
//! Reading tolerates states written by haex-vault 2.x (Legacy), which know neither `version` nor the TOTP
//! parameters nor `autofillAliases`.
//!
//! Maintainability exception (spaex 500-LoC rule): taking, reading and restoring a state share the
//! one `SnapshotData` format and its compatibility rules, so they have stayed together. Concrete
//! split plan, if this grows further: move the restore (`restore`, `restore_key_values`,
//! `restore_attachments`, `text_patch`, `number_patch`) into `snapshots_restore.rs`.

use std::fmt;

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use super::model::{
    HistorySecret, Patch, RestoreOutcome, RevealedSecret, SnapshotAttachmentView, SnapshotHeader,
    SnapshotKeyValueView, SnapshotView,
};
use super::sets::Sets;
use super::{binaries, clock, tags};
use crate::error::{HolziError, Result};
use crate::storage::query::Query;

const CURRENT_VERSION: u32 = 1;

fn current_version() -> u32 {
    CURRENT_VERSION
}

/// A custom field of a state, with its value.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotKeyValue {
    pub key: Option<String>,
    pub value: Option<String>,
}

/// An attachment of a state: the name under which it was linked and the hash of its binary data.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotAttachment {
    pub file_name: String,
    pub binary_hash: String,
}

/// The JSON of a state (`item_snapshots.snapshot_data`). Missing fields read as empty. It holds
/// secrets, so `Debug` prints none of them.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SnapshotData {
    #[serde(default = "current_version")]
    pub version: u32,
    pub title: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub url: Option<String>,
    pub note: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub expires_at: Option<String>,
    pub otp_secret: Option<String>,
    pub otp_digits: Option<u32>,
    pub otp_period: Option<u32>,
    pub otp_algorithm: Option<String>,
    pub autofill_aliases: Option<serde_json::Value>,
    pub tag_names: Vec<String>,
    pub key_values: Vec<SnapshotKeyValue>,
    pub attachments: Vec<SnapshotAttachment>,
}

impl Default for SnapshotData {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            title: None,
            username: None,
            password: None,
            url: None,
            note: None,
            icon: None,
            color: None,
            expires_at: None,
            otp_secret: None,
            otp_digits: None,
            otp_period: None,
            otp_algorithm: None,
            autofill_aliases: None,
            tag_names: Vec::new(),
            key_values: Vec::new(),
            attachments: Vec::new(),
        }
    }
}

impl fmt::Debug for SnapshotData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SnapshotData")
            .field("version", &self.version)
            .field("title", &self.title)
            .field("secrets", &"<redacted>")
            .finish_non_exhaustive()
    }
}

/// The state of an entry as it is now; `None` for an entry that does not exist.
fn build_current(
    q: &mut impl Query,
    item_id: &str,
) -> Result<Option<(SnapshotData, Option<String>)>> {
    type Row = (SnapshotData, Option<String>, Option<String>);
    let Some((mut data, aliases, updated_at)) = q.query_row(
        "SELECT title, username, password, url, note, icon, color, expires_at, otp_secret, \
                otp_digits, otp_period, otp_algorithm, autofill_aliases, updated_at \
         FROM haex_passwords_item_details WHERE id = ?1",
        params![item_id],
        |r| -> haex_crdt::rusqlite::Result<Row> {
            Ok((
                SnapshotData {
                    title: r.get(0)?,
                    username: r.get(1)?,
                    password: r.get(2)?,
                    url: r.get(3)?,
                    note: r.get(4)?,
                    icon: r.get(5)?,
                    color: r.get(6)?,
                    expires_at: r.get(7)?,
                    otp_secret: r.get(8)?,
                    otp_digits: r
                        .get::<_, Option<i64>>(9)?
                        .and_then(|v| u32::try_from(v).ok()),
                    otp_period: r
                        .get::<_, Option<i64>>(10)?
                        .and_then(|v| u32::try_from(v).ok()),
                    otp_algorithm: r.get(11)?,
                    ..SnapshotData::default()
                },
                r.get(12)?,
                r.get(13)?,
            ))
        },
    )?
    else {
        return Ok(None);
    };
    data.autofill_aliases =
        aliases.map(|text| serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text)));
    let mut names = tags::names_of_item(q, item_id)?;
    names.sort();
    data.tag_names = names;
    data.key_values = q.query_map(
        "SELECT key, value FROM haex_passwords_item_key_values WHERE item_id = ?1 ORDER BY rowid",
        params![item_id],
        |r| {
            Ok(SnapshotKeyValue {
                key: r.get(0)?,
                value: r.get(1)?,
            })
        },
    )?;
    let mut attachments: Vec<SnapshotAttachment> = q.query_map(
        "SELECT file_name, binary_hash FROM haex_passwords_item_binaries WHERE item_id = ?1",
        params![item_id],
        |r| {
            Ok(SnapshotAttachment {
                file_name: r.get(0)?,
                binary_hash: r.get(1)?,
            })
        },
    )?;
    attachments.sort_by(|a, b| (&a.file_name, &a.binary_hash).cmp(&(&b.file_name, &b.binary_hash)));
    data.attachments = attachments;
    Ok(Some((data, updated_at)))
}

fn parse(text: &str) -> Result<SnapshotData> {
    serde_json::from_str(text).map_err(|_| HolziError::CrdtInit {
        reason: "a history state is not readable".to_string(),
    })
}

/// Saves the entry's current state as a new history state unless it equals the newest one.
/// Returns whether a state was written.
pub fn take_snapshot(tx: &mut CrdtTransaction<'_>, item_id: &str) -> Result<bool> {
    let Some((current, updated_at)) = build_current(tx, item_id)? else {
        return Ok(false);
    };
    let newest: Option<String> = tx.query_row(
        "SELECT snapshot_data FROM haex_passwords_item_snapshots WHERE item_id = ?1 \
         ORDER BY modified_at DESC, created_at DESC, rowid DESC LIMIT 1",
        params![item_id],
        |r| r.get(0),
    )?;
    if let Some(text) = newest {
        if parse(&text).is_ok_and(|previous| previous == current) {
            return Ok(false);
        }
    }
    let id = Uuid::new_v4().to_string();
    let now = clock::now();
    let text = serde_json::to_string(&current).map_err(|_| HolziError::CrdtInit {
        reason: "a history state could not be written".to_string(),
    })?;
    tx.execute(
        "INSERT INTO haex_passwords_item_snapshots \
         (id, item_id, snapshot_data, created_at, modified_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            id,
            item_id,
            text,
            now,
            updated_at.unwrap_or_else(|| now.clone())
        ],
    )?;
    for attachment in &current.attachments {
        tx.execute(
            "INSERT INTO haex_passwords_snapshot_binaries (id, snapshot_id, binary_hash, file_name) \
             VALUES (?1, ?2, ?3, ?4)",
            params![
                Uuid::new_v4().to_string(),
                id,
                attachment.binary_hash,
                attachment.file_name
            ],
        )?;
    }
    Ok(true)
}

/// The names of the fields in which `next` differs from `previous` (all non-empty fields when there
/// is no previous state). The window maps the names to labels.
pub fn changed_fields(previous: Option<&SnapshotData>, next: &SnapshotData) -> Vec<String> {
    let empty = SnapshotData::default();
    let before = previous.unwrap_or(&empty);
    let mut names = Vec::new();
    let mut check = |name: &str, differs: bool| {
        if differs {
            names.push(name.to_string());
        }
    };
    check("title", before.title != next.title);
    check("username", before.username != next.username);
    check("password", before.password != next.password);
    check("url", before.url != next.url);
    check("note", before.note != next.note);
    check("icon", before.icon != next.icon);
    check("color", before.color != next.color);
    check("expiresAt", before.expires_at != next.expires_at);
    check("otpSecret", before.otp_secret != next.otp_secret);
    check("otpDigits", before.otp_digits != next.otp_digits);
    check("otpPeriod", before.otp_period != next.otp_period);
    check("otpAlgorithm", before.otp_algorithm != next.otp_algorithm);
    check(
        "autofillAliases",
        before.autofill_aliases != next.autofill_aliases,
    );
    check("tags", before.tag_names != next.tag_names);
    check("keyValues", before.key_values != next.key_values);
    check("attachments", before.attachments != next.attachments);
    names
}

struct Row {
    id: String,
    item_id: String,
    modified_at: Option<String>,
    data: SnapshotData,
}

fn load_rows(q: &mut impl Query, sql: &str, arg: &str) -> Result<Vec<Row>> {
    let raw: Vec<(String, String, Option<String>, String)> =
        q.query_map(sql, params![arg], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?;
    raw.into_iter()
        .map(|(id, item_id, modified_at, text)| {
            Ok(Row {
                id,
                item_id,
                modified_at,
                data: parse(&text)?,
            })
        })
        .collect()
}

/// The states of an entry, newest first, each with the names of what changed against the state
/// before it.
pub fn list(q: &mut impl Query, item_id: &str) -> Result<Vec<SnapshotHeader>> {
    let rows = load_rows(
        q,
        "SELECT id, item_id, modified_at, snapshot_data FROM haex_passwords_item_snapshots \
         WHERE item_id = ?1 ORDER BY modified_at DESC, created_at DESC, rowid DESC",
        item_id,
    )?;
    Ok(rows
        .iter()
        .enumerate()
        .map(|(i, row)| SnapshotHeader {
            id: row.id.clone(),
            item_id: row.item_id.clone(),
            modified_at: row.modified_at.clone(),
            changed_fields: changed_fields(rows.get(i + 1).map(|older| &older.data), &row.data),
            attachment_count: u32::try_from(row.data.attachments.len()).unwrap_or(u32::MAX),
        })
        .collect())
}

fn one_row(q: &mut impl Query, snapshot_id: &str) -> Result<Row> {
    load_rows(
        q,
        "SELECT id, item_id, modified_at, snapshot_data FROM haex_passwords_item_snapshots \
         WHERE id = ?1",
        snapshot_id,
    )?
    .into_iter()
    .next()
    .ok_or(HolziError::PasswordsNotFound)
}

/// One state without its secrets.
pub fn get(q: &mut impl Query, snapshot_id: &str) -> Result<SnapshotView> {
    let row = one_row(q, snapshot_id)?;
    let data = row.data;
    let mut attachments = Vec::new();
    for attachment in &data.attachments {
        let available = q
            .query_row(
                "SELECT COUNT(*) FROM haex_passwords_binaries WHERE hash = ?1",
                params![attachment.binary_hash],
                |r| r.get::<_, i64>(0),
            )?
            .unwrap_or(0)
            > 0;
        attachments.push(SnapshotAttachmentView {
            file_name: attachment.file_name.clone(),
            binary_hash: attachment.binary_hash.clone(),
            available,
        });
    }
    Ok(SnapshotView {
        id: row.id,
        item_id: row.item_id,
        modified_at: row.modified_at,
        has_password: data.password.as_deref().is_some_and(|p| !p.is_empty()),
        has_otp_secret: data
            .otp_secret
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty()),
        title: data.title,
        username: data.username,
        url: data.url,
        note: data.note,
        icon: data.icon,
        color: data.color,
        expires_at: data.expires_at,
        otp_digits: data.otp_digits,
        otp_period: data.otp_period,
        otp_algorithm: data.otp_algorithm,
        tags: data.tag_names,
        key_values: data
            .key_values
            .iter()
            .map(|field| SnapshotKeyValueView {
                key: field.key.clone(),
                has_value: field.value.as_deref().is_some_and(|v| !v.is_empty()),
            })
            .collect(),
        attachments,
    })
}

/// A secret of a state, on the user's explicit request.
pub fn reveal(
    q: &mut impl Query,
    snapshot_id: &str,
    field: &HistorySecret,
) -> Result<RevealedSecret> {
    let data = one_row(q, snapshot_id)?.data;
    let value = match field {
        HistorySecret::Password => data.password,
        HistorySecret::OtpSecret => data.otp_secret,
        HistorySecret::KeyValue { key } => {
            data.key_values
                .into_iter()
                .find(|kv| kv.key.as_deref() == Some(key.as_str()))
                .ok_or(HolziError::PasswordsNotFound)?
                .value
        }
    };
    Ok(RevealedSecret {
        value: Zeroizing::new(value.unwrap_or_default()),
    })
}

fn text_patch(value: &Option<String>) -> Patch<String> {
    match value {
        Some(text) => Patch::Set(text.clone()),
        None => Patch::Clear,
    }
}

fn number_patch(value: Option<u32>) -> Patch<i64> {
    match value {
        Some(number) => Patch::Set(i64::from(number)),
        None => Patch::Clear,
    }
}

/// Makes a state the current one if the entry still has the token `expected_updated_at`
/// (`PasswordsConflict { changed | deleted }` otherwise): the fields, the tags by name, the custom
/// fields and the attachments (re-linked by hash and name; one whose binary data is gone is
/// skipped and reported). The folder stays. The result is a new state of its own, so the state it
/// replaces is kept.
pub fn restore(
    tx: &mut CrdtTransaction<'_>,
    item_id: &str,
    snapshot_id: &str,
    expected_updated_at: &str,
) -> Result<RestoreOutcome> {
    let current = tx
        .query_row(
            "SELECT updated_at FROM haex_passwords_item_details WHERE id = ?1",
            params![item_id],
            |r| r.get::<_, Option<String>>(0),
        )?
        .ok_or_else(|| HolziError::PasswordsConflict {
            reason: "deleted".to_string(),
        })?;
    if current.as_deref() != Some(expected_updated_at) {
        return Err(HolziError::PasswordsConflict {
            reason: "changed".to_string(),
        });
    }
    let row = one_row(tx, snapshot_id)?;
    if row.item_id != item_id {
        return Err(HolziError::PasswordsNotFound);
    }
    let data = row.data;

    let stored = build_current(tx, item_id)?
        .map(|(state, _)| state)
        .ok_or(HolziError::PasswordsNotFound)?;
    let mut sets = Sets::default();
    sets.text("title", &stored.title, &text_patch(&data.title));
    sets.text("username", &stored.username, &text_patch(&data.username));
    sets.text("password", &stored.password, &text_patch(&data.password));
    sets.text("url", &stored.url, &text_patch(&data.url));
    sets.text("note", &stored.note, &text_patch(&data.note));
    sets.text("icon", &stored.icon, &text_patch(&data.icon));
    sets.text("color", &stored.color, &text_patch(&data.color));
    sets.text(
        "expires_at",
        &stored.expires_at,
        &text_patch(&data.expires_at),
    );
    sets.text(
        "otp_secret",
        &stored.otp_secret,
        &text_patch(&data.otp_secret),
    );
    sets.text(
        "otp_algorithm",
        &stored.otp_algorithm,
        &text_patch(&data.otp_algorithm),
    );
    sets.number(
        "otp_digits",
        stored.otp_digits.map(i64::from),
        &number_patch(data.otp_digits),
    );
    sets.number(
        "otp_period",
        stored.otp_period.map(i64::from),
        &number_patch(data.otp_period),
    );
    let aliases = data.autofill_aliases.as_ref().map(|value| match value {
        serde_json::Value::String(text) => text.clone(),
        other => other.to_string(),
    });
    let stored_aliases = stored.autofill_aliases.as_ref().map(|value| match value {
        serde_json::Value::String(text) => text.clone(),
        other => other.to_string(),
    });
    sets.text("autofill_aliases", &stored_aliases, &text_patch(&aliases));
    let token = clock::now_after(current.as_deref());
    sets.push("updated_at", token.clone());
    sets.execute(tx, "haex_passwords_item_details", "id", item_id)?;

    tags::set_item_tags(tx, item_id, &data.tag_names)?;
    restore_key_values(tx, item_id, &data.key_values, &token)?;
    let skipped = restore_attachments(tx, item_id, &data.attachments)?;
    take_snapshot(tx, item_id)?;
    Ok(RestoreOutcome {
        updated_at: token,
        skipped_attachments: skipped,
    })
}

/// Makes the custom fields those of the state; a field of the same name keeps its row.
fn restore_key_values(
    tx: &mut CrdtTransaction<'_>,
    item_id: &str,
    wanted: &[SnapshotKeyValue],
    now: &str,
) -> Result<()> {
    let mut existing: Vec<(String, Option<String>, Option<String>)> = tx.query_map(
        "SELECT id, key, value FROM haex_passwords_item_key_values WHERE item_id = ?1 ORDER BY rowid",
        params![item_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    for field in wanted {
        let at = existing.iter().position(|(_, key, _)| *key == field.key);
        match at {
            Some(index) => {
                let (id, _, value) = existing.remove(index);
                if value != field.value {
                    tx.execute(
                        "UPDATE haex_passwords_item_key_values SET value = ?1, updated_at = ?2 WHERE id = ?3",
                        params![field.value, now, id],
                    )?;
                }
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
    for (id, _, _) in existing {
        tx.execute(
            "DELETE FROM haex_passwords_item_key_values WHERE id = ?1",
            params![id],
        )?;
    }
    Ok(())
}

/// Makes the attachments those of the state: links by hash and name, missing binary data skipped.
fn restore_attachments(
    tx: &mut CrdtTransaction<'_>,
    item_id: &str,
    wanted: &[SnapshotAttachment],
) -> Result<Vec<String>> {
    let existing: Vec<(String, String, String)> = tx.query_map(
        "SELECT id, binary_hash, file_name FROM haex_passwords_item_binaries WHERE item_id = ?1",
        params![item_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let mut skipped = Vec::new();
    let mut keep = Vec::new();
    for attachment in wanted {
        if let Some((id, _, _)) = existing.iter().find(|(_, hash, name)| {
            *hash == attachment.binary_hash && *name == attachment.file_name
        }) {
            keep.push(id.clone());
            continue;
        }
        let present = tx
            .query_row(
                "SELECT COUNT(*) FROM haex_passwords_binaries WHERE hash = ?1",
                params![attachment.binary_hash],
                |r| r.get::<_, i64>(0),
            )?
            .unwrap_or(0)
            > 0;
        if !present {
            skipped.push(attachment.file_name.clone());
            continue;
        }
        tx.execute(
            "INSERT INTO haex_passwords_item_binaries (id, item_id, binary_hash, file_name) \
             VALUES (?1, ?2, ?3, ?4)",
            params![
                Uuid::new_v4().to_string(),
                item_id,
                attachment.binary_hash,
                attachment.file_name
            ],
        )?;
        binaries::clear_orphan_mark(tx, &attachment.binary_hash)?;
    }
    for (id, hash, _) in existing.iter().filter(|(id, _, _)| !keep.contains(id)) {
        tx.execute(
            "DELETE FROM haex_passwords_item_binaries WHERE id = ?1",
            params![id],
        )?;
        binaries::mark_if_unreferenced(tx, hash)?;
    }
    Ok(skipped)
}
