//! Tests for attachments and the clean-up of binary data (spec 034, US5, FR-019..FR-022, research
//! R4): hashing, deduplication, the size limit checked before reading, file names as text, links
//! that come and go, the grace period of seven days measured from the loss of the last link, icons
//! that stay while something names them, and byte-exact saving.

// The tests read raw columns and write sparse files, which the CRDT write path does not expose.
#![allow(clippy::disallowed_methods, clippy::redundant_closure)]

use haex_crdt::rusqlite::params;
use haex_crdt::Database;

use super::binaries::{
    add_bytes, attachment_data, hash_bytes, preview_bytes, prune_binaries, read_attachment_file,
    remove_link, rename_attachment, sanitize_file_name, save_to,
};
use super::clock::{format_millis, unix_millis};
use super::items;
use super::model::ItemInput;
use super::test_support::open_test_vault;
use super::ATTACHMENT_LIMIT_BYTES;
use crate::error::HaexError;
use crate::files::test_support::{picked, PathOpener};
use crate::storage::query;

const DAY: i64 = 86_400_000;

fn write<R>(
    db: &Database,
    f: impl FnOnce(&mut haex_crdt::CrdtTransaction<'_>) -> crate::error::Result<R>,
) -> crate::error::Result<R> {
    db.write(|tx| f(tx).map_err(haex_crdt::Error::from))
        .map_err(HaexError::from)
}

fn read<R>(
    db: &Database,
    f: impl FnOnce(&mut query::Reader<'_, '_>) -> crate::error::Result<R>,
) -> crate::error::Result<R> {
    query::read(db, |q| f(q).map_err(haex_crdt::Error::from)).map_err(HaexError::from)
}

fn item(db: &Database) -> String {
    write(db, |tx| items::create_item(tx, &ItemInput::default(), None)).expect("item")
}

fn count(db: &Database, sql: &str) -> i64 {
    db.with_connection(|c| Ok(c.query_row(sql, [], |r| r.get(0))?))
        .expect("count")
}

fn now() -> i64 {
    unix_millis(std::time::SystemTime::now())
}

#[test]
fn the_hash_is_lowercase_hex_sha256_of_the_raw_bytes() {
    assert_eq!(
        hash_bytes(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        hash_bytes(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn adding_the_same_file_twice_stores_one_binary_and_two_links() {
    let (_dir, db) = open_test_vault();
    let a = item(&db);
    let b = item(&db);
    let data = vec![7u8; 100];
    let first = write(&db, |tx| add_bytes(tx, &a, "x.bin", &data)).expect("first");
    let second = write(&db, |tx| add_bytes(tx, &b, "y.bin", &data)).expect("second");
    assert_eq!(first.binary_hash, second.binary_hash);
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_binaries"),
        1
    );
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_item_binaries"),
        2
    );
    assert_eq!(first.size, 100, "the stored size is the byte length");
    let stored_size: i64 = db
        .with_connection(|c| {
            Ok(c.query_row("SELECT size FROM haex_passwords_binaries", [], |r| r.get(0))?)
        })
        .expect("size");
    assert_eq!(stored_size, 100);
}

#[test]
fn an_empty_attachment_is_refused() {
    let (_dir, db) = open_test_vault();
    let id = item(&db);
    let result = write(&db, |tx| add_bytes(tx, &id, "empty.txt", b""));
    assert!(matches!(result, Err(HaexError::InvalidInput { reason }) if reason == "empty"));
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_binaries"),
        0
    );
}

#[test]
fn a_file_above_the_limit_is_refused_before_it_is_read() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("big.bin");
    let file = std::fs::File::create(&path).expect("create");
    // A sparse file: the size is there, the bytes are never written.
    file.set_len(ATTACHMENT_LIMIT_BYTES + 1)
        .expect("set length");
    let result = read_attachment_file(&PathOpener::default(), &picked(&path));
    assert!(
        matches!(
            result,
            Err(HaexError::PasswordsAttachmentTooLarge { bytes, limit })
                if bytes == ATTACHMENT_LIMIT_BYTES + 1 && limit == ATTACHMENT_LIMIT_BYTES
        ),
        "{result:?}"
    );
    // At the limit it is read.
    file.set_len(ATTACHMENT_LIMIT_BYTES).expect("set length");
    let (name, bytes) =
        read_attachment_file(&PathOpener::default(), &picked(&path)).expect("at the limit");
    assert_eq!(name, "big.bin");
    assert_eq!(bytes.len() as u64, ATTACHMENT_LIMIT_BYTES);
}

#[test]
fn an_empty_or_missing_file_is_invalid_input() {
    let dir = tempfile::tempdir().expect("tempdir");
    let empty = dir.path().join("empty.txt");
    std::fs::File::create(&empty).expect("create");
    assert!(matches!(
        read_attachment_file(&PathOpener::default(), &picked(&empty)),
        Err(HaexError::InvalidInput { reason }) if reason == "empty"
    ));
    assert!(matches!(
        read_attachment_file(&PathOpener::default(), &picked(&dir.path().join("missing"))),
        Err(HaexError::InvalidInput { reason }) if reason == "unreadable"
    ));
}

#[test]
fn file_names_are_text_never_paths() {
    assert_eq!(sanitize_file_name("report.pdf"), "report.pdf");
    assert_eq!(sanitize_file_name("../../etc/passwd"), ".._.._etc_passwd");
    assert_eq!(sanitize_file_name("a\\b/c"), "a_b_c");
    assert_eq!(sanitize_file_name("line\nbreak\t\u{0}"), "line_break__");
    assert_eq!(sanitize_file_name("Übergrößen 🔑.txt"), "Übergrößen 🔑.txt");
    assert_eq!(sanitize_file_name(&"x".repeat(300)).chars().count(), 255);
    let (_dir, db) = open_test_vault();
    let id = item(&db);
    let view = write(&db, |tx| add_bytes(tx, &id, "../evil/name.txt", b"data")).expect("add");
    assert_eq!(view.file_name, ".._evil_name.txt");
    let renamed = write(&db, |tx| rename_attachment(tx, &view.id, "new/name.txt")).expect("rename");
    assert_eq!(renamed, "new_name.txt");
}

#[test]
fn removing_a_link_keeps_the_binary_and_starts_its_grace_period_with_the_last_link() {
    let (_dir, db) = open_test_vault();
    let a = item(&db);
    let b = item(&db);
    let data = vec![1u8; 10];
    let first = write(&db, |tx| add_bytes(tx, &a, "a.bin", &data)).expect("a");
    let second = write(&db, |tx| add_bytes(tx, &b, "b.bin", &data)).expect("b");
    write(&db, |tx| remove_link(tx, &first.id)).expect("remove one");
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_binaries"),
        1
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_binaries WHERE orphaned_at IS NULL"
        ),
        1,
        "another entry still links it"
    );
    write(&db, |tx| remove_link(tx, &second.id)).expect("remove the last");
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_binaries"),
        1,
        "the binary stays"
    );
    // The snapshots still link it (the state with the attachment), so it is not orphaned yet.
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_binaries WHERE orphaned_at IS NULL"
        ),
        1,
        "history states keep it in use (FR-018)"
    );
}

#[test]
fn a_link_that_arrives_late_ends_the_grace_period() {
    let (_dir, db) = open_test_vault();
    let a = item(&db);
    // A binary that nothing links, marked as orphaned.
    write(&db, |tx| {
        tx.execute(
            "INSERT INTO haex_passwords_binaries (hash, data, size, orphaned_at) \
             VALUES ('h', x'01', 1, '2026-01-01T00:00:00.000Z')",
            &[],
        )?;
        tx.execute(
            "INSERT INTO haex_passwords_item_binaries (id, item_id, binary_hash, file_name) \
             VALUES ('l', ?1, 'h', 'late.bin')",
            params![a],
        )?;
        Ok(())
    })
    .expect("late link");
    let deleted = write(&db, |tx| prune_binaries(tx, now())).expect("prune");
    assert_eq!(deleted, 0);
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_binaries WHERE orphaned_at IS NULL"
        ),
        1,
        "the mark is cleared"
    );
}

fn orphan(db: &Database, hash: &str, kind: &str, orphaned_at: Option<&str>) {
    write(db, |tx| {
        tx.execute(
            "INSERT INTO haex_passwords_binaries (hash, data, size, type, orphaned_at) \
             VALUES (?1, x'01', 1, ?2, ?3)",
            params![hash, kind, orphaned_at],
        )?;
        Ok(())
    })
    .expect("orphan");
}

#[test]
fn prune_deletes_only_unreferenced_binaries_orphaned_for_seven_days() {
    let (_dir, db) = open_test_vault();
    let t = now();
    orphan(&db, "old", "attachment", Some(&format_millis(t - 8 * DAY)));
    orphan(
        &db,
        "fresh",
        "attachment",
        Some(&format_millis(t - 2 * DAY)),
    );
    // An old binary that is detected as unreferenced only now keeps its grace period: the mark
    // starts at the first detection, not at `created_at`.
    write(&db, |tx| {
        tx.execute(
            "INSERT INTO haex_passwords_binaries (hash, data, size, created_at) \
             VALUES ('ancient', x'01', 1, '2020-01-01T00:00:00.000Z')",
            &[],
        )?;
        Ok(())
    })
    .expect("ancient");
    let deleted = write(&db, |tx| prune_binaries(tx, t)).expect("prune");
    assert_eq!(deleted, 1);
    let left: Vec<String> = db
        .with_connection(|c| {
            let mut stmt = c.prepare("SELECT hash FROM haex_passwords_binaries ORDER BY hash")?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
        .expect("left");
    assert_eq!(left, ["ancient", "fresh"]);
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_binaries WHERE hash = 'ancient' AND orphaned_at IS NOT NULL"),
        1,
        "marked at the first detection"
    );
    // Eight days later it goes.
    let deleted = write(&db, |tx| prune_binaries(tx, t + 8 * DAY)).expect("prune later");
    assert_eq!(deleted, 2);
}

#[test]
fn a_binary_that_an_entry_or_a_state_links_is_never_pruned() {
    let (_dir, db) = open_test_vault();
    let id = item(&db);
    let data = vec![3u8; 5];
    let view = write(&db, |tx| add_bytes(tx, &id, "keep.bin", &data)).expect("add");
    // Remove the entry's link: the history state still links the binary.
    write(&db, |tx| remove_link(tx, &view.id)).expect("remove");
    let deleted = write(&db, |tx| prune_binaries(tx, now() + 30 * DAY)).expect("prune");
    assert_eq!(deleted, 0, "a state of the history needs it");
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_binaries"),
        1
    );
}

#[test]
fn a_custom_icon_stays_while_something_names_it() {
    let (_dir, db) = open_test_vault();
    let t = now();
    let far = format_millis(t - 30 * DAY);
    for hash in ["by-item", "by-group", "by-passkey", "by-state", "unused"] {
        orphan(&db, hash, "icon", Some(&far));
    }
    let id = item(&db);
    write(&db, |tx| {
        tx.execute(
            "UPDATE haex_passwords_item_details SET icon = 'binary:by-item' WHERE id = ?1",
            params![id],
        )?;
        tx.execute(
            "INSERT INTO haex_passwords_groups (id, name, icon) VALUES ('g', 'G', 'binary:by-group')",
            &[],
        )?;
        tx.execute(
            "INSERT INTO haex_passwords_passkeys \
             (id, item_id, credential_id, relying_party_id, user_handle, private_key, public_key, icon) \
             VALUES ('pk', ?1, 'c', 'rp', 'h', 'k', 'p', 'binary:by-passkey')",
            params![id],
        )?;
        tx.execute(
            "INSERT INTO haex_passwords_item_snapshots (id, item_id, snapshot_data, modified_at) \
             VALUES ('s', ?1, '{\"icon\":\"binary:by-state\"}', '2026-01-01T00:00:00.000Z')",
            params![id],
        )?;
        Ok(())
    })
    .expect("references");
    let deleted = write(&db, |tx| prune_binaries(tx, t)).expect("prune");
    assert_eq!(deleted, 1, "only the icon that nothing names goes");
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_binaries WHERE hash = 'unused'"
        ),
        0
    );
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_passwords_binaries"),
        4
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM haex_passwords_binaries WHERE orphaned_at IS NOT NULL"
        ),
        0,
        "a named icon is not marked"
    );
}

#[test]
fn a_linked_binary_cannot_be_deleted_directly() {
    let (_dir, db) = open_test_vault();
    let id = item(&db);
    write(&db, |tx| add_bytes(tx, &id, "x.bin", b"x").map(|_| ())).expect("add");
    let refused = db.with_connection(|c| {
        Ok(c.execute("DELETE FROM haex_passwords_binaries", [])
            .is_err())
    });
    assert!(refused.expect("run"), "ON DELETE RESTRICT (A4)");
}

#[test]
fn the_data_is_read_by_its_own_query_and_saved_byte_for_byte() {
    let (_dir, db) = open_test_vault();
    let id = item(&db);
    // A pseudo-random file of 3 MiB and a file with every byte value.
    let mut random = vec![0u8; 3 * 1024 * 1024];
    getrandom::fill(&mut random).expect("random");
    let every: Vec<u8> = (0..=255u8).collect();
    let dir = tempfile::tempdir().expect("tempdir");
    for (name, data) in [("random.bin", &random), ("every.bin", &every)] {
        let view = write(&db, |tx| add_bytes(tx, &id, name, data)).expect("add");
        let (file_name, bytes) = read(&db, |q| attachment_data(q, &view.id)).expect("data");
        assert_eq!(file_name, name);
        assert_eq!(&bytes, data);
        let target = dir.path().join(format!("saved-{name}"));
        save_to(&PathOpener::default(), &picked(&target), &bytes).expect("save");
        assert_eq!(&std::fs::read(&target).expect("read back"), data, "{name}");
    }
    // The detail view of the entry lists the attachments without any data.
    let detail = read(&db, |q| items::get_item(q, &id))
        .expect("get")
        .expect("exists");
    assert_eq!(detail.attachments.len(), 2);
    let missing = read(&db, |q| attachment_data(q, "nope").map(|_| ()));
    assert!(matches!(missing, Err(HaexError::PasswordsNotFound)));
}

#[test]
fn only_images_can_be_previewed() {
    let (_dir, db) = open_test_vault();
    let id = item(&db);
    let png = write(&db, |tx| add_bytes(tx, &id, "Photo.PNG", b"\x89PNG data")).expect("png");
    let pdf = write(&db, |tx| add_bytes(tx, &id, "doc.pdf", b"%PDF data")).expect("pdf");
    let (bytes, mime) = read(&db, |q| preview_bytes(q, &png.id)).expect("preview");
    assert_eq!(mime, "image/png");
    assert_eq!(bytes, b"\x89PNG data");
    for (name, mime) in [
        ("a.jpg", "image/jpeg"),
        ("a.jpeg", "image/jpeg"),
        ("a.gif", "image/gif"),
        ("a.webp", "image/webp"),
    ] {
        let view = write(&db, |tx| add_bytes(tx, &id, name, b"x")).expect("add");
        assert_eq!(
            read(&db, |q| preview_bytes(q, &view.id))
                .expect("preview")
                .1,
            mime
        );
    }
    let refused = read(&db, |q| preview_bytes(q, &pdf.id).map(|_| ()));
    assert!(
        matches!(refused, Err(HaexError::InvalidInput { reason }) if reason == "not_previewable")
    );
}
