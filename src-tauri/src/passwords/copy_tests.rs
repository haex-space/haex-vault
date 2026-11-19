//! Spec 036, FR-015, FR-022 (research R7): copying entries and folders in one transaction.
// These tests read raw vault state that the CRDT write path does not expose.
#![allow(clippy::disallowed_methods, clippy::redundant_closure)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::copy::{copy, CopyOptions, CopyReport, CopyTitle};
use super::model::{ItemInput, KeyValueInput, Target, TargetKind};
use super::test_support::open_test_vault;
use super::{binaries, groups, items, trash};
use crate::error::HaexError;

fn write<R>(
    db: &Database,
    f: impl FnOnce(&mut haex_crdt::CrdtTransaction<'_>) -> crate::error::Result<R>,
) -> crate::error::Result<R> {
    db.write(|tx| f(tx).map_err(haex_crdt::Error::from))
        .map_err(HaexError::from)
}

fn count(db: &Database, sql: &str) -> i64 {
    db.with_connection(|c| Ok(c.query_row(sql, [], |r| r.get(0))?))
        .expect("count")
}

fn text(db: &Database, sql: &str, id: &str) -> Option<String> {
    db.with_connection(|c| Ok(c.query_row(sql, params![id], |r| r.get(0))?))
        .expect("text")
}

fn options(title: CopyTitle) -> CopyOptions {
    CopyOptions {
        title,
        history: false,
        username_as_reference: false,
        password_as_reference: false,
        passkeys_as_links: None,
    }
}

fn suffix() -> CopyOptions {
    options(CopyTitle::Suffix(" – Kopie".to_string()))
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

fn full_item(db: &Database, title: &str, group: Option<&str>) -> String {
    write(db, |tx| {
        items::create_item(
            tx,
            &ItemInput {
                title: Some(title.to_string()),
                username: Some("anna".to_string()),
                password: Some("geheim".to_string()),
                url: Some("https://example.invalid".to_string()),
                note: Some("note".to_string()),
                otp_secret: Some("JBSWY3DPEHPK3PXP".to_string()),
                expires_at: Some("2030-01-01".to_string()),
                icon: Some("lucide:key".to_string()),
                color: Some("#ff0000".to_string()),
                autofill_aliases: Some("{}".to_string()),
                tags: vec!["work".to_string()],
                key_values: vec![KeyValueInput {
                    key: "PIN".to_string(),
                    value: Some("1234".to_string()),
                }],
                ..ItemInput::default()
            },
            group,
        )
    })
    .expect("item")
}

fn copy_ids(db: &Database, original: &str) -> Vec<String> {
    db.with_connection(|c| {
        let mut stmt =
            c.prepare("SELECT id FROM haex_passwords_item_details WHERE id <> ?1 ORDER BY rowid")?;
        let ids = stmt
            .query_map(params![original], |r| r.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(ids)
    })
    .expect("ids")
}

#[test]
fn a_single_entry_takes_the_exact_title_and_every_field_by_value() {
    let (_dir, db) = open_test_vault();
    let original = full_item(&db, "Mail", None);
    let report = write(&db, |tx| {
        copy(
            tx,
            &[item_target(&original)],
            None,
            &options(CopyTitle::Exact("Mail – Kopie".to_string())),
        )
    })
    .expect("copy");
    assert_eq!(
        report,
        CopyReport {
            items_created: 1,
            ..CopyReport::default()
        }
    );
    let copy_id = copy_ids(&db, &original).remove(0);
    let sql =
        |column: &str| format!("SELECT {column} FROM haex_passwords_item_details WHERE id = ?1");
    assert_eq!(
        text(&db, &sql("title"), &copy_id).as_deref(),
        Some("Mail – Kopie")
    );
    for column in [
        "username",
        "password",
        "url",
        "note",
        "otp_secret",
        "expires_at",
        "icon",
        "color",
        "autofill_aliases",
    ] {
        assert_eq!(
            text(&db, &sql(column), &copy_id),
            text(&db, &sql(column), &original),
            "{column}"
        );
    }
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_item_tags"),
        2,
        "the copy carries the tag too"
    );
    assert_eq!(
        text(
            &db,
            "SELECT value FROM haex_passwords_item_key_values WHERE item_id = ?1",
            &copy_id
        )
        .as_deref(),
        Some("1234")
    );
    // Without the history the copy has one state of its own.
    assert_eq!(
        count(
            &db,
            &format!(
                "SELECT COUNT(*) FROM haex_passwords_item_snapshots WHERE item_id = '{copy_id}'"
            )
        ),
        1
    );
}

#[test]
fn a_suffix_copy_preserves_whitespace_in_the_original_title() {
    let (_dir, db) = open_test_vault();
    let original = full_item(&db, "  Mail  ", None);
    write(&db, |tx| {
        copy(tx, &[item_target(&original)], None, &suffix())
    })
    .expect("copy");

    let copy_id = copy_ids(&db, &original).remove(0);
    assert_eq!(
        text(
            &db,
            "SELECT title FROM haex_passwords_item_details WHERE id = ?1",
            &copy_id
        )
        .as_deref(),
        Some("  Mail   – Kopie")
    );
}

#[test]
fn an_exact_title_needs_exactly_one_entry() {
    let (_dir, db) = open_test_vault();
    let a = full_item(&db, "A", None);
    let b = full_item(&db, "B", None);
    let folder = write(&db, |tx| {
        groups::create_group(tx, "F", None, None, None, None)
    })
    .expect("folder");
    let exact = options(CopyTitle::Exact("X".to_string()));
    for targets in [
        vec![item_target(&a), item_target(&b)],
        vec![group_target(&folder)],
    ] {
        let result = write(&db, |tx| copy(tx, &targets, None, &exact));
        assert!(
            matches!(&result, Err(HaexError::InvalidInput { reason }) if reason == "options.title"),
            "{result:?}"
        );
    }
}

#[test]
fn a_folder_is_copied_with_every_subfolder_and_entry_and_attachments_share_the_binary() {
    let (_dir, db) = open_test_vault();
    let work = write(&db, |tx| {
        groups::create_group(tx, "Arbeit", None, None, None, None)
    })
    .expect("work");
    let servers = write(&db, |tx| {
        groups::create_group(tx, "Server", None, None, None, Some(&work))
    })
    .expect("servers");
    let mail = full_item(&db, "Mail", Some(&work));
    full_item(&db, "Db", Some(&servers));
    write(&db, |tx| {
        binaries::add_bytes(tx, &mail, "file.txt", b"attachment bytes")?;
        Ok(())
    })
    .expect("attach");
    let target = write(&db, |tx| {
        groups::create_group(tx, "Ziel", None, None, None, None)
    })
    .expect("target");
    let binaries_before = count(&db, "SELECT COUNT(*) FROM haex_passwords_binaries");
    let report = write(&db, |tx| {
        copy(tx, &[group_target(&work)], Some(&target), &suffix())
    })
    .expect("copy");
    assert_eq!(report.groups_created, 2);
    assert_eq!(report.items_created, 2);
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_groups WHERE name = 'Arbeit – Kopie'"
        ),
        1
    );
    // The subfolder keeps its name and sits below the copy.
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_groups c JOIN haex_passwords_groups p \
             ON c.parent_id = p.id WHERE c.name = 'Server' AND p.name = 'Arbeit – Kopie'"
        ),
        1
    );
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_item_binaries"),
        2
    );
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_binaries"),
        binaries_before,
        "the binary is stored once"
    );
    // The original is unchanged.
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_groups WHERE name = 'Arbeit'"
        ),
        1
    );
}

#[test]
fn folders_without_a_name_and_a_parent_loop_are_copied_as_they_are() {
    let (_dir, db) = open_test_vault();
    let root = write(&db, |tx| {
        groups::create_group(tx, "Import", None, None, None, None)
    })
    .expect("root");
    // An unnamed KeePass group and a folder from another device without a name.
    let empty = write(&db, |tx| {
        groups::insert_group_row(tx, Some(""), None, None, None, Some(&root))
    })
    .expect("empty");
    write(&db, |tx| {
        groups::insert_group_row(tx, None, None, None, None, Some(&empty))
    })
    .expect("null");
    write(&db, |tx| {
        groups::insert_group_row(tx, Some("  Work "), None, None, None, Some(&root))
    })
    .expect("spaces");
    let report = write(&db, |tx| copy(tx, &[group_target(&root)], None, &suffix())).expect("copy");
    assert_eq!(report.groups_created, 4);
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_groups WHERE name = ''"
        ),
        2
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_groups WHERE name IS NULL"
        ),
        2
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_groups WHERE name = '  Work '"
        ),
        2
    );

    // Two folders that are each other's parent (concurrent moves): the copy ends.
    let a = write(&db, |tx| {
        groups::create_group(tx, "A", None, None, None, None)
    })
    .expect("a");
    let b = write(&db, |tx| {
        groups::create_group(tx, "B", None, None, None, Some(&a))
    })
    .expect("b");
    write(&db, |tx| {
        tx.execute(
            "UPDATE haex_passwords_groups SET parent_id = ?1 WHERE id = ?2",
            params![b, a],
        )?;
        Ok(())
    })
    .expect("loop");
    let report =
        write(&db, |tx| copy(tx, &[group_target(&a)], None, &suffix())).expect("copy loop");
    assert_eq!(report.groups_created, 2);
}

#[test]
fn the_history_is_taken_on_request() {
    let (_dir, db) = open_test_vault();
    let original = full_item(&db, "Mail", None);
    write(&db, |tx| {
        tx.execute(
            "UPDATE haex_passwords_item_details SET password = 'neu', updated_at = '9' WHERE id = ?1",
            params![original],
        )?;
        super::snapshots::take_snapshot(tx, &original)?;
        Ok(())
    })
    .expect("second state");
    let mut with_history = suffix();
    with_history.history = true;
    write(&db, |tx| {
        copy(tx, &[item_target(&original)], None, &with_history)
    })
    .expect("copy");
    let copy_id = copy_ids(&db, &original).remove(0);
    // Two states of the original plus the copy's own on top.
    assert_eq!(
        count(
            &db,
            &format!(
                "SELECT COUNT(*) FROM haex_passwords_item_snapshots WHERE item_id = '{copy_id}'"
            )
        ),
        3
    );
}

#[test]
fn username_and_password_can_point_at_the_original_and_an_empty_value_stays_empty() {
    let (_dir, db) = open_test_vault();
    let original = full_item(&db, "Mail", None);
    let empty = write(&db, |tx| {
        items::create_item(tx, &ItemInput::default(), None)
    })
    .expect("empty");
    let mut by_reference = suffix();
    by_reference.username_as_reference = true;
    by_reference.password_as_reference = true;
    write(&db, |tx| {
        copy(
            tx,
            &[item_target(&original), item_target(&empty)],
            None,
            &by_reference,
        )
    })
    .expect("copy");
    let copied = |source: &str, column: &str| {
        db.with_connection(|c| {
            Ok(c.query_row(
                &format!(
                    "SELECT {column} FROM haex_passwords_item_details \
                     WHERE id NOT IN (?1, ?2) AND IFNULL(title, '') LIKE ?3"
                ),
                params![original, empty, source],
                |r| r.get::<_, Option<String>>(0),
            )?)
        })
        .expect("copied")
    };
    assert_eq!(
        copied("Mail%", "password").as_deref(),
        Some(format!("{{${original}:password}}").as_str())
    );
    assert_eq!(
        copied("Mail%", "username").as_deref(),
        Some(format!("{{${original}:username}}").as_str())
    );
    assert_eq!(copied(" – Kopie", "password"), None);
}

#[test]
fn the_trash_is_no_target() {
    let (_dir, db) = open_test_vault();
    let original = full_item(&db, "Mail", None);
    write(&db, |tx| trash::ensure_trash(tx)).expect("trash");
    let into_trash = write(&db, |tx| {
        copy(tx, &[item_target(&original)], Some("trash"), &suffix())
    });
    assert!(matches!(into_trash, Err(HaexError::PasswordsIntoTrash)));
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_item_details"),
        1,
        "nothing was copied"
    );
}

#[test]
fn a_missing_or_trashed_source_is_skipped_and_counted() {
    let (_dir, db) = open_test_vault();
    let live = full_item(&db, "Live", None);
    let trashed = full_item(&db, "Old", None);
    write(&db, |tx| trash::trash(tx, &[item_target(&trashed)])).expect("trash");
    let report = write(&db, |tx| {
        copy(
            tx,
            &[
                item_target(&live),
                item_target(&trashed),
                item_target("00000000-0000-4000-8000-000000000000"),
                group_target("gone"),
            ],
            None,
            &suffix(),
        )
    })
    .expect("copy");
    assert_eq!(report.items_created, 1);
    assert_eq!(report.skipped_missing, 3);
}

#[test]
fn a_copy_of_hundreds_of_entries_is_one_transaction() {
    let (_dir, db) = open_test_vault();
    let folder = write(&db, |tx| {
        groups::create_group(tx, "Viele", None, None, None, None)
    })
    .expect("folder");
    write(&db, |tx| {
        for i in 0..300 {
            items::create_item(
                tx,
                &ItemInput {
                    title: Some(format!("E{i}")),
                    ..ItemInput::default()
                },
                Some(&folder),
            )?;
        }
        Ok(())
    })
    .expect("seed");
    let before = count(&db, "SELECT COUNT(*) FROM haex_passwords_item_details");
    // A failure after the copy work rolls back everything it did.
    let result: crate::error::Result<()> = write(&db, |tx| {
        copy(tx, &[group_target(&folder)], None, &suffix())?;
        Err(HaexError::InvalidInput {
            reason: "late".to_string(),
        })
    });
    assert!(result.is_err());
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_item_details"),
        before
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_groups WHERE name = 'Viele – Kopie'"
        ),
        0
    );
    let report = write(&db, |tx| {
        copy(tx, &[group_target(&folder)], None, &suffix())
    })
    .expect("copy");
    assert_eq!(report.items_created, 300);
}
