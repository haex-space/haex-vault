use super::*;

fn folder(prefix: &str, etag: Option<&str>) -> Arc<OpenFolder> {
    Arc::new(OpenFolder::new(
        prefix.to_owned(),
        [1; 16],
        etag.map(str::to_owned),
        &Zeroizing::new([2; 32]),
    ))
}

#[test]
fn an_opened_folder_counts_only_for_the_head_it_was_opened_with() {
    let folders = EncryptedFolders::default();
    folders.keep("s1", folder("a.hxef/", Some("\"1\"")));
    assert!(folders.opened("s1", "a.hxef/", Some("\"1\"")).is_some());
    assert!(folders.opened("s1", "a.hxef/", Some("\"2\"")).is_none());
    assert!(folders.opened("s2", "a.hxef/", Some("\"1\"")).is_none());
    folders.set_heads("s1", "", Vec::new());
    assert!(folders.heads("s1", "").is_some());
    folders.clear();
    assert!(folders.opened("s1", "a.hxef/", Some("\"1\"")).is_none());
    assert!(folders.heads("s1", "").is_none());
}

#[test]
fn neither_errors_nor_debug_output_name_anything_of_a_folder() {
    let open = folder("Privat/geheim.hxef/", None);
    assert_eq!(format!("{open:?}"), "OpenFolder");
    for state in [
        EncryptedState::OtherVault,
        EncryptedState::NewerFormat,
        EncryptedState::Damaged,
    ] {
        let error = state_error(state);
        assert!(!error.message.contains("Privat"));
    }
    assert_eq!(damaged().code, FilesErrorCode::Damaged);
}
