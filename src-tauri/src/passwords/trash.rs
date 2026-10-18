//! The trash (spec 034, US4, FR-015, FR-016, research R3). Deleting moves an entry or a folder into
//! the trash, a folder row with the fixed id `trash` that is created on demand, and remembers the
//! place it came from (`trashed_from_group_id` of the entry, `trashed_from_parent_id` of the
//! folder). Restoring returns it there, or to the top level when the folder is gone. Deleting what
//! is in the trash removes it for good with everything that depends on it, **children first and one
//! statement per table**: a remote delete is applied without foreign keys, so the sync relies on
//! every removed row having its own delete marker and never on a cascade.
//!
//! `trash` on something that is already in the trash is `delete_permanently`; that is why the
//! service never hands `trash` to a caller from outside an entry that lies in the trash (rule Z13).
//! Binary data of removed links is only marked (`orphaned_at`); the clean-up removes it after the
//! grace period.

use haex_crdt::rusqlite::params;
use haex_crdt::CrdtTransaction;

use super::groups::is_in_trash;
use super::model::{Target, TargetKind};
use super::{binaries, clock, TRASH_GROUP_ID};
use crate::error::{HolziError, Result};
use crate::storage::query::Query;

fn invalid(reason: &str) -> HolziError {
    HolziError::InvalidInput {
        reason: reason.to_string(),
    }
}

fn exists(q: &mut impl Query, sql: &str, id: &str) -> Result<bool> {
    Ok(q.query_row(sql, params![id], |r| r.get::<_, i64>(0))?
        .unwrap_or(0)
        > 0)
}

/// Creates the trash row if it is missing; an error is passed on (unlike haex-vault 2.x (Legacy), which
/// ignores it and then fails later on a missing folder).
pub fn ensure_trash(tx: &mut CrdtTransaction<'_>) -> Result<()> {
    if !exists(
        tx,
        "SELECT COUNT(*) FROM haex_passwords_groups WHERE id = ?1",
        TRASH_GROUP_ID,
    )? {
        let now = clock::now();
        tx.execute(
            "INSERT INTO haex_passwords_groups (id, created_at, updated_at) VALUES (?1, ?2, ?2)",
            params![TRASH_GROUP_ID, now],
        )?;
    }
    Ok(())
}

/// The folder an entry lies in (`None` at the top level or without a link).
fn group_of_item(tx: &mut CrdtTransaction<'_>, item_id: &str) -> Result<Option<String>> {
    Ok(tx
        .query_row(
            "SELECT group_id FROM haex_passwords_group_items WHERE item_id = ?1",
            params![item_id],
            |r| r.get::<_, Option<String>>(0),
        )?
        .flatten())
}

fn item_exists(tx: &mut CrdtTransaction<'_>, id: &str) -> Result<bool> {
    exists(
        tx,
        "SELECT COUNT(*) FROM haex_passwords_item_details WHERE id = ?1",
        id,
    )
}

fn group_exists(tx: &mut CrdtTransaction<'_>, id: &str) -> Result<bool> {
    exists(
        tx,
        "SELECT COUNT(*) FROM haex_passwords_groups WHERE id = ?1",
        id,
    )
}

/// Whether the target lies in the trash (an entry in the trash folder or below it, a folder below
/// the trash).
fn target_in_trash(tx: &mut CrdtTransaction<'_>, target: &Target) -> Result<bool> {
    match target.kind {
        TargetKind::Item => match group_of_item(tx, &target.id)? {
            Some(group) => is_in_trash(tx, &group),
            None => Ok(false),
        },
        TargetKind::Group => is_in_trash(tx, &target.id),
    }
}

fn check_target(tx: &mut CrdtTransaction<'_>, target: &Target) -> Result<()> {
    match target.kind {
        TargetKind::Item => {
            if !item_exists(tx, &target.id)? {
                return Err(HolziError::PasswordsNotFound);
            }
        }
        TargetKind::Group => {
            if target.id == TRASH_GROUP_ID {
                return Err(invalid("trash"));
            }
            if !group_exists(tx, &target.id)? {
                return Err(HolziError::PasswordsNotFound);
            }
        }
    }
    Ok(())
}

/// Moves entries and folders into the trash and remembers where they were; what is in the trash
/// already is deleted for good. Returns the number of entries and folders affected.
pub fn trash(tx: &mut CrdtTransaction<'_>, targets: &[Target]) -> Result<u32> {
    let mut affected = 0;
    for target in targets {
        check_target(tx, target)?;
        if target_in_trash(tx, target)? {
            affected += purge(tx, target)?;
            continue;
        }
        ensure_trash(tx)?;
        match target.kind {
            TargetKind::Item => trash_item(tx, &target.id)?,
            TargetKind::Group => trash_group(tx, &target.id)?,
        }
        affected += 1;
    }
    Ok(affected)
}

fn trash_item(tx: &mut CrdtTransaction<'_>, item_id: &str) -> Result<()> {
    let from = group_of_item(tx, item_id)?;
    let linked = exists(
        tx,
        "SELECT COUNT(*) FROM haex_passwords_group_items WHERE item_id = ?1",
        item_id,
    )?;
    if linked {
        tx.execute(
            "UPDATE haex_passwords_group_items SET group_id = ?1, trashed_from_group_id = ?2 \
             WHERE item_id = ?3",
            params![TRASH_GROUP_ID, from, item_id],
        )?;
    } else {
        tx.execute(
            "INSERT INTO haex_passwords_group_items (item_id, group_id, trashed_from_group_id) \
             VALUES (?1, ?2, ?3)",
            params![item_id, TRASH_GROUP_ID, from],
        )?;
    }
    Ok(())
}

fn trash_group(tx: &mut CrdtTransaction<'_>, group_id: &str) -> Result<()> {
    let from = tx
        .query_row(
            "SELECT parent_id FROM haex_passwords_groups WHERE id = ?1",
            params![group_id],
            |r| r.get::<_, Option<String>>(0),
        )?
        .flatten();
    tx.execute(
        "UPDATE haex_passwords_groups SET parent_id = ?1, trashed_from_parent_id = ?2, \
         updated_at = ?3 WHERE id = ?4",
        params![TRASH_GROUP_ID, from, clock::now(), group_id],
    )?;
    Ok(())
}

/// Takes entries and folders out of the trash, back to where they were: the remembered folder if
/// it still exists and is not in the trash itself, else the top level. An entry inside a trashed
/// folder goes to the top level. Something that is not in the trash is left alone.
pub fn restore(tx: &mut CrdtTransaction<'_>, targets: &[Target]) -> Result<u32> {
    let mut affected = 0;
    for target in targets {
        check_target(tx, target)?;
        if !target_in_trash(tx, target)? {
            continue;
        }
        match target.kind {
            TargetKind::Item => restore_item(tx, &target.id)?,
            TargetKind::Group => restore_group(tx, &target.id)?,
        }
        affected += 1;
    }
    Ok(affected)
}

/// The remembered place if it is still there and live, else the top level.
fn live_place(tx: &mut CrdtTransaction<'_>, remembered: Option<String>) -> Result<Option<String>> {
    let Some(place) = remembered else {
        return Ok(None);
    };
    if group_exists(tx, &place)? && !is_in_trash(tx, &place)? {
        Ok(Some(place))
    } else {
        Ok(None)
    }
}

fn restore_item(tx: &mut CrdtTransaction<'_>, item_id: &str) -> Result<()> {
    let remembered = tx
        .query_row(
            "SELECT trashed_from_group_id FROM haex_passwords_group_items WHERE item_id = ?1",
            params![item_id],
            |r| r.get::<_, Option<String>>(0),
        )?
        .flatten();
    let place = live_place(tx, remembered)?;
    tx.execute(
        "UPDATE haex_passwords_group_items SET group_id = ?1, trashed_from_group_id = NULL \
         WHERE item_id = ?2",
        params![place, item_id],
    )?;
    Ok(())
}

fn restore_group(tx: &mut CrdtTransaction<'_>, group_id: &str) -> Result<()> {
    let remembered = tx
        .query_row(
            "SELECT trashed_from_parent_id FROM haex_passwords_groups WHERE id = ?1",
            params![group_id],
            |r| r.get::<_, Option<String>>(0),
        )?
        .flatten();
    let place = live_place(tx, remembered)?;
    tx.execute(
        "UPDATE haex_passwords_groups SET parent_id = ?1, trashed_from_parent_id = NULL, \
         updated_at = ?2 WHERE id = ?3",
        params![place, clock::now(), group_id],
    )?;
    Ok(())
}

/// Deletes what is in the trash for good. Only what lies in the trash can be deleted this way
/// (`not_in_trash` otherwise): the user deletes in the trash, nothing else removes for good.
/// Returns the number of entries and folders removed, folders counted with their content.
pub fn delete_permanently(tx: &mut CrdtTransaction<'_>, targets: &[Target]) -> Result<u32> {
    for target in targets {
        check_target(tx, target)?;
        if !target_in_trash(tx, target)? {
            return Err(invalid("not_in_trash"));
        }
    }
    let mut affected = 0;
    for target in targets {
        affected += purge(tx, target)?;
    }
    Ok(affected)
}

/// Empties the trash: everything in it and below it goes, the trash row stays.
pub fn empty_trash(tx: &mut CrdtTransaction<'_>) -> Result<u32> {
    let items: Vec<String> = tx.query_map(
        "SELECT item_id FROM haex_passwords_group_items WHERE group_id = ?1",
        params![TRASH_GROUP_ID],
        |r| r.get(0),
    )?;
    let groups: Vec<String> = tx.query_map(
        "SELECT id FROM haex_passwords_groups WHERE parent_id = ?1",
        params![TRASH_GROUP_ID],
        |r| r.get(0),
    )?;
    let mut affected = 0;
    for id in items {
        affected += purge_item(tx, &id)?;
    }
    for id in groups {
        affected += purge_group(tx, &id)?;
    }
    Ok(affected)
}

fn purge(tx: &mut CrdtTransaction<'_>, target: &Target) -> Result<u32> {
    match target.kind {
        TargetKind::Item => purge_item(tx, &target.id),
        TargetKind::Group => purge_group(tx, &target.id),
    }
}

/// Removes an entry and everything that hangs on it, children first: passkey links (as target and
/// to its own passkeys), passkeys, custom fields, tag links, attachment links, history states with
/// their links, the folder link, then the entry. The tag itself and the binary data stay (see the
/// module doc). No check whether it is in the trash: the import rollback uses this too.
pub fn purge_item(tx: &mut CrdtTransaction<'_>, item_id: &str) -> Result<u32> {
    if !item_exists(tx, item_id)? {
        return Ok(0);
    }
    let hashes: Vec<String> = tx.query_map(
        "SELECT binary_hash FROM haex_passwords_item_binaries WHERE item_id = ?1 \
         UNION \
         SELECT sb.binary_hash FROM haex_passwords_snapshot_binaries sb \
         JOIN haex_passwords_item_snapshots s ON s.id = sb.snapshot_id WHERE s.item_id = ?1",
        params![item_id],
        |r| r.get(0),
    )?;
    let snapshot_ids: Vec<String> = tx.query_map(
        "SELECT id FROM haex_passwords_item_snapshots WHERE item_id = ?1",
        params![item_id],
        |r| r.get(0),
    )?;
    for snapshot in &snapshot_ids {
        tx.execute(
            "DELETE FROM haex_passwords_snapshot_binaries WHERE snapshot_id = ?1",
            params![snapshot],
        )?;
    }
    // Spec 036, research R6: the links as target and to its own passkeys go before the passkeys.
    super::passkey_links::delete_for_item(tx, item_id)?;
    for table in [
        "haex_passwords_item_snapshots",
        "haex_passwords_passkeys",
        "haex_passwords_item_key_values",
        "haex_passwords_item_tags",
        "haex_passwords_item_binaries",
        "haex_passwords_group_items",
    ] {
        tx.execute(
            &format!("DELETE FROM {table} WHERE item_id = ?1"),
            params![item_id],
        )?;
    }
    tx.execute(
        "DELETE FROM haex_passwords_item_details WHERE id = ?1",
        params![item_id],
    )?;
    for hash in hashes {
        binaries::mark_if_unreferenced(tx, &hash)?;
    }
    Ok(1)
}

/// Removes a folder bottom-up: its subfolders, its entries, then the folder. Returns how many
/// folders and entries went.
pub fn purge_group(tx: &mut CrdtTransaction<'_>, group_id: &str) -> Result<u32> {
    if group_id == TRASH_GROUP_ID || !group_exists(tx, group_id)? {
        return Ok(0);
    }
    let mut affected = 0;
    let children: Vec<String> = tx.query_map(
        "SELECT id FROM haex_passwords_groups WHERE parent_id = ?1",
        params![group_id],
        |r| r.get(0),
    )?;
    for child in children {
        affected += purge_group(tx, &child)?;
    }
    let items: Vec<String> = tx.query_map(
        "SELECT item_id FROM haex_passwords_group_items WHERE group_id = ?1",
        params![group_id],
        |r| r.get(0),
    )?;
    for item in items {
        affected += purge_item(tx, &item)?;
    }
    tx.execute(
        "DELETE FROM haex_passwords_groups WHERE id = ?1",
        params![group_id],
    )?;
    Ok(affected + 1)
}
