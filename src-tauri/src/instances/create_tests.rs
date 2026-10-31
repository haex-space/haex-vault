//! Tests for Genesis publication — in particular that a vault is never
//! reported as created while its pending marker still exists.

use super::*;

#[test]
fn genesis_is_not_published_if_pending_marker_cannot_be_removed() {
    let tmp = tempfile::tempdir().unwrap();
    let db = open_new_database(
        "test-passphrase",
        &tmp.path().join("vault.db"),
        &installation_id_path(tmp.path()),
    )
    .unwrap();
    let marker = tmp.path().join("vault.db.pending");
    std::fs::create_dir(&marker).unwrap(); // remove_file deterministically fails
    let state = AppState::default();
    assert!(publish_active(&state, "vault", &db, &marker).is_err());
    assert!(state.active_name().unwrap().is_none());
}

#[test]
fn published_genesis_survives_startup_cleanup() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("vault.db");
    let db =
        open_new_database("test-passphrase", &path, &installation_id_path(tmp.path())).unwrap();
    let marker = get_pending_marker_path(&path);
    std::fs::write(&marker, "").unwrap();
    let state = AppState::default();
    publish_active(&state, "vault", &db, &marker).unwrap();
    assert!(!marker.exists());
    assert_eq!(
        super::super::startup::cleanup_orphans_in_dir(tmp.path()).unwrap(),
        0
    );
    assert!(path.is_file());
}

/// Spec 069 (FR-022, SC-006): a new vault has the preset marketplace as one ordinary row; a vault
/// without it (from before the marketplace) gets it when it is opened, once, and a switched-off
/// preset stays off. A copy adds nothing: it gets the row from the sync.
#[test]
fn the_preset_marketplace_is_always_there_on_a_main_device() {
    use crate::instances::open::open_existing_database;
    use crate::marketplace::store;

    fn marketplaces(db: &Database) -> Vec<store::MarketplaceRow> {
        crate::storage::query::read(db, |q| Ok(store::list(q)?)).expect("list")
    }

    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("vault.db");
    let id_file = installation_id_path(tmp.path());
    let db = open_new_database("test-passphrase", &path, &id_file).unwrap();
    let rows = marketplaces(&db);
    assert_eq!(rows.len(), 1, "one row");
    assert_eq!(rows[0].id, store::PRESET_ID);
    assert_eq!(rows[0].name, store::PRESET_NAME);
    assert_eq!(rows[0].base_url, store::PRESET_URL);
    assert!(rows[0].enabled);
    // A vault from before the marketplace has no row.
    db.write(|tx| {
        tx.execute(
            "DELETE FROM haex_marketplaces",
            haex_crdt::rusqlite::params![],
        )?;
        Ok(())
    })
    .expect("forget the row");
    drop(db);

    let reopened = open_existing_database("test-passphrase", &path, &id_file).unwrap();
    let rows = marketplaces(&reopened);
    assert_eq!(rows.len(), 1, "opening brings the preset");
    assert_eq!(rows[0].id, store::PRESET_ID);
    reopened
        .write(|tx| Ok(store::set_enabled(tx, store::PRESET_ID, false, 1)?))
        .expect("switch off");
    drop(reopened);

    let again = open_existing_database("test-passphrase", &path, &id_file).unwrap();
    let rows = marketplaces(&again);
    assert_eq!(rows.len(), 1, "no second row");
    assert!(!rows[0].enabled, "the switch stays off");
    // Without the vault secret this device is a copy.
    again
        .write(|tx| {
            tx.execute(
                "DELETE FROM haex_marketplaces",
                haex_crdt::rusqlite::params![],
            )?;
            tx.execute(
                "DELETE FROM vault_identity_secret_no_sync",
                haex_crdt::rusqlite::params![],
            )?;
            Ok(())
        })
        .expect("become a copy");
    drop(again);

    let copy = open_existing_database("test-passphrase", &path, &id_file).unwrap();
    assert!(marketplaces(&copy).is_empty(), "a copy waits for the sync");
}
