//! A device folder as a place: links skipped, an unreadable folder fails the listing (FR-022), the
//! rule trash, hashes and text written without a part file left.

use std::path::PathBuf;

use super::*;

fn folder() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let real = std::fs::canonicalize(dir.path()).unwrap();
    (dir, real)
}

#[tokio::test]
async fn a_tree_lists_files_and_folders_and_skips_links() {
    let (_dir, root) = folder();
    std::fs::create_dir_all(root.join("a/b")).unwrap();
    std::fs::write(root.join("a/b/c.txt"), "abc").unwrap();
    std::fs::write(root.join("top.txt"), "t").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join("top.txt"), root.join("link.txt")).unwrap();
    let place = DevicePlace::with_rule_trash(&root);
    let listing = place.list_tree(&CancellationToken::new()).await.unwrap();
    assert_eq!(
        listing.files.keys().collect::<Vec<_>>(),
        ["a/b/c.txt", "top.txt"]
    );
    assert_eq!(listing.files["a/b/c.txt"].size, 3);
    assert!(listing.files["a/b/c.txt"].mtime_ms > 0);
    assert_eq!(listing.folders.iter().collect::<Vec<_>>(), ["a", "a/b"]);
    #[cfg(unix)]
    assert_eq!(listing.skipped, [("link.txt".to_owned(), SkipReason::Link)]);
}

#[tokio::test]
async fn a_missing_root_is_unreachable() {
    let (_dir, root) = folder();
    let place = DevicePlace::with_rule_trash(root.join("gone"));
    let error = place
        .list_tree(&CancellationToken::new())
        .await
        .unwrap_err();
    assert_eq!(error.code, FilesErrorCode::Unreachable);
}

#[cfg(unix)]
#[tokio::test]
async fn an_unreadable_folder_fails_the_listing() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, root) = folder();
    std::fs::create_dir_all(root.join("closed")).unwrap();
    std::fs::write(root.join("closed/x.txt"), "x").unwrap();
    std::fs::set_permissions(root.join("closed"), std::fs::Permissions::from_mode(0o000)).unwrap();
    // Root ignores permissions; nothing to check then.
    if std::fs::read_dir(root.join("closed")).is_ok() {
        return;
    }
    let place = DevicePlace::with_rule_trash(&root);
    let result = place.list_tree(&CancellationToken::new()).await;
    std::fs::set_permissions(root.join("closed"), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(result.unwrap_err().code, FilesErrorCode::NoAccess);
}

#[tokio::test]
async fn the_rule_trash_keeps_the_path_and_rename_makes_folders() {
    let (_dir, root) = folder();
    std::fs::create_dir_all(root.join("a")).unwrap();
    std::fs::write(root.join("a/x.txt"), "x").unwrap();
    let place = DevicePlace::with_rule_trash(&root);
    let trashed = place.trash("a/x.txt", "run1").await.unwrap();
    assert_eq!(
        trashed,
        Trashed::Rule(".holzi-trash/run1/a/x.txt".to_owned())
    );
    assert!(root.join(".holzi-trash/run1/a/x.txt").is_file());
    assert!(!root.join("a/x.txt").exists());
    place
        .rename(".holzi-trash/run1/a/x.txt", "b/c/x.txt")
        .await
        .unwrap();
    assert!(root.join("b/c/x.txt").is_file());
}

#[tokio::test]
async fn hash_is_sha256_of_the_content() {
    let (_dir, root) = folder();
    std::fs::write(root.join("abc.txt"), "abc").unwrap();
    let place = DevicePlace::with_rule_trash(&root);
    assert_eq!(
        place
            .hash("abc.txt", &CancellationToken::new())
            .await
            .unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[tokio::test]
async fn text_is_written_whole_and_read_back_with_a_limit() {
    let (_dir, root) = folder();
    let place = DevicePlace::with_rule_trash(&root);
    assert_eq!(place.read_text(".haex-ignore", 10).await.unwrap(), None);
    place.write_text(".haex-ignore", "*.tmp\n").await.unwrap();
    assert_eq!(
        place
            .read_text(".haex-ignore", 10)
            .await
            .unwrap()
            .as_deref(),
        Some("*.tmp\n")
    );
    assert_eq!(
        place.read_text(".haex-ignore", 3).await.unwrap_err().code,
        FilesErrorCode::TooLarge
    );
    let names: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, [".haex-ignore"], "no part file left");
}

#[tokio::test]
async fn stat_and_delete() {
    let (_dir, root) = folder();
    std::fs::write(root.join("x.txt"), "xy").unwrap();
    let place = DevicePlace::with_rule_trash(&root);
    assert_eq!(place.stat("x.txt").await.unwrap().unwrap().size, 2);
    place.delete("x.txt").await.unwrap();
    assert_eq!(place.stat("x.txt").await.unwrap(), None);
}
