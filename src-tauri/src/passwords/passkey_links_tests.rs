//! Tests for passkeys shown by a link (spec 036, T059, T065, research R6): no link to an entry's
//! own passkey or to one without an entry, one row per pair, the link goes with its passkey and
//! with either entry deleted for good, "Verweis lösen" drops only the link, and a copy shows the
//! passkeys by a link, never by value. (The deletes and the copy live here and not in
//! `trash_tests.rs` and `copy_tests.rs`, which are at the 500-line boundary.)

// The tests read raw rows that the CRDT write path does not expose.
#![allow(clippy::disallowed_methods, clippy::redundant_closure)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;
use zeroize::Zeroizing;

use super::copy::{copy, CopyOptions, CopyTitle};
use super::ids::passkey_link_id;
use super::items;
use super::model::{ItemInput, Target, TargetKind};
use super::passkey_links;
use super::passkeys::{self, InsertOutcome, PasskeyInput};
use super::test_support::{open_test_vault, MARKER};
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

fn item(db: &Database, title: &str) -> String {
    let input = ItemInput {
        title: Some(title.to_string()),
        ..ItemInput::default()
    };
    write(db, |tx| items::create_item(tx, &input, None)).expect("item")
}

fn passkey(db: &Database, item_id: Option<&str>, credential: &str) -> String {
    let input = PasskeyInput {
        item_id: item_id.map(str::to_string),
        credential_id: credential.to_string(),
        relying_party_id: "example.com".to_string(),
        relying_party_name: Some("Example".to_string()),
        user_name: Some("alice".to_string()),
        user_display_name: None,
        user_handle: "dXNlcg".to_string(),
        private_key: Zeroizing::new(format!("{MARKER}-private")),
        public_key: "public".to_string(),
        algorithm: -7,
        sign_count: 0,
        is_discoverable: true,
        icon: None,
        color: None,
        nickname: None,
        created_at: None,
        last_used_at: None,
    };
    match write(db, |tx| passkeys::insert(tx, &input)).expect("insert") {
        InsertOutcome::Created(id) => id,
        InsertOutcome::Duplicate => panic!("a new credential"),
    }
}

fn count(db: &Database, sql: &str) -> i64 {
    db.with_connection(|conn| Ok(conn.query_row(sql, [], |r| r.get(0))?))
        .expect("count")
}

fn links(db: &Database) -> i64 {
    count(db, "SELECT COUNT(*) FROM haex_passwords_passkey_links")
}

/// A passkey at `source` shown at `target`; returns the passkey id.
fn linked(db: &Database, source: &str, target: &str, credential: &str) -> String {
    let id = passkey(db, Some(source), credential);
    write(db, |tx| passkey_links::link(tx, target, &id)).expect("link");
    id
}

#[test]
fn a_link_to_the_entrys_own_passkey_or_to_one_without_an_entry_is_refused() {
    let (_dir, db) = open_test_vault();
    let source = item(&db, "Konto");
    let own = passkey(&db, Some(&source), "b3du");
    let orphan = passkey(&db, None, "b3JwaGFu");

    let refused = write(&db, |tx| passkey_links::link(tx, &source, &own));
    assert!(
        matches!(refused, Err(HaexError::InvalidInput { ref reason }) if reason == "passkeyId"),
        "{refused:?}"
    );
    let target = item(&db, "Kopie");
    let refused = write(&db, |tx| passkey_links::link(tx, &target, &orphan));
    assert!(
        matches!(refused, Err(HaexError::InvalidInput { .. })),
        "{refused:?}"
    );
    let missing = write(&db, |tx| passkey_links::link(tx, &target, "nope"));
    assert!(matches!(missing, Err(HaexError::PasswordsNotFound)));
    let no_entry = write(&db, |tx| passkey_links::link(tx, "nope", &own));
    assert!(matches!(no_entry, Err(HaexError::PasswordsNotFound)));
    assert_eq!(links(&db), 0);
}

#[test]
fn linking_twice_is_one_row_with_the_derived_id() {
    let (_dir, db) = open_test_vault();
    let source = item(&db, "Konto");
    let target = item(&db, "Kopie");
    let pk = passkey(&db, Some(&source), "Y3JlZA==");
    let first = write(&db, |tx| passkey_links::link(tx, &target, &pk)).expect("link");
    let second = write(&db, |tx| passkey_links::link(tx, &target, &pk)).expect("link again");
    assert_eq!(first, second);
    assert_eq!(first, passkey_link_id(&target, &pk).to_string());
    assert_eq!(links(&db), 1);
}

#[test]
fn the_target_shows_the_linked_passkey_with_its_source() {
    let (_dir, db) = open_test_vault();
    let source = item(&db, "Konto");
    let target = item(&db, "Kopie");
    let pk = passkey(&db, Some(&source), "Y3JlZA==");
    write(&db, |tx| passkey_links::link(tx, &target, &pk)).expect("link");

    let shown = query::read(&db, |q| {
        passkeys::list_for_item(q, &target).map_err(Into::into)
    })
    .expect("list");
    assert_eq!(shown.len(), 1);
    let linked = shown[0].linked_from.as_ref().expect("a link");
    assert_eq!(linked.item_id, source);
    assert_eq!(linked.title.as_deref(), Some("Konto"));
    assert_eq!(shown[0].item_id.as_deref(), Some(source.as_str()));

    let own = query::read(&db, |q| {
        passkeys::list_for_item(q, &source).map_err(Into::into)
    })
    .expect("list");
    assert_eq!(own.len(), 1);
    assert!(own[0].linked_from.is_none());
    assert_eq!(
        query::read(&db, |q| {
            passkey_links::links_to_items(q, std::slice::from_ref(&source)).map_err(Into::into)
        })
        .expect("count")
        .get(&source),
        Some(&1)
    );
}

#[test]
fn deleting_the_passkey_removes_its_links_and_unlinking_keeps_the_passkey() {
    let (_dir, db) = open_test_vault();
    let source = item(&db, "Konto");
    let target = item(&db, "Kopie");
    let pk = passkey(&db, Some(&source), "Y3JlZA==");
    write(&db, |tx| passkey_links::link(tx, &target, &pk)).expect("link");

    write(&db, |tx| passkey_links::unlink(tx, &target, &pk)).expect("unlink");
    assert_eq!(links(&db), 0);
    assert_eq!(
        query::read(&db, |q| passkeys::list_for_item(q, &source)
            .map_err(Into::into))
        .expect("list")
        .len(),
        1,
        "the passkey stays at its entry"
    );
    assert!(matches!(
        write(&db, |tx| passkey_links::unlink(tx, &target, &pk)),
        Err(HaexError::PasswordsNotFound)
    ));

    write(&db, |tx| passkey_links::link(tx, &target, &pk)).expect("link again");
    write(&db, |tx| passkeys::delete(tx, &pk)).expect("delete the passkey");
    assert_eq!(links(&db), 0, "the link goes with its passkey");
}

#[test]
fn deleting_the_source_for_good_removes_the_links_to_its_passkeys() {
    let (_dir, db) = open_test_vault();
    let source = item(&db, "Konto");
    let target = item(&db, "Kopie");
    linked(&db, &source, &target, "Y3JlZA==");
    write(&db, |tx| trash::purge_item(tx, &source)).expect("purge");
    assert_eq!(links(&db), 0);
    assert!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_deleted_rows \
             WHERE table_name = 'haex_passwords_passkey_links'"
        ) >= 1,
        "the removed link leaves its own marker"
    );
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_passkeys"),
        0
    );
}

#[test]
fn deleting_the_target_for_good_removes_its_links_and_keeps_the_passkey() {
    let (_dir, db) = open_test_vault();
    let source = item(&db, "Konto");
    let target = item(&db, "Kopie");
    linked(&db, &source, &target, "Y3JlZA==");
    write(&db, |tx| trash::purge_item(tx, &target)).expect("purge");
    assert_eq!(links(&db), 0);
    assert_eq!(
        query::read(&db, |q| passkeys::list_for_item(q, &source)
            .map_err(Into::into))
        .expect("list")
        .len(),
        1
    );
}

fn copy_options(passkeys_as_links: bool) -> CopyOptions {
    CopyOptions {
        title: CopyTitle::Suffix(" – Kopie".to_string()),
        history: false,
        username_as_reference: false,
        password_as_reference: false,
        passkeys_as_links: Some(passkeys_as_links),
    }
}

#[test]
fn a_copy_shows_the_passkeys_by_link_never_by_value() {
    let (_dir, db) = open_test_vault();
    let original = item(&db, "Mail");
    let other = item(&db, "Konto");
    let own = passkey(&db, Some(&original), "b3du");
    let foreign = linked(&db, &other, &original, "Zm9yZWlnbg==");
    let target = [Target {
        kind: TargetKind::Item,
        id: original.clone(),
    }];

    let plain = write(&db, |tx| copy(tx, &target, None, &copy_options(false))).expect("copy");
    assert_eq!(plain.passkey_links, 0);
    assert_eq!(links(&db), 1);

    let report = write(&db, |tx| copy(tx, &target, None, &copy_options(true))).expect("copy");
    assert_eq!(report.items_created, 1);
    assert_eq!(
        report.passkey_links, 2,
        "its own passkey and the linked one"
    );
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_passkeys"),
        2,
        "no key was copied"
    );
    let shown: Vec<String> = db
        .with_connection(|c| {
            let mut stmt = c.prepare(
                "SELECT passkey_id FROM haex_passwords_passkey_links \
                 WHERE item_id NOT IN (?1, ?2) ORDER BY rowid",
            )?;
            let rows = stmt.query_map(params![original, other], |r| r.get(0))?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
        .expect("links of the copy");
    assert_eq!(shown, [own, foreign]);
}
