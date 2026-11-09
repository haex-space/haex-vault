use super::*;

#[test]
fn a_name_is_taken_once_and_leaves_the_marker_in_its_folder() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("vaults").join("anna").join("vault.db");

    reserve(&db).unwrap();
    assert!(db.with_file_name("vault.db.pending").is_file());
    assert!(!db.exists());

    let again = reserve(&db).unwrap_err();
    assert_eq!(again.kind(), io::ErrorKind::AlreadyExists);
}

#[test]
fn a_name_whose_folder_exists_is_taken_even_without_a_database() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("vaults").join("ben").join("vault.db");
    fs::create_dir_all(db.parent().unwrap()).unwrap();

    assert_eq!(
        reserve(&db).unwrap_err().kind(),
        io::ErrorKind::AlreadyExists
    );
}
