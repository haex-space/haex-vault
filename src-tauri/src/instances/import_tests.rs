use super::*;

#[test]
fn the_name_comes_from_the_file_name() {
    assert_eq!(name_for("Anna's Tresor.db"), "Anna-s-Tresor");
    assert_eq!(name_for("arbeit.db"), "arbeit");
    assert_eq!(name_for("_.hidden.db"), "hidden");
    assert_eq!(name_for("…"), "vault");
    assert_eq!(name_for(&format!("{}.db", "a".repeat(80))).len(), 64);
}

#[test]
fn a_taken_name_gets_a_number_within_the_length() {
    assert_eq!(candidate("anna", 1), "anna");
    assert_eq!(candidate("anna", 2), "anna-2");
    let long = "a".repeat(64);
    assert_eq!(candidate(&long, 12).len(), 64);
    assert!(candidate(&long, 12).ends_with("-12"));
}

#[test]
fn reserving_skips_names_with_a_folder() {
    let dir = tempfile::tempdir().unwrap();
    let paths = VaultPaths::new(dir.path());
    std::fs::create_dir_all(paths.folder("anna")).unwrap();
    std::fs::create_dir_all(paths.folder("anna-2")).unwrap();
    let (name, path) = reserve_name(&paths, "anna").unwrap();
    assert_eq!(name, "anna-3");
    assert_eq!(path, paths.folder("anna-3").join("vault.db"));
    assert!(paths.folder("anna-3").join("vault.db.pending").exists());
}
