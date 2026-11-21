use super::*;
use crate::instances::vault_paths::VaultPaths;

#[test]
fn every_file_of_the_vault_goes_and_others_stay() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("anna.db");
    for name in [
        "anna.db",
        "anna.db.pending",
        "anna.db.lock",
        "anna.db-wal",
        "anna.db-shm",
        "anna.db.vault-id",
        "anna.db.1f2e.tmp",
        "anna-2.db",
        "anna-2.db.vault-id",
        "anna.dbx",
    ] {
        std::fs::write(dir.path().join(name), b"x").unwrap();
    }

    assert!(remove_vault_files(&db));

    let mut left: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    left.sort();
    assert_eq!(left, ["anna-2.db", "anna-2.db.vault-id", "anna.dbx"]);
}

#[test]
fn missing_files_are_no_failure() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("b.db.pending"), b"").unwrap();
    assert!(remove_vault_files(&dir.path().join("b.db")));
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn the_marker_stays_while_a_file_of_the_vault_cannot_go() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("c.db.pending"), b"").unwrap();
    // A folder where the database file should be: removing it as a file fails.
    std::fs::create_dir(dir.path().join("c.db")).unwrap();
    assert!(!remove_vault_files(&dir.path().join("c.db")));
    assert!(dir.path().join("c.db.pending").exists());
}

#[test]
fn taking_back_a_vault_removes_its_folder_with_everything_in_it() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("vaults").join("anna");
    let other = dir.path().join("vaults").join("ben");
    for folder in [&folder, &other] {
        std::fs::create_dir_all(folder.join("agent-browser")).unwrap();
        std::fs::write(folder.join("vault.db"), b"x").unwrap();
        std::fs::write(folder.join("vault.db.pending"), b"").unwrap();
        std::fs::write(folder.join("agent-browser").join("Cookies"), b"x").unwrap();
    }

    assert!(take_back(&folder.join("vault.db")));

    assert!(!folder.exists());
    assert!(other.join("vault.db").exists(), "another vault stays");
}

#[test]
fn the_folder_stays_while_a_file_of_the_vault_cannot_go() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("anna");
    std::fs::create_dir_all(folder.join("vault.db")).unwrap(); // a folder where the file belongs
    std::fs::write(folder.join("vault.db.pending"), b"").unwrap();

    assert!(!take_back(&folder.join("vault.db")));
    assert!(folder.join("vault.db.pending").exists());
}

/// Fills the tool area and the home folder of the vault `name` the way installs and runs do.
fn populate_tools_and_home(paths: &VaultPaths, name: &str) {
    let version = paths.cli_tool_dir(name, "claude").join("1.0.0");
    std::fs::create_dir_all(version.join("lib")).unwrap();
    std::fs::write(version.join("claude"), b"program").unwrap();
    std::fs::write(version.join("install.json"), b"{}").unwrap();
    std::fs::write(version.join("lib").join("engine"), b"library").unwrap();
    std::fs::create_dir_all(paths.cli_tool_dir(name, "codex").join("2.0.0")).unwrap();
    std::fs::create_dir_all(paths.home(name).join(".claude")).unwrap();
    std::fs::write(paths.home(name).join(".claude").join("history"), b"x").unwrap();
    std::fs::create_dir_all(paths.run_root(name).join("run-1")).unwrap();
    std::fs::create_dir_all(paths.records_dir(name)).unwrap();
    std::fs::write(
        paths.records_dir(name).join("tools.log"),
        b"1 install_started\n",
    )
    .unwrap();
}

#[test]
fn taking_back_a_vault_leaves_no_file_of_its_tools_or_home_and_keeps_those_of_another_vault() {
    let dir = tempfile::tempdir().unwrap();
    let paths = VaultPaths::new(dir.path());
    for name in ["anna", "ben"] {
        std::fs::create_dir_all(paths.folder(name)).unwrap();
        std::fs::write(paths.database(name), b"x").unwrap();
        std::fs::write(get_pending_marker_path(&paths.database(name)), b"").unwrap();
        populate_tools_and_home(&paths, name);
    }

    assert!(take_back(&paths.database("anna")));

    assert!(
        !paths.folder("anna").exists(),
        "nothing of the vault is left"
    );
    assert!(!paths.cli_tools("anna").exists());
    assert!(!paths.home("anna").exists());
    assert!(
        paths
            .cli_tool_dir("ben", "claude")
            .join("1.0.0")
            .join("lib")
            .join("engine")
            .is_file(),
        "the tools of another vault stay"
    );
    assert!(paths.records_dir("ben").join("tools.log").is_file());
}

#[test]
fn taking_back_a_vault_takes_its_log_with_it() {
    // Spec 098 FR-017: the diagnostic log lies in the vault's folder and goes with it.
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("vaults").join("anna");
    std::fs::create_dir_all(folder.join("logs")).unwrap();
    std::fs::write(folder.join("vault.db"), b"x").unwrap();
    std::fs::write(folder.join("logs").join("haex.log"), b"line").unwrap();
    std::fs::write(folder.join("logs").join("haex.log.1"), b"older").unwrap();

    assert!(take_back(&folder.join("vault.db")));

    assert!(!folder.exists());
}
