use super::*;
use crate::remote_storage::test_support::vault;
use crate::vault_gate::VaultDb;

const FOLDER: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaa";

fn head(object_key: &str, etag: &str, name: &str) -> HeadRow {
    HeadRow {
        object_key: object_key.to_owned(),
        etag: etag.to_owned(),
        folder_id: FOLDER.to_owned(),
        name: Some(name.to_owned()),
        state: EncryptedState::Readable,
        key_id: Some([3; 16]),
        open_ack: false,
    }
}

fn put(db: &VaultDb, row: HeadRow) {
    db.write_blocking(move |tx| put_head(tx, "s1", &row, 1))
        .expect("put head");
}

fn heads(db: &VaultDb, parent: &str) -> Vec<HeadRow> {
    let parent = parent.to_owned();
    db.read_blocking(move |q| heads_under(q, "s1", &parent))
        .expect("read heads")
}

fn acknowledge(db: &VaultDb, object_key: &str) {
    let key = object_key.to_owned();
    db.write_blocking(move |tx| {
        tx.execute(
            "UPDATE encrypted_folder_heads_no_sync SET open_ack = 1 WHERE object_key = ?1",
            params![key],
        )?;
        Ok(())
    })
    .expect("acknowledge");
}

#[test]
fn heads_are_read_back_for_their_parent_only() {
    let (_dir, db) = vault();
    put(
        &db,
        head(&format!("{FOLDER}.hxef/h"), "\"1\"", "Unterlagen"),
    );
    put(
        &db,
        head(&format!("Privat/{FOLDER}.hxef/h"), "\"2\"", "Belege"),
    );
    let root = heads(&db, "");
    assert_eq!(root.len(), 1, "a deeper head is not a child of the root");
    assert_eq!(root[0].name.as_deref(), Some("Unterlagen"));
    assert_eq!(root[0].key_id, Some([3; 16]));
    let privat = heads(&db, "Privat/");
    assert_eq!(privat.len(), 1);
    assert_eq!(privat[0].etag, "\"2\"");
}

#[test]
fn open_ack_survives_a_reload_with_a_new_etag_or_name() {
    let (_dir, db) = vault();
    let key = format!("{FOLDER}.hxef/h");
    put(&db, head(&key, "\"1\"", "Unterlagen"));
    acknowledge(&db, &key);
    put(&db, head(&key, "\"2\"", "Akten"));
    let rows = heads(&db, "");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name.as_deref(), Some("Akten"));
    assert!(rows[0].open_ack);
}

#[test]
fn open_ack_moves_with_the_folder_to_a_new_object_key() {
    let (_dir, db) = vault();
    let old = format!("{FOLDER}.hxef/h");
    put(&db, head(&old, "\"1\"", "Unterlagen"));
    acknowledge(&db, &old);
    put(
        &db,
        head(&format!("Archiv/{FOLDER}.hxef/h"), "\"1\"", "Unterlagen"),
    );
    assert!(heads(&db, "Archiv/")[0].open_ack);
    db.write_blocking(move |tx| drop_heads(tx, "s1", &[old]))
        .expect("drop");
    assert!(heads(&db, "").is_empty());
}

fn file_row(sid: u8) -> EntryRow {
    EntryRow {
        sid: [sid; 16],
        etag: format!("\"{sid}\""),
        sidecar: Sidecar {
            entry: [10; 16],
            parent: [11; 16],
            name: "strom.pdf".to_owned(),
            revision: 2,
            base: Some([1; 16]),
            written: 1_700_000_000_000,
            kind: SidecarKind::File(FileFacts {
                size: 70_000,
                modified: 1_699_999_999_000,
                content_type: Some("application/pdf".to_owned()),
                sha256: [9; 32],
                content: [12; 16],
                cid: [13; 16],
                dek: WrappedDek::from_bytes(&[7; 72]).expect("72 bytes"),
            }),
        },
    }
}

#[test]
fn sidecars_round_trip_and_vanish_when_dropped() {
    let (_dir, db) = vault();
    let folder = EntryRow {
        sid: [4; 16],
        etag: "\"f\"".to_owned(),
        sidecar: Sidecar {
            entry: [11; 16],
            parent: [0; 16],
            name: "Belege".to_owned(),
            revision: 1,
            base: None,
            written: 1,
            kind: SidecarKind::Folder,
        },
    };
    let rows = vec![file_row(2), folder.clone()];
    db.write_blocking(move |tx| put_entries(tx, "s1", FOLDER, &rows))
        .expect("put");
    let mut read = db
        .read_blocking(|q| entries(q, "s1", FOLDER))
        .expect("read");
    read.sort_by_key(|row| row.sid);
    assert_eq!(read, vec![file_row(2), folder]);

    db.write_blocking(|tx| drop_entries(tx, "s1", FOLDER, &[[2; 16]]))
        .expect("drop");
    let left = db
        .read_blocking(|q| entries(q, "s1", FOLDER))
        .expect("read");
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].sid, [4; 16]);
}
