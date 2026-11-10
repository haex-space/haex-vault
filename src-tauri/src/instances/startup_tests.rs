//! Tests for `startup::cleanup_orphans_in_dir` — pure filesystem worker
//! that does not need a Tauri `AppHandle`.

use std::fs;

use crate::instances::presence::ProcessPresence;
use crate::instances::startup::cleanup_orphans_in_dir;
use crate::models::import::cleanup_staging_in_dir;

#[test]
fn cleanup_removes_the_folder_of_a_vault_with_a_marker() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();

    // Orphan: database, marker and a sidecar in its folder.
    fs::create_dir_all(dir.join("orphan")).unwrap();
    fs::write(dir.join("orphan/vault.db"), b"partial-genesis").unwrap();
    fs::write(dir.join("orphan/vault.db-wal"), b"x").unwrap();
    fs::write(dir.join("orphan/vault.db.pending"), b"").unwrap();

    // Healthy instance without marker — must be left alone.
    fs::create_dir_all(dir.join("healthy")).unwrap();
    fs::write(dir.join("healthy/vault.db"), b"complete").unwrap();

    let removed = cleanup_orphans_in_dir(dir).unwrap();
    assert_eq!(removed, 1);

    assert!(!dir.join("orphan").exists(), "orphan folder removed");
    assert!(
        dir.join("healthy/vault.db").exists(),
        "healthy instance untouched"
    );
}

#[test]
fn cleanup_survives_marker_without_database() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();

    // Marker exists but the database was never created (crash between
    // the reservation and Database::open). Cleanup must still succeed
    // and free the name.
    fs::create_dir_all(dir.join("half")).unwrap();
    fs::write(dir.join("half/vault.db.pending"), b"").unwrap();

    let removed = cleanup_orphans_in_dir(dir).unwrap();
    assert_eq!(removed, 1);
    assert!(!dir.join("half").exists());
}

#[test]
fn cleanup_missing_dir_is_not_error() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("does-not-exist");
    assert_eq!(cleanup_orphans_in_dir(&missing).unwrap(), 0);
}

/// T067/FR-026/SC-010: the combined startup cleanup (instance orphans plus model staging
/// leftovers, the same two pure functions `cleanup_orphans_on_startup` calls) runs only through
/// `ProcessPresence::announce`, so it runs while alone and is skipped while another presence is
/// alive — no AppHandle needed, since both cleanup functions already take a plain `&Path`.
#[test]
fn combined_startup_cleanup_runs_through_presence_when_alone_and_skips_while_another_is_alive() {
    let app_local_data = tempfile::tempdir().unwrap();
    let instances_dir = app_local_data.path().join("vaults");
    let models_root = app_local_data.path().join("models");
    fs::create_dir_all(instances_dir.join("orphan")).unwrap();
    fs::create_dir_all(&models_root).unwrap();
    fs::write(instances_dir.join("orphan/vault.db"), b"partial").unwrap();
    fs::write(instances_dir.join("orphan/vault.db.pending"), b"").unwrap();
    let staging_dir = models_root.join("model-slug");
    fs::create_dir_all(&staging_dir).unwrap();
    fs::write(staging_dir.join("model.gguf.tmp"), b"partial").unwrap();

    let cleanup = |instances_dir: std::path::PathBuf, models_root: std::path::PathBuf| {
        move || {
            let _ = cleanup_orphans_in_dir(&instances_dir);
            let _ = cleanup_staging_in_dir(&models_root);
        }
    };

    // Alone: both leftovers are removed.
    let presence = ProcessPresence::announce(
        app_local_data.path(),
        cleanup(instances_dir.clone(), models_root.clone()),
    )
    .expect("announce alone");
    assert!(!instances_dir.join("orphan").exists());
    assert!(!staging_dir.join("model.gguf.tmp").exists());

    // A second leftover pair appears, as a real overlapping second process's own half-finished
    // work would — the second announce below must leave it alone entirely.
    fs::create_dir_all(instances_dir.join("orphan2")).unwrap();
    fs::write(instances_dir.join("orphan2/vault.db"), b"partial").unwrap();
    fs::write(instances_dir.join("orphan2/vault.db.pending"), b"").unwrap();

    let _second_presence = ProcessPresence::announce(
        app_local_data.path(),
        cleanup(instances_dir.clone(), models_root.clone()),
    )
    .expect("announce while another presence is alive");
    assert!(
        instances_dir.join("orphan2/vault.db").exists(),
        "left alone while another presence is alive"
    );
    assert!(instances_dir.join("orphan2/vault.db.pending").exists());

    drop(presence);
}

#[test]
fn cleanup_keeps_marker_when_sibling_cannot_be_removed() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let folder = dir.join("blocked");

    // A directory at the database path makes remove_file fail without relying
    // on platform-specific permissions or running the test as a non-root user.
    fs::create_dir_all(folder.join("vault.db")).unwrap();
    fs::write(folder.join("vault.db.pending"), b"").unwrap();

    assert_eq!(cleanup_orphans_in_dir(dir).unwrap(), 0);
    assert!(folder.join("vault.db").is_dir());
    assert!(folder.join("vault.db.pending").exists());
}

#[test]
fn cleanup_leaves_healthy_vaults_hidden_folders_and_files_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    fs::create_dir_all(dir.join("anna")).unwrap();
    fs::write(dir.join("anna/vault.db"), b"x").unwrap();
    fs::create_dir_all(dir.join(".moving-ben")).unwrap();
    fs::write(dir.join(".moving-ben/vault.db.pending"), b"").unwrap();
    fs::write(dir.join("stray.txt"), b"x").unwrap();
    // A creation that stopped before its marker left an empty folder: it goes, so the name is free.
    fs::create_dir_all(dir.join("empty")).unwrap();

    assert_eq!(cleanup_orphans_in_dir(dir).unwrap(), 1);
    assert!(dir.join("anna/vault.db").exists());
    assert!(dir.join(".moving-ben/vault.db.pending").exists());
    assert!(dir.join("stray.txt").exists());
    assert!(!dir.join("empty").exists());
}
