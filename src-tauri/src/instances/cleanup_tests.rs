use super::*;

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
