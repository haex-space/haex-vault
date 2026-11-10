//! The probe sees an open vault and lets go of a free one (spec 084, T003).

use haex_crdt::DatabaseLock;

use super::{probe, Probe};

#[test]
fn a_vault_nobody_holds_is_free_and_stays_ours_until_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("anna.db");

    let Probe::Free(guard) = probe(&db).unwrap() else {
        panic!("a new vault is free");
    };
    // While we keep the guard, a second probe sees the vault as held.
    assert!(matches!(probe(&db).unwrap(), Probe::Held));
    drop(guard);
    assert!(matches!(probe(&db).unwrap(), Probe::Free(_)));
}

#[test]
fn a_vault_another_handle_holds_is_held() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("anna.db");
    // What `Database::open` does for an open vault.
    let _open = DatabaseLock::try_acquire(&db).unwrap();

    assert!(matches!(probe(&db).unwrap(), Probe::Held));
}

#[test]
fn a_lock_file_that_cannot_be_made_is_an_error_not_a_free_vault() {
    let dir = tempfile::tempdir().unwrap();
    // A file where the folder of the vault should be.
    let blocker = dir.path().join("instances");
    std::fs::write(&blocker, b"x").unwrap();

    assert!(probe(&blocker.join("anna.db")).is_err());
}
