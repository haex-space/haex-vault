use std::fs::File;
use std::io;
use std::path::PathBuf;

use tauri_plugin_fs::FilePath;

use super::*;
use crate::files::test_support::{picked, PathOpener};

fn desktop() -> PathOpener {
    PathOpener::default()
}

#[test]
fn reads_a_chosen_path_up_to_one_byte_past_the_limit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, b"0123456789").unwrap();
    assert_eq!(
        read(&desktop(), &picked(&path), 100).unwrap(),
        b"0123456789"
    );
    assert_eq!(read(&desktop(), &picked(&path), 4).unwrap(), b"01234");
}

#[test]
fn size_is_the_length_of_the_chosen_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.bin");
    std::fs::write(&path, vec![1u8; 1234]).unwrap();
    assert_eq!(size(&desktop(), &picked(&path)).unwrap(), 1234);
    let error = size(&desktop(), &picked(&dir.path().join("gone"))).unwrap_err();
    assert!(matches!(error, HaexError::Unreadable), "{error:?}");
}

#[test]
fn a_missing_file_is_unreadable() {
    let dir = tempfile::tempdir().unwrap();
    let error = read(&desktop(), &picked(&dir.path().join("gone")), 10).unwrap_err();
    assert!(matches!(error, HaexError::Unreadable), "{error:?}");
}

#[test]
fn copy_into_writes_the_target_and_leaves_no_temporary_file() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.db");
    std::fs::write(&source, vec![7u8; 200_000]).unwrap();
    let target_dir = dir.path().join("instances");
    std::fs::create_dir(&target_dir).unwrap();
    let target = target_dir.join("copy.db");

    assert_eq!(
        copy_into(&desktop(), &picked(&source), &target).unwrap(),
        200_000
    );
    assert_eq!(std::fs::read(&target).unwrap(), vec![7u8; 200_000]);
    let names: Vec<_> = std::fs::read_dir(&target_dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, ["copy.db"]);
}

#[test]
fn a_failed_copy_leaves_the_target_untouched_and_nothing_beside_it() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.db");
    std::fs::write(&source, b"new").unwrap();
    // A folder in the way: the final rename fails.
    let target = dir.path().join("taken.db");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("keep"), b"old").unwrap();

    assert!(copy_into(&desktop(), &picked(&source), &target).is_err());
    assert_eq!(std::fs::read(target.join("keep")).unwrap(), b"old");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
}

#[test]
fn write_replaces_the_content_of_the_saved_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("export.json");
    std::fs::write(&path, b"a much longer old content").unwrap();
    write(&desktop(), &picked(&path), b"new").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"new");
}

#[test]
fn display_name_is_the_file_name_or_the_provider_name() {
    assert_eq!(
        display_name(&desktop(), &PickedFile::from("/home/anna/Bilder/Foto.png")),
        "Foto.png"
    );
    assert_eq!(
        display_name(
            &desktop(),
            &PickedFile::from("content://com.android.providers.downloads.documents/document/42")
        ),
        "Bericht.pdf"
    );
}

#[test]
fn display_name_falls_back_to_the_decoded_last_part_of_the_address() {
    struct Nameless;
    impl Opener for Nameless {
        fn open(&self, _file: FilePath, _write: bool) -> io::Result<File> {
            Err(io::Error::other("unused"))
        }
        fn path_roots(&self) -> Option<Vec<PathBuf>> {
            None
        }
        fn provider_name(&self, _uri: &str) -> Option<String> {
            None
        }
    }
    let file = PickedFile::from(
        "content://com.android.externalstorage.documents/document/primary%3ADownload%2Fvault.db",
    );
    assert_eq!(display_name(&Nameless, &file), "vault.db");
}

#[test]
fn without_free_paths_a_path_must_lie_in_the_app_storage() {
    let storage = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let inner = storage.path().join("files");
    std::fs::create_dir(&inner).unwrap();
    std::fs::write(inner.join("a.txt"), b"x").unwrap();
    std::fs::write(outside.path().join("b.txt"), b"y").unwrap();
    let device = PathOpener {
        roots: Some(vec![storage.path().to_path_buf()]),
        ..PathOpener::default()
    };

    assert_eq!(
        read(&device, &picked(&inner.join("a.txt")), 10).unwrap(),
        b"x"
    );
    for refused in [
        outside.path().join("b.txt"),
        inner.join("..").join("..").join("b.txt"),
        PathBuf::from("files/a.txt"),
    ] {
        let error = read(&device, &picked(&refused), 10).unwrap_err();
        assert!(
            matches!(error, HaexError::InvalidInput { .. }),
            "{refused:?}: {error:?}"
        );
    }
    let as_address = format!("file://{}", outside.path().join("b.txt").display());
    assert!(matches!(
        read(&device, &PickedFile(as_address), 10).unwrap_err(),
        HaexError::InvalidInput { .. }
    ));
}

#[cfg(unix)]
#[test]
fn a_link_in_the_app_storage_to_a_file_outside_is_refused() {
    let storage = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret"), b"s").unwrap();
    let link = storage.path().join("link");
    std::os::unix::fs::symlink(outside.path().join("secret"), &link).unwrap();
    assert!(!inside(&link, &[storage.path().to_path_buf()]));
    assert!(inside(
        &storage.path().join("not-yet-saved.json"),
        &[storage.path().to_path_buf()]
    ));
}

#[test]
fn other_addresses_are_refused() {
    let error = read(
        &desktop(),
        &PickedFile::from("https://example.org/a.txt"),
        10,
    )
    .unwrap_err();
    assert!(matches!(error, HaexError::InvalidInput { .. }), "{error:?}");
}
