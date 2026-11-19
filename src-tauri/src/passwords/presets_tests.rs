//! Tests for the generator presets (spec 034, US3, FR-014): saving, exactly one default, the
//! newest default winning when two devices set one at once, deleting, and no seeded rows.

// The tests read raw columns that the CRDT write path does not expose.
#![allow(clippy::disallowed_methods, clippy::redundant_closure)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::model::PresetInput;
use super::presets;
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

fn list(db: &Database) -> Vec<super::model::Preset> {
    query::read(db, |q| presets::list(q).map_err(Into::into)).expect("list")
}

fn input(name: &str) -> PresetInput {
    PresetInput {
        name: name.to_string(),
        ..PresetInput::default()
    }
}

#[test]
fn nothing_is_seeded() {
    let (_dir, db) = open_test_vault();
    assert!(list(&db).is_empty());
}

#[test]
fn a_new_preset_has_the_defaults_of_the_table() {
    let (_dir, db) = open_test_vault();
    let id = write(&db, |tx| presets::save(tx, &input("Standard"))).expect("save");
    let presets = list(&db);
    assert_eq!(presets.len(), 1);
    let p = &presets[0];
    assert_eq!(p.id, id);
    assert_eq!(
        (p.length, p.uppercase, p.lowercase, p.numbers, p.symbols),
        (16, true, true, true, true)
    );
    assert_eq!(
        (
            p.exclude_chars.as_str(),
            p.use_pattern,
            p.pattern.as_str(),
            p.is_default
        ),
        ("", false, "", false)
    );
}

#[test]
fn a_name_is_required_and_the_length_is_in_range() {
    let (_dir, db) = open_test_vault();
    let blank = write(&db, |tx| presets::save(tx, &input("   ")));
    assert!(matches!(blank, Err(HaexError::InvalidInput { reason }) if reason == "name"));
    for length in [0, 257] {
        let result = write(&db, |tx| {
            presets::save(
                tx,
                &PresetInput {
                    length,
                    ..input("Bad")
                },
            )
        });
        assert!(
            matches!(result, Err(HaexError::InvalidInput { reason }) if reason == "length"),
            "{length}"
        );
    }
    assert!(list(&db).is_empty(), "nothing was written");
}

#[test]
fn save_updates_a_preset_by_its_id() {
    let (_dir, db) = open_test_vault();
    let id = write(&db, |tx| presets::save(tx, &input("Old"))).expect("save");
    write(&db, |tx| {
        presets::save(
            tx,
            &PresetInput {
                id: id.clone(),
                name: "Renamed".to_string(),
                length: 24,
                symbols: false,
                exclude_chars: "lO".to_string(),
                ..PresetInput::default()
            },
        )
    })
    .expect("update");
    let presets = list(&db);
    assert_eq!(presets.len(), 1);
    assert_eq!(presets[0].name, "Renamed");
    assert_eq!((presets[0].length, presets[0].symbols), (24, false));
    assert_eq!(presets[0].exclude_chars, "lO");
    let missing = write(&db, |tx| {
        presets::save(
            tx,
            &PresetInput {
                id: "nope".to_string(),
                ..input("X")
            },
        )
    });
    assert!(matches!(missing, Err(HaexError::PasswordsNotFound)));
}

#[test]
fn making_a_preset_the_default_clears_the_others_in_the_same_write() {
    let (_dir, db) = open_test_vault();
    let first = write(&db, |tx| {
        presets::save(
            tx,
            &PresetInput {
                is_default: true,
                ..input("First")
            },
        )
    })
    .expect("first");
    let second = write(&db, |tx| {
        presets::save(
            tx,
            &PresetInput {
                is_default: true,
                ..input("Second")
            },
        )
    })
    .expect("second");
    let presets = list(&db);
    let default: Vec<&str> = presets
        .iter()
        .filter(|p| p.is_default)
        .map(|p| p.id.as_str())
        .collect();
    assert_eq!(default, [second.as_str()]);
    let raw_defaults: i64 = db
        .with_connection(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM haex_passwords_generator_presets WHERE is_default = 1",
                [],
                |r| r.get(0),
            )?)
        })
        .expect("count");
    assert_eq!(
        raw_defaults, 1,
        "the flag is cleared in the table, not only hidden"
    );
    assert!(presets.iter().any(|p| p.id == first && !p.is_default));
}

#[test]
fn two_defaults_from_concurrent_writes_read_as_the_newest() {
    let (_dir, db) = open_test_vault();
    let older = write(&db, |tx| presets::save(tx, &input("Older"))).expect("older");
    let newer = write(&db, |tx| presets::save(tx, &input("Newer"))).expect("newer");
    // Two devices each set their own default: both rows are flagged.
    write(&db, |tx| {
        tx.execute(
            "UPDATE haex_passwords_generator_presets SET is_default = 1, updated_at = ?1 WHERE id = ?2",
            params!["2026-10-01T10:00:00.000Z", older],
        )?;
        tx.execute(
            "UPDATE haex_passwords_generator_presets SET is_default = 1, updated_at = ?1 WHERE id = ?2",
            params!["2026-10-01T11:00:00.000Z", newer],
        )?;
        Ok(())
    })
    .expect("simulate sync");
    let presets = list(&db);
    let default: Vec<&str> = presets
        .iter()
        .filter(|p| p.is_default)
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(default, ["Newer"]);
}

#[test]
fn delete_removes_only_that_preset() {
    let (_dir, db) = open_test_vault();
    let keep = write(&db, |tx| presets::save(tx, &input("Keep"))).expect("keep");
    let drop = write(&db, |tx| presets::save(tx, &input("Drop"))).expect("drop");
    write(&db, |tx| presets::delete(tx, &drop)).expect("delete");
    let presets = list(&db);
    assert_eq!(presets.len(), 1);
    assert_eq!(presets[0].id, keep);
    let missing = write(&db, |tx| presets::delete(tx, &drop));
    assert!(matches!(missing, Err(HaexError::PasswordsNotFound)));
}
