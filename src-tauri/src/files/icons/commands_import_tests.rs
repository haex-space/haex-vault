//! The two steps of the import and the removal (spec 053 US3, T042 and T043): what is written, in
//! how many rows, and what is refused. The database part runs on a vault of the test; the source is
//! the fixture folder.

// These tests read `haex_file_icon_sets` and `haex_deleted_rows` directly, which the CRDT write
// path does not expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::Database;

use super::*;
use crate::files::icons::import::{prepare, ImportError, Prepared, MAX_SETS};
use crate::files::icons::import_support::{fixture, png, zip_of};
use crate::files::icons::sources::{Archive, Folder};
use crate::files::icons::SELECTION_KEY;
use crate::storage::preferences::{self, PrefScope};
use crate::storage::query::{read, Query};

fn open_vault() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = crate::sync::test_support::open_vault(dir.path());
    (dir, db)
}

fn prepared(name: &str) -> Prepared {
    prepare(&Folder::open(&fixture(name)).expect("folder"), name).expect("prepared")
}

fn rows(db: &Database) -> i64 {
    read(db, |q| store::count(q).map(|n| n as i64)).expect("count")
}

fn write(
    db: &Database,
    set: &Prepared,
    id: &str,
    replace: Option<&str>,
) -> std::result::Result<(), ImportError> {
    db.write(|tx| write_set(tx, set, id, "2026-10-15T00:00:00Z", replace))
        .expect("a transaction")
}

fn choice(db: &Database) -> Option<String> {
    read(db, |q| store::selection_value(q)).expect("choice")
}

fn set_choice(db: &Database, value: &str) {
    db.write(|tx| {
        preferences::insert_or_update(tx, PrefScope::Vault, SELECTION_KEY, value)?;
        Ok(())
    })
    .expect("choice");
}

const A: &str = "0b9a2c52-6a53-4f4e-9a1b-3c1a1d0a77aa";
const B: &str = "5d2f6c4e-1f3a-4a45-8d3e-6f1d2b9c8e10";

#[test]
fn the_preview_writes_nothing_and_the_import_writes_exactly_one_row() {
    let (_dir, db) = open_vault();
    let set = prepared("theme-basic");
    assert_eq!(rows(&db), 0, "nothing before");
    // The preview needs no database write: it reads the name clash only.
    let clash = read(&db, |q| store::id_by_name(q, &set.report.name)).expect("read");
    assert_eq!(clash, None);
    assert_eq!(rows(&db), 0, "the preview wrote nothing");
    write(&db, &set, A, None).expect("written");
    assert_eq!(rows(&db), 1);
    let row = read(&db, |q| {
        q.query_row(
            "SELECT name, image_count, byte_size, sha256, created_at FROM haex_file_icon_sets WHERE id = ?1",
            &[&A],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
    })
    .expect("row")
    .expect("a row");
    assert_eq!(
        row,
        (
            "theme-basic".to_owned(),
            4,
            set.pack.len() as i64,
            set.sha256.clone(),
            "2026-10-15T00:00:00Z".to_owned()
        )
    );
}

#[test]
fn the_same_name_is_found_without_regard_to_case() {
    let (_dir, db) = open_vault();
    write(&db, &prepared("theme-basic"), A, None).expect("written");
    let found = read(&db, |q| store::id_by_name(q, "THEME-basic")).expect("read");
    assert_eq!(found.as_deref(), Some(A));
    assert_eq!(
        read(&db, |q| store::id_by_name(q, "other")).expect("read"),
        None
    );
}

#[test]
fn at_most_ten_sets_the_eleventh_is_refused() {
    let (_dir, db) = open_vault();
    let set = prepared("theme-basic");
    for index in 0..MAX_SETS {
        write(
            &db,
            &set,
            &format!("00000000-0000-4000-8000-{index:012}"),
            None,
        )
        .expect("written");
    }
    assert_eq!(rows(&db), 10);
    assert_eq!(write(&db, &set, A, None), Err(ImportError::TooManySets));
    assert_eq!(rows(&db), 10, "a refusal writes nothing");
    // Replacing a set does not make the eleventh.
    let first = "00000000-0000-4000-8000-000000000000";
    write(&db, &set, A, Some(first)).expect("a replacement");
    assert_eq!(rows(&db), 10);
}

#[test]
fn replacing_writes_the_new_row_deletes_the_old_one_and_moves_the_choice() {
    let (_dir, db) = open_vault();
    write(&db, &prepared("theme-basic"), A, None).expect("written");
    set_choice(&db, &format!("custom:{A}"));
    write(&db, &prepared("outside-ref"), B, Some(A)).expect("replaced");
    assert_eq!(rows(&db), 1);
    assert!(read(&db, |q| store::exists(q, B)).expect("read"));
    assert!(!read(&db, |q| store::exists(q, A)).expect("read"));
    assert_eq!(choice(&db).as_deref(), Some(format!("custom:{B}").as_str()));
    let marker = read(&db, |q| {
        q.query_row(
            "SELECT COUNT(*) FROM haex_deleted_rows WHERE table_name = 'haex_file_icon_sets'",
            &[],
            |row| row.get::<_, i64>(0),
        )
    })
    .expect("read");
    assert_eq!(marker, Some(1), "the delete reaches the other devices");
    assert_eq!(
        write(
            &db,
            &prepared("theme-basic"),
            A,
            Some("00000000-0000-4000-8000-0000000000ff")
        ),
        Err(ImportError::NoSuchSet)
    );
}

#[test]
fn removing_the_chosen_set_falls_back_to_the_default() {
    let (_dir, db) = open_vault();
    write(&db, &prepared("theme-basic"), A, None).expect("written");
    write(&db, &prepared("theme-basic"), B, None).expect("written");
    set_choice(&db, &format!("custom:{A}"));
    db.write(|tx| remove_set(tx, B)).expect("removed");
    assert_eq!(
        choice(&db).as_deref(),
        Some(format!("custom:{A}").as_str()),
        "removing another set leaves the choice"
    );
    db.write(|tx| remove_set(tx, A)).expect("removed");
    assert_eq!(rows(&db), 0);
    assert_eq!(
        choice(&db).as_deref(),
        Some(DEFAULT_SET.selection_value().as_str())
    );
}

#[test]
fn a_folder_where_the_device_cannot_read_one_is_invalid_input() {
    // `open` is the one place that asks; the check itself is the capability table of the platform.
    let table = crate::platform::capabilities();
    if !table.folder_pick {
        let error: HolziError = ImportError::FolderUnavailable.into();
        assert!(matches!(error, HolziError::InvalidInput { .. }));
    }
    assert!(ImportError::FolderUnavailable
        .to_string()
        .contains("archive"));
}

#[test]
fn the_second_read_decides_when_the_source_changed_in_between() {
    let first = Archive::open(zip_of(&[
        (
            "icon-theme.json",
            br#"{"iconDefinitions":{"a":{"iconPath":"a.png"}},"file":"a"}"#,
        ),
        ("a.png", &png(16, 16, 1)),
    ]))
    .expect("archive");
    let report = prepare(&first, "x").expect("preview").report;
    assert_eq!(report.image_count, 1);
    // The file is replaced by one with a broken image before the import reads it again.
    let second = Archive::open(zip_of(&[
        (
            "icon-theme.json",
            br#"{"iconDefinitions":{"a":{"iconPath":"a.png"}},"file":"a"}"#,
        ),
        ("a.png", b"\x89PNG\r\n\x1a\nbroken"),
    ]))
    .expect("archive");
    assert_eq!(
        prepare(&second, "x").err(),
        Some(ImportError::NoUsableImage)
    );
}

#[test]
fn the_answer_names_the_source_by_no_path() {
    let report = prepared("theme-basic").report;
    let json = serde_json::to_string(&report).expect("json");
    assert!(!json.contains("fixtures"), "{json}");
    assert!(!json.contains(env!("CARGO_MANIFEST_DIR")), "{json}");
}
