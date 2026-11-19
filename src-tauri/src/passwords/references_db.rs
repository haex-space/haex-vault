//! The references between entries against the vault (spec 036, `contracts/references.md`, research
//! R3, R4, R12): resolving with the rights of a caller, the cycle check on save, the marks for the
//! window, how many entries use a source, and replacing placeholders by their values before a
//! source is deleted for good. The grammar and the walk are in `references.rs`.
//!
//! Maintainability exception (spaex 500-LoC rule): every function here reads the same entry
//! texts with the caller's rights, so resolving, checking and inlining have stayed together.
//! Concrete split plan, if this grows further: move the usage views and the inlining before a
//! final delete (`item_references`, `targets`, `targets_of`, `inline_text`, `inline_all`,
//! `items_in_groups`) into `references_usage.rs`, leaving resolving and the save check here.

use std::collections::{BTreeSet, HashSet};

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use zeroize::Zeroizing;

use super::access::{authorize_read, Caller, Grant};
use super::items::item_state;
use super::model::{ItemPatch, Patch};
use super::model_references::{
    ItemReferences, KeyValueReferences, RefMark, RefMarkKind, RefStatus, ReferenceUsage,
};
use super::references::{
    build_token, find, find_cycle, replace, resolve, utf16_offset, Field, Lookup, RefKind,
    ReferenceError,
};
use super::{clock, snapshots};
use crate::error::{HaexError, Result};
use crate::storage::query::Query;

/// Who resolves: the source of a placeholder must be visible to this caller (FR-047).
#[derive(Clone, Copy)]
pub struct Reader<'a> {
    pub caller: &'a Caller,
    pub grants: &'a [Grant],
}

static USER: Caller = Caller::User;

impl Reader<'static> {
    /// The user, who sees every entry, also in the trash.
    pub fn user() -> Self {
        Reader {
            caller: &USER,
            grants: &[],
        }
    }
}

impl From<ReferenceError> for HaexError {
    fn from(error: ReferenceError) -> Self {
        HaexError::PasswordsReference {
            reason: error.as_str().to_string(),
        }
    }
}

/// The raw stored value a placeholder points at, whoever may see it; `None` when the entry or the
/// custom field does not exist. An empty cell reads as an empty text.
pub fn raw_value(q: &mut impl Query, item_id: &str, kind: &RefKind) -> Result<Option<String>> {
    Ok(match kind {
        RefKind::Username | RefKind::Password => {
            let column = if *kind == RefKind::Username {
                "username"
            } else {
                "password"
            };
            // `column` is one of the two literals above, never input.
            let sql = format!("SELECT {column} FROM haex_passwords_item_details WHERE id = ?1");
            q.query_row(&sql, params![item_id], |r| r.get::<_, Option<String>>(0))?
                .map(Option::unwrap_or_default)
        }
        RefKind::Extra(key) => q
            .query_row(
                "SELECT value FROM haex_passwords_item_key_values \
                 WHERE item_id = ?1 AND key = ?2 ORDER BY rowid LIMIT 1",
                params![item_id, key],
                |r| r.get::<_, Option<String>>(0),
            )?
            .map(Option::unwrap_or_default),
    })
}

fn visible(q: &mut impl Query, reader: Reader<'_>, item_id: &str) -> Result<bool> {
    let Some(state) = item_state(q, item_id)? else {
        return Ok(false);
    };
    Ok(authorize_read(reader.caller, reader.grants, &state.view()).is_ok())
}

/// Resolves the placeholders of `text`, the value of `field` of `item_id`, for `reader`. The outer
/// error is a failure of the vault, the inner one a reference that does not resolve.
pub fn resolve_value(
    q: &mut impl Query,
    reader: Reader<'_>,
    item_id: &str,
    field: Field,
    text: &str,
) -> Result<std::result::Result<Zeroizing<String>, ReferenceError>> {
    if find(text).is_empty() {
        return Ok(Ok(Zeroizing::new(text.to_string())));
    }
    let mut lookup = |id: &str, kind: &RefKind| -> Result<Lookup> {
        if !visible(q, reader, id)? {
            return Ok(Lookup::Missing);
        }
        Ok(raw_value(q, id, kind)?.map_or(Lookup::Missing, Lookup::Value))
    };
    Ok(resolve(text, (item_id.to_string(), field), &mut lookup)?.map(Zeroizing::new))
}

/// Like [`resolve_value`], with a reference that does not resolve as the error of the call: what
/// reveal and copy deliver (FR-045: the placeholder is never handed out as text).
pub fn resolve_or_error(
    q: &mut impl Query,
    reader: Reader<'_>,
    item_id: &str,
    field: Field,
    text: &str,
) -> Result<Zeroizing<String>> {
    resolve_value(q, reader, item_id, field, text)?.map_err(HaexError::from)
}

/// The texts of an entry's reference fields as a save leaves them.
pub struct FieldTexts {
    pub username: Option<String>,
    pub password: Option<String>,
    pub url: Option<String>,
    pub note: Option<String>,
    /// Key and value of every custom field, in the order the rows have after the save (a stored
    /// row may have no key; no placeholder can reach it).
    pub key_values: Vec<(Option<String>, String)>,
}

impl FieldTexts {
    /// Every text, every custom field included.
    fn all_texts(&self) -> impl Iterator<Item = &str> {
        [&self.username, &self.password, &self.url, &self.note]
            .into_iter()
            .filter_map(|text| text.as_deref())
            .chain(self.key_values.iter().map(|(_, value)| value.as_str()))
    }

    fn as_fields(&self) -> Vec<(Field, String)> {
        let mut fields = Vec::new();
        for (field, text) in [
            (Field::Username, &self.username),
            (Field::Password, &self.password),
            (Field::Url, &self.url),
            (Field::Note, &self.note),
        ] {
            if let Some(text) = text {
                fields.push((field, text.clone()));
            }
        }
        // Every custom field is checked, also a second one with the same key: a placeholder reaches
        // the first of them (`find_cycle` takes the first own text of a field), but the text of any
        // of them can lead back.
        for (key, value) in &self.key_values {
            if let Some(key) = key {
                fields.push((Field::Extra(key.clone()), value.clone()));
            }
        }
        fields
    }
}

/// For a caller other than the user (FR-047: a reference grants no access): every placeholder in
/// `texts` must point at an entry this caller may read, or the write is refused as a reference
/// whose source is missing, the same answer as for an entry that does not exist. Without this a
/// caller could point at an entry outside its scope, and turning references into own values before
/// a delete for good would copy that secret into its scope.
pub fn check_sources_visible<'t>(
    q: &mut impl Query,
    reader: Reader<'_>,
    texts: impl IntoIterator<Item = &'t str>,
) -> Result<()> {
    if matches!(reader.caller, Caller::User) {
        return Ok(());
    }
    for text in texts {
        for hit in find(text) {
            if !visible(q, reader, &hit.item_id)? {
                return Err(ReferenceError::Missing.into());
            }
        }
    }
    Ok(())
}

/// The check on save (research R4): refuses texts that lead back to the same field of the entry,
/// as `PasswordsReferenceCycle` naming the source of the first step. A reference to an entry that
/// does not exist is allowed (it may arrive by sync).
pub fn validate(q: &mut impl Query, item_id: &str, texts: &FieldTexts) -> Result<()> {
    let fields = texts.as_fields();
    if !fields.iter().any(|(_, text)| !find(text).is_empty()) {
        return Ok(());
    }
    let mut lookup = |id: &str, kind: &RefKind| raw_value(q, id, kind);
    match find_cycle(&item_id.to_ascii_lowercase(), &fields, &mut lookup)? {
        Some(source_item_id) => Err(HaexError::PasswordsReferenceCycle { source_item_id }),
        None => Ok(()),
    }
}

/// The stored texts of an entry's reference fields; `None` for a missing entry.
pub fn stored_texts(q: &mut impl Query, item_id: &str) -> Result<Option<FieldTexts>> {
    let Some((username, password, url, note)) = q.query_row(
        "SELECT username, password, url, note FROM haex_passwords_item_details WHERE id = ?1",
        params![item_id],
        |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        },
    )?
    else {
        return Ok(None);
    };
    let key_values = q.query_map(
        "SELECT key, value FROM haex_passwords_item_key_values WHERE item_id = ?1 ORDER BY rowid",
        params![item_id],
        |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            ))
        },
    )?;
    Ok(Some(FieldTexts {
        username,
        password,
        url,
        note,
        key_values,
    }))
}

/// The texts an update leaves in the reference fields: the patch over the stored ones; `None` for
/// a missing entry (the update reports that itself).
pub fn texts_after_patch(
    q: &mut impl Query,
    item_id: &str,
    patch: &ItemPatch,
) -> Result<Option<FieldTexts>> {
    let Some(stored) = stored_texts(q, item_id)? else {
        return Ok(None);
    };
    let apply = |stored: Option<String>, patch: &Patch<String>| match patch {
        Patch::Keep => stored,
        Patch::Clear => None,
        Patch::Set(value) => Some(value.clone()),
    };
    let key_values = match &patch.key_values {
        None => stored.key_values,
        Some(fields) => {
            let stored_rows = q.query_map(
                "SELECT id, value FROM haex_passwords_item_key_values WHERE item_id = ?1 \
                 ORDER BY rowid",
                params![item_id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    ))
                },
            )?;
            let sent: Vec<_> = fields
                .iter()
                .filter(|field| !field.key.trim().is_empty())
                .collect();
            // `items::replace_key_values` keeps the rows it knows (in their rowid order) and
            // appends the new ones in the order sent, so a placeholder reaches the same first
            // field here as after the save.
            let mut out = Vec::with_capacity(sent.len());
            for (stored_id, stored_value) in &stored_rows {
                if let Some(field) = sent
                    .iter()
                    .find(|field| field.id.as_deref() == Some(stored_id.as_str()))
                {
                    // A field sent without a value keeps the stored one.
                    let value = field.value.clone().unwrap_or_else(|| stored_value.clone());
                    out.push((Some(field.key.clone()), value));
                }
            }
            for field in &sent {
                let known = field
                    .id
                    .as_deref()
                    .is_some_and(|id| stored_rows.iter().any(|(stored_id, _)| stored_id == id));
                if !known {
                    out.push((
                        Some(field.key.clone()),
                        field.value.clone().unwrap_or_default(),
                    ));
                }
            }
            out
        }
    };
    Ok(Some(FieldTexts {
        username: apply(stored.username, &patch.username),
        password: apply(stored.password, &patch.password),
        url: apply(stored.url, &patch.url),
        note: apply(stored.note, &patch.note),
        key_values,
    }))
}

/// The marks of the placeholders in `text` for the user: source, its title and whether it resolves.
pub fn marks(q: &mut impl Query, text: &str) -> Result<Vec<RefMark>> {
    let mut out = Vec::new();
    for hit in find(text) {
        let title = q
            .query_row(
                "SELECT title FROM haex_passwords_item_details WHERE id = ?1",
                params![hit.item_id],
                |r| r.get::<_, Option<String>>(0),
            )?
            .flatten();
        let token = build_token(&hit.item_id, &hit.kind).unwrap_or_default();
        // The chain from this placeholder alone; the field holding it is not known here.
        let status = match resolve_value(q, Reader::user(), "", Field::Note, &token)? {
            Ok(_) => RefStatus::Ok,
            Err(ReferenceError::Missing) => RefStatus::Missing,
            Err(ReferenceError::Cycle) => RefStatus::Cycle,
            Err(ReferenceError::TooDeep) => RefStatus::TooDeep,
        };
        let (kind, key) = match &hit.kind {
            RefKind::Username => (RefMarkKind::Username, None),
            RefKind::Password => (RefMarkKind::Password, None),
            RefKind::Extra(key) => (RefMarkKind::Extra, Some(key.clone())),
        };
        out.push(RefMark {
            start: u32::try_from(utf16_offset(text, hit.start)).unwrap_or(u32::MAX),
            end: u32::try_from(utf16_offset(text, hit.end)).unwrap_or(u32::MAX),
            source_item_id: hit.item_id,
            source_title: title,
            kind,
            key,
            status,
        });
    }
    Ok(out)
}

/// The marks in every reference field of an entry (`ItemDetail.references`).
pub fn item_references(q: &mut impl Query, item_id: &str) -> Result<ItemReferences> {
    let Some(texts) = stored_texts(q, item_id)? else {
        return Ok(ItemReferences::default());
    };
    let mut marks_of = |text: &Option<String>| -> Result<Vec<RefMark>> {
        text.as_deref()
            .map_or(Ok(Vec::new()), |text| marks(q, text))
    };
    let mut references = ItemReferences {
        username: marks_of(&texts.username)?,
        password: marks_of(&texts.password)?,
        url: marks_of(&texts.url)?,
        note: marks_of(&texts.note)?,
        key_values: Vec::new(),
    };
    let fields = q.query_map(
        "SELECT id, value FROM haex_passwords_item_key_values WHERE item_id = ?1 ORDER BY rowid",
        params![item_id],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            ))
        },
    )?;
    for (id, value) in fields {
        let found = marks(q, &value)?;
        if !found.is_empty() {
            references
                .key_values
                .push(KeyValueReferences { id, marks: found });
        }
    }
    Ok(references)
}

/// The entries (not among `sources`) whose text fields hold a placeholder on one of `sources`.
fn targets(q: &mut impl Query, sources: &[String]) -> Result<Vec<(String, BTreeSet<String>)>> {
    let source_set: HashSet<&str> = sources.iter().map(String::as_str).collect();
    let mut out: Vec<(String, BTreeSet<String>)> = Vec::new();
    for source in sources {
        let pattern = format!("%{{${source}:%");
        let candidates = q.query_map(
            "SELECT id FROM haex_passwords_item_details \
             WHERE username LIKE ?1 OR password LIKE ?1 OR url LIKE ?1 OR note LIKE ?1 \
             UNION SELECT item_id FROM haex_passwords_item_key_values WHERE value LIKE ?1",
            params![pattern],
            |r| r.get::<_, String>(0),
        )?;
        for candidate in candidates {
            if source_set.contains(candidate.as_str()) {
                continue;
            }
            // The LIKE is a prefilter; only a real placeholder counts.
            let Some(texts) = stored_texts(q, &candidate)? else {
                continue;
            };
            let points_here = texts.all_texts().any(|text| {
                find(text)
                    .iter()
                    .any(|hit| hit.item_id == source.to_ascii_lowercase())
            });
            if !points_here {
                continue;
            }
            match out.iter_mut().find(|(id, _)| *id == candidate) {
                Some((_, hits)) => {
                    hits.insert(source.clone());
                }
                None => out.push((candidate, BTreeSet::from([source.clone()]))),
            }
        }
    }
    Ok(out)
}

/// How many other entries point at each of `sources` (FR-048, research R12), and how many links
/// to its passkeys other entries hold (research R6; they drop with it).
pub fn targets_of(q: &mut impl Query, sources: &[String]) -> Result<Vec<ReferenceUsage>> {
    let found = targets(q, sources)?;
    let links = super::passkey_links::links_to_items(q, sources)?;
    Ok(sources
        .iter()
        .map(|source| ReferenceUsage {
            item_id: source.clone(),
            target_items: u32::try_from(
                found
                    .iter()
                    .filter(|(_, hits)| hits.contains(source))
                    .count(),
            )
            .unwrap_or(u32::MAX),
            passkey_links: links.get(source).copied().unwrap_or(0),
        })
        .collect())
}

/// `text` with every placeholder on one of `sources` replaced by its value for the user. A
/// placeholder whose chain does not resolve (a source further on is gone, or a cycle that came by
/// sync) takes the raw value of the source instead, so it does not point at a deleted entry; only a
/// placeholder whose own source field is already missing stays. `None` when nothing changed.
fn inline_text(
    q: &mut impl Query,
    sources: &HashSet<String>,
    item_id: &str,
    field: Field,
    text: &str,
) -> Result<Option<String>> {
    let found = find(text);
    if !found.iter().any(|hit| sources.contains(&hit.item_id)) {
        return Ok(None);
    }
    let mut values = Vec::with_capacity(found.len());
    for hit in &found {
        let original = text[hit.start..hit.end].to_string();
        if !sources.contains(&hit.item_id) {
            values.push(original);
            continue;
        }
        match resolve_value(q, Reader::user(), item_id, field.clone(), &original)? {
            Ok(value) => values.push(value.to_string()),
            Err(_) => values.push(raw_value(q, &hit.item_id, &hit.kind)?.unwrap_or(original)),
        }
    }
    let next = replace(text, &found, &values);
    Ok((next != text).then_some(next))
}

/// Replaces in every entry that points at `sources` each such placeholder by today's value for the
/// user, and takes a state of each changed entry (research R12). Runs in the transaction that then
/// deletes the sources, so a deleted source leaves no placeholder behind unless the value it pointed
/// at was already missing (see [`inline_text`]). Returns the number of entries changed.
pub fn inline_all(tx: &mut CrdtTransaction<'_>, sources: &[String]) -> Result<u32> {
    let source_set: HashSet<String> = sources.iter().map(|s| s.to_ascii_lowercase()).collect();
    let mut changed = 0u32;
    for (target, _) in targets(tx, sources)? {
        let Some(texts) = stored_texts(tx, &target)? else {
            continue;
        };
        let mut touched = false;
        for (column, field, text) in [
            ("username", Field::Username, &texts.username),
            ("password", Field::Password, &texts.password),
            ("url", Field::Url, &texts.url),
            ("note", Field::Note, &texts.note),
        ] {
            let Some(text) = text else { continue };
            if let Some(next) = inline_text(tx, &source_set, &target, field, text)? {
                // `column` is one of the four literals above, never input.
                let sql =
                    format!("UPDATE haex_passwords_item_details SET {column} = ?1 WHERE id = ?2");
                tx.execute(&sql, params![next, target])?;
                touched = true;
            }
        }
        let fields = tx.query_map(
            "SELECT id, key, value FROM haex_passwords_item_key_values WHERE item_id = ?1",
            params![target],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                ))
            },
        )?;
        for (id, key, value) in fields {
            // A row without a key cannot be reached by a placeholder; an empty key stands for it.
            let field = Field::Extra(key.unwrap_or_default());
            if let Some(next) = inline_text(tx, &source_set, &target, field, &value)? {
                tx.execute(
                    "UPDATE haex_passwords_item_key_values SET value = ?1, updated_at = ?2 \
                     WHERE id = ?3",
                    params![next, clock::now(), id],
                )?;
                touched = true;
            }
        }
        if touched {
            let previous = tx
                .query_row(
                    "SELECT updated_at FROM haex_passwords_item_details WHERE id = ?1",
                    params![target],
                    |r| r.get::<_, Option<String>>(0),
                )?
                .flatten();
            tx.execute(
                "UPDATE haex_passwords_item_details SET updated_at = ?1 WHERE id = ?2",
                params![clock::now_after(previous.as_deref()), target],
            )?;
            snapshots::take_snapshot(tx, &target)?;
            changed += 1;
        }
    }
    Ok(changed)
}

/// The ids of the entries in these folders and every folder below them.
pub fn items_in_groups(q: &mut impl Query, group_ids: &[String]) -> Result<Vec<String>> {
    let mut items = Vec::new();
    for group in group_ids {
        items.extend(q.query_map(
            "WITH RECURSIVE below(id) AS ( \
               SELECT ?1 \
               UNION \
               SELECT g.id FROM haex_passwords_groups g JOIN below b ON g.parent_id = b.id) \
             SELECT gi.item_id FROM haex_passwords_group_items gi JOIN below b ON b.id = gi.group_id",
            params![group],
            |r| r.get::<_, String>(0),
        )?);
    }
    Ok(items)
}
