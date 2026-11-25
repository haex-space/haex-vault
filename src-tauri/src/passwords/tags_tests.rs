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

fn tag_count_of(db: &Database, name: &str) -> i64 {
    count(
        db,
        &format!("SELECT COUNT(*) FROM haex_passwords_tags WHERE name = '{name}'"),
    )
}

fn sorted_names(db: &Database, item_id: &str) -> Vec<String> {
    let mut names = tag_names_of(db, item_id);
    names.sort();
    names
}

fn merge(db: &Database, source: &str, target: &str) -> crate::error::Result<()> {
    write(db, |tx| tags::merge_tags(tx, source, target, None))
}

fn merge_named(db: &Database, source: &str, target: &str, name: &str) -> crate::error::Result<()> {
    write(db, |tx| tags::merge_tags(tx, source, target, Some(name)))
}

fn name_of_tag(db: &Database, id: &str) -> String {
    db.with_connection(|c| {
        Ok(c.query_row(
            "SELECT name FROM haex_passwords_tags WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )?)
    })
    .expect("name")
}

#[test]
fn merge_gives_every_entry_of_the_source_the_target_once_and_removes_the_source() {
    let (_dir, db) = open_test_vault();
    let only_job = item_with(&db, &["Job"]);
    let both = item_with(&db, &["Job", "Arbeit"]);
    let only_work = item_with(&db, &["Arbeit"]);
    let none = item_with(&db, &[]);
    let job = tag_id("Job").to_string();
    let arbeit = tag_id("Arbeit").to_string();

    merge(&db, &job, &arbeit).expect("merge");

    assert_eq!(sorted_names(&db, &only_job), ["Arbeit"]);
    assert_eq!(sorted_names(&db, &both), ["Arbeit"]);
    assert_eq!(sorted_names(&db, &only_work), ["Arbeit"]);
    assert!(tag_names_of(&db, &none).is_empty());
    assert_eq!(tag_count_of(&db, "Job"), 0);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM haex_passwords_tags"), 1);
    // Three entries carry the target, the one that carried both has a single link.
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_item_tags"),
        3
    );
    assert_eq!(
        count(
            &db,
            &format!("SELECT COUNT(*) FROM haex_passwords_item_tags WHERE tag_id = '{job}'")
        ),
        0
    );
}

#[test]
fn merge_keeps_name_and_color_of_the_target_and_takes_nothing_from_the_source() {
    let (_dir, db) = open_test_vault();
    item_with(&db, &["Job", "Arbeit"]);
    let job = tag_id("Job").to_string();
    let arbeit = tag_id("Arbeit").to_string();
    write(&db, |tx| tags::set_color(tx, &job, Some("#aa0000"))).expect("source color");

    merge(&db, &job, &arbeit).expect("merge");

    let (name, color): (String, Option<String>) = db
        .with_connection(|c| {
            Ok(c.query_row(
                "SELECT name, color FROM haex_passwords_tags WHERE id = ?1",
                params![arbeit],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?)
        })
        .expect("target");
    assert_eq!(name, "Arbeit");
    assert_eq!(
        color, None,
        "the source's color must not move to the target"
    );
}

#[test]
fn merging_a_tag_without_entries_removes_it() {
    let (_dir, db) = open_test_vault();
    let used = item_with(&db, &["Arbeit"]);
    let unused = write(&db, |tx| tags::get_or_create(tx, "Job")).expect("unused");
    let arbeit = tag_id("Arbeit").to_string();

    merge(&db, &unused, &arbeit).expect("merge");

    assert_eq!(tag_count_of(&db, "Job"), 0);
    assert_eq!(sorted_names(&db, &used), ["Arbeit"]);
}

#[test]
fn merge_moves_many_links_in_one_call() {
    let (_dir, db) = open_test_vault();
    let items: Vec<String> = (0..200).map(|_| item_with(&db, &["Job"])).collect();
    item_with(&db, &["Arbeit"]);
    let job = tag_id("Job").to_string();
    let arbeit = tag_id("Arbeit").to_string();

    merge(&db, &job, &arbeit).expect("merge");

    assert_eq!(
        count(
            &db,
            &format!("SELECT COUNT(*) FROM haex_passwords_item_tags WHERE tag_id = '{arbeit}'")
        ),
        201
    );
    assert_eq!(sorted_names(&db, &items[199]), ["Arbeit"]);
}

#[test]
fn a_refused_merge_changes_nothing() {
    let (_dir, db) = open_test_vault();
    let a = item_with(&db, &["Job", "Arbeit"]);
    let job = tag_id("Job").to_string();
    let arbeit = tag_id("Arbeit").to_string();
    let tags_before = count(&db, "SELECT COUNT(*) FROM haex_passwords_tags");
    let links_before = count(&db, "SELECT COUNT(*) FROM haex_passwords_item_tags");
    let markers_before = count(&db, "SELECT COUNT(*) FROM haex_deleted_rows");

    assert_eq!(reason_of(merge(&db, &job, &job)), "same");
    assert!(matches!(
        merge(&db, "nope", &arbeit),
        Err(HaexError::PasswordsNotFound)
    ));
    assert!(matches!(
        merge(&db, &job, "nope"),
        Err(HaexError::PasswordsNotFound)
    ));

    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_tags"),
        tags_before
    );
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_item_tags"),
        links_before
    );
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_deleted_rows"),
        markers_before,
        "a refusal must not leave a delete marker"
    );
    assert_eq!(sorted_names(&db, &a), ["Arbeit", "Job"]);
}

#[test]
fn merging_the_same_source_twice_finds_nothing_the_second_time() {
    let (_dir, db) = open_test_vault();
    item_with(&db, &["Job", "Arbeit"]);
    let job = tag_id("Job").to_string();
    let arbeit = tag_id("Arbeit").to_string();
    merge(&db, &job, &arbeit).expect("first");
    assert!(matches!(
        merge(&db, &job, &arbeit),
        Err(HaexError::PasswordsNotFound)
    ));
}

#[test]
fn merge_leaves_a_delete_marker_for_every_removed_row() {
    let (_dir, db) = open_test_vault();
    item_with(&db, &["Job"]);
    item_with(&db, &["Job", "Arbeit"]);
    let job = tag_id("Job").to_string();
    let arbeit = tag_id("Arbeit").to_string();

    merge(&db, &job, &arbeit).expect("merge");

    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_deleted_rows WHERE table_name = 'haex_passwords_tags' AND haex_hlc_no_sync IS NOT NULL"),
        1,
        "the source tag has a marker"
    );
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_deleted_rows WHERE table_name = 'haex_passwords_item_tags' AND haex_hlc_no_sync IS NOT NULL"),
        2,
        "each link of the source has a marker of its own"
    );
}

#[test]
fn a_tag_name_needs_a_character_that_is_not_a_space_wherever_a_tag_is_made() {
    let (_dir, db) = open_test_vault();
    let work = write(&db, |tx| tags::get_or_create(tx, "Work")).expect("work");
    let entry = item_with(&db, &["keep"]);
    let tags_before = count(&db, "SELECT COUNT(*) FROM haex_passwords_tags");

    for blank in ["", " ", "   ", "\t", "\u{a0}"] {
        assert_eq!(
            reason_of(write(&db, |tx| tags::rename_tag(tx, &work, blank))),
            "empty",
            "rename to {blank:?}"
        );
        assert_eq!(
            reason_of(write(&db, |tx| tags::get_or_create(tx, blank))),
            "empty",
            "create {blank:?}"
        );
        assert_eq!(
            reason_of(write(&db, |tx| tags::bulk_set(
                tx,
                std::slice::from_ref(&entry),
                &[blank.to_string()],
                &[]
            ))),
            "empty",
            "bulk tagging with {blank:?}"
        );
        assert_eq!(
            reason_of(write(&db, |tx| tags::set_item_tags(
                tx,
                &entry,
                &[blank.to_string()]
            ))),
            "empty",
            "tags of an entry with {blank:?}"
        );
    }

    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_tags"),
        tags_before,
        "no tag may have been made"
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_tags WHERE trim(name) = ''"
        ),
        0
    );
    assert_eq!(sorted_names(&db, &entry), ["keep"]);
}

#[test]
fn a_merge_can_name_its_result_and_moves_every_entry_to_it() {
    let (_dir, db) = open_test_vault();
    let only_job = item_with(&db, &["Job"]);
    let both = item_with(&db, &["Job", "Arbeit"]);
    let job = tag_id("Job").to_string();
    let arbeit = tag_id("Arbeit").to_string();

    merge_named(&db, &job, &arbeit, "  Beruf ").expect("merge");

    assert_eq!(name_of_tag(&db, &arbeit), "Beruf", "trimmed, on the target");
    assert_eq!(sorted_names(&db, &only_job), ["Beruf"]);
    assert_eq!(sorted_names(&db, &both), ["Beruf"]);
    assert_eq!(tag_count_of(&db, "Job"), 0);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM haex_passwords_tags"), 1);
}

#[test]
fn the_name_of_the_tag_that_goes_is_free_for_the_result() {
    let (_dir, db) = open_test_vault();
    let entry = item_with(&db, &["Job", "Arbeit"]);
    let job = tag_id("Job").to_string();
    let arbeit = tag_id("Arbeit").to_string();

    // The result takes the name of the source, in another case: the source is gone after the merge.
    merge_named(&db, &job, &arbeit, "JOB").expect("merge");

    assert_eq!(name_of_tag(&db, &arbeit), "JOB");
    assert_eq!(sorted_names(&db, &entry), ["JOB"]);
}

#[test]
fn an_unchanged_or_missing_name_leaves_the_name_of_the_target() {
    let (_dir, db) = open_test_vault();
    item_with(&db, &["Job", "Arbeit", "Home", "Spiel"]);
    let arbeit = tag_id("Arbeit").to_string();

    merge_named(&db, &tag_id("Job").to_string(), &arbeit, " Arbeit ").expect("same name");
    assert_eq!(name_of_tag(&db, &arbeit), "Arbeit");
    merge(&db, &tag_id("Home").to_string(), &arbeit).expect("no name");
    assert_eq!(name_of_tag(&db, &arbeit), "Arbeit");
    // Only a change of case is a change.
    merge_named(&db, &tag_id("Spiel").to_string(), &arbeit, "ARBEIT").expect("case");
    assert_eq!(name_of_tag(&db, &arbeit), "ARBEIT");
}

#[test]
fn a_refused_name_stops_the_merge_before_anything_is_written() {
    let (_dir, db) = open_test_vault();
    let entry = item_with(&db, &["Job", "Arbeit", "Home"]);
    let job = tag_id("Job").to_string();
    let arbeit = tag_id("Arbeit").to_string();
    let tags_before = count(&db, "SELECT COUNT(*) FROM haex_passwords_tags");
    let links_before = count(&db, "SELECT COUNT(*) FROM haex_passwords_item_tags");
    let markers_before = count(&db, "SELECT COUNT(*) FROM haex_deleted_rows");

    // The name of a tag that stays, in another case and in the other form of an umlaut.
    assert_eq!(reason_of(merge_named(&db, &job, &arbeit, "HOME")), "exists");
    assert_eq!(reason_of(merge_named(&db, &job, &arbeit, "")), "empty");
    assert_eq!(reason_of(merge_named(&db, &job, &arbeit, "   ")), "empty");
    assert_eq!(
        reason_of(merge_named(&db, &job, &arbeit, &"x".repeat(65))),
        "too_long"
    );

    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_tags"),
        tags_before
    );
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_item_tags"),
        links_before
    );
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_deleted_rows"),
        markers_before
    );
    assert_eq!(name_of_tag(&db, &arbeit), "Arbeit");
    assert_eq!(sorted_names(&db, &entry), ["Arbeit", "Home", "Job"]);
}
