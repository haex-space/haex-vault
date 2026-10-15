//! The sets of the vault and the choice (spec 053, data-model.md): reading the table
//! `haex_file_icon_sets`, the preference `files.iconSet` and which set is in use. Opened packs are
//! kept by their hash, so a pack of a set is opened and checked once per run, however often the
//! window asks (a pack that fails the checks is remembered as damaged under the same hash).

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use haex_crdt::rusqlite::params;
use haex_crdt::{CrdtTransaction, Result};

use super::pack::{open_pack, Pack};
use super::{BuiltinSet, IconSetRef, IconSetState, SELECTION_KEY};
use crate::storage::preferences::{self, PrefScope};
use crate::storage::query::Query;

/// A row of `haex_file_icon_sets` without its pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetRow {
    pub id: String,
    pub name: String,
    pub author: String,
    pub license: String,
    pub source: String,
    pub image_count: u32,
    pub sha256: String,
}

/// Where the images of a set come from: the standard set draws symbols, anything else is a pack.
#[derive(Debug, Clone)]
pub enum Source {
    Lucide,
    Pack(Arc<Pack>),
}

/// The set that is in use for a request and why.
#[derive(Debug, Clone)]
pub struct InUse {
    pub source: Source,
    pub state: IconSetState,
}

impl InUse {
    fn lucide(state: IconSetState) -> Self {
        Self {
            source: Source::Lucide,
            state,
        }
    }
}

/// Packs opened so far by hash; `None` for one that failed the checks.
static OPENED: LazyLock<Mutex<HashMap<String, Option<Arc<Pack>>>>> = LazyLock::new(Mutex::default);
/// Enough for every set of the vault and the built-in ones; a longer run starts over.
const OPENED_MAX: usize = 32;

/// Opens `bytes` (or takes the pack opened before under `hash`); `None` when it is damaged.
pub fn open_cached(hash: &str, bytes: &[u8]) -> Option<Arc<Pack>> {
    let mut opened = OPENED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(known) = opened.get(hash) {
        return known.clone();
    }
    let pack = open_pack(bytes, Some(hash)).map(Arc::new).ok();
    if opened.len() >= OPENED_MAX {
        opened.clear();
    }
    opened.insert(hash.to_owned(), pack.clone());
    pack
}

/// The rows of the table, by name.
pub fn rows(q: &mut impl Query) -> Result<Vec<SetRow>> {
    let mut rows = q.query_map(
        "SELECT id, name, author, license, source, image_count, sha256 \
         FROM haex_file_icon_sets",
        &[],
        |row| {
            Ok(SetRow {
                id: row.get(0)?,
                name: row.get(1)?,
                author: row.get(2)?,
                license: row.get(3)?,
                source: row.get(4)?,
                image_count: row.get::<_, i64>(5)?.try_into().unwrap_or(0),
                sha256: row.get(6)?,
            })
        },
    )?;
    rows.sort_by_cached_key(|row| (row.name.to_lowercase(), row.id.clone()));
    Ok(rows)
}

/// How many custom sets the vault holds.
pub fn count(q: &mut impl Query) -> Result<usize> {
    Ok(
        q.query_row("SELECT COUNT(*) FROM haex_file_icon_sets", &[], |row| {
            row.get::<_, i64>(0)
        })?
        .unwrap_or(0)
        .try_into()
        .unwrap_or(0),
    )
}

/// The id of a custom set with this name (not told apart by case), if there is one.
pub fn id_by_name(q: &mut impl Query, name: &str) -> Result<Option<String>> {
    q.query_row(
        "SELECT id FROM haex_file_icon_sets WHERE lower(name) = lower(?1) ORDER BY id LIMIT 1",
        &[&name],
        |row| row.get(0),
    )
}

/// Whether a custom set with this id exists.
pub fn exists(q: &mut impl Query, id: &str) -> Result<bool> {
    Ok(q.query_row(
        "SELECT COUNT(*) FROM haex_file_icon_sets WHERE id = ?1",
        &[&id],
        |row| row.get::<_, i64>(0),
    )?
    .unwrap_or(0)
        > 0)
}

/// A custom set to write: its row without the id, which the writer makes.
pub struct NewSet<'a> {
    pub name: &'a str,
    pub author: &'a str,
    pub license: &'a str,
    pub source: &'a str,
    pub image_count: u32,
    pub sha256: &'a str,
    pub pack: &'a [u8],
    pub created_at: &'a str,
}

/// Writes a set under `id`. A pack never changes after this: replacing a set is a new row and the
/// delete of the old one.
pub fn insert(tx: &mut CrdtTransaction<'_>, id: &str, set: &NewSet<'_>) -> Result<()> {
    tx.execute(
        "INSERT INTO haex_file_icon_sets \
           (id, name, author, license, source, image_count, byte_size, sha256, pack, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            id,
            set.name,
            set.author,
            set.license,
            set.source,
            set.image_count,
            set.pack.len() as i64,
            set.sha256,
            set.pack,
            set.created_at,
        ],
    )?;
    Ok(())
}

/// Deletes a set; the delete log carries it to the other devices.
pub fn delete(tx: &mut CrdtTransaction<'_>, id: &str) -> Result<()> {
    tx.execute("DELETE FROM haex_file_icon_sets WHERE id = ?1", params![id])?;
    Ok(())
}

/// The hash and the pack of a row; `None` when there is no such set.
pub fn pack_of(q: &mut impl Query, id: &str) -> Result<Option<(String, Vec<u8>)>> {
    q.query_row(
        "SELECT sha256, pack FROM haex_file_icon_sets WHERE id = ?1",
        &[&id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
}

/// Whether the set `id` was deleted (a marker in the delete log), as opposed to not yet arrived.
fn was_deleted(q: &mut impl Query, id: &str) -> Result<bool> {
    Ok(q.query_row(
        "SELECT COUNT(*) FROM haex_deleted_rows \
         WHERE table_name = 'haex_file_icon_sets' AND json_extract(row_pks, '$.id') = ?1",
        &[&id],
        |row| row.get::<_, i64>(0),
    )?
    .unwrap_or(0)
        > 0)
}

/// The raw value of the choice, as the preference holds it.
pub fn selection_value(q: &mut impl Query) -> Result<Option<String>> {
    preferences::get(q, PrefScope::Vault, SELECTION_KEY)
}

/// What `set` is right now: its pack when it is available, otherwise Lucide and the reason.
pub fn in_use(q: &mut impl Query, set: &IconSetRef) -> Result<InUse> {
    match set {
        IconSetRef::Builtin {
            name: BuiltinSet::Lucide,
        } => Ok(InUse {
            source: Source::Lucide,
            state: IconSetState::Ready,
        }),
        IconSetRef::Builtin { name } => Ok(match super::builtin::pack(*name) {
            Some(pack) => InUse {
                source: Source::Pack(pack),
                state: IconSetState::Ready,
            },
            None => InUse::lucide(IconSetState::Damaged),
        }),
        IconSetRef::Custom { id } => match pack_of(q, id)? {
            Some((hash, bytes)) => Ok(match open_cached(&hash, &bytes) {
                Some(pack) => InUse {
                    source: Source::Pack(pack),
                    state: IconSetState::Ready,
                },
                None => InUse::lucide(IconSetState::Damaged),
            }),
            None if was_deleted(q, id)? => Ok(InUse::lucide(IconSetState::Missing)),
            None => Ok(InUse::lucide(IconSetState::Syncing)),
        },
    }
}

/// The set the preference chooses and what it is in use as. An absent value is the standard set;
/// a value this app does not know (a set of a newer app) shows Lucide and says `unknown`.
pub fn chosen(q: &mut impl Query) -> Result<(IconSetRef, InUse)> {
    let value = selection_value(q)?;
    let Some(value) = value else {
        return Ok((super::DEFAULT_SET, in_use(q, &super::DEFAULT_SET)?));
    };
    match IconSetRef::parse_selection(&value) {
        Some(set) => {
            let in_use = in_use(q, &set)?;
            Ok((set, in_use))
        }
        None => Ok((super::DEFAULT_SET, InUse::lucide(IconSetState::Unknown))),
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
