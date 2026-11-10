use super::*;

#[test]
fn the_file_sits_beside_the_vault() {
    assert_eq!(
        vault_id_path(Path::new("/a/vaults/anna/vault.db")),
        PathBuf::from("/a/vaults/anna/vault.db.vault-id")
    );
}

#[test]
fn the_fingerprint_is_the_sha256_of_the_key_as_hex() {
    let fingerprint = fingerprint_of(&[0u8; 32]);
    assert_eq!(fingerprint.len(), 64);
    assert_eq!(
        fingerprint,
        "66687aadf862bd776c8fc18b8e9f8e20089714856ee233b3902a591d0d5f2925"
    );
}

#[test]
fn the_owner_is_another_vault_with_the_same_fingerprint() {
    let dir = tempfile::tempdir().unwrap();
    for (name, text) in [("anna", "abc\n"), ("ben", "def\n")] {
        let folder = dir.path().join(name);
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(folder.join("vault.db.vault-id"), text).unwrap();
    }
    // A folder being moved, a folder without the file and a loose file are no owners.
    let staging = dir.path().join(".moving-cara");
    std::fs::create_dir(&staging).unwrap();
    std::fs::write(staging.join("vault.db.vault-id"), "abc\n").unwrap();
    std::fs::create_dir(dir.path().join("dora")).unwrap();
    std::fs::write(dir.path().join("abc"), "not a vault folder").unwrap();

    assert_eq!(owner_of(dir.path(), "abc", "new"), Some("anna".to_string()));
    assert_eq!(owner_of(dir.path(), "abc", "anna"), None);
    assert_eq!(owner_of(dir.path(), "xyz", "new"), None);
}
