//! The states of a chosen set (data-model.md): `ready`, `syncing`, `missing`, `damaged` and
//! `unknown`; in every state but `ready` the standard set answers.

// These tests read the vault with the CRDT write path where they insert rows, and `sqlite_master`
// style reads where the production code has no function.
#![allow(clippy::disallowed_methods)]

use std::io::{Cursor, Write};

use haex_crdt::Database;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use super::*;
use crate::files::icons::pack::pack_hash;
use crate::storage::preferences;

const ID: &str = "0b9a2c52-6a53-4f4e-9a1b-3c1a1d0a77aa";

fn open_vault(_name: &str) -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = crate::sync::test_support::open_vault(dir.path());
    (dir, db)
}

fn pack_bytes() -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    writer
        .start_file("icon-theme.json", options)
        .expect("start");
    writer
        .write_all(br#"{"file": "a", "iconDefinitions": {"a": {"iconPath": "a.svg"}}}"#)
        .expect("write");
    writer.start_file("a.svg", options).expect("start");
    writer
        .write_all(br#"<svg xmlns="http://www.w3.org/2000/svg"/>"#)
        .expect("write");
    writer.finish().expect("finish").into_inner()
}

fn insert(db: &Database, id: &str, sha: &str, pack: &[u8]) {
    db.write(|tx| {
        tx.execute(
            "INSERT INTO haex_file_icon_sets (id, name, image_count, byte_size, sha256, pack, created_at) \
             VALUES (?1, 'Mine', 1, ?2, ?3, ?4, 't')",
            &[&id, &(pack.len() as i64), &sha, &pack],
        )?;
        Ok(())
    })
    .expect("insert");
}

fn set_choice(db: &Database, value: &str) {
    db.write(|tx| {
        preferences::insert_or_update(tx, PrefScope::Vault, SELECTION_KEY, value)?;
        Ok(())
    })
    .expect("choose");
}

fn state_of(db: &Database) -> (IconSetRef, IconSetState) {
    let (set, in_use) = crate::storage::query::read(db, |q| chosen(q)).expect("chosen");
    (set, in_use.state)
}

#[test]
fn no_choice_is_the_standard_set() {
    let (_dir, db) = open_vault("icons-default");
    assert_eq!(
        state_of(&db),
        (super::super::DEFAULT_SET, IconSetState::Ready)
    );
}

#[test]
fn a_built_in_set_is_ready() {
    let (_dir, db) = open_vault("icons-builtin");
    set_choice(&db, "builtin:phosphor");
    let (set, state) = state_of(&db);
    assert_eq!(
        set,
        IconSetRef::Builtin {
            name: BuiltinSet::Phosphor
        }
    );
    assert_eq!(state, IconSetState::Ready);
}

#[test]
fn a_custom_set_with_a_good_pack_is_ready_and_answers_with_images() {
    let (_dir, db) = open_vault("icons-custom");
    let pack = pack_bytes();
    insert(&db, ID, &pack_hash(&pack), &pack);
    set_choice(&db, &format!("custom:{ID}"));
    let (_, in_use) = crate::storage::query::read(&db, |q| chosen(q)).expect("chosen");
    assert_eq!(in_use.state, IconSetState::Ready);
    assert!(matches!(in_use.source, Source::Pack(_)));
}

#[test]
fn a_chosen_set_whose_row_is_not_there_is_syncing_or_missing() {
    let (_dir, db) = open_vault("icons-gone");
    set_choice(&db, &format!("custom:{ID}"));
    assert_eq!(
        state_of(&db).1,
        IconSetState::Syncing,
        "no row and no marker: it has not arrived"
    );

    let pack = pack_bytes();
    insert(&db, ID, &pack_hash(&pack), &pack);
    db.write(|tx| {
        tx.execute("DELETE FROM haex_file_icon_sets WHERE id = ?1", &[&ID])?;
        Ok(())
    })
    .expect("delete");
    assert_eq!(
        state_of(&db).1,
        IconSetState::Missing,
        "the marker says it was deleted"
    );
}

#[test]
fn a_pack_with_a_wrong_hash_or_broken_bytes_is_damaged() {
    let (_dir, db) = open_vault("icons-damaged");
    let pack = pack_bytes();
    insert(&db, ID, &"0".repeat(64), &pack);
    set_choice(&db, &format!("custom:{ID}"));
    assert_eq!(
        state_of(&db).1,
        IconSetState::Damaged,
        "the hash does not match"
    );

    let other = "1b9a2c52-6a53-4f4e-9a1b-3c1a1d0a77aa";
    let junk = b"not a zip".to_vec();
    insert(&db, other, &pack_hash(&junk), &junk);
    set_choice(&db, &format!("custom:{other}"));
    assert_eq!(
        state_of(&db).1,
        IconSetState::Damaged,
        "the bytes are no pack"
    );
}

#[test]
fn a_built_in_name_of_a_newer_app_is_unknown_and_shows_the_standard_set() {
    let (_dir, db) = open_vault("icons-unknown");
    // The preference check refuses it, but another device of a newer app can have written it.
    db.write(|tx| {
        preferences::insert_or_update(tx, PrefScope::Vault, SELECTION_KEY, "builtin:future")?;
        Ok(())
    })
    .expect("a raw write");
    let (set, state) = state_of(&db);
    assert_eq!(set, super::super::DEFAULT_SET);
    assert_eq!(state, IconSetState::Unknown);
}

#[test]
fn in_every_state_but_ready_the_standard_set_answers() {
    let (_dir, db) = open_vault("icons-fallback");
    set_choice(&db, &format!("custom:{ID}"));
    let (_, in_use) = crate::storage::query::read(&db, |q| chosen(q)).expect("chosen");
    assert!(matches!(in_use.source, Source::Lucide));
}

#[test]
fn the_list_shows_the_custom_sets_after_the_built_in_ones() {
    let (_dir, db) = open_vault("icons-list");
    let pack = pack_bytes();
    insert(&db, ID, &pack_hash(&pack), &pack);
    let sets = crate::storage::query::read(&db, |q| super::super::commands::list(q)).expect("list");
    assert_eq!(sets.len(), 4);
    assert_eq!(sets[3].name, "Mine");
    assert!(sets[3].removable && sets[3].state == IconSetState::Ready);
}
