//! Tests for folders (spec 034, US2, FR-009, FR-010): nesting, moving an entry to one folder,
//! cycles, the trash as a target, the order of siblings and its explicit reordering.

// The tests read raw columns that the CRDT write path does not expose.
#![allow(clippy::disallowed_methods, clippy::redundant_closure)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::groups::{self, siblings};
use super::items;
use super::model::{GroupPatch, ItemInput, Patch, Target, TargetKind};
use super::test_support::open_test_vault;
use super::trash;
use crate::error::HaexError;
use crate::storage::query;

fn write<R>(
    db: &Database,
    f: impl FnOnce(&mut haex_crdt::CrdtTransaction<'_>) -> crate::error::Result<R>,
) -> crate::error::Result<R> {
    db.write(|tx| f(tx).map_err(haex_crdt::Error::from))
        .map_err(HaexError::from)
}

fn read<R>(
    db: &Database,
    f: impl FnOnce(&mut query::Reader<'_, '_>) -> crate::error::Result<R>,
) -> crate::error::Result<R> {
    query::read(db, |q| f(q).map_err(haex_crdt::Error::from)).map_err(HaexError::from)
}

fn group(db: &Database, name: &str, parent: Option<&str>) -> String {
    write(db, |tx| {
        groups::create_group(tx, name, None, None, None, parent)
    })
    .expect("group")
}

fn item(db: &Database) -> String {
    write(db, |tx| items::create_item(tx, &ItemInput::default(), None)).expect("item")
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

fn group_of(db: &Database, item_id: &str) -> Option<String> {
    db.with_connection(|conn| {
        Ok(conn.query_row(
            "SELECT group_id FROM haex_passwords_group_items WHERE item_id = ?1",
            params![item_id],
            |r| r.get::<_, Option<String>>(0),
        )?)
    })
    .expect("group of item")
}

fn parent_of(db: &Database, group_id: &str) -> Option<String> {
    db.with_connection(|conn| {
        Ok(conn.query_row(
            "SELECT parent_id FROM haex_passwords_groups WHERE id = ?1",
            params![group_id],
            |r| r.get::<_, Option<String>>(0),
        )?)
    })
    .expect("parent")
}

fn reason_of<R: std::fmt::Debug>(result: crate::error::Result<R>) -> String {
    match result {
        Err(HaexError::InvalidInput { reason }) => reason,
        other => panic!("expected InvalidInput, got {other:?}"),
    }
}

fn names(db: &Database, parent: Option<&str>) -> Vec<String> {
    read(db, |q| siblings(q, parent))
        .expect("siblings")
        .into_iter()
        .map(|g| g.name.unwrap_or_default())
        .collect()
}

#[test]
fn groups_nest_and_need_a_name() {
    let (_dir, db) = open_test_vault();
    let work = group(&db, "Work", None);
    let nested = group(&db, "Servers", Some(&work));
    assert_eq!(parent_of(&db, &nested), Some(work));
    let empty = write(&db, |tx| {
        groups::create_group(tx, "  ", None, None, None, None)
    });
    assert_eq!(reason_of(empty), "name");
    let orphan = write(&db, |tx| {
        groups::create_group(tx, "X", None, None, None, Some("nope"))
    });
    assert_eq!(reason_of(orphan), "parent");
}

#[test]
fn an_entry_lies_in_exactly_one_folder() {
    let (_dir, db) = open_test_vault();
    let (a, b) = (group(&db, "A", None), group(&db, "B", None));
    let id = item(&db);
    write(&db, |tx| {
        groups::move_targets(tx, &[item_target(&id)], Some(&a))
    })
    .expect("to a");
    assert_eq!(group_of(&db, &id), Some(a.clone()));
    write(&db, |tx| {
        groups::move_targets(tx, &[item_target(&id)], Some(&b))
    })
    .expect("to b");
    assert_eq!(group_of(&db, &id), Some(b));
    assert_eq!(
        db.with_connection(|c| Ok(c.query_row(
            "SELECT COUNT(*) FROM haex_passwords_group_items WHERE item_id = ?1",
            params![id],
            |r| r.get::<_, i64>(0)
        )?))
        .expect("count"),
        1
    );
    write(&db, |tx| {
        groups::move_targets(tx, &[item_target(&id)], None)
    })
    .expect("to root");
    assert_eq!(
        group_of(&db, &id),
        None,
        "moving to nowhere puts it at the root"
    );
}

#[test]
fn a_folder_cannot_go_into_itself_or_below_itself() {
    let (_dir, db) = open_test_vault();
    let top = group(&db, "Top", None);
    let child = group(&db, "Child", Some(&top));
    let grandchild = group(&db, "Grandchild", Some(&child));
    for to in [&top, &child, &grandchild] {
        let result = write(&db, |tx| {
            groups::move_targets(tx, &[group_target(&top)], Some(to))
        });
        assert_eq!(reason_of(result), "cycle", "into {to}");
    }
    // Bulk: one bad target stops the whole move and nothing changed.
    let other = group(&db, "Other", None);
    let result = write(&db, |tx| {
        groups::move_targets(
            tx,
            &[group_target(&other), group_target(&child)],
            Some(&grandchild),
        )
    });
    assert_eq!(reason_of(result), "cycle");
    assert_eq!(
        parent_of(&db, &other),
        None,
        "atomic: the first target did not move either"
    );
    // A legal move works.
    write(&db, |tx| {
        groups::move_targets(tx, &[group_target(&other)], Some(&grandchild))
    })
    .expect("ok");
    assert_eq!(parent_of(&db, &other), Some(grandchild));
}

#[test]
fn nothing_can_be_moved_into_the_trash() {
    let (_dir, db) = open_test_vault();
    write(&db, |tx| {
        tx.execute(
            "INSERT INTO haex_passwords_groups (id, name) VALUES ('trash', NULL)",
            &[],
        )?;
        Ok(())
    })
    .expect("trash row");
    let below = write(&db, |tx| {
        groups::create_group(tx, "Below", None, None, None, Some("trash"))
    });
    // A folder created below the trash is refused as well.
    assert_eq!(reason_of(below), "target_in_trash");
    let id = item(&db);
    let to_trash = write(&db, |tx| {
        groups::move_targets(tx, &[item_target(&id)], Some("trash"))
    });
    assert_eq!(reason_of(to_trash), "target_in_trash");
    let missing = write(&db, |tx| {
        groups::move_targets(tx, &[item_target("nope")], None)
    });
    assert_eq!(reason_of(missing), "target_missing");
    let trash_itself = write(&db, |tx| {
        groups::move_targets(tx, &[group_target("trash")], None)
    });
    assert_eq!(reason_of(trash_itself), "target_in_trash");

    let trashed_item = item(&db);
    write(&db, |tx| trash::trash(tx, &[item_target(&trashed_item)])).expect("trash item");
    let moving_trashed_item = write(&db, |tx| {
        groups::move_targets(tx, &[item_target(&trashed_item)], None)
    });
    assert_eq!(reason_of(moving_trashed_item), "target_in_trash");
    assert_eq!(group_of(&db, &trashed_item), Some("trash".to_string()));

    let trashed_group = group(&db, "Trashed", None);
    write(&db, |tx| trash::trash(tx, &[group_target(&trashed_group)])).expect("trash group");
    let moving_trashed_group = write(&db, |tx| {
        groups::move_targets(tx, &[group_target(&trashed_group)], None)
    });
    assert_eq!(reason_of(moving_trashed_group), "target_in_trash");
    assert_eq!(parent_of(&db, &trashed_group), Some("trash".to_string()));
}

#[test]
fn siblings_sort_by_order_then_name() {
    let (_dir, db) = open_test_vault();
    group(&db, "banana", None);
    group(&db, "Apple", None);
    let cherry = group(&db, "cherry", None);
    assert_eq!(
        names(&db, None),
        ["Apple", "banana", "cherry"],
        "no order: by name, any case"
    );
    write(&db, |tx| {
        groups::update_group(
            tx,
            &cherry,
            &GroupPatch {
                sort_order: Patch::Set(-1),
                ..GroupPatch::default()
            },
        )
    })
    .expect("order");
    assert_eq!(names(&db, None), ["cherry", "Apple", "banana"]);
}

#[test]
fn reorder_sets_zero_one_two_for_exactly_the_siblings() {
    let (_dir, db) = open_test_vault();
    let (a, b, c) = (
        group(&db, "A", None),
        group(&db, "B", None),
        group(&db, "C", None),
    );
    let inner = group(&db, "Inner", Some(&a));
    let wanted = [c.clone(), a.clone(), b.clone()];
    write(&db, |tx| groups::reorder_groups(tx, None, &wanted)).expect("reorder");
    assert_eq!(
        names(&db, None),
        ["C", "A", "B"],
        "the new order survives a reload"
    );
    let orders: Vec<i32> = read(&db, |q| siblings(q, None))
        .expect("siblings")
        .iter()
        .map(|g| g.sort_order.unwrap_or(-9))
        .collect();
    assert_eq!(orders, [0, 1, 2]);
    // The inner folder is not a sibling of the top level.
    let foreign = write(&db, |tx| {
        groups::reorder_groups(tx, None, &[a.clone(), b.clone(), c.clone(), inner.clone()])
    });
    assert_eq!(reason_of(foreign), "not_siblings");
    // A list that misses a sibling is not the set of siblings either.
    let partial = write(&db, |tx| {
        groups::reorder_groups(tx, None, &[a.clone(), b.clone()])
    });
    assert_eq!(reason_of(partial), "not_siblings");
}

#[test]
fn the_trash_subtree_cannot_be_reordered() {
    let (_dir, db) = open_test_vault();
    write(&db, |tx| {
        tx.execute(
            "INSERT INTO haex_passwords_groups (id) VALUES ('trash')",
            &[],
        )?;
        tx.execute(
            "INSERT INTO haex_passwords_groups (id, name, parent_id) VALUES ('t1', 'Old', 'trash')",
            &[],
        )?;
        Ok(())
    })
    .expect("seed");
    let result = write(&db, |tx| {
        groups::reorder_groups(tx, Some("trash"), &["t1".to_string()])
    });
    assert_eq!(reason_of(result), "in_trash");
}

#[test]
fn update_group_changes_only_what_is_given_and_keeps_a_name() {
    let (_dir, db) = open_test_vault();
    let id = group(&db, "Old", None);
    write(&db, |tx| {
        groups::update_group(
            tx,
            &id,
            &GroupPatch {
                name: Patch::Set("New".to_string()),
                description: Patch::Set("notes".to_string()),
                icon: Patch::Set("lucide:folder".to_string()),
                color: Patch::Set("#336699".to_string()),
                ..GroupPatch::default()
            },
        )
    })
    .expect("update");
    let row = read(&db, |q| siblings(q, None))
        .expect("siblings")
        .remove(0);
    assert_eq!(row.name.as_deref(), Some("New"));
    assert_eq!(row.description.as_deref(), Some("notes"));
    assert_eq!(row.icon.as_deref(), Some("lucide:folder"));
    let empty = write(&db, |tx| {
        groups::update_group(
            tx,
            &id,
            &GroupPatch {
                name: Patch::Set("   ".to_string()),
                ..GroupPatch::default()
            },
        )
    });
    assert_eq!(reason_of(empty), "name");
    let cleared = write(&db, |tx| {
        groups::update_group(
            tx,
            &id,
            &GroupPatch {
                name: Patch::Clear,
                ..GroupPatch::default()
            },
        )
    });
    assert_eq!(reason_of(cleared), "name", "a folder keeps a name");
    let missing = write(&db, |tx| {
        groups::update_group(tx, "nope", &GroupPatch::default())
    });
    assert!(matches!(missing, Err(HaexError::PasswordsNotFound)));
}
