//! Tests for the history of a pass (spec 092, FR-032, research R2): a state without `version` reads
//! as 1, a state of version 1 is a login, a state of version 2 with a pass block round-trips, an
//! unchanged pass writes no state, the window sees code formats and no value, `reveal` hands out one
//! on request, and a restore brings back the typed fields and the codes but never moves or re-links.

// The tests read and write raw columns that the CRDT write path does not expose.
#![allow(clippy::disallowed_methods, clippy::redundant_closure)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::items;
use super::model::{HistorySecret, ItemInput};
use super::snapshots::{self, SnapshotData};
use super::test_support::open_test_vault;
use crate::error::HolziError;
use crate::storage::query;

const CODE: &str = "SECRET-MARKER-history-code";

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

fn exec(db: &Database, sql: &str, args: &[&dyn haex_crdt::rusqlite::ToSql]) {
    db.with_connection(|conn| {
        conn.execute(sql, args)?;
        Ok(())
    })
    .expect("execute");
}

/// A pass in a folder with a parent, one code and a start; returns (id, folder id).
fn pass(db: &Database) -> (String, String) {
    exec(
        db,
        "INSERT INTO haex_passwords_groups (id, name) VALUES ('folder', 'Tickets')",
        &[],
    );
    let id = write(db, |tx| {
        items::create_item(
            tx,
            &ItemInput {
                title: Some("Concert".to_string()),
                ..ItemInput::default()
            },
            Some("folder"),
        )
    })
    .expect("create");
    exec(
        db,
        "UPDATE haex_passwords_item_details SET kind = 'pass' WHERE id = ?1",
        &[&id],
    );
    exec(
        db,
        "INSERT INTO haex_passwords_pass_details (item_id, pass_kind, start_at, start_zone, parent_id) \
         VALUES (?1, 'ticket', '2030-05-01T19:00:00.000Z', 'Europe/London', 'the-parent')",
        &[&id],
    );
    exec(
        db,
        "INSERT INTO haex_passwords_pass_barcodes (id, item_id, format, value, label, position) \
         VALUES ('c1', ?1, 'qr', ?2, 'Entry', 0)",
        &[&id, &CODE],
    );
    (id, "folder".to_owned())
}

fn token(db: &Database, id: &str) -> String {
    read(db, |q| items::get_item(q, id))
        .expect("get")
        .expect("exists")
        .header
        .updated_at
        .expect("token")
}

fn states(db: &Database, id: &str) -> usize {
    read(db, |q| snapshots::list(q, id)).expect("list").len()
}

fn newest_text(db: &Database, id: &str) -> String {
    db.with_connection(|conn| {
        Ok(conn.query_row(
            "SELECT snapshot_data FROM haex_passwords_item_snapshots WHERE item_id = ?1 \
             ORDER BY modified_at DESC, created_at DESC, rowid DESC LIMIT 1",
            [id],
            |r| r.get::<_, String>(0),
        )?)
    })
    .expect("newest state")
}

#[test]
fn a_stored_state_without_a_version_reads_as_version_1_and_a_new_one_is_version_2() {
    let legacy: SnapshotData = serde_json::from_str(r#"{"title":"Old"}"#).expect("parse");
    assert_eq!(
        legacy.version, 1,
        "no version means an older build wrote it"
    );
    assert!(legacy.pass.is_none(), "a state of version 1 is a login");
    assert_eq!(
        SnapshotData::default().version,
        2,
        "a new state is version 2"
    );
    let (_dir, db) = open_test_vault();
    let (id, _) = pass(&db);
    write(&db, |tx| snapshots::take_snapshot(tx, &id)).expect("snapshot");
    assert!(newest_text(&db, &id).contains("\"version\":2"));
}

#[test]
fn a_version_2_state_of_a_pass_round_trips_and_an_equal_pass_writes_no_state() {
    let (_dir, db) = open_test_vault();
    let (id, _) = pass(&db);
    assert!(write(&db, |tx| snapshots::take_snapshot(tx, &id)).expect("first"));
    let data: SnapshotData = serde_json::from_str(&newest_text(&db, &id)).expect("parse");
    let block = data.pass.as_ref().expect("a pass block");
    assert_eq!(block.pass_kind.as_deref(), Some("ticket"));
    assert_eq!(block.start_at.as_deref(), Some("2030-05-01T19:00:00.000Z"));
    assert_eq!(block.start_zone.as_deref(), Some("Europe/London"));
    assert_eq!(block.codes.len(), 1);
    assert_eq!(block.codes[0].value, CODE);
    assert!(!format!("{data:?}").contains(CODE), "Debug prints no code");
    assert!(
        !write(&db, |tx| snapshots::take_snapshot(tx, &id)).expect("second"),
        "an unchanged pass writes no state"
    );
    assert_eq!(states(&db, &id), 2, "the creation state and the pass state");
}

#[test]
fn a_version_1_state_equals_the_same_login_read_now_and_writes_no_state() {
    let (_dir, db) = open_test_vault();
    let id = write(&db, |tx| {
        items::create_item(
            tx,
            &ItemInput {
                title: Some("Mail".to_string()),
                ..ItemInput::default()
            },
            None,
        )
    })
    .expect("create");
    write(&db, |tx| snapshots::take_snapshot(tx, &id)).expect("state");
    // The same document as an older build wrote it: version 1.
    let text = newest_text(&db, &id).replace("\"version\":2", "\"version\":1");
    exec(
        &db,
        "UPDATE haex_passwords_item_snapshots SET snapshot_data = ?2 WHERE item_id = ?1",
        &[&id, &text],
    );
    assert!(
        !write(&db, |tx| snapshots::take_snapshot(tx, &id)).expect("again"),
        "the version alone is no change"
    );
    assert_eq!(states(&db, &id), 1);
}

#[test]
fn the_window_sees_code_formats_and_labels_and_reveal_hands_out_one_value() {
    let (_dir, db) = open_test_vault();
    let (id, _) = pass(&db);
    write(&db, |tx| snapshots::take_snapshot(tx, &id)).expect("state");
    let header = read(&db, |q| snapshots::list(q, &id))
        .expect("list")
        .remove(0);
    // Against the state of creation, which was a login: the typed fields are new.
    assert_eq!(header.changed_fields, ["kindOfPass", "start", "codes"]);
    let view = read(&db, |q| snapshots::get(q, &header.id)).expect("view");
    let block = view.pass.as_ref().expect("pass view");
    assert_eq!(block.codes[0].format, "qr");
    assert_eq!(block.codes[0].label.as_deref(), Some("Entry"));
    assert!(!format!("{view:?}").contains(CODE), "no value in the view");
    let value = read(&db, |q| {
        snapshots::reveal(q, &header.id, &HistorySecret::Code { index: 0 })
    })
    .expect("reveal");
    assert_eq!(value.value.as_str(), CODE);
    assert!(matches!(
        read(&db, |q| snapshots::reveal(
            q,
            &header.id,
            &HistorySecret::Code { index: 5 }
        )),
        Err(HolziError::PasswordsNotFound)
    ));
}

#[test]
fn restoring_a_state_brings_back_start_and_codes_and_never_moves_or_re_links() {
    let (_dir, db) = open_test_vault();
    let (id, folder) = pass(&db);
    write(&db, |tx| snapshots::take_snapshot(tx, &id)).expect("state of before");
    let before = read(&db, |q| snapshots::list(q, &id))
        .expect("list")
        .remove(0)
        .id;

    // The organiser moves the event, the code changes, and the pass is linked to another entry.
    exec(
        &db,
        "UPDATE haex_passwords_pass_details SET start_at = '2030-06-02T18:00:00.000Z', \
         parent_id = 'another-parent' WHERE item_id = ?1",
        &[&id],
    );
    exec(
        &db,
        "UPDATE haex_passwords_pass_barcodes SET value = 'moved-code' WHERE id = 'c1'",
        &[],
    );
    exec(
        &db,
        "INSERT INTO haex_passwords_pass_barcodes (id, item_id, format, value, position) \
         VALUES ('c2', ?1, 'code128', 'extra', 1)",
        &[&id],
    );
    write(&db, |tx| snapshots::take_snapshot(tx, &id)).expect("state of after");
    assert_eq!(states(&db, &id), 3, "creation, before, after");

    let t = token(&db, &id);
    write(&db, |tx| snapshots::restore(tx, &id, &before, &t)).expect("restore");

    let (start, parent): (String, String) = db
        .with_connection(|conn| {
            Ok(conn.query_row(
                "SELECT start_at, parent_id FROM haex_passwords_pass_details WHERE item_id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?)
        })
        .expect("detail");
    assert_eq!(start, "2030-05-01T19:00:00.000Z", "the start is back");
    assert_eq!(parent, "another-parent", "a restore never re-links");
    let codes: Vec<(String, String)> = db
        .with_connection(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, value FROM haex_passwords_pass_barcodes WHERE item_id = ?1 \
                 ORDER BY position",
            )?;
            let rows = stmt.query_map([&id], |r| Ok((r.get(0)?, r.get(1)?)))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .expect("codes");
    assert_eq!(codes.len(), 1, "the code added later is gone");
    assert_eq!(codes[0].1, CODE, "the code value is back");
    let group: String = db
        .with_connection(|conn| {
            Ok(conn.query_row(
                "SELECT group_id FROM haex_passwords_group_items WHERE item_id = ?1",
                params![id],
                |r| r.get(0),
            )?)
        })
        .expect("folder");
    assert_eq!(group, folder, "a restore never moves the entry");
    assert_eq!(states(&db, &id), 4, "the restore is a state of its own");
}
