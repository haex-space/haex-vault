//! Tests for tags (spec 034, FR-011, research R2): one tag per folded name, rename without a
//! collision, bulk add and remove, delete with its links.

// The tests read raw columns that the CRDT write path does not expose.
#![allow(clippy::disallowed_methods, clippy::redundant_closure)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::ids::tag_id;
use super::items;
use super::model::ItemInput;
use super::tags;
use super::test_support::open_test_vault;
use crate::error::HaexError;
use crate::storage::query;

fn write<R>(
    db: &Database,
    f: impl FnOnce(&mut haex_crdt::CrdtTransaction<'_>) -> crate::error::Result<R>,
) -> crate::error::Result<R> {
    db.write(|tx| f(tx).map_err(haex_crdt::Error::from))
        .map_err(HaexError::from)
}

fn item_with(db: &Database, tag_names: &[&str]) -> String {
    let input = ItemInput {
        tags: tag_names.iter().map(|n| n.to_string()).collect(),
        ..ItemInput::default()
    };
    write(db, |tx| items::create_item(tx, &input, None)).expect("item")
}

fn tag_names_of(db: &Database, item_id: &str) -> Vec<String> {
    query::read(db, |q| tags::names_of_item(q, item_id).map_err(Into::into)).expect("names")
}

fn count(db: &Database, sql: &str) -> i64 {
    db.with_connection(|c| Ok(c.query_row(sql, [], |r| r.get(0))?))
        .expect("count")
}

fn reason_of<R: std::fmt::Debug>(result: crate::error::Result<R>) -> String {
    match result {
        Err(HaexError::InvalidInput { reason }) => reason,
        other => panic!("expected InvalidInput, got {other:?}"),
    }
}

#[test]
fn names_that_differ_in_case_or_umlaut_form_are_one_tag() {
    let (_dir, db) = open_test_vault();
    let first = write(&db, |tx| tags::get_or_create(tx, "Work")).expect("first");
    let second = write(&db, |tx| tags::get_or_create(tx, " work ")).expect("second");
    assert_eq!(first, second);
    assert_eq!(first, tag_id("Work").to_string());
    let composed = write(&db, |tx| tags::get_or_create(tx, "M\u{fc}ller")).expect("composed");
    let decomposed = write(&db, |tx| tags::get_or_create(tx, "Mu\u{308}ller")).expect("decomposed");
    assert_eq!(composed, decomposed);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM haex_passwords_tags"), 2);
    // The spelling of the first creator stays.
    let stored: String = db
        .with_connection(|c| {
            Ok(c.query_row(
                "SELECT name FROM haex_passwords_tags WHERE id = ?1",
                params![first],
                |r| r.get(0),
            )?)
        })
        .expect("name");
    assert_eq!(stored, "Work");
}

#[test]
fn a_tag_name_must_be_neither_empty_nor_long() {
    let (_dir, db) = open_test_vault();
    assert_eq!(
        reason_of(write(&db, |tx| tags::get_or_create(tx, "  "))),
        "empty"
    );
    let long = "x".repeat(65);
    assert_eq!(
        reason_of(write(&db, |tx| tags::get_or_create(tx, &long))),
        "too_long"
    );
    write(&db, |tx| tags::get_or_create(tx, &"x".repeat(64))).expect("64 is fine");
}

#[test]
fn rename_keeps_the_id_and_refuses_a_name_that_exists() {
    let (_dir, db) = open_test_vault();
    let work = write(&db, |tx| tags::get_or_create(tx, "Work")).expect("work");
    let home = write(&db, |tx| tags::get_or_create(tx, "Home")).expect("home");
    let collision = write(&db, |tx| tags::rename_tag(tx, &work, "HOME"));
    assert_eq!(reason_of(collision), "exists");
    assert_eq!(
        reason_of(write(&db, |tx| tags::rename_tag(tx, &work, " "))),
        "empty"
    );
    assert_eq!(
        reason_of(write(&db, |tx| tags::rename_tag(
            tx,
            &work,
            &"y".repeat(65)
        ))),
        "too_long"
    );
    // A change of case on the tag itself is no collision.
    write(&db, |tx| tags::rename_tag(tx, &work, "WORK")).expect("case only");
    write(&db, |tx| tags::rename_tag(tx, &home, "Private")).expect("rename");
    let names: Vec<(String, String)> = db
        .with_connection(|c| {
            let mut stmt = c.prepare("SELECT id, name FROM haex_passwords_tags ORDER BY name")?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
        .expect("tags");
    assert_eq!(
        names,
        vec![
            (home.clone(), "Private".to_string()),
            (work.clone(), "WORK".to_string())
        ]
    );
    // The renamed tag is found under its new name, a new tag under the old one is a new row.
    let again = write(&db, |tx| tags::get_or_create(tx, "private")).expect("lookup");
    assert_eq!(again, home);
    let missing = write(&db, |tx| tags::rename_tag(tx, "nope", "X"));
    assert!(matches!(missing, Err(HaexError::PasswordsNotFound)));
}

#[test]
fn bulk_set_adds_and_removes_on_many_entries() {
    let (_dir, db) = open_test_vault();
    let a = item_with(&db, &["keep", "drop"]);
    let b = item_with(&db, &["drop"]);
    let c = item_with(&db, &[]);
    let changed = write(&db, |tx| {
        tags::bulk_set(
            tx,
            &[a.clone(), b.clone(), c.clone()],
            &["New".to_string(), "keep".to_string()],
            &["DROP".to_string()],
        )
    })
    .expect("bulk");
    assert_eq!(changed, 3);
    let sorted = |id: &str| {
        let mut names = tag_names_of(&db, id);
        names.sort();
        names
    };
    assert_eq!(sorted(&a), ["New", "keep"]);
    assert_eq!(sorted(&b), ["New", "keep"]);
    assert_eq!(sorted(&c), ["New", "keep"]);
    // The same again changes nothing.
    let again = write(&db, |tx| {
        tags::bulk_set(
            tx,
            &[a.clone(), b.clone(), c.clone()],
            &["New".to_string(), "keep".to_string()],
            &["DROP".to_string()],
        )
    })
    .expect("bulk again");
    assert_eq!(again, 0);
    let missing = write(&db, |tx| {
        tags::bulk_set(tx, &["nope".to_string()], &["x".to_string()], &[])
    });
    assert_eq!(reason_of(missing), "target_missing");
}

#[test]
fn delete_removes_the_links_and_then_the_tag() {
    let (_dir, db) = open_test_vault();
    let a = item_with(&db, &["Work", "Home"]);
    let b = item_with(&db, &["Work"]);
    let work = tag_id("Work").to_string();
    write(&db, |tx| tags::delete_tag(tx, &work)).expect("delete");
    assert_eq!(tag_names_of(&db, &a), ["Home"]);
    assert!(tag_names_of(&db, &b).is_empty());
    assert_eq!(count(&db, "SELECT COUNT(*) FROM haex_passwords_tags"), 1);
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_item_tags"),
        1
    );
    // Every removed row left a delete marker for the sync.
    assert!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_deleted_rows WHERE table_name = 'haex_passwords_item_tags'"
        ) >= 2
    );
    let missing = write(&db, |tx| tags::delete_tag(tx, &work));
    assert!(matches!(missing, Err(HaexError::PasswordsNotFound)));
}

#[test]
fn set_color_changes_only_the_color() {
    let (_dir, db) = open_test_vault();
    let id = write(&db, |tx| tags::get_or_create(tx, "Work")).expect("work");
    write(&db, |tx| tags::set_color(tx, &id, Some("#336699"))).expect("color");
    let color: Option<String> = db
        .with_connection(|c| {
            Ok(c.query_row(
                "SELECT color FROM haex_passwords_tags WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )?)
        })
        .expect("read");
    assert_eq!(color.as_deref(), Some("#336699"));
    write(&db, |tx| tags::set_color(tx, &id, None)).expect("clear");
    let missing = write(&db, |tx| tags::set_color(tx, "nope", None));
    assert!(matches!(missing, Err(HaexError::PasswordsNotFound)));
}

/// Two tags with equal names under different ids, as a rename race between two devices leaves them.
fn duplicate_tags(db: &Database) -> (String, String, String, String) {
    let a = item_with(db, &[]);
    let b = item_with(db, &[]);
    write(db, |tx| {
        for (id, name) in [("tag-aaa", "Work"), ("tag-zzz", "work")] {
            tx.execute(
                "INSERT INTO haex_passwords_tags (id, name, created_at) VALUES (?1, ?2, 'x')",
                params![id, name],
            )?;
        }
        tx.execute(
            "INSERT INTO haex_passwords_item_tags (id, item_id, tag_id) VALUES ('l1', ?1, 'tag-aaa')",
            params![a],
        )?;
        tx.execute(
            "INSERT INTO haex_passwords_item_tags (id, item_id, tag_id) VALUES ('l2', ?1, 'tag-zzz')",
            params![a],
        )?;
        tx.execute(
            "INSERT INTO haex_passwords_item_tags (id, item_id, tag_id) VALUES ('l3', ?1, 'tag-zzz')",
            params![b],
        )?;
        Ok(())
    })
    .expect("duplicates");
    (a, b, "tag-aaa".to_string(), "tag-zzz".to_string())
}

#[test]
fn reconcile_keeps_the_smallest_id_and_every_entry_keeps_its_tag() {
    let (_dir, db) = open_test_vault();
    let (a, b, keep, gone) = duplicate_tags(&db);
    let removed = write(&db, |tx| tags::reconcile_tags(tx)).expect("reconcile");
    assert_eq!(removed, 1);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM haex_passwords_tags"), 1);
    assert_eq!(
        count(
            &db,
            &format!("SELECT COUNT(*) FROM haex_passwords_tags WHERE id = '{keep}'")
        ),
        1
    );
    assert_eq!(
        count(
            &db,
            &format!("SELECT COUNT(*) FROM haex_passwords_item_tags WHERE tag_id = '{gone}'")
        ),
        0
    );
    // Both entries have the one tag, the entry that had both links has it once.
    assert_eq!(tag_names_of(&db, &a), vec!["Work".to_string()]);
    assert_eq!(tag_names_of(&db, &b), vec!["Work".to_string()]);
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_item_tags"),
        2
    );
}

#[test]
fn reconcile_moves_a_link_to_a_derived_id_and_leaves_markers_for_what_it_removed() {
    let (_dir, db) = open_test_vault();
    let (_a, b, keep, _gone) = duplicate_tags(&db);
    write(&db, |tx| tags::reconcile_tags(tx)).expect("reconcile");
    let link_id: String = db
        .with_connection(|c| {
            Ok(c.query_row(
                "SELECT id FROM haex_passwords_item_tags WHERE item_id = ?1",
                params![b],
                |r| r.get(0),
            )?)
        })
        .expect("link");
    assert_eq!(link_id, super::ids::item_tag_id(&b, &keep).to_string());
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_deleted_rows WHERE table_name = 'haex_passwords_tags' AND haex_hlc_no_sync IS NOT NULL"),
        1
    );
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_deleted_rows WHERE table_name = 'haex_passwords_item_tags' AND haex_hlc_no_sync IS NOT NULL"),
        2,
        "each link that moved or was dropped has a marker of its own"
    );
}

#[test]
fn reconcile_is_idempotent_and_leaves_distinct_tags_alone() {
    let (_dir, db) = open_test_vault();
    item_with(&db, &["alpha", "beta"]);
    assert_eq!(write(&db, |tx| tags::reconcile_tags(tx)).expect("none"), 0);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM haex_passwords_tags"), 2);
    let (_a, _b, _keep, _gone) = duplicate_tags(&db);
    assert_eq!(write(&db, |tx| tags::reconcile_tags(tx)).expect("first"), 1);
    assert_eq!(
        write(&db, |tx| tags::reconcile_tags(tx)).expect("second"),
        0
    );
    assert_eq!(count(&db, "SELECT COUNT(*) FROM haex_passwords_tags"), 3);
}
