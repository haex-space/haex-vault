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

/// Spec 069 (FR-022, SC-006): a new vault has the preset marketplace as one ordinary row, opening
/// it again adds nothing, and a deleted preset does not come back.
#[test]
fn a_new_vault_gets_the_preset_marketplace_once_and_never_again() {
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
    // The row is written after genesis, with a clock, so it syncs like any other row.
    db.write(|tx| Ok(store::delete(tx, store::PRESET_ID)?))
        .expect("delete");
    drop(db);

    let reopened = Database::open(vault_config("test-passphrase", &path, &id_file, false)).unwrap();
    crate::sync::genesis::run_after_open(&reopened, &id_file, false);
    assert!(
        marketplaces(&reopened).is_empty(),
        "opening does not bring the preset back"
    );
}
