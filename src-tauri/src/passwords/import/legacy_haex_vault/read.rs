//! The rows of haex-vault 2.x's (Legacy) password manager as an [`ImportModel`] (spec 037,
//! `contracts/haex-vault-mapping.md`). Columns are always named, never `SELECT *`: the sync layer
//! of haex-vault 2.x adds columns of its own at run time.

use std::collections::{HashMap, HashSet};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use haex_crdt::rusqlite::{params, Connection, OptionalExtension};
use zeroize::Zeroizing;

use super::corrupt;
use super::groups::{read_folders, read_tags};
use super::history::read_history;
use super::icons::map_icon;
use crate::error::Result;
use crate::passwords::import::{
    check_otp, holzi_time, push_tag, IconRef, ImportAttachment, ImportItem, ImportModel, Problem,
};
use crate::passwords::model::{AttentionKind, KeyValueInput, PresetInput};
use crate::passwords::passkeys::PasskeyInput;

/// Reads binary rows on demand. haex-vault 2.x stores them as standard Base64 text; holzi hashes the
/// decoded bytes again when it writes them.
pub(super) struct Binaries<'c> {
    conn: &'c Connection,
}

pub(super) enum Binary {
    Bytes(Vec<u8>),
    Missing,
    Unreadable,
}

impl<'c> Binaries<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    pub fn get(&self, hash: &str) -> Result<Binary> {
        let data: Option<String> = self
            .conn
            .query_row(
                "SELECT data FROM haex_passwords_binaries WHERE hash = ?1",
                params![hash],
                |r| r.get(0),
            )
            .optional()
            .map_err(corrupt)?;
        Ok(match data {
            None => Binary::Missing,
            Some(text) => match STANDARD.decode(text.trim()) {
                Ok(bytes) => Binary::Bytes(bytes),
                Err(_) => Binary::Unreadable,
            },
        })
    }
}

/// The picture holzi shows for a haex-vault 2.x icon name; `binary:` values are kept as they are
/// (holzi derives the same SHA-256 of the same bytes).
pub(super) fn icon_name(icon: &str) -> Option<String> {
    if icon.starts_with("binary:") {
        return Some(icon.to_string());
    }
    map_icon(icon).map(str::to_string)
}

/// The icon of an entry or folder: a picture of holzi, or the bytes of a picture of its own.
/// What cannot arrive is a problem `IconNotMapped`.
pub(super) fn icon_ref(
    binaries: &Binaries<'_>,
    icon: Option<&str>,
    problems: &mut Vec<Problem>,
) -> Result<Option<IconRef>> {
    let Some(icon) = icon.map(str::trim).filter(|i| !i.is_empty()) else {
        return Ok(None);
    };
    if let Some(hash) = icon.strip_prefix("binary:") {
        return Ok(match binaries.get(hash)? {
            Binary::Bytes(bytes) => Some(IconRef::Custom(bytes)),
            Binary::Missing | Binary::Unreadable => {
                problems.push(Problem::field(AttentionKind::IconNotMapped, "icon"));
                None
            }
        });
    }
    Ok(match map_icon(icon) {
        Some(name) => Some(IconRef::Standard(name.to_string())),
        None => {
            problems.push(Problem::field(AttentionKind::IconNotMapped, "icon"));
            None
        }
    })
}

/// An attachment by hash, within the limit; what cannot arrive is a problem.
pub(super) fn attachment(
    binaries: &Binaries<'_>,
    hash: &str,
    file_name: &str,
    problems: &mut Vec<Problem>,
) -> Result<Option<ImportAttachment>> {
    Ok(match binaries.get(hash)? {
        Binary::Bytes(bytes) => match ImportAttachment::within_limit(file_name, bytes) {
            Ok(file) => Some(file),
            Err(problem) => {
                problems.push(problem);
                None
            }
        },
        Binary::Missing | Binary::Unreadable => {
            problems.push(Problem {
                file_name: Some(file_name.to_string()),
                ..Problem::new(AttentionKind::AttachmentUnreadable)
            });
            None
        }
    })
}

/// Every row of the password manager as the model of the import.
pub(super) fn to_model(conn: &Connection) -> Result<ImportModel> {
    let binaries = Binaries::new(conn);
    let mut source_problems = Vec::new();
    let folders = read_folders(conn, &binaries, &mut source_problems)?;
    let tags = read_tags(conn, &mut source_problems)?;
    let mut item_problems: HashMap<String, Vec<Problem>> = HashMap::new();
    let mut history = read_history(conn, &binaries, &mut item_problems)?;
    let mut tag_links = links(
        conn,
        "SELECT item_id, tag_id FROM haex_passwords_item_tags ORDER BY rowid",
    )?;
    let group_of: HashMap<String, Option<String>> = pairs(
        conn,
        "SELECT item_id, group_id FROM haex_passwords_group_items",
    )?
    .into_iter()
    .collect();
    let mut key_values = key_values(conn)?;
    let mut files = attachment_rows(conn)?;
    let mut passkeys = passkeys(conn)?;

    let mut items = Vec::new();
    let mut item_ids = HashSet::new();
    let mut statement = conn
        .prepare(
            "SELECT id, title, username, password, note, icon, color, url, otp_secret, \
                    otp_digits, otp_period, otp_algorithm, expires_at, autofill_aliases, \
                    created_at, updated_at \
             FROM haex_passwords_item_details ORDER BY rowid",
        )
        .map_err(corrupt)?;
    let mut rows = statement.query([]).map_err(corrupt)?;
    while let Some(row) = rows.next().map_err(corrupt)? {
        let id: String = row.get(0).map_err(corrupt)?;
        let get = |i: usize| row.get::<_, Option<String>>(i).map_err(corrupt);
        let mut problems = item_problems.remove(&id).unwrap_or_default();
        let icon = icon_ref(&binaries, get(5)?.as_deref(), &mut problems)?;
        let group_ref = group_of
            .get(&id)
            .cloned()
            .flatten()
            .filter(|g| folders.exists(g));
        let mut item = ImportItem {
            title: get(1)?,
            username: get(2)?,
            password: get(3)?,
            note: get(4)?,
            icon,
            color: get(6)?,
            url: get(7)?,
            otp_raw: get(8)?,
            otp_digits: row.get(9).map_err(corrupt)?,
            otp_period: row.get(10).map_err(corrupt)?,
            otp_algorithm: get(11)?,
            expires_at: expiry(get(12)?.as_deref(), &mut problems),
            autofill_aliases: aliases(get(13)?.as_deref(), &mut problems),
            created_at: holzi_time(get(14)?.as_deref()),
            updated_at: holzi_time(get(15)?.as_deref()),
            trashed: group_ref.as_deref().is_some_and(|g| folders.in_trash(g)),
            group_ref,
            history: history.remove(&id).unwrap_or_default(),
            passkeys: passkeys.remove(&Some(id.clone())).unwrap_or_default(),
            key_values: key_values_of(key_values.remove(&id), &mut problems),
            ..ImportItem::default()
        };
        for tag in tag_links.remove(&id).unwrap_or_default() {
            if let Some(name) = tags.names.get(&tag) {
                push_tag(&mut item.tags, name);
            }
        }
        for (hash, file_name) in files.remove(&id).unwrap_or_default() {
            if let Some(file) = attachment(&binaries, &hash, &file_name, &mut problems)? {
                item.attachments.push(file);
            }
        }
        item.problems = problems;
        check_otp(&mut item);
        item_ids.insert(id);
        items.push(item);
    }
    // Passkeys of no entry, or of an entry that is not there.
    let standalone = passkeys
        .into_values()
        .flatten()
        .map(|mut passkey| {
            passkey.item_id = None;
            passkey
        })
        .collect();

    Ok(ImportModel {
        groups: folders.groups,
        items,
        source_problems,
        tag_colors: tags.colors,
        passkeys: standalone,
        presets: presets(conn)?,
    })
}

/// `expires_at` is a date (`YYYY-MM-DD`) in haex-vault 2.x; another form is read as a time.
fn expiry(text: Option<&str>, problems: &mut Vec<Problem>) -> Option<String> {
    let text = text.map(str::trim).filter(|t| !t.is_empty())?;
    let is_date = text.len() == 10
        && text.char_indices().all(|(i, c)| {
            if i == 4 || i == 7 {
                c == '-'
            } else {
                c.is_ascii_digit()
            }
        });
    if is_date {
        return Some(text.to_string());
    }
    match holzi_time(Some(text)) {
        Some(time) => Some(time.chars().take(10).collect()),
        None => {
            problems.push(Problem::field(
                AttentionKind::ValueNotStorable,
                "expires_at",
            ));
            None
        }
    }
}

/// The JSON object of autofill aliases; anything else is reported and left out.
fn aliases(text: Option<&str>, problems: &mut Vec<Problem>) -> Option<serde_json::Value> {
    let text = text.map(str::trim).filter(|t| !t.is_empty())?;
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(value) if value.is_object() => Some(value),
        _ => {
            problems.push(Problem::field(
                AttentionKind::UnknownSourceData,
                "autofill_aliases",
            ));
            None
        }
    }
}

/// Custom fields in the order of the editor; one without a name cannot be stored by holzi.
fn key_values_of(
    rows: Option<Vec<(Option<String>, Option<String>)>>,
    problems: &mut Vec<Problem>,
) -> Vec<KeyValueInput> {
    let mut fields = Vec::new();
    for (key, value) in rows.unwrap_or_default() {
        match key.filter(|k| !k.trim().is_empty()) {
            Some(key) => fields.push(KeyValueInput { key, value }),
            None if value.as_deref().is_some_and(|v| !v.is_empty()) => {
                problems.push(Problem::field(
                    AttentionKind::ValueNotStorable,
                    "custom field without a name",
                ))
            }
            None => {}
        }
    }
    fields
}

fn pairs(conn: &Connection, sql: &str) -> Result<Vec<(String, Option<String>)>> {
    let mut statement = conn.prepare(sql).map_err(corrupt)?;
    let rows = statement
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .and_then(Iterator::collect)
        .map_err(corrupt)?;
    Ok(rows)
}

fn links(conn: &Connection, sql: &str) -> Result<HashMap<String, Vec<String>>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for (from, to) in pairs(conn, sql)? {
        if let Some(to) = to {
            map.entry(from).or_default().push(to);
        }
    }
    Ok(map)
}

type KeyValueRows = HashMap<String, Vec<(Option<String>, Option<String>)>>;

fn key_values(conn: &Connection) -> Result<KeyValueRows> {
    let mut statement = conn
        .prepare("SELECT item_id, key, value FROM haex_passwords_item_key_values ORDER BY rowid")
        .map_err(corrupt)?;
    let rows: Vec<(String, Option<String>, Option<String>)> = statement
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .and_then(Iterator::collect)
        .map_err(corrupt)?;
    let mut map: KeyValueRows = HashMap::new();
    for (item, key, value) in rows {
        map.entry(item).or_default().push((key, value));
    }
    Ok(map)
}

fn attachment_rows(conn: &Connection) -> Result<HashMap<String, Vec<(String, String)>>> {
    let mut statement = conn
        .prepare(
            "SELECT item_id, binary_hash, file_name FROM haex_passwords_item_binaries \
             ORDER BY rowid",
        )
        .map_err(corrupt)?;
    let rows: Vec<(String, String, String)> = statement
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .and_then(Iterator::collect)
        .map_err(corrupt)?;
    let mut map: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for (item, hash, name) in rows {
        map.entry(item).or_default().push((hash, name));
    }
    Ok(map)
}

/// Every passkey 1:1, keyed by the entry it belongs to (`None` for none). haex-vault 2.x stores the
/// public key next to the private one, so nothing is derived.
fn passkeys(conn: &Connection) -> Result<HashMap<Option<String>, Vec<PasskeyInput>>> {
    let mut statement = conn
        .prepare(
            "SELECT item_id, credential_id, relying_party_id, relying_party_name, user_handle, \
                    user_name, user_display_name, private_key, public_key, algorithm, \
                    sign_count, is_discoverable, icon, color, nickname, created_at, last_used_at \
             FROM haex_passwords_passkeys ORDER BY rowid",
        )
        .map_err(corrupt)?;
    let rows: Vec<PasskeyInput> = statement
        .query_map([], |r| {
            Ok(PasskeyInput {
                item_id: r.get(0)?,
                credential_id: r.get(1)?,
                relying_party_id: r.get(2)?,
                relying_party_name: r.get(3)?,
                user_handle: r.get(4)?,
                user_name: r.get(5)?,
                user_display_name: r.get(6)?,
                private_key: Zeroizing::new(r.get(7)?),
                public_key: r.get(8)?,
                algorithm: r.get(9)?,
                sign_count: r.get(10)?,
                is_discoverable: r.get::<_, i64>(11)? != 0,
                icon: r
                    .get::<_, Option<String>>(12)?
                    .and_then(|icon| map_icon(&icon).map(str::to_string)),
                color: r.get(13)?,
                nickname: r.get(14)?,
                created_at: holzi_time(r.get::<_, Option<String>>(15)?.as_deref()),
                last_used_at: holzi_time(r.get::<_, Option<String>>(16)?.as_deref()),
            })
        })
        .and_then(Iterator::collect)
        .map_err(corrupt)?;
    let mut map: HashMap<Option<String>, Vec<PasskeyInput>> = HashMap::new();
    for passkey in rows {
        map.entry(passkey.item_id.clone())
            .or_default()
            .push(passkey);
    }
    Ok(map)
}

/// The generator presets, as new presets (empty id).
fn presets(conn: &Connection) -> Result<Vec<PresetInput>> {
    let mut statement = conn
        .prepare(
            "SELECT name, length, uppercase, lowercase, numbers, symbols, exclude_chars, \
                    use_pattern, pattern, is_default \
             FROM haex_passwords_generator_presets ORDER BY rowid",
        )
        .map_err(corrupt)?;
    let rows = statement
        .query_map([], |r| {
            Ok(PresetInput {
                id: String::new(),
                name: r.get(0)?,
                length: u32::try_from(r.get::<_, i64>(1)?).unwrap_or(0),
                uppercase: r.get::<_, i64>(2)? != 0,
                lowercase: r.get::<_, i64>(3)? != 0,
                numbers: r.get::<_, i64>(4)? != 0,
                symbols: r.get::<_, i64>(5)? != 0,
                exclude_chars: r.get::<_, Option<String>>(6)?.unwrap_or_default(),
                use_pattern: r.get::<_, i64>(7)? != 0,
                pattern: r.get::<_, Option<String>>(8)?.unwrap_or_default(),
                is_default: r.get::<_, i64>(9)? != 0,
            })
        })
        .and_then(Iterator::collect)
        .map_err(corrupt)?;
    Ok(rows)
}
