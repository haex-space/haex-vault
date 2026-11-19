//! Tests for the trash (spec 034, US4, FR-015, FR-016, research R3): deleting moves into the trash
//! and remembers the place, restoring returns it, deleting in the trash removes an entry with
//! everything that hangs on it (children first, every row with its own delete marker), and the trash
//! row itself is created on demand.

// The tests read raw columns that the CRDT write path does not expose.
#![allow(clippy::disallowed_methods, clippy::redundant_closure)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;
use zeroize::Zeroizing;

use super::groups;
use super::items;
use super::model::{ItemInput, KeyValueInput, Target, TargetKind};
use super::passkeys::{self, PasskeyInput};
use super::test_support::open_test_vault;
use super::trash;
use crate::error::HaexError;

fn write<R>(
    db: &Database,
    f: impl FnOnce(&mut haex_crdt::CrdtTransaction<'_>) -> crate::error::Result<R>,
) -> crate::error::Result<R> {
    db.write(|tx| f(tx).map_err(haex_crdt::Error::from))
        .map_err(HaexError::from)
}

fn item_target(id: &str) -> Target {
    Target {
        kind: TargetKind::Item,
        id: id.to_string(),
    }
}

fn group_target(id: &str) -> Target {
    Target {
        kind: TargetKind::Group,
        id: id.to_string(),
    }
}

fn group(db: &Database, name: &str, parent: Option<&str>) -> String {
    write(db, |tx| {
        groups::create_group(tx, name, None, None, None, parent)
    })
    .expect("group")
}

fn item_in(db: &Database, group_id: Option<&str>) -> String {
    write(db, |tx| {
        items::create_item(
            tx,
            &ItemInput {
                title: Some("Entry".to_string()),
                tags: vec!["work".to_string()],
                key_values: vec![KeyValueInput {
                    key: "PIN".to_string(),
                    value: Some("1234".to_string()),
                }],
                ..ItemInput::default()
            },
            group_id,
        )
    })
    .expect("item")
}

fn count(db: &Database, sql: &str) -> i64 {
    db.with_connection(|c| Ok(c.query_row(sql, [], |r| r.get(0))?))
        .expect("count")
}

fn cell(db: &Database, sql: &str, arg: &str) -> Option<String> {
    db.with_connection(|c| Ok(c.query_row(sql, params![arg], |r| r.get::<_, Option<String>>(0))?))
        .expect("cell")
}

fn markers(db: &Database, table: &str) -> i64 {
    count(
        db,
        &format!("SELECT COUNT(*) FROM haex_deleted_rows WHERE table_name = '{table}'"),
    )
}

#[test]
fn trashing_creates_the_trash_row_and_remembers_the_folder() {
    let (_dir, db) = open_test_vault();
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_groups WHERE id = 'trash'"
        ),
        0
    );
    let folder = group(&db, "Work", None);
    let in_folder = item_in(&db, Some(&folder));
    let at_root = item_in(&db, None);
    let affected = write(&db, |tx| {
        trash::trash(tx, &[item_target(&in_folder), item_target(&at_root)])
    })
    .expect("trash");
    assert_eq!(affected, 2);
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_groups WHERE id = 'trash' AND name IS NULL AND parent_id IS NULL"
        ),
        1
    );
    let sql = "SELECT group_id FROM haex_passwords_group_items WHERE item_id = ?1";
    let from = "SELECT trashed_from_group_id FROM haex_passwords_group_items WHERE item_id = ?1";
    assert_eq!(cell(&db, sql, &in_folder).as_deref(), Some("trash"));
    assert_eq!(
        cell(&db, from, &in_folder).as_deref(),
        Some(folder.as_str())
    );
    assert_eq!(cell(&db, sql, &at_root).as_deref(), Some("trash"));
    assert_eq!(cell(&db, from, &at_root), None, "the root has no origin");
    // The data is intact: trashing does not delete.
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_item_details"),
        2
    );
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_item_key_values"),
        2
    );
}

#[test]
fn a_folder_goes_to_the_trash_with_its_structure_and_remembers_its_parent() {
    let (_dir, db) = open_test_vault();
    let top = group(&db, "Top", None);
    let child = group(&db, "Child", Some(&top));
    let inner = item_in(&db, Some(&child));
    write(&db, |tx| trash::trash(tx, &[group_target(&top)])).expect("trash");
    let parent = "SELECT parent_id FROM haex_passwords_groups WHERE id = ?1";
    let from = "SELECT trashed_from_parent_id FROM haex_passwords_groups WHERE id = ?1";
    assert_eq!(cell(&db, parent, &top).as_deref(), Some("trash"));
    assert_eq!(cell(&db, from, &top), None, "the top level has no origin");
    assert_eq!(
        cell(&db, parent, &child).as_deref(),
        Some(top.as_str()),
        "structure kept"
    );
    assert_eq!(
        cell(&db, from, &child),
        None,
        "only the folder deleted directly has an origin"
    );
    assert_eq!(
        cell(
            &db,
            "SELECT group_id FROM haex_passwords_group_items WHERE item_id = ?1",
            &inner
        )
        .as_deref(),
        Some(child.as_str())
    );
    // A nested folder remembers its parent.
    let (a, b) = (group(&db, "A", None), group(&db, "B", None));
    let nested = group(&db, "Nested", Some(&a));
    write(&db, |tx| trash::trash(tx, &[group_target(&nested)])).expect("trash nested");
    assert_eq!(cell(&db, from, &nested).as_deref(), Some(a.as_str()));
    let _ = b;
}

#[test]
fn restore_returns_the_entry_to_its_folder_or_the_root() {
    let (_dir, db) = open_test_vault();
    let folder = group(&db, "Work", None);
    let id = item_in(&db, Some(&folder));
    write(&db, |tx| trash::trash(tx, &[item_target(&id)])).expect("trash");
    write(&db, |tx| trash::restore(tx, &[item_target(&id)])).expect("restore");
    let sql = "SELECT group_id FROM haex_passwords_group_items WHERE item_id = ?1";
    assert_eq!(cell(&db, sql, &id).as_deref(), Some(folder.as_str()));
    assert_eq!(
        cell(
            &db,
            "SELECT trashed_from_group_id FROM haex_passwords_group_items WHERE item_id = ?1",
            &id
        ),
        None,
        "the origin is cleared"
    );
    // The folder is gone meanwhile: the root.
    write(&db, |tx| trash::trash(tx, &[item_target(&id)])).expect("trash again");
    db.with_connection(|c| {
        c.execute_batch("PRAGMA foreign_keys = OFF")?;
        c.execute(
            "DELETE FROM haex_passwords_groups WHERE id = ?1",
            params![folder],
        )?;
        c.execute_batch("PRAGMA foreign_keys = ON")?;
        Ok(())
    })
    .expect("folder removed elsewhere");
    write(&db, |tx| trash::restore(tx, &[item_target(&id)])).expect("restore");
    assert_eq!(cell(&db, sql, &id), None, "at the root");
}

#[test]
fn restore_puts_an_entry_of_a_trashed_folder_at_the_root_and_a_folder_back_under_its_parent() {
    let (_dir, db) = open_test_vault();
    let top = group(&db, "Top", None);
    let sub = group(&db, "Sub", Some(&top));
    let inner = item_in(&db, Some(&sub));
    write(&db, |tx| trash::trash(tx, &[group_target(&sub)])).expect("trash sub");
    // The entry lies in the trashed folder, which is not restored: restoring it goes to the root.
    write(&db, |tx| trash::restore(tx, &[item_target(&inner)])).expect("restore entry");
    assert_eq!(
        cell(
            &db,
            "SELECT group_id FROM haex_passwords_group_items WHERE item_id = ?1",
            &inner
        ),
        None
    );
    // The folder returns under its old parent.
    write(&db, |tx| trash::restore(tx, &[group_target(&sub)])).expect("restore folder");
    assert_eq!(
        cell(
            &db,
            "SELECT parent_id FROM haex_passwords_groups WHERE id = ?1",
            &sub
        )
        .as_deref(),
        Some(top.as_str())
    );
}

#[test]
fn trashing_what_is_already_in_the_trash_deletes_it_for_good() {
    let (_dir, db) = open_test_vault();
    let id = item_in(&db, None);
    write(&db, |tx| trash::trash(tx, &[item_target(&id)])).expect("first");
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_item_details"),
        1
    );
    write(&db, |tx| trash::trash(tx, &[item_target(&id)])).expect("second");
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_item_details"),
        0
    );
}

fn passkey(item: &str) -> PasskeyInput {
    PasskeyInput {
        item_id: Some(item.to_string()),
        credential_id: format!("cred-{item}"),
        relying_party_id: "example.com".to_string(),
        relying_party_name: None,
        user_name: None,
        user_display_name: None,
        user_handle: "dXNlcg==".to_string(),
        private_key: Zeroizing::new("private".to_string()),
        public_key: "public".to_string(),
        algorithm: -7,
        sign_count: 0,
        is_discoverable: true,
        icon: None,
        color: None,
        nickname: None,
        created_at: None,
        last_used_at: None,
    }
}

/// An entry with every kind of child: a passkey, an attachment, a snapshot with an attachment.
fn full_entry(db: &Database) -> String {
    let id = item_in(db, None);
    write(db, |tx| passkeys::insert(tx, &passkey(&id)).map(|_| ())).expect("passkey");
    db.write(|tx| {
        tx.execute(
            "INSERT INTO haex_passwords_binaries (hash, data, size) VALUES ('h-attach', x'01', 1)",
            &[],
        )?;
        tx.execute(
            "INSERT INTO haex_passwords_binaries (hash, data, size) VALUES ('h-snap', x'02', 1)",
            &[],
        )?;
        tx.execute(
            "INSERT INTO haex_passwords_item_binaries (id, item_id, binary_hash, file_name) \
             VALUES ('ib1', ?1, 'h-attach', 'a.txt')",
            params![id],
        )?;
        tx.execute(
            "INSERT INTO haex_passwords_item_snapshots (id, item_id, snapshot_data, modified_at) \
             VALUES ('s1', ?1, '{}', '2026-10-01T10:00:00.000Z')",
            params![id],
        )?;
        tx.execute(
            "INSERT INTO haex_passwords_snapshot_binaries (id, snapshot_id, binary_hash, file_name) \
             VALUES ('sb1', 's1', 'h-snap', 'old.txt')",
            &[],
        )?;
        Ok(())
    })
    .expect("children");
    id
}

#[test]
fn deleting_in_the_trash_removes_the_entry_and_everything_that_hangs_on_it() {
    let (_dir, db) = open_test_vault();
    let id = full_entry(&db);
    write(&db, |tx| trash::trash(tx, &[item_target(&id)])).expect("trash");
    let affected =
        write(&db, |tx| trash::delete_permanently(tx, &[item_target(&id)])).expect("delete");
    assert_eq!(affected, 1);
    for table in [
        "haex_passwords_item_details",
        "haex_passwords_item_key_values",
        "haex_passwords_item_tags",
        "haex_passwords_item_binaries",
        "haex_passwords_item_snapshots",
        "haex_passwords_snapshot_binaries",
        "haex_passwords_group_items",
        "haex_passwords_passkeys",
    ] {
        assert_eq!(
            count(&db, &format!("SELECT COUNT(*) FROM {table}")),
            0,
            "{table}"
        );
        assert!(
            markers(&db, table) >= 1,
            "{table}: every removed row leaves its own marker"
        );
    }
    // The tag itself and the binaries stay: the tag may be used elsewhere, the binaries are pruned
    // later, after the grace period.
    assert_eq!(count(&db, "SELECT COUNT(*) FROM haex_passwords_tags"), 1);
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_binaries"),
        2
    );
    // The last link is gone, so the binaries start their grace period.
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_binaries WHERE orphaned_at IS NOT NULL"
        ),
        2
    );
}

#[test]
fn a_binary_that_another_entry_still_links_is_not_marked() {
    let (_dir, db) = open_test_vault();
    let first = full_entry(&db);
    let second = item_in(&db, None);
    db.write(|tx| {
        tx.execute(
            "INSERT INTO haex_passwords_item_binaries (id, item_id, binary_hash, file_name) \
             VALUES ('ib2', ?1, 'h-attach', 'same.txt')",
            params![second],
        )?;
        Ok(())
    })
    .expect("shared link");
    write(&db, |tx| trash::trash(tx, &[item_target(&first)])).expect("trash");
    write(&db, |tx| {
        trash::delete_permanently(tx, &[item_target(&first)])
    })
    .expect("delete");
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_binaries WHERE hash = 'h-attach' AND orphaned_at IS NULL"),
        1
    );
}

#[test]
fn a_folder_is_deleted_bottom_up_with_its_entries() {
    let (_dir, db) = open_test_vault();
    let top = group(&db, "Top", None);
    let child = group(&db, "Child", Some(&top));
    item_in(&db, Some(&top));
    item_in(&db, Some(&child));
    write(&db, |tx| trash::trash(tx, &[group_target(&top)])).expect("trash");
    let affected = write(&db, |tx| {
        trash::delete_permanently(tx, &[group_target(&top)])
    })
    .expect("delete");
    assert_eq!(
        affected, 4,
        "two folders and two entries are counted one by one"
    );
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_item_details"),
        0
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_groups WHERE id <> 'trash'"
        ),
        0
    );
    assert!(markers(&db, "haex_passwords_groups") >= 2);
}

#[test]
fn only_what_is_in_the_trash_can_be_deleted_for_good_by_the_user_path() {
    let (_dir, db) = open_test_vault();
    let id = item_in(&db, None);
    let live = write(&db, |tx| trash::delete_permanently(tx, &[item_target(&id)]));
    assert!(matches!(live, Err(HaexError::InvalidInput { reason }) if reason == "not_in_trash"));
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_item_details"),
        1
    );
    let missing = write(&db, |tx| {
        trash::delete_permanently(tx, &[item_target("nope")])
    });
    assert!(matches!(missing, Err(HaexError::PasswordsNotFound)));
}

#[test]
fn empty_trash_removes_the_children_and_keeps_the_trash_row() {
    let (_dir, db) = open_test_vault();
    let a = item_in(&db, None);
    let folder = group(&db, "F", None);
    item_in(&db, Some(&folder));
    let keep = item_in(&db, None);
    write(&db, |tx| {
        trash::trash(tx, &[item_target(&a), group_target(&folder)])
    })
    .expect("trash");
    let affected = write(&db, |tx| trash::empty_trash(tx)).expect("empty");
    assert_eq!(affected, 3, "an entry, a folder and the entry in it");
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_item_details"),
        1
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_groups WHERE id = 'trash'"
        ),
        1
    );
    assert_eq!(
        cell(
            &db,
            "SELECT id FROM haex_passwords_item_details WHERE id = ?1",
            &keep
        )
        .as_deref(),
        Some(keep.as_str())
    );
    // Emptying an empty trash is fine.
    assert_eq!(write(&db, |tx| trash::empty_trash(tx)).expect("again"), 0);
}

#[test]
fn the_trash_row_itself_cannot_be_trashed_restored_or_deleted() {
    let (_dir, db) = open_test_vault();
    write(&db, |tx| trash::ensure_trash(tx)).expect("ensure");
    for result in [
        write(&db, |tx| trash::trash(tx, &[group_target("trash")])),
        write(&db, |tx| trash::restore(tx, &[group_target("trash")])),
        write(&db, |tx| {
            trash::delete_permanently(tx, &[group_target("trash")])
        }),
    ] {
        assert!(
            matches!(result, Err(HaexError::InvalidInput { .. })),
            "{result:?}"
        );
    }
    // Ensuring twice is fine and leaves one row.
    write(&db, |tx| trash::ensure_trash(tx)).expect("ensure again");
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_groups WHERE id = 'trash'"
        ),
        1
    );
}

#[test]
fn a_statement_that_removes_several_rows_leaves_one_marker_per_row() {
    let (_dir, db) = open_test_vault();
    let id = write(&db, |tx| {
        items::create_item(
            tx,
            &ItemInput {
                tags: vec!["a".to_string(), "b".to_string(), "c".to_string()],
                key_values: (1..=3)
                    .map(|n| KeyValueInput {
                        key: format!("k{n}"),
                        value: Some("v".to_string()),
                    })
                    .collect(),
                ..ItemInput::default()
            },
            None,
        )
    })
    .expect("item");
    write(&db, |tx| trash::trash(tx, &[item_target(&id)])).expect("trash");
    write(&db, |tx| trash::delete_permanently(tx, &[item_target(&id)])).expect("delete");
    assert_eq!(markers(&db, "haex_passwords_item_tags"), 3);
    assert_eq!(markers(&db, "haex_passwords_item_key_values"), 3);
    assert_eq!(markers(&db, "haex_passwords_item_details"), 1);
}
