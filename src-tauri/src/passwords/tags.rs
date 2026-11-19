//! Tags of the entries (spec 034, FR-011, research R2): creating a tag by name, the tags of an
//! entry, renaming, deleting and the bulk operations.
//!
//! There is no UNIQUE constraint (a conflict halts the sync): a tag's id is derived from its name
//! (`ids::tag_id`), and the application keeps names unique by [`fold`] — case, surrounding space and
//! the spelling of umlauts do not make a second tag.

use std::collections::BTreeSet;

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;

use super::ids::{fold, item_tag_id, tag_id};
use crate::error::{HaexError, Result};
use crate::storage::query::Query;

/// The longest tag name, in characters.
pub const MAX_TAG_NAME_CHARS: usize = 64;

/// The trimmed name, or `InvalidInput` with the reason `empty` or `too_long`.
pub fn validate_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(HaexError::InvalidInput {
            reason: "empty".to_string(),
        });
    }
    if trimmed.chars().count() > MAX_TAG_NAME_CHARS {
        return Err(HaexError::InvalidInput {
            reason: "too_long".to_string(),
        });
    }
    Ok(trimmed.to_string())
}

/// The id of the tag with this name (by [`fold`]), created when there is none. The stored spelling
/// of an existing tag stays, the spelling of the first creator wins. A renamed tag keeps its old id,
/// so the lookup goes by the folded name, not by the derived id.
pub fn get_or_create(tx: &mut CrdtTransaction<'_>, name: &str) -> Result<String> {
    let name = validate_name(name)?;
    let wanted = fold(&name);
    let existing: Vec<(String, String)> =
        tx.query_map("SELECT id, name FROM haex_passwords_tags", &[], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    if let Some((id, _)) = existing.iter().find(|(_, stored)| fold(stored) == wanted) {
        return Ok(id.clone());
    }
    let id = tag_id(&name).to_string();
    tx.execute(
        "INSERT INTO haex_passwords_tags (id, name, created_at) VALUES (?1, ?2, ?3)",
        params![id, name, super::clock::now()],
    )?;
    Ok(id)
}

/// Makes the tags of the entry exactly `names` (by [`fold`]): links that are no longer wanted go,
/// missing links come, existing links stay untouched so a concurrent change of another link merges.
pub fn set_item_tags(tx: &mut CrdtTransaction<'_>, item_id: &str, names: &[String]) -> Result<()> {
    let mut wanted = BTreeSet::new();
    for name in names {
        wanted.insert(get_or_create(tx, name)?);
    }
    let current: Vec<(String, String)> = tx.query_map(
        "SELECT id, tag_id FROM haex_passwords_item_tags WHERE item_id = ?1",
        params![item_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    for (link_id, tag) in &current {
        if !wanted.contains(tag) {
            tx.execute(
                "DELETE FROM haex_passwords_item_tags WHERE id = ?1",
                params![link_id],
            )?;
        }
    }
    let have: BTreeSet<&String> = current.iter().map(|(_, tag)| tag).collect();
    for tag in wanted.iter().filter(|tag| !have.contains(tag)) {
        tx.execute(
            "INSERT INTO haex_passwords_item_tags (id, item_id, tag_id) VALUES (?1, ?2, ?3)",
            params![item_tag_id(item_id, tag).to_string(), item_id, tag],
        )?;
    }
    Ok(())
}

/// The tag names of an entry, in their stored spelling.
pub fn names_of_item(q: &mut impl Query, item_id: &str) -> Result<Vec<String>> {
    Ok(q.query_map(
        "SELECT t.name FROM haex_passwords_item_tags it \
         JOIN haex_passwords_tags t ON t.id = it.tag_id \
         WHERE it.item_id = ?1 ORDER BY t.name",
        params![item_id],
        |r| r.get::<_, String>(0),
    )?)
}

/// Renames a tag; the id stays. A name that another tag already has (by [`fold`]) is
/// `InvalidInput { exists }`; a change of case of the tag's own name is fine.
pub fn rename_tag(tx: &mut CrdtTransaction<'_>, id: &str, name: &str) -> Result<()> {
    let name = validate_name(name)?;
    let all: Vec<(String, String)> =
        tx.query_map("SELECT id, name FROM haex_passwords_tags", &[], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    let Some((_, current)) = all.iter().find(|(tag, _)| tag == id) else {
        return Err(HaexError::PasswordsNotFound);
    };
    let wanted = fold(&name);
    if all
        .iter()
        .any(|(tag, stored)| tag != id && fold(stored) == wanted)
    {
        return Err(HaexError::InvalidInput {
            reason: "exists".to_string(),
        });
    }
    if *current != name {
        tx.execute(
            "UPDATE haex_passwords_tags SET name = ?1 WHERE id = ?2",
            params![name, id],
        )?;
    }
    Ok(())
}

/// Sets or clears the color of a tag.
pub fn set_color(tx: &mut CrdtTransaction<'_>, id: &str, color: Option<&str>) -> Result<()> {
    let changed = tx.execute(
        "UPDATE haex_passwords_tags SET color = ?1 WHERE id = ?2",
        params![color, id],
    )?;
    if changed == 0 {
        return Err(HaexError::PasswordsNotFound);
    }
    Ok(())
}

/// Deletes a tag for every entry: the links go first, one by one (a remote delete is applied
/// without foreign keys, so every row needs its own marker), then the tag.
pub fn delete_tag(tx: &mut CrdtTransaction<'_>, id: &str) -> Result<()> {
    let links: Vec<String> = tx.query_map(
        "SELECT id FROM haex_passwords_item_tags WHERE tag_id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    for link in &links {
        tx.execute(
            "DELETE FROM haex_passwords_item_tags WHERE id = ?1",
            params![link],
        )?;
    }
    let removed = tx.execute("DELETE FROM haex_passwords_tags WHERE id = ?1", params![id])?;
    if removed == 0 {
        return Err(HaexError::PasswordsNotFound);
    }
    Ok(())
}

/// Merges tags whose names are equal by [`fold`] but whose ids differ (a rename race, or a tag
/// renamed on one device and created again by name on another): the smallest id stays, the links
/// of the others move to it (a link id is derived, so a link that is already there is not doubled)
/// and the other tags go, each row with a marker of its own. Returns how many tags were removed;
/// running it again changes nothing.
///
/// ponytail: ceiling is that duplicates stay visible until the next open (the window merges equal
/// names for display meanwhile); upgrade path is an own `ApplyPolicy` in the sync of spec 024.
pub fn reconcile_tags(tx: &mut CrdtTransaction<'_>) -> Result<u32> {
    let all: Vec<(String, String)> = tx.query_map(
        "SELECT id, name FROM haex_passwords_tags ORDER BY id",
        &[],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let mut by_name: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for (id, name) in all {
        by_name.entry(fold(&name)).or_default().push(id);
    }
    let mut removed = 0;
    for ids in by_name.values().filter(|ids| ids.len() > 1) {
        let keep = &ids[0];
        for other in &ids[1..] {
            let links: Vec<(String, String)> = tx.query_map(
                "SELECT id, item_id FROM haex_passwords_item_tags WHERE tag_id = ?1",
                params![other],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            for (link_id, item_id) in links {
                tx.execute(
                    "DELETE FROM haex_passwords_item_tags WHERE id = ?1",
                    params![link_id],
                )?;
                let already = tx
                    .query_row(
                        "SELECT COUNT(*) FROM haex_passwords_item_tags \
                         WHERE item_id = ?1 AND tag_id = ?2",
                        params![item_id, keep],
                        |r| r.get::<_, i64>(0),
                    )?
                    .unwrap_or(0);
                if already == 0 {
                    tx.execute(
                        "INSERT INTO haex_passwords_item_tags (id, item_id, tag_id) \
                         VALUES (?1, ?2, ?3)",
                        params![item_tag_id(&item_id, keep).to_string(), item_id, keep],
                    )?;
                }
            }
            tx.execute(
                "DELETE FROM haex_passwords_tags WHERE id = ?1",
                params![other],
            )?;
            removed += 1;
        }
    }
    Ok(removed)
}

/// Adds and removes tags (by [`fold`]) on many entries at once and returns how many entries
/// changed. Missing tags are created; an entry that does not exist is `target_missing`.
pub fn bulk_set(
    tx: &mut CrdtTransaction<'_>,
    item_ids: &[String],
    add: &[String],
    remove: &[String],
) -> Result<u32> {
    let mut add_ids = Vec::new();
    for name in add {
        add_ids.push(get_or_create(tx, name)?);
    }
    let remove_folded: BTreeSet<String> = remove.iter().map(|name| fold(name)).collect();
    let mut changed = 0;
    for item in item_ids {
        let exists = tx
            .query_row(
                "SELECT COUNT(*) FROM haex_passwords_item_details WHERE id = ?1",
                params![item],
                |r| r.get::<_, i64>(0),
            )?
            .unwrap_or(0);
        if exists == 0 {
            return Err(HaexError::InvalidInput {
                reason: "target_missing".to_string(),
            });
        }
        let links: Vec<(String, String, String)> = tx.query_map(
            "SELECT it.id, it.tag_id, t.name FROM haex_passwords_item_tags it \
             JOIN haex_passwords_tags t ON t.id = it.tag_id WHERE it.item_id = ?1",
            params![item],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let mut touched = false;
        for (link, _, name) in &links {
            if remove_folded.contains(&fold(name)) {
                tx.execute(
                    "DELETE FROM haex_passwords_item_tags WHERE id = ?1",
                    params![link],
                )?;
                touched = true;
            }
        }
        for tag in &add_ids {
            if !links.iter().any(|(_, existing, _)| existing == tag) {
                tx.execute(
                    "INSERT INTO haex_passwords_item_tags (id, item_id, tag_id) VALUES (?1, ?2, ?3)",
                    params![item_tag_id(item, tag).to_string(), item, tag],
                )?;
                touched = true;
            }
        }
        if touched {
            changed += 1;
        }
    }
    Ok(changed)
}
