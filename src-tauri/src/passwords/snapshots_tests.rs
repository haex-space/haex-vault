//! Tests for the history of an entry (spec 034, US4, FR-017, FR-018, research R5): a state with
//! the new values after every change, none when nothing changed, the names of what changed, reading
//! states from haex-vault 2.x (Legacy), and restoring a state without losing any.

// The tests read raw columns that the CRDT write path does not expose.
#![allow(clippy::disallowed_methods, clippy::redundant_closure)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::items;
use super::model::{HistorySecret, ItemInput, ItemPatch, KeyValueInput, KeyValuePatch, Patch};
use super::snapshots::{self, SnapshotData};
use super::test_support::{open_test_vault, MARKER};
use crate::error::HolziError;
use crate::storage::query;

fn write<R>(
    db: &Database,
    f: impl FnOnce(&mut haex_crdt::CrdtTransaction<'_>) -> crate::error::Result<R>,
) -> crate::error::Result<R> {
    db.write(|tx| f(tx).map_err(haex_crdt::Error::from))
        .map_err(HolziError::from)
}

fn read<R>(
    db: &Database,
    f: impl FnOnce(&mut query::Reader<'_, '_>) -> crate::error::Result<R>,
) -> crate::error::Result<R> {
    query::read(db, |q| f(q).map_err(haex_crdt::Error::from)).map_err(HolziError::from)
}

fn create(db: &Database) -> String {
    write(db, |tx| {
        items::create_item(
            tx,
            &ItemInput {
                title: Some("Mail".to_string()),
                username: Some("alice".to_string()),
                password: Some(format!("{MARKER}-one")),
                note: Some("first note".to_string()),
                otp_secret: Some("JBSWY3DPEHPK3PXP".to_string()),
                tags: vec!["work".to_string()],
                key_values: vec![KeyValueInput {
                    key: "PIN".to_string(),
                    value: Some(format!("{MARKER}-pin")),
                }],
                ..ItemInput::default()
            },
            None,
        )
    })
    .expect("create")
}

fn token(db: &Database, id: &str) -> String {
    read(db, |q| items::get_item(q, id))
        .expect("get")
        .expect("exists")
        .header
        .updated_at
        .expect("token")
}

fn update(db: &Database, id: &str, patch: ItemPatch) -> String {
    let t = token(db, id);
    write(db, |tx| items::update_item(tx, id, &t, &patch)).expect("update")
}

fn count(db: &Database, sql: &str) -> i64 {
    db.with_connection(|c| Ok(c.query_row(sql, [], |r| r.get(0))?))
        .expect("count")
}

fn list(db: &Database, id: &str) -> Vec<super::model::SnapshotHeader> {
    read(db, |q| snapshots::list(q, id)).expect("list")
}

fn raw_data(db: &Database, snapshot_id: &str) -> SnapshotData {
    let text: String = db
        .with_connection(|c| {
            Ok(c.query_row(
                "SELECT snapshot_data FROM haex_passwords_item_snapshots WHERE id = ?1",
                params![snapshot_id],
                |r| r.get(0),
            )?)
        })
        .expect("snapshot data");
    serde_json::from_str(&text).expect("parse")
}

#[test]
fn create_writes_the_first_state_with_the_new_values() {
    let (_dir, db) = open_test_vault();
    let id = create(&db);
    let states = list(&db, &id);
    assert_eq!(states.len(), 1);
    let data = raw_data(&db, &states[0].id);
    assert_eq!(data.version, 1);
    assert_eq!(data.title.as_deref(), Some("Mail"));
    assert_eq!(data.password.as_deref(), Some(&*format!("{MARKER}-one")));
    assert_eq!(data.tag_names, ["work"]);
    assert_eq!(data.key_values.len(), 1);
    assert_eq!(data.otp_digits, Some(6));
    let modified = states[0].modified_at.as_deref().expect("time");
    assert_eq!(modified.len(), 24, "{modified}");
}

#[test]
fn an_update_that_changes_something_adds_a_state_and_one_that_changes_nothing_does_not() {
    let (_dir, db) = open_test_vault();
    let id = create(&db);
    update(
        &db,
        &id,
        ItemPatch {
            password: Patch::Set(format!("{MARKER}-two")),
            ..ItemPatch::default()
        },
    );
    assert_eq!(list(&db, &id).len(), 2);
    update(&db, &id, ItemPatch::default());
    assert_eq!(list(&db, &id).len(), 2, "nothing changed, no new state");
    // Setting a field to the value it has is no change either.
    update(
        &db,
        &id,
        ItemPatch {
            title: Patch::Set("Mail".to_string()),
            ..ItemPatch::default()
        },
    );
    assert_eq!(list(&db, &id).len(), 2);
    // The newest state holds the new password, the older one the old password.
    let states = list(&db, &id);
    assert_eq!(
        raw_data(&db, &states[0].id).password.as_deref(),
        Some(&*format!("{MARKER}-two"))
    );
    assert_eq!(
        raw_data(&db, &states[1].id).password.as_deref(),
        Some(&*format!("{MARKER}-one"))
    );
}

#[test]
fn the_list_names_what_changed_against_the_state_before() {
    let (_dir, db) = open_test_vault();
    let id = create(&db);
    update(
        &db,
        &id,
        ItemPatch {
            title: Patch::Set("Work mail".to_string()),
            password: Patch::Set("changed".to_string()),
            tags: Some(vec!["work".to_string(), "home".to_string()]),
            ..ItemPatch::default()
        },
    );
    let states = list(&db, &id);
    let mut changed = states[0].changed_fields.clone();
    changed.sort();
    assert_eq!(changed, ["password", "tags", "title"]);
    // The first state lists what the entry started with.
    assert!(states[1].changed_fields.contains(&"title".to_string()));
    assert!(states[1].changed_fields.contains(&"password".to_string()));
    assert!(
        !states[1].changed_fields.contains(&"icon".to_string()),
        "empty fields are not listed"
    );
}

#[test]
fn changed_fields_lists_exactly_the_differences() {
    let base = SnapshotData {
        title: Some("a".to_string()),
        ..SnapshotData::default()
    };
    let mut next = base.clone();
    assert!(snapshots::changed_fields(Some(&base), &next).is_empty());
    next.username = Some("u".to_string());
    next.otp_period = Some(60);
    next.attachments = vec![super::snapshots::SnapshotAttachment {
        file_name: "f".to_string(),
        binary_hash: "h".to_string(),
    }];
    let mut names = snapshots::changed_fields(Some(&base), &next);
    names.sort();
    assert_eq!(names, ["attachments", "otpPeriod", "username"]);
}

#[test]
fn the_masked_view_has_flags_and_the_values_come_only_through_reveal() {
    let (_dir, db) = open_test_vault();
    let id = create(&db);
    let state = list(&db, &id).remove(0).id;
    let view = read(&db, |q| snapshots::get(q, &state)).expect("view");
    let json = serde_json::to_string(&view).expect("json");
    for secret in ["-one", "-pin", "JBSWY3DPEHPK3PXP"] {
        assert!(!json.contains(secret), "{secret} in {json}");
    }
    assert!(view.has_password && view.has_otp_secret);
    assert_eq!(view.key_values[0].key.as_deref(), Some("PIN"));
    assert!(view.key_values[0].has_value);
    let password = read(&db, |q| {
        snapshots::reveal(q, &state, &HistorySecret::Password)
    })
    .expect("reveal");
    assert_eq!(password.value.as_str(), format!("{MARKER}-one"));
    let pin = read(&db, |q| {
        snapshots::reveal(
            q,
            &state,
            &HistorySecret::KeyValue {
                key: "PIN".to_string(),
            },
        )
    })
    .expect("pin");
    assert_eq!(pin.value.as_str(), format!("{MARKER}-pin"));
    assert!(matches!(
        read(&db, |q| snapshots::reveal(
            q,
            &state,
            &HistorySecret::KeyValue {
                key: "nope".to_string()
            }
        )
        .map(|_| ())),
        Err(HolziError::PasswordsNotFound)
    ));
    assert!(matches!(
        read(&db, |q| snapshots::get(q, "nope").map(|_| ())),
        Err(HolziError::PasswordsNotFound)
    ));
}

#[test]
fn attachments_of_the_entry_are_mirrored_into_the_state() {
    let (_dir, db) = open_test_vault();
    let id = create(&db);
    write(&db, |tx| {
        tx.execute(
            "INSERT INTO haex_passwords_binaries (hash, data, size) VALUES ('h1', x'0102', 2)",
            &[],
        )?;
        tx.execute(
            "INSERT INTO haex_passwords_item_binaries (id, item_id, binary_hash, file_name) \
             VALUES ('ib1', ?1, 'h1', 'a.txt')",
            params![id],
        )?;
        snapshots::take_snapshot(tx, &id).map(|_| ())
    })
    .expect("attach");
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_snapshot_binaries"),
        1
    );
    let newest = list(&db, &id).remove(0);
    assert_eq!(newest.attachment_count, 1);
    assert_eq!(newest.changed_fields, ["attachments"]);
    // A state without a change is not written, so the mirror does not double.
    write(&db, |tx| snapshots::take_snapshot(tx, &id).map(|_| ())).expect("again");
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_snapshot_binaries"),
        1
    );
}

#[test]
fn a_state_from_legacy_haex_vault_without_version_or_otp_parameters_reads_fine() {
    let (_dir, db) = open_test_vault();
    let id = create(&db);
    write(&db, |tx| {
        tx.execute(
            "INSERT INTO haex_passwords_item_snapshots (id, item_id, snapshot_data, modified_at) \
             VALUES ('legacy', ?1, ?2, '2020-01-01T00:00:00.000Z')",
            params![
                id,
                r#"{"title":"Old","username":"u","password":"p","url":null,"note":null,"icon":null,"color":null,"expiresAt":null,"otpSecret":"JBSWY3DPEHPK3PXP","tagNames":["x"],"keyValues":[{"key":"K","value":"v"}],"attachments":[]}"#
            ],
        )?;
        Ok(())
    })
    .expect("legacy state");
    let view = read(&db, |q| snapshots::get(q, "legacy")).expect("view");
    assert_eq!(view.title.as_deref(), Some("Old"));
    assert_eq!(view.tags, ["x"]);
    assert_eq!(view.otp_digits, None);
    // Listed in time order: the legacy state is the oldest.
    let states = list(&db, &id);
    assert_eq!(states.last().expect("oldest").id, "legacy");
    // And it can be restored.
    let t = token(&db, &id);
    write(&db, |tx| snapshots::restore(tx, &id, "legacy", &t)).expect("restore");
    let detail = read(&db, |q| items::get_item(q, &id))
        .expect("get")
        .expect("exists");
    assert_eq!(detail.header.title.as_deref(), Some("Old"));
    assert_eq!(detail.header.tags[0].name, "x");
}

#[test]
fn restoring_makes_the_old_state_current_and_keeps_every_state() {
    let (_dir, db) = open_test_vault();
    let id = create(&db);
    let first = list(&db, &id).remove(0).id;
    // Change a lot: password, tags, custom fields.
    update(
        &db,
        &id,
        ItemPatch {
            password: Patch::Set(format!("{MARKER}-two")),
            title: Patch::Set("Renamed".to_string()),
            tags: Some(vec!["other".to_string()]),
            key_values: Some(vec![KeyValuePatch {
                id: None,
                key: "Other".to_string(),
                value: Some("x".to_string()),
            }]),
            ..ItemPatch::default()
        },
    );
    assert_eq!(list(&db, &id).len(), 2);
    let t = token(&db, &id);
    let outcome = write(&db, |tx| snapshots::restore(tx, &id, &first, &t)).expect("restore");
    assert!(outcome.skipped_attachments.is_empty());
    let detail = read(&db, |q| items::get_item(q, &id))
        .expect("get")
        .expect("exists");
    assert_eq!(detail.header.title.as_deref(), Some("Mail"));
    assert_eq!(detail.header.tags.len(), 1);
    assert_eq!(detail.header.tags[0].name, "work");
    assert_eq!(detail.key_values.len(), 1);
    assert_eq!(detail.key_values[0].key.as_deref(), Some("PIN"));
    assert_eq!(
        detail.header.updated_at.as_deref(),
        Some(outcome.updated_at.as_str())
    );
    let password = read(&db, |q| {
        super::reveal::reveal(q, &id, &super::model::SecretField::Password)
    })
    .expect("password");
    assert_eq!(password.value.as_str(), format!("{MARKER}-one"));
    // The state before the restore stays as a state of its own: three in all.
    let states = list(&db, &id);
    assert_eq!(states.len(), 3);
    assert_eq!(
        raw_data(&db, &states[1].id).password.as_deref(),
        Some(&*format!("{MARKER}-two")),
        "the bridge state is kept"
    );
    assert_eq!(
        raw_data(&db, &states[0].id).password.as_deref(),
        Some(&*format!("{MARKER}-one")),
        "the restored state is the newest"
    );
}

#[test]
fn restore_re_links_attachments_by_hash_and_name_and_skips_missing_ones() {
    let (_dir, db) = open_test_vault();
    let id = create(&db);
    write(&db, |tx| {
        tx.execute(
            "INSERT INTO haex_passwords_binaries (hash, data, size) VALUES ('keep', x'01', 1)",
            &[],
        )?;
        tx.execute(
            "INSERT INTO haex_passwords_binaries (hash, data, size) VALUES ('gone', x'02', 1)",
            &[],
        )?;
        tx.execute(
            "INSERT INTO haex_passwords_item_binaries (id, item_id, binary_hash, file_name) \
             VALUES ('l1', ?1, 'keep', 'keep.txt')",
            params![id],
        )?;
        tx.execute(
            "INSERT INTO haex_passwords_item_binaries (id, item_id, binary_hash, file_name) \
             VALUES ('l2', ?1, 'gone', 'gone.txt')",
            params![id],
        )?;
        snapshots::take_snapshot(tx, &id).map(|_| ())
    })
    .expect("attach");
    let with_both = list(&db, &id).remove(0).id;
    // Both attachments are removed from the entry; the second binary disappears altogether.
    write(&db, |tx| {
        tx.execute(
            "DELETE FROM haex_passwords_item_binaries WHERE item_id = ?1",
            params![id],
        )?;
        snapshots::take_snapshot(tx, &id).map(|_| ())
    })
    .expect("detach");
    db.with_connection(|c| {
        c.execute_batch("PRAGMA foreign_keys = OFF")?;
        c.execute(
            "DELETE FROM haex_passwords_binaries WHERE hash = 'gone'",
            [],
        )?;
        c.execute_batch("PRAGMA foreign_keys = ON")?;
        Ok(())
    })
    .expect("binary pruned elsewhere");
    let t = token(&db, &id);
    let outcome = write(&db, |tx| snapshots::restore(tx, &id, &with_both, &t)).expect("restore");
    assert_eq!(outcome.skipped_attachments, ["gone.txt"]);
    let detail = read(&db, |q| items::get_item(q, &id))
        .expect("get")
        .expect("exists");
    assert_eq!(detail.attachments.len(), 1);
    assert_eq!(detail.attachments[0].file_name, "keep.txt");
}

#[test]
fn restore_keeps_the_folder_and_checks_the_token() {
    let (_dir, db) = open_test_vault();
    let id = create(&db);
    let first = list(&db, &id).remove(0).id;
    write(&db, |tx| {
        tx.execute(
            "INSERT INTO haex_passwords_groups (id, name) VALUES ('g1', 'Folder')",
            &[],
        )?;
        tx.execute(
            "INSERT INTO haex_passwords_group_items (item_id, group_id) VALUES (?1, 'g1')",
            params![id],
        )?;
        Ok(())
    })
    .expect("move");
    update(
        &db,
        &id,
        ItemPatch {
            title: Patch::Set("Changed".to_string()),
            ..ItemPatch::default()
        },
    );
    let stale = write(&db, |tx| {
        snapshots::restore(tx, &id, &first, "2000-01-01T00:00:00.000Z")
    });
    assert!(matches!(stale, Err(HolziError::PasswordsConflict { reason }) if reason == "changed"));
    let t = token(&db, &id);
    write(&db, |tx| snapshots::restore(tx, &id, &first, &t)).expect("restore");
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_group_items WHERE group_id = 'g1'"
        ),
        1,
        "the folder is not part of a state"
    );
    let missing_entry = write(&db, |tx| snapshots::restore(tx, "nope", &first, &t));
    assert!(
        matches!(missing_entry, Err(HolziError::PasswordsConflict { reason }) if reason == "deleted")
    );
    let t = token(&db, &id);
    let missing_state = write(&db, |tx| snapshots::restore(tx, &id, "nope", &t));
    assert!(matches!(missing_state, Err(HolziError::PasswordsNotFound)));
    // A state of another entry is not found for this one.
    let other = create(&db);
    let foreign = list(&db, &other).remove(0).id;
    let wrong = write(&db, |tx| snapshots::restore(tx, &id, &foreign, &t));
    assert!(matches!(wrong, Err(HolziError::PasswordsNotFound)));
}

#[test]
fn the_debug_print_of_a_state_holds_no_value() {
    let (_dir, db) = open_test_vault();
    let id = create(&db);
    let state = list(&db, &id).remove(0).id;
    let data = raw_data(&db, &state);
    assert!(!format!("{data:?}").contains(MARKER));
}
