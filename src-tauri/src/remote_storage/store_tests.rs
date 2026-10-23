//! The tables of storage connections (038 and 050 data-model.md): rows written and read, the field
//! rules per protocol, and removals that take the permissions of extensions with them in the same
//! write (FR-007, T030).

// These tests count rows and delete markers directly, which the CRDT write path does not expose.
#![allow(clippy::disallowed_methods)]

use super::store::{self, bucket_ok, folder_ok};
use super::test_support::{grant_storage, vault};
use super::{
    Addressing, ConnectionRow, EndpointScope, Protocol, ProviderKind, StorageRow, TestOutcome,
};
use crate::error::HolziError;
use crate::vault_gate::VaultDb;

fn connection(id: &str, item: &str) -> ConnectionRow {
    ConnectionRow {
        id: id.to_owned(),
        protocol: Protocol::S3,
        provider_name: "RustFS".to_owned(),
        provider_kind: ProviderKind::Rustfs,
        endpoint: "http://127.0.0.1:9000".to_owned(),
        endpoint_scope: EndpointScope::Local,
        region: "us-east-1".to_owned(),
        addressing: Addressing::Path,
        host_key: None,
        host_key_confirmed_at: None,
        credentials_item_id: item.to_owned(),
        created_at: "2026-10-06T10:00:00.000Z".to_owned(),
        updated_at: "2026-10-06T10:00:00.000Z".to_owned(),
    }
}

fn storage(id: &str, connection_id: &str) -> StorageRow {
    StorageRow {
        id: id.to_owned(),
        connection_id: connection_id.to_owned(),
        name: format!("Speicher {id}"),
        bucket: "holzi-test".to_owned(),
        created_at: "2026-10-06T10:00:00.000Z".to_owned(),
        updated_at: "2026-10-06T10:00:00.000Z".to_owned(),
    }
}

fn put(db: &VaultDb, c: Option<ConnectionRow>, s: Option<StorageRow>) -> Result<(), HolziError> {
    db.write_blocking(move |tx| {
        if let Some(c) = &c {
            store::put_connection(tx, c)?;
        }
        if let Some(s) = &s {
            store::put_storage(tx, s)?;
        }
        Ok(())
    })
}

fn count(db: &VaultDb, sql: &str) -> i64 {
    let sql = sql.to_owned();
    db.read_blocking(move |q| Ok(q.query_row(&sql, &[], |r| r.get::<_, i64>(0))?.unwrap_or(0)))
        .expect("count")
}

use crate::storage::query::Query;

#[test]
fn rows_are_written_read_back_and_updated() {
    let (_dir, db) = vault();
    put(
        &db,
        Some(connection("c1", "item-1")),
        Some(storage("s1", "c1")),
    )
    .expect("insert");

    let mut changed = connection("c1", "item-1");
    changed.provider_name = "Heim".to_owned();
    changed.created_at = "ignored on update".to_owned();
    changed.updated_at = "2026-10-06T11:00:00.000Z".to_owned();
    put(&db, Some(changed), None).expect("update");

    let (c, s) = db
        .read_blocking(|q| {
            Ok((
                store::connection(q, "c1")?.expect("connection"),
                store::storages(q)?,
            ))
        })
        .expect("read");
    assert_eq!(c.provider_name, "Heim");
    assert_eq!(c.created_at, "2026-10-06T10:00:00.000Z", "created_at stays");
    assert_eq!(c.updated_at, "2026-10-06T11:00:00.000Z");
    assert_eq!(s, vec![storage("s1", "c1")]);
}

#[test]
fn field_rules_are_checked_before_a_write() {
    let (_dir, db) = vault();
    let field = |error: HolziError| match error {
        HolziError::StorageInvalid { field } => field,
        other => panic!("unexpected {other:?}"),
    };
    let mut c = connection("c1", "item-1");
    c.provider_name = " ".to_owned();
    assert_eq!(field(put(&db, Some(c), None).unwrap_err()), "providerName");
    let mut c = connection("c1", "item-1");
    c.provider_name = "x".repeat(81);
    assert_eq!(field(put(&db, Some(c), None).unwrap_err()), "providerName");
    let mut c = connection("c1", "item-1");
    c.region = String::new();
    assert_eq!(field(put(&db, Some(c), None).unwrap_err()), "region");
    let mut c = connection("c1", "item-1");
    c.endpoint = String::new();
    assert_eq!(
        field(put(&db, Some(c.clone()), None).unwrap_err()),
        "endpoint"
    );
    c.provider_kind = ProviderKind::Aws;
    put(&db, Some(c), None).expect("aws needs no endpoint");

    let mut s = storage("s1", "c1");
    s.bucket = "Holzi".to_owned();
    assert_eq!(field(put(&db, None, Some(s)).unwrap_err()), "bucket");
    assert!(matches!(
        put(&db, None, Some(storage("s1", "missing"))),
        Err(HolziError::StorageNotFound)
    ));
}

/// A WebDAV connection of the user, the shape spec 050 saves.
fn webdav(id: &str) -> ConnectionRow {
    ConnectionRow {
        protocol: Protocol::WebDav,
        provider_name: "Storage Box".to_owned(),
        provider_kind: ProviderKind::HetznerStorageBox,
        endpoint: "https://u123.your-storagebox.de/".to_owned(),
        endpoint_scope: EndpointScope::Public,
        region: String::new(),
        ..connection(id, "item-1")
    }
}

/// An SFTP connection with a confirmed host key.
fn sftp(id: &str) -> ConnectionRow {
    ConnectionRow {
        protocol: Protocol::Sftp,
        provider_name: "Storage Box".to_owned(),
        provider_kind: ProviderKind::HetznerStorageBox,
        endpoint: "sftp://u123.your-storagebox.de:23".to_owned(),
        endpoint_scope: EndpointScope::Public,
        region: String::new(),
        host_key: Some("ssh-ed25519 AAAAplaceholder".to_owned()),
        host_key_confirmed_at: Some("2026-10-12T10:00:00.000Z".to_owned()),
        ..connection(id, "item-1")
    }
}

#[test]
fn webdav_and_sftp_connections_follow_their_rules() {
    let (_dir, db) = vault();
    let field = |error: HolziError| match error {
        HolziError::StorageInvalid { field } => field,
        other => panic!("unexpected {other:?}"),
    };
    put(&db, Some(webdav("w1")), None).expect("a WebDAV connection");
    put(&db, Some(sftp("f1")), None).expect("an SFTP connection");
    let read = db
        .read_blocking(|q| Ok(store::connection(q, "f1")?))
        .expect("read")
        .expect("there");
    assert_eq!(read, sftp("f1"));

    for (endpoint, why) in [
        ("https://u123.your-storagebox.de", "no trailing slash"),
        ("ftp://u123.your-storagebox.de/", "another scheme"),
        (
            "https://user:pw@u123.your-storagebox.de/",
            "a user in the address",
        ),
        ("https://u123.your-storagebox.de/?x=1", "a query"),
    ] {
        let mut c = webdav("w2");
        c.endpoint = endpoint.to_owned();
        assert_eq!(
            field(put(&db, Some(c), None).unwrap_err()),
            "endpoint",
            "{why}"
        );
    }
    for (endpoint, why) in [
        ("sftp://u123.your-storagebox.de:23/home", "a path"),
        ("https://u123.your-storagebox.de/", "another scheme"),
        (
            "sftp://user@u123.your-storagebox.de",
            "a user in the address",
        ),
    ] {
        let mut c = sftp("f2");
        c.endpoint = endpoint.to_owned();
        assert_eq!(
            field(put(&db, Some(c), None).unwrap_err()),
            "endpoint",
            "{why}"
        );
    }
    let mut c = webdav("w3");
    c.region = "eu-central-1".to_owned();
    assert_eq!(field(put(&db, Some(c), None).unwrap_err()), "region");
    let mut c = webdav("w3");
    c.provider_kind = ProviderKind::Aws;
    assert_eq!(field(put(&db, Some(c), None).unwrap_err()), "providerKind");
    let mut c = sftp("f3");
    c.host_key = None;
    assert_eq!(
        field(put(&db, Some(c), None).unwrap_err()),
        "hostKey",
        "an SFTP connection is never saved without a confirmed host key"
    );
    let mut c = webdav("w3");
    c.host_key = Some("ssh-ed25519 AAAA".to_owned());
    assert_eq!(field(put(&db, Some(c), None).unwrap_err()), "hostKey");
}

#[test]
fn the_protocol_of_a_saved_connection_never_changes() {
    let (_dir, db) = vault();
    put(&db, Some(webdav("c1")), None).expect("a WebDAV connection");
    let mut changed = sftp("c1");
    changed.credentials_item_id = "item-1".to_owned();
    match put(&db, Some(changed), None) {
        Err(HolziError::StorageInvalid { field }) => assert_eq!(field, "protocol"),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn a_storage_on_webdav_holds_a_folder() {
    let (_dir, db) = vault();
    put(&db, Some(webdav("c1")), None).expect("connection");
    let mut s = storage("s1", "c1");
    s.bucket = "Fotos/2026".to_owned();
    put(&db, None, Some(s.clone())).expect("a folder");
    s.bucket = String::new();
    put(&db, None, Some(s.clone())).expect("the root");
    s.bucket = "/Fotos".to_owned();
    assert!(matches!(
        put(&db, None, Some(s)),
        Err(HolziError::StorageInvalid { field }) if field == "folder"
    ));
}

#[test]
fn folders_follow_the_rules_of_webdav_and_sftp() {
    for ok in ["", "holzi", "Fotos/2026", "Ä ö/ü", &"a".repeat(255)] {
        assert!(folder_ok(ok), "{ok:?}");
    }
    for bad in [
        "/holzi",
        "holzi/",
        "a//b",
        "a/./b",
        "a/../b",
        "..",
        &"a".repeat(256),
        "a\u{7}b",
    ] {
        assert!(!folder_ok(bad), "{bad:?}");
    }
}

#[test]
fn bucket_names_follow_the_s3_rules() {
    for ok in ["abc", "holzi-test", "a.b.c", &"a".repeat(63)] {
        assert!(bucket_ok(ok), "{ok}");
    }
    for bad in [
        "ab",
        &"a".repeat(64),
        "Abc",
        "a_b",
        "-ab",
        "ab-",
        "a..b",
        "a b",
        "äbc",
    ] {
        assert!(!bucket_ok(bad), "{bad}");
    }
}

#[test]
fn removing_a_storage_takes_its_permissions_and_test_result_with_it() {
    let (_dir, db) = vault();
    put(
        &db,
        Some(connection("c1", "item-1")),
        Some(storage("s1", "c1")),
    )
    .expect("insert");
    put(&db, None, Some(storage("s2", "c1"))).expect("insert");
    grant_storage(&db, "notes", "s1", "granted");
    grant_storage(&db, "photos", "s2", "granted");
    grant_storage(&db, "backup", "*", "granted");
    db.write_blocking(|tx| Ok(store::record_test(tx, "s1", TestOutcome::Passed, "now")?))
        .expect("record");

    db.write_blocking(|tx| Ok(store::remove_storage(tx, "s1")?))
        .expect("remove");

    let targets: Vec<String> = db
        .read_blocking(|q| {
            q.query_map(
                "SELECT target FROM extension_permissions ORDER BY target",
                &[],
                |r| r.get(0),
            )
        })
        .expect("targets");
    assert_eq!(
        targets,
        ["*", "s2"],
        "only the permission that names s1 goes"
    );
    assert_eq!(count(&db, "SELECT COUNT(*) FROM storage_tests_no_sync"), 0);
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM haex_storages WHERE id = 's1'"),
        0
    );
    for table in ["haex_storages", "extension_permissions"] {
        assert_eq!(
            count(
                &db,
                &format!("SELECT COUNT(*) FROM haex_deleted_rows WHERE table_name = '{table}'")
            ),
            1,
            "the removal of {table} reaches the other devices with the same write"
        );
    }
}

#[test]
fn removing_a_connection_frees_its_credentials_only_when_unused() {
    let (_dir, db) = vault();
    put(
        &db,
        Some(connection("c1", "shared")),
        Some(storage("s1", "c1")),
    )
    .expect("insert");
    put(&db, Some(connection("c2", "shared")), None).expect("insert");
    put(&db, Some(connection("c3", "own")), None).expect("insert");

    let freed = db
        .write_blocking(|tx| Ok(store::remove_connection(tx, "c1")?))
        .expect("remove c1");
    assert_eq!(freed, None, "c2 still uses the entry");
    assert_eq!(count(&db, "SELECT COUNT(*) FROM haex_storages"), 0);
    let freed = db
        .write_blocking(|tx| Ok(store::remove_connection(tx, "c3")?))
        .expect("remove c3");
    assert_eq!(freed.as_deref(), Some("own"));
    assert!(matches!(
        db.write_blocking(|tx| Ok(store::remove_connection(tx, "c3")?)),
        Err(HolziError::StorageNotFound)
    ));
}

#[test]
fn test_results_are_kept_per_storage_and_replaced() {
    let (_dir, db) = vault();
    db.write_blocking(|tx| {
        store::record_test(tx, "s1", TestOutcome::Unreachable, "t1")?;
        store::record_test(tx, "s1", TestOutcome::Passed, "t2")?;
        Ok(())
    })
    .expect("record");
    let tests = db.read_blocking(|q| Ok(store::tests(q)?)).expect("tests");
    assert_eq!(
        tests.get("s1"),
        Some(&("t2".to_owned(), TestOutcome::Passed))
    );
}

#[test]
fn the_extensions_of_a_storage_are_those_granted_it_by_id() {
    let (_dir, db) = vault();
    grant_storage(&db, "notes", "s1", "granted");
    grant_storage(&db, "photos", "s1", "denied");
    grant_storage(&db, "backup", "*", "granted");
    let names = db
        .read_blocking(|q| Ok(store::extensions_of(q, "s1")?))
        .expect("names");
    assert_eq!(names, ["notes"]);
}

#[test]
fn removing_a_storage_drops_what_this_device_cached_of_its_encrypted_folders() {
    use crate::files::encrypted::cache::{self, HeadRow};
    use crate::files::EncryptedState;
    let (_dir, db) = vault();
    put(
        &db,
        Some(connection("c1", "item-1")),
        Some(storage("s1", "c1")),
    )
    .expect("insert");
    put(&db, None, Some(storage("s2", "c1"))).expect("insert");
    for storage_id in ["s1", "s2"] {
        db.write_blocking(move |tx| {
            cache::put_head(
                tx,
                storage_id,
                &HeadRow {
                    object_key: "aaaaaaaaaaaaaaaaaaaaaaaaaa.hxef/h".to_owned(),
                    etag: "\"e\"".to_owned(),
                    folder_id: "aaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
                    name: Some("Unterlagen".to_owned()),
                    state: EncryptedState::Readable,
                    key_id: None,
                    open_ack: false,
                },
                1,
            )?;
            Ok(())
        })
        .expect("cache a head");
    }

    db.write_blocking(|tx| Ok(store::remove_storage(tx, "s1")?))
        .expect("remove");

    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM encrypted_folder_heads_no_sync WHERE storage_id = 's1'"
        ),
        0
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM encrypted_folder_heads_no_sync WHERE storage_id = 's2'"
        ),
        1,
        "another storage keeps its cache"
    );
}

#[test]
fn a_local_folder_is_never_a_connection_nor_a_storage() {
    let mut row = connection("c-local", "item-1");
    row.protocol = Protocol::Local;
    row.provider_kind = ProviderKind::Other;
    row.region = String::new();
    assert!(matches!(
        store::check_connection(&row),
        Err(HolziError::StorageInvalid { .. })
    ));
    assert!(!store::target_ok(Protocol::Local, "/home"));
    assert!(!store::target_ok(Protocol::Local, ""));
}

#[test]
fn another_device_is_never_a_connection_nor_a_storage() {
    let mut row = connection("c-peer", "item-1");
    row.protocol = Protocol::Peer;
    row.provider_kind = ProviderKind::Other;
    row.region = String::new();
    assert!(matches!(
        store::check_connection(&row),
        Err(HolziError::StorageInvalid { .. })
    ));
    assert!(!store::target_ok(Protocol::Peer, "peer:device"));
}
