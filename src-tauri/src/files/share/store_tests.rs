//! The rows of file shares (data-model.md): derived ids, one share per source and device (FR-017),
//! every grant change moving `changed_at`, the pending state (FR-026), sources that cannot be
//! shared (FR-015), and changes from another own device (FR-019).

// These tests insert devices directly, which the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use haex_crdt::rusqlite::params;

use super::{
    all_shares, create_share, delete_grant, delete_share, get_share, grant_for, grants_of,
    rename_share, save_grant, set_served, share_at, share_id, shares_for_vault, NewFileShare,
};
use crate::files::share::{FileShareItemKind, FileShareLevel};
use crate::files::SourceRef;
use crate::remote_storage::test_support::vault;
use crate::vault_gate::VaultDb;

const LAPTOP: &str = "11111111-1111-4111-8111-111111111111";
const PHONE: &str = "22222222-2222-4222-8222-222222222222";
const BEN: &str = "b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0";
const CLARA: &str = "c1a7a0c1a7a0c1a7a0c1a7a0c1a7a0c1a7a0c1a7a0c1a7a0c1a7a0c1a7a0c1a7";

fn with_devices(db: &VaultDb) {
    db.write_blocking(|tx| {
        for (installation, device) in [("inst-laptop", LAPTOP), ("inst-phone", PHONE)] {
            tx.execute(
                "INSERT INTO known_devices (installation_uuid, vault_device_uuid, alias, first_seen) \
                 VALUES (?1, ?2, NULL, 0)",
                params![installation, device],
            )?;
        }
        Ok(())
    })
    .expect("devices");
}

fn share(db: &VaultDb, source: SourceRef, path: &str, name: &str, now: &str) -> String {
    let (path, name, now) = (path.to_owned(), name.to_owned(), now.to_owned());
    db.write_blocking(move |tx| {
        Ok(create_share(
            tx,
            &NewFileShare {
                vault_device_uuid: LAPTOP,
                source: &source,
                path: &path,
                item_kind: FileShareItemKind::Folder,
                name: &name,
            },
            &now,
        )?)
    })
    .expect("create")
}

fn grant(db: &VaultDb, share: &str, vault: &str, level: FileShareLevel, now: &str) {
    let (share, vault, now) = (share.to_owned(), vault.to_owned(), now.to_owned());
    db.write_blocking(move |tx| Ok(save_grant(tx, &share, &vault, level, &now)?))
        .expect("grant");
}

#[test]
fn ids_are_the_same_on_every_device() {
    assert_eq!(
        share_id(LAPTOP, "device", "/home/a/Fotos"),
        share_id(LAPTOP, "device", "/home/a/Fotos")
    );
    assert_ne!(
        share_id(LAPTOP, "device", "/home/a/Fotos"),
        share_id(PHONE, "device", "/home/a/Fotos"),
        "the same path on another device is another share"
    );
}

#[test]
fn sharing_the_same_source_twice_keeps_one_share() {
    let (_dir, db) = vault();
    with_devices(&db);
    let first = share(
        &db,
        SourceRef::Device,
        "/home/a/Fotos",
        "Fotos",
        "2026-10-23T10:00:00.000Z",
    );
    let again = share(
        &db,
        SourceRef::Device,
        "/home/a/Fotos",
        "Urlaub",
        "2026-10-23T10:01:00.000Z",
    );
    assert_eq!(first, again, "FR-017");
    let all = db.read_blocking(|q| Ok(all_shares(q)?)).expect("list");
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name, "Urlaub");
    let found = db
        .read_blocking(|q| Ok(share_at(q, LAPTOP, &SourceRef::Device, "/home/a/Fotos")?))
        .expect("read");
    assert_eq!(found.map(|s| s.id), Some(first));
}

#[test]
fn every_grant_change_moves_changed_at_and_pends_until_served() {
    let (_dir, db) = vault();
    with_devices(&db);
    let id = share(
        &db,
        SourceRef::Device,
        "/data",
        "Daten",
        "2026-10-23T10:00:00.000Z",
    );
    let changed = |db: &VaultDb, id: &str| {
        let id = id.to_owned();
        db.read_blocking(move |q| Ok(get_share(q, &id)?))
            .expect("read")
            .expect("share")
    };
    assert!(changed(&db, &id).is_pending(), "never served yet");

    grant(
        &db,
        &id,
        BEN,
        FileShareLevel::Read,
        "2026-10-23T10:01:00.000Z",
    );
    assert_eq!(changed(&db, &id).changed_at, "2026-10-23T10:01:00.000Z");
    {
        let id = id.clone();
        db.write_blocking(move |tx| Ok(set_served(tx, &id, "2026-10-23T10:01:00.000Z")?))
            .expect("served");
    }
    assert!(!changed(&db, &id).is_pending());

    grant(
        &db,
        &id,
        BEN,
        FileShareLevel::ReadWrite,
        "2026-10-23T10:02:00.000Z",
    );
    assert!(
        changed(&db, &id).is_pending(),
        "a raised level waits for the serving device"
    );
    {
        let id = id.clone();
        db.write_blocking(move |tx| Ok(delete_grant(tx, &id, BEN, "2026-10-23T10:03:00.000Z")?))
            .expect("revoke");
    }
    let row = changed(&db, &id);
    assert_eq!(
        row.changed_at, "2026-10-23T10:03:00.000Z",
        "a revocation moves it too"
    );
    assert!(row.is_pending());
}

#[test]
fn grants_are_one_per_vault_and_read_on_every_request() {
    let (_dir, db) = vault();
    with_devices(&db);
    let id = share(
        &db,
        SourceRef::Device,
        "/data",
        "Daten",
        "2026-10-23T10:00:00.000Z",
    );
    grant(
        &db,
        &id,
        BEN,
        FileShareLevel::Read,
        "2026-10-23T10:01:00.000Z",
    );
    grant(
        &db,
        &id,
        BEN,
        FileShareLevel::ReadWrite,
        "2026-10-23T10:02:00.000Z",
    );
    grant(
        &db,
        &id,
        CLARA,
        FileShareLevel::Read,
        "2026-10-23T10:03:00.000Z",
    );
    let (grants, ben, served_to_ben) = {
        let id = id.clone();
        db.read_blocking(move |q| {
            Ok((
                grants_of(q, &id)?,
                grant_for(q, &id, BEN)?,
                shares_for_vault(q, LAPTOP, BEN)?,
            ))
        })
        .expect("read")
    };
    assert_eq!(grants.len(), 2);
    assert_eq!(ben, Some(FileShareLevel::ReadWrite));
    assert_eq!(served_to_ben.len(), 1);
    assert_eq!(served_to_ben[0].1, FileShareLevel::ReadWrite);
    let phone_serves = db
        .read_blocking(|q| Ok(shares_for_vault(q, PHONE, BEN)?))
        .expect("read");
    assert!(
        phone_serves.is_empty(),
        "only the serving device answers (FR-018)"
    );
}

#[test]
fn sources_of_other_devices_and_received_shares_cannot_be_shared() {
    let (_dir, db) = vault();
    with_devices(&db);
    for storage_id in ["peer:22222222-2222-4222-8222-222222222222", "share:abc"] {
        let source = SourceRef::Storage {
            storage_id: storage_id.to_owned(),
        };
        let result = db.write_blocking(move |tx| {
            Ok(create_share(
                tx,
                &NewFileShare {
                    vault_device_uuid: LAPTOP,
                    source: &source,
                    path: "/x",
                    item_kind: FileShareItemKind::Folder,
                    name: "x",
                },
                "2026-10-23T10:00:00.000Z",
            )?)
        });
        assert!(result.is_err(), "{storage_id} is refused (FR-015)");
    }
    let storage = share(
        &db,
        SourceRef::Storage {
            storage_id: "s3-family".to_owned(),
        },
        "/Fotos",
        "Fotos",
        "2026-10-23T10:00:00.000Z",
    );
    assert!(!storage.is_empty(), "a storage of this vault can be shared");
}

#[test]
fn another_own_device_changes_name_and_grants_but_not_source_or_device() {
    let (_dir, db) = vault();
    with_devices(&db);
    let id = share(
        &db,
        SourceRef::Device,
        "/data",
        "Daten",
        "2026-10-23T10:00:00.000Z",
    );
    // The phone edits the laptop's share: the same rows, written on another device (FR-019).
    {
        let id = id.clone();
        db.write_blocking(move |tx| {
            rename_share(tx, &id, "Projektdaten", "2026-10-23T11:00:00.000Z")?;
            save_grant(
                tx,
                &id,
                CLARA,
                FileShareLevel::Read,
                "2026-10-23T11:01:00.000Z",
            )?;
            Ok(())
        })
        .expect("edit");
    }
    let row = {
        let id = id.clone();
        db.read_blocking(move |q| Ok(get_share(q, &id)?))
            .expect("read")
            .expect("share")
    };
    assert_eq!(row.name, "Projektdaten");
    assert_eq!(row.vault_device_uuid, LAPTOP);
    assert_eq!(row.source, SourceRef::Device);
    assert_eq!(row.path, "/data");
    assert_eq!(row.changed_at, "2026-10-23T11:01:00.000Z");
}

#[test]
fn bad_names_paths_and_vaults_are_refused_and_delete_takes_the_grants() {
    let (_dir, db) = vault();
    with_devices(&db);
    let id = share(
        &db,
        SourceRef::Device,
        "/data",
        "Daten",
        "2026-10-23T10:00:00.000Z",
    );
    {
        let id = id.clone();
        let results = db
            .write_blocking(move |tx| {
                Ok((
                    rename_share(tx, &id, "   ", "t").is_err(),
                    rename_share(tx, &id, &"x".repeat(256), "t").is_err(),
                    save_grant(tx, &id, "B0B0", FileShareLevel::Read, "t").is_err(),
                    save_grant(tx, &id, &BEN.to_uppercase(), FileShareLevel::Read, "t").is_err(),
                ))
            })
            .expect("checks");
        assert_eq!(results, (true, true, true, true));
    }
    let relative = db.write_blocking(|tx| {
        Ok(create_share(
            tx,
            &NewFileShare {
                vault_device_uuid: LAPTOP,
                source: &SourceRef::Device,
                path: "data",
                item_kind: FileShareItemKind::Folder,
                name: "x",
            },
            "t",
        )?)
    });
    assert!(relative.is_err(), "a root is a window path");

    grant(
        &db,
        &id,
        BEN,
        FileShareLevel::Read,
        "2026-10-23T10:01:00.000Z",
    );
    {
        let id = id.clone();
        db.write_blocking(move |tx| Ok(delete_share(tx, &id)?))
            .expect("delete");
    }
    let left = db
        .read_blocking(move |q| Ok(grants_of(q, &id)?))
        .expect("read");
    assert!(left.is_empty());
}
