//! Writing and reading a pass in the caller's transaction (spec 092, data-model.md): the shared
//! columns go through `items::create_item_as` / `update_item_with`, so the folder, the tags, the free
//! fields, the conflict token and the history state work as for a login; this module adds the
//! detail row and the codes. A code is validated here against its format, and a value is never
//! written into an error.

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;
use uuid::Uuid;

use super::codes::{check, CodeFormat, Reason};
use super::model::{CodeInput, CodeView, PassInput, PassPatch, Start};
use crate::error::{HolziError, Result};
use crate::passwords::kind::EntryType;
use crate::passwords::model::{ItemInput, ItemPatch, Patch};
use crate::passwords::sets::Sets;
use crate::passwords::{clock, items};
use crate::storage::query::Query;

/// `flag` of a code that arrived by import or sync and does not fit its format.
pub const FLAG_INVALID_VALUE: &str = "invalid_value";
/// `flag` of a code whose format this build does not know.
pub const FLAG_UNKNOWN_FORMAT: &str = "unknown_format";
/// How far a walk up the parents goes: a loop that arrived by sync must not hang a write.
const PARENT_WALK_LIMIT: usize = 1000;
/// The longest text of a zone: an IANA name is shorter.
const ZONE_LIMIT: usize = 64;

pub(crate) fn refused(code: &str) -> HolziError {
    HolziError::PassRefused {
        code: code.to_owned(),
        position: None,
        reason: None,
    }
}

fn invalid(reason: &str) -> HolziError {
    HolziError::InvalidInput {
        reason: reason.to_owned(),
    }
}

/// The type of an entry, `None` when there is no such entry.
pub(crate) fn entry_type(q: &mut impl Query, id: &str) -> Result<Option<EntryType>> {
    Ok(q.query_row(
        "SELECT kind FROM haex_passwords_item_details WHERE id = ?1",
        params![id],
        |r| r.get::<_, String>(0),
    )?
    .map(|text| EntryType::parse(&text)))
}

/// The refusal for an entry that is not a pass: an unknown type is never written over (FR-042).
pub(crate) fn not_a_pass(kind: &EntryType) -> HolziError {
    match kind {
        EntryType::Unknown(_) => refused("unknown_kind"),
        _ => refused("not_a_pass"),
    }
}

/// The three start columns of a start: `(start_date, start_at, start_zone)`; exactly one of the
/// date or the moment is set (data-model.md).
pub(crate) fn start_columns(
    start: &Start,
) -> Result<(Option<String>, Option<String>, Option<String>)> {
    match start {
        Start::Date { date } => {
            if !is_real_date(date) {
                return Err(invalid("start"));
            }
            Ok((Some(date.clone()), None, None))
        }
        Start::At { at, zone } => {
            // The parser rolls a day that does not exist (30 February) over into the next month,
            // which would save a different moment than the one typed.
            let day = at.trim().get(..10).unwrap_or_default();
            if !is_real_date(day) {
                return Err(invalid("start"));
            }
            let ms = clock::parse_iso_millis(at).ok_or_else(|| invalid("start"))?;
            let zone = zone.trim();
            if zone.is_empty() || zone.len() > ZONE_LIMIT || zone.chars().any(char::is_control) {
                return Err(invalid("startZone"));
            }
            Ok((None, Some(clock::format_millis(ms)), Some(zone.to_owned())))
        }
    }
}

/// Whether the text is a calendar date `YYYY-MM-DD` that exists.
pub(crate) fn is_real_date(date: &str) -> bool {
    date.len() == 10
        && clock::parse_iso_millis(&format!("{date}T00:00:00Z"))
            .is_some_and(|ms| clock::format_millis(ms).starts_with(date))
}

/// A code as it is stored.
struct StoredCode {
    id: String,
    format: String,
    value: String,
    label: Option<String>,
    position: i64,
    flag: Option<String>,
}

fn stored_codes(q: &mut impl Query, item_id: &str) -> Result<Vec<StoredCode>> {
    Ok(q.query_map(
        "SELECT id, format, value, label, position, flag FROM haex_passwords_pass_barcodes \
         WHERE item_id = ?1 ORDER BY position, id",
        params![item_id],
        |r| {
            Ok(StoredCode {
                id: r.get(0)?,
                format: r.get(1)?,
                value: r.get(2)?,
                label: r.get(3)?,
                position: r.get(4)?,
                flag: r.get(5)?,
            })
        },
    )?)
}

/// The stored codes (of any pass) whose ids the codes of an input name.
fn stored_codes_by_id(q: &mut impl Query, codes: &[CodeInput]) -> Result<Vec<StoredCode>> {
    let mut found = Vec::new();
    for id in codes.iter().filter_map(|code| code.id.as_deref()) {
        found.extend(q.query_map(
            "SELECT id, format, value, label, position, flag FROM haex_passwords_pass_barcodes \
             WHERE id = ?1",
            params![id],
            |r| {
                Ok(StoredCode {
                    id: r.get(0)?,
                    format: r.get(1)?,
                    value: r.get(2)?,
                    label: r.get(3)?,
                    position: r.get(4)?,
                    flag: r.get(5)?,
                })
            },
        )?);
    }
    Ok(found)
}

/// The codes of a pass with their values, in order. For the user's own screen only.
pub fn codes_of(q: &mut impl Query, item_id: &str) -> Result<Vec<CodeView>> {
    Ok(stored_codes(q, item_id)?
        .into_iter()
        .map(|c| CodeView {
            id: c.id,
            format: c.format,
            label: c.label,
            position: c.position,
            flag: c.flag,
            drawing: None,
            value: c.value,
        })
        .collect())
}

/// One code of a pass, `None` if the pass has no such code.
pub fn code_of(q: &mut impl Query, item_id: &str, code_id: &str) -> Result<Option<CodeView>> {
    Ok(codes_of(q, item_id)?.into_iter().find(|c| c.id == code_id))
}

/// A code that passed the check, ready to be written.
struct Prepared {
    id: Option<String>,
    format: String,
    value: String,
    label: Option<String>,
    position: i64,
    flag: Option<String>,
}

/// Checks every code of an input. A code from the Rust side carries its flag and is kept as it is;
/// a code that is the stored one unchanged keeps its flag, so a repairable code stays repairable
/// until its value or format is edited; every other code must fit its format.
fn prepare(codes: &[CodeInput], stored: &[StoredCode]) -> Result<Vec<Prepared>> {
    // The list is the whole list by id: one stored row cannot be given two values.
    let mut seen = std::collections::HashSet::new();
    for id in codes.iter().filter_map(|code| code.id.as_deref()) {
        if !seen.insert(id) {
            return Err(refused("duplicate_code"));
        }
    }
    codes
        .iter()
        .map(|code| {
            let unchanged = code.id.as_deref().and_then(|id| {
                stored
                    .iter()
                    .find(|s| s.id == id && s.format == code.format && s.value == code.value)
            });
            let flag = if let Some(flag) = &code.flag {
                Some(flag.clone())
            } else if let Some(same) = unchanged {
                same.flag.clone()
            } else {
                let reason = match CodeFormat::parse(&code.format) {
                    None => Some(Reason::UnknownFormat),
                    Some(format) => check(format, &code.value).err(),
                };
                if let Some(reason) = reason {
                    return Err(HolziError::PassRefused {
                        code: "code_invalid".to_owned(),
                        position: u32::try_from(code.position).ok(),
                        reason: Some(reason.as_text().to_owned()),
                    });
                }
                None
            };
            Ok(Prepared {
                id: code.id.clone(),
                format: code.format.clone(),
                value: code.value.clone(),
                label: code.label.clone().filter(|l| !l.trim().is_empty()),
                position: code.position,
                flag,
            })
        })
        .collect()
}

/// Makes the codes of a pass the given list by code id: a code with a stored id keeps its row and
/// has only its changed columns written, one without a stored id is a new row, and a stored row
/// that is not listed is deleted, so two replicas that edit different codes merge row by row.
fn write_codes(tx: &mut CrdtTransaction<'_>, item_id: &str, prepared: &[Prepared]) -> Result<()> {
    let stored = stored_codes(tx, item_id)?;
    let mut kept = Vec::new();
    for code in prepared {
        let same = code
            .id
            .as_deref()
            .and_then(|id| stored.iter().find(|s| s.id == id));
        match same {
            Some(row) => {
                kept.push(row.id.clone());
                let mut sets = Sets::default();
                if row.format != code.format {
                    sets.push("format", code.format.clone());
                }
                if row.value != code.value {
                    sets.push("value", code.value.clone());
                }
                if row.label != code.label {
                    sets.push("label", code.label.clone());
                }
                if row.position != code.position {
                    sets.push("position", code.position);
                }
                if row.flag != code.flag {
                    sets.push("flag", code.flag.clone());
                }
                sets.execute(tx, "haex_passwords_pass_barcodes", "id", &row.id)?;
            }
            None => {
                tx.execute(
                    "INSERT INTO haex_passwords_pass_barcodes \
                     (id, item_id, format, value, label, position, flag) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        Uuid::new_v4().to_string(),
                        item_id,
                        code.format,
                        code.value,
                        code.label,
                        code.position,
                        code.flag
                    ],
                )?;
            }
        }
    }
    for row in stored.iter().filter(|s| !kept.contains(&s.id)) {
        tx.execute(
            "DELETE FROM haex_passwords_pass_barcodes WHERE id = ?1",
            params![row.id],
        )?;
    }
    Ok(())
}

/// Refuses a parent that is the pass itself or has the pass among its ancestors. A parent that is
/// not in the vault (not yet synced) is fine (the link is soft), and a loop that arrived by sync
/// without the pass in it ends the walk instead of hanging it.
pub fn ensure_no_parent_loop(q: &mut impl Query, id: &str, parent: &str) -> Result<()> {
    let mut current = parent.to_owned();
    for _ in 0..PARENT_WALK_LIMIT {
        if current == id {
            return Err(refused("parent_loop"));
        }
        let next = q
            .query_row(
                "SELECT parent_id FROM haex_passwords_pass_details WHERE item_id = ?1",
                params![current],
                |r| r.get::<_, Option<String>>(0),
            )?
            .flatten();
        match next {
            Some(next) => current = next,
            None => return Ok(()),
        }
    }
    Ok(())
}

/// Creates a pass with its tags, free fields, folder, detail row, codes and first state in the
/// caller's transaction. A code typed by hand that does not fit its format is refused with its
/// position and the reason.
pub fn create(
    tx: &mut CrdtTransaction<'_>,
    input: &PassInput,
    group_id: Option<&str>,
) -> Result<String> {
    // A code that carries the id of a stored code with the same format and value (a "save as new"
    // after a conflict) keeps that code's flag, so a code that arrived flagged can be copied;
    // nothing else takes a flag from the window.
    let stored = stored_codes_by_id(tx, &input.codes)?;
    let prepared = prepare(&input.codes, &stored)?;
    let (start_date, start_at, start_zone) = match &input.start {
        Some(start) => start_columns(start)?,
        None => (None, None, None),
    };
    let item = item_input(input);
    let parent = input.parent_id.clone().filter(|p| !p.is_empty());
    items::create_item_as(tx, &item, group_id, &EntryType::Pass, |tx, id| {
        tx.execute(
            "INSERT INTO haex_passwords_pass_details \
             (item_id, pass_kind, start_date, start_at, start_zone, parent_id, source_id) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id,
                input.pass_kind.as_text(),
                start_date,
                start_at,
                start_zone,
                parent,
                input.source_id
            ],
        )?;
        write_codes(tx, id, &prepared)
    })
}

/// Applies a partial update to a pass if `expected_updated_at` is still its token, and returns the
/// new token. A missing entry is `PasswordsConflict { deleted }`; an entry that is not a pass is
/// refused and an unknown type is never written over.
pub fn update(
    tx: &mut CrdtTransaction<'_>,
    id: &str,
    expected_updated_at: &str,
    patch: &PassPatch,
) -> Result<String> {
    match entry_type(tx, id)? {
        None => {
            return Err(HolziError::PasswordsConflict {
                reason: "deleted".to_owned(),
            })
        }
        Some(kind) if !kind.is_pass() => return Err(not_a_pass(&kind)),
        Some(_) => {}
    }
    // The codes are checked against the stored ones before anything is written.
    let prepared = match &patch.codes {
        Some(codes) => Some(prepare(codes, &stored_codes(tx, id)?)?),
        None => None,
    };
    let start = match &patch.start {
        Patch::Set(start) => Some(start_columns(start)?),
        _ => None,
    };
    if let Patch::Set(parent) = &patch.parent_id {
        ensure_no_parent_loop(tx, id, parent)?;
    }
    let item = item_patch(patch);
    items::update_item_with(tx, id, expected_updated_at, &item, |tx, id| {
        write_details(tx, id, patch, start)?;
        match &prepared {
            Some(codes) => write_codes(tx, id, codes),
            None => Ok(()),
        }
    })
}

/// The detail row under a patch. A pass whose row is missing gets one first (it reads as `other`
/// until then).
fn write_details(
    tx: &mut CrdtTransaction<'_>,
    id: &str,
    patch: &PassPatch,
    start: Option<(Option<String>, Option<String>, Option<String>)>,
) -> Result<()> {
    let stored = tx.query_row(
        "SELECT pass_kind, start_date, start_at, start_zone, parent_id \
         FROM haex_passwords_pass_details WHERE item_id = ?1",
        params![id],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        },
    )?;
    let Some((kind, date, at, zone, parent)) = stored else {
        tx.execute(
            "INSERT INTO haex_passwords_pass_details (item_id, pass_kind) VALUES (?1, 'other')",
            params![id],
        )?;
        return write_details_over(tx, id, patch, start, "other", (None, None, None), None);
    };
    write_details_over(tx, id, patch, start, &kind, (date, at, zone), parent)
}

fn write_details_over(
    tx: &mut CrdtTransaction<'_>,
    id: &str,
    patch: &PassPatch,
    start: Option<(Option<String>, Option<String>, Option<String>)>,
    kind: &str,
    stored_start: (Option<String>, Option<String>, Option<String>),
    parent: Option<String>,
) -> Result<()> {
    let mut sets = Sets::default();
    if let Some(new_kind) = &patch.pass_kind {
        if new_kind.as_text() != kind {
            sets.push("pass_kind", new_kind.as_text().to_owned());
        }
    }
    let target = match (&patch.start, start) {
        (Patch::Set(_), Some(columns)) => Some(columns),
        (Patch::Clear, _) => Some((None, None, None)),
        _ => None,
    };
    if let Some((date, at, zone)) = target {
        if date != stored_start.0 {
            sets.push("start_date", date);
        }
        if at != stored_start.1 {
            sets.push("start_at", at);
        }
        if zone != stored_start.2 {
            sets.push("start_zone", zone);
        }
    }
    match &patch.parent_id {
        Patch::Set(new_parent) if parent.as_deref() != Some(new_parent.as_str()) => {
            sets.push("parent_id", new_parent.clone());
        }
        Patch::Clear if parent.is_some() => sets.push("parent_id", Option::<String>::None),
        _ => {}
    }
    sets.execute(tx, "haex_passwords_pass_details", "item_id", id)
}

/// The shared columns of a pass input, as the entry code takes them.
pub(crate) fn item_input(input: &PassInput) -> ItemInput {
    ItemInput {
        title: input.title.clone(),
        note: input.note.clone(),
        url: input.url.clone(),
        icon: input.icon.clone(),
        color: input.color.clone(),
        expires_at: input.expires_at.clone(),
        tags: input.tag_names.clone(),
        key_values: input.key_values.clone(),
        ..Default::default()
    }
}

/// The shared columns of a pass patch, as the entry code takes them.
pub(crate) fn item_patch(patch: &PassPatch) -> ItemPatch {
    ItemPatch {
        title: patch.title.clone(),
        note: patch.note.clone(),
        url: patch.url.clone(),
        icon: patch.icon.clone(),
        color: patch.color.clone(),
        expires_at: patch.expires_at.clone(),
        tags: patch.tag_names.clone(),
        key_values: patch.key_values.clone(),
        ..Default::default()
    }
}
