// These tests read CRDT metadata of the identity row, which the CRDT write
// path does not expose.
#![allow(clippy::disallowed_methods)]

use uuid::Uuid;

use super::*;
use crate::storage::query;
use crate::sync::content_keys;
use crate::sync::device_list;
use crate::sync::test_support::open_vault;

#[test]
fn a_new_vault_gets_identity_keys_first_list_and_content_key() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = open_vault(dir.path());
    let installation = Uuid::new_v4();

    let state = ensure_sync_state(&db, installation, true).expect("sync state");

    assert!(state.is_main);
    assert!(state.issued_first_list);
    let vault_pubkey = state.vault_pubkey.expect("identity");
    let device_pubkey = state.device_pubkey.expect("device key");

    let hlc: Option<String> = db
        .with_connection(|conn| {
            Ok(conn.query_row(
                "SELECT haex_hlc_no_sync FROM vault_identity WHERE id = 1",
                [],
                |r| r.get(0),
            )?)
        })
        .expect("identity row");
    assert!(hlc.is_some(), "the identity is published with an HLC");

    let rows = query::read(&db, |r| device_list::load_all(r)).expect("lists");
    let valid = device_list::valid_lists(&rows, &vault_pubkey);
    let effective = device_list::effective(&valid).expect("a valid first list");
    assert_eq!(effective.list.generation, 1);
    assert!(effective.list.is_main(&device_pubkey));
    assert_eq!(effective.list.devices[0].vault_device_uuid, db.device_id());

    let key = query::read(&db, |r| content_keys::current_key(r, &[]))
        .expect("read keys")
        .expect("a content key");
    assert_eq!(key.generation, 1);
    let name =
        content_keys::open_name(&key, &device_pubkey, &effective.list.devices[0].name_sealed)
            .expect("the name opens with the first key");
    assert_eq!(name, "HaexVault");
}

#[test]
fn a_second_run_changes_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = open_vault(dir.path());
    let installation = Uuid::new_v4();

    let first = ensure_sync_state(&db, installation, true).expect("first");
    let second = ensure_sync_state(&db, installation, true).expect("second");

    assert_eq!(first.vault_pubkey, second.vault_pubkey);
    assert_eq!(first.device_pubkey, second.device_pubkey);
    assert!(!second.issued_first_list);
    let lists = query::read(&db, |r| device_list::load_all(r)).expect("lists");
    assert_eq!(lists.len(), 1);
}

#[test]
fn without_genesis_a_vault_without_identity_stays_untouched() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = open_vault(dir.path());

    let state = ensure_sync_state(&db, Uuid::new_v4(), false).expect("sync state");

    assert_eq!(state.vault_pubkey, None);
    assert!(query::read(&db, |r| device_list::load_all(r))
        .expect("lists")
        .is_empty());
}
