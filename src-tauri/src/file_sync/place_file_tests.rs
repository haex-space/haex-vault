//! One file as a place (spec 074 research R1, R5): the listing holds the one file or nothing, every other
//! call passes through with the name as the path, the exclusion file reads as absent, and a folder that is
//! gone is an error and never an empty listing. The paths of a file split into its folder and its name.

use std::path::PathBuf;

use super::*;
use crate::file_sync::place_device::DevicePlace;
use crate::file_sync::test_support::MemPlace;
use crate::files::FilesErrorCode;

fn folder() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let real = std::fs::canonicalize(dir.path()).unwrap();
    (dir, real)
}

fn one(root: &std::path::Path, name: &str) -> OneFilePlace {
    OneFilePlace::new(Box::new(DevicePlace::with_rule_trash(root)), name)
}

#[tokio::test]
async fn the_listing_holds_the_one_file_with_the_facts_of_the_inner_place() {
    let (_dir, root) = folder();
    std::fs::write(root.join("Haushalt.ods"), "abcd").unwrap();
    std::fs::write(root.join("anderes.txt"), "x").unwrap();
    std::fs::create_dir(root.join("Unterordner")).unwrap();
    let place = one(&root, "Haushalt.ods");
    let listing = place.list_tree(&CancellationToken::new()).await.unwrap();
    assert_eq!(listing.files.keys().collect::<Vec<_>>(), ["Haushalt.ods"]);
    assert_eq!(listing.files["Haushalt.ods"].size, 4);
    assert!(listing.files["Haushalt.ods"].mtime_ms > 0);
    assert!(
        listing.folders.is_empty(),
        "the folder's other entries stay out"
    );
    assert!(listing.skipped.is_empty());
}

#[tokio::test]
async fn a_missing_file_and_a_folder_at_its_name_list_nothing() {
    let (_dir, root) = folder();
    let place = one(&root, "Haushalt.ods");
    let none = place.list_tree(&CancellationToken::new()).await.unwrap();
    assert!(none.files.is_empty());
    std::fs::create_dir(root.join("Haushalt.ods")).unwrap();
    let folder_there = place.list_tree(&CancellationToken::new()).await.unwrap();
    assert!(
        folder_there.files.is_empty(),
        "a folder is not the file of the rule"
    );
}

#[tokio::test]
async fn a_folder_that_is_gone_is_unreachable_not_an_empty_listing() {
    let (_dir, root) = folder();
    let place = one(&root.join("gone"), "Haushalt.ods");
    let error = place
        .list_tree(&CancellationToken::new())
        .await
        .unwrap_err();
    assert_eq!(error.code, FilesErrorCode::Unreachable);
    assert_eq!(
        place.check_root().await.unwrap_err().code,
        FilesErrorCode::Unreachable
    );
}

#[tokio::test]
async fn the_other_calls_pass_through_with_the_name_as_the_path() {
    let (_dir, root) = folder();
    std::fs::write(root.join("Haushalt.ods"), "abcd").unwrap();
    let place = one(&root, "Haushalt.ods");
    assert_eq!(place.stat("Haushalt.ods").await.unwrap().unwrap().size, 4);
    let hash = place
        .hash("Haushalt.ods", &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(hash.len(), 64);
    place
        .rename("Haushalt.ods", "Haushalt-alt.ods")
        .await
        .unwrap();
    assert!(root.join("Haushalt-alt.ods").is_file());
    assert!(matches!(
        place.side(),
        crate::files::transfer::remote::Side::Device
    ));
    assert_eq!(
        place.full_path("Haushalt.ods"),
        DevicePlace::with_rule_trash(&root).full_path("Haushalt.ods")
    );
}

#[tokio::test]
async fn the_exclusion_file_reads_as_absent_even_when_the_folder_has_one() {
    let (_dir, root) = folder();
    std::fs::write(root.join(".haex-ignore"), "*.tmp").unwrap();
    let inner = DevicePlace::with_rule_trash(&root);
    assert_eq!(
        inner.read_text(".haex-ignore", 1024).await.unwrap(),
        Some("*.tmp".to_owned()),
        "the folder does have one"
    );
    let place = one(&root, "Haushalt.ods");
    assert_eq!(place.read_text(".haex-ignore", 1024).await.unwrap(), None);
}

#[tokio::test]
async fn a_second_inner_place_works_the_same_way() {
    let memory = MemPlace::new();
    memory.put("Haushalt.ods", b"abc", 5);
    memory.put("anderes.txt", b"x", 6);
    let place = OneFilePlace::new(Box::new(memory), "Haushalt.ods");
    let listing = place.list_tree(&CancellationToken::new()).await.unwrap();
    assert_eq!(listing.files.keys().collect::<Vec<_>>(), ["Haushalt.ods"]);
    assert_eq!(listing.files["Haushalt.ods"].mtime_ms, 5);
}

#[test]
fn a_device_path_splits_into_its_folder_and_its_name() {
    let device = SourceRef::Device;
    assert_eq!(
        split_file_path(&device, "/home/anna/Dokumente/Haushalt.ods"),
        Some(("/home/anna/Dokumente".to_owned(), "Haushalt.ods".to_owned()))
    );
    assert_eq!(
        split_file_path(&device, "/Haushalt.ods"),
        Some(("/".to_owned(), "Haushalt.ods".to_owned()))
    );
    assert_eq!(split_file_path(&device, "/"), None, "a root has no name");
}

#[test]
fn a_window_path_of_a_storage_splits_the_same_way() {
    let storage = SourceRef::Storage {
        storage_id: "s3".to_owned(),
    };
    assert_eq!(
        split_file_path(&storage, "/Fotos/a.jpg"),
        Some(("/Fotos".to_owned(), "a.jpg".to_owned()))
    );
    assert_eq!(
        split_file_path(&storage, "/a.jpg"),
        Some(("/".to_owned(), "a.jpg".to_owned()))
    );
    assert_eq!(split_file_path(&storage, "/Fotos/"), None);
    assert_eq!(
        split_file_path(&storage, "a.jpg"),
        None,
        "not a window path"
    );
}

// ---- the check of the source file (spec 074 research R3) ----

#[test]
fn the_mount_root_is_where_an_external_medium_appears() {
    let root = |path: &str| mount_root(path).map(|p| p.to_string_lossy().into_owned());
    assert_eq!(
        root("/media/anna/STICK/a/b.txt").as_deref(),
        Some("/media/anna/STICK")
    );
    assert_eq!(
        root("/run/media/anna/STICK/x.txt").as_deref(),
        Some("/run/media/anna/STICK")
    );
    assert_eq!(root("/mnt/disk/Fotos/a.jpg").as_deref(), Some("/mnt/disk"));
    assert_eq!(
        root("/Volumes/Disk/a.txt").as_deref(),
        Some("/Volumes/Disk")
    );
    assert_eq!(
        root("/storage/1A2B-3C4D/DCIM/a.jpg").as_deref(),
        Some("/storage/1A2B-3C4D")
    );
    assert_eq!(root(r"D:\x\y.txt").as_deref(), Some(r"D:\"));
    assert_eq!(root("e:/x/y.txt").as_deref(), Some("e:/"));
    assert_eq!(root("/home/anna/x.txt"), None, "the system's own disk");
    assert_eq!(root("/media/anna"), None, "no label yet");
    assert_eq!(root("relative/x.txt"), None);
}

fn exists(_: &std::path::Path) -> bool {
    true
}

#[tokio::test]
async fn a_file_is_valid_and_a_missing_one_or_a_folder_at_its_path_is_no_file() {
    let (_dir, root) = folder();
    std::fs::write(root.join("Haushalt.ods"), "x").unwrap();
    let place = one(&root, "Haushalt.ods");
    assert_eq!(
        source_check(&place, "Haushalt.ods", None, &exists)
            .await
            .unwrap(),
        SourceCheck::Valid
    );
    std::fs::remove_file(root.join("Haushalt.ods")).unwrap();
    assert_eq!(
        source_check(&place, "Haushalt.ods", None, &exists)
            .await
            .unwrap(),
        SourceCheck::Invalid(InvalidReason::NoFile)
    );
    std::fs::create_dir(root.join("Haushalt.ods")).unwrap();
    assert_eq!(
        source_check(&place, "Haushalt.ods", None, &exists)
            .await
            .unwrap(),
        SourceCheck::Invalid(InvalidReason::NoFile),
        "a place answers only for files"
    );
}

#[tokio::test]
async fn a_missing_folder_is_folder_gone_and_a_missing_drive_is_unreachable() {
    let (_dir, root) = folder();
    let place = one(&root.join("gone"), "Haushalt.ods");
    assert_eq!(
        source_check(&place, "Haushalt.ods", None, &exists)
            .await
            .unwrap(),
        SourceCheck::Invalid(InvalidReason::FolderGone)
    );
    // The same missing path on a medium that is there, and on one that is not.
    let on_stick = "/media/anna/STICK/Dokumente/Haushalt.ods";
    assert_eq!(
        source_check(&place, "Haushalt.ods", Some(on_stick), &|_| true)
            .await
            .unwrap(),
        SourceCheck::Invalid(InvalidReason::FolderGone)
    );
    let error = source_check(&place, "Haushalt.ods", Some(on_stick), &|_| false)
        .await
        .unwrap_err();
    assert_eq!(error.code, FilesErrorCode::Unreachable);
}

#[tokio::test]
async fn a_place_that_cannot_be_reached_is_an_error_and_never_an_invalid_source() {
    let memory = MemPlace::new();
    memory.set_unreachable(true);
    let place = OneFilePlace::new(Box::new(memory), "Haushalt.ods");
    let error = source_check(&place, "Haushalt.ods", None, &exists)
        .await
        .unwrap_err();
    assert_eq!(error.code, FilesErrorCode::Unreachable);
}

#[test]
fn the_reasons_have_the_keys_the_window_translates() {
    assert_eq!(InvalidReason::NoFile.key(), "noFile");
    assert_eq!(InvalidReason::FolderGone.key(), "folderGone");
}

#[tokio::test]
async fn a_rename_onto_a_name_that_lies_beside_the_file_overwrites_nothing() {
    let memory = MemPlace::new();
    memory.put("Haushalt.ods", b"new", 5);
    memory.put("Haushalt (Konflikt Laptop 2026-11-09).ods", b"old copy", 4);
    let place = OneFilePlace::new(Box::new(memory), "Haushalt.ods");

    let refused = place
        .rename("Haushalt.ods", "Haushalt (Konflikt Laptop 2026-11-09).ods")
        .await
        .unwrap_err();

    assert_eq!(refused.code, FilesErrorCode::Exists);
    let kept = place
        .stat("Haushalt (Konflikt Laptop 2026-11-09).ods")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(kept.mtime_ms, 4, "the earlier copy is untouched");
    assert!(place.stat("Haushalt.ods").await.unwrap().is_some());
    place
        .rename(
            "Haushalt.ods",
            "Haushalt (Konflikt Laptop 2026-11-09 2).ods",
        )
        .await
        .expect("a free name works");
}
