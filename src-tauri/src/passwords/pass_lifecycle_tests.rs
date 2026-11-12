//! Tests for the hand-kept lists of tables (spec 092, research R12): purging an entry removes the
//! detail row and the codes of a pass, purging a parent clears the link of the passes that belonged
//! to it and nothing else, and copying a pass copies its codes into new rows and keeps its type.

// The tests read and write raw columns that the CRDT write path does not expose.
#![allow(clippy::disallowed_methods, clippy::redundant_closure)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::copy::{copy, CopyOptions, CopyTitle};
use super::model::{ItemInput, Target, TargetKind};
use super::test_support::open_test_vault;
use super::{items, trash};
use crate::error::HolziError;

fn write<R>(
    db: &Database,
    f: impl FnOnce(&mut haex_crdt::CrdtTransaction<'_>) -> crate::error::Result<R>,
) -> crate::error::Result<R> {
    db.write(|tx| f(tx).map_err(haex_crdt::Error::from))
        .map_err(HolziError::from)
}

fn count(db: &Database, sql: &str) -> i64 {
    db.with_connection(|c| Ok(c.query_row(sql, [], |r| r.get(0))?))
        .expect("count")
}

fn exec(db: &Database, sql: &str, args: &[&dyn haex_crdt::rusqlite::ToSql]) {
    db.with_connection(|conn| {
        conn.execute(sql, args)?;
        Ok(())
    })
    .expect("execute");
}

fn entry(db: &Database, title: &str) -> String {
    write(db, |tx| {
        items::create_item(
            tx,
            &ItemInput {
                title: Some(title.to_string()),
                ..ItemInput::default()
            },
            None,
        )
    })
    .expect("create")
}

fn pass(db: &Database, title: &str, parent: Option<&str>) -> String {
    let id = entry(db, title);
    exec(
        db,
        "UPDATE haex_passwords_item_details SET kind = 'pass' WHERE id = ?1",
        &[&id],
    );
    exec(
        db,
        "INSERT INTO haex_passwords_pass_details (item_id, pass_kind, start_date, parent_id, source_id) \
         VALUES (?1, 'voucher', '2030-01-01', ?2, 'pass.example/42')",
        &[&id, &parent],
    );
    exec(
        db,
        "INSERT INTO haex_passwords_pass_barcodes (id, item_id, format, value, label, position) \
         VALUES (?1, ?2, 'qr', 'SECRET-MARKER-lifecycle', 'One', 0)",
        &[&format!("code-{id}"), &id],
    );
    id
}

#[test]
fn purging_a_pass_removes_its_detail_row_and_its_codes() {
    let (_dir, db) = open_test_vault();
    let id = pass(&db, "Ticket", None);
    write(&db, |tx| trash::purge_item(tx, &id)).expect("purge");
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_pass_details"),
        0
    );
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_pass_barcodes"),
        0
    );
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_item_details"),
        0
    );
}

#[test]
fn purging_a_parent_clears_the_link_of_its_passes_and_keeps_them() {
    let (_dir, db) = open_test_vault();
    let account = entry(&db, "Train account");
    let one = pass(&db, "Trip", Some(&account));
    let two = pass(&db, "Return", Some(&account));
    let other = pass(&db, "Other", Some("elsewhere"));
    write(&db, |tx| trash::purge_item(tx, &account)).expect("purge the parent");
    for id in [&one, &two] {
        let parent: Option<String> = db
            .with_connection(|c| {
                Ok(c.query_row(
                    "SELECT parent_id FROM haex_passwords_pass_details WHERE item_id = ?1",
                    params![id],
                    |r| r.get(0),
                )?)
            })
            .expect("parent");
        assert_eq!(parent, None, "the link is cleared");
    }
    let kept: String = db
        .with_connection(|c| {
            Ok(c.query_row(
                "SELECT parent_id FROM haex_passwords_pass_details WHERE item_id = ?1",
                params![other],
                |r| r.get(0),
            )?)
        })
        .expect("other parent");
    assert_eq!(kept, "elsewhere", "a link to another entry is left alone");
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_pass_details"),
        3
    );
}

#[test]
fn copying_a_pass_copies_its_type_start_and_codes_into_new_rows_without_the_source() {
    let (_dir, db) = open_test_vault();
    let account = entry(&db, "Card");
    let id = pass(&db, "Voucher", Some(&account));
    let options = CopyOptions {
        title: CopyTitle::Suffix(" – Copy".to_string()),
        history: false,
        username_as_reference: false,
        password_as_reference: false,
        passkeys_as_links: None,
    };
    let target = Target {
        kind: TargetKind::Item,
        id: id.clone(),
    };
    write(&db, |tx| copy(tx, &[target], None, &options)).expect("copy");
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_item_details WHERE kind = 'pass'"
        ),
        2
    );
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_pass_details"),
        2
    );
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_pass_barcodes"),
        2
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(DISTINCT id) FROM haex_passwords_pass_barcodes"
        ),
        2,
        "the copy has rows of its own"
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_pass_details \
             WHERE source_id IS NULL AND parent_id IS NOT NULL AND start_date = '2030-01-01'"
        ),
        1,
        "the copy keeps start and parent but not the imported file's source"
    );
}
