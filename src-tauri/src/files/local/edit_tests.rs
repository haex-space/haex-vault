use std::path::PathBuf;

use super::*;
use crate::files::local::OwnPlaces;
use crate::files::{EntryKind, FilesErrorCode};

fn folder() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let real = std::fs::canonicalize(dir.path()).unwrap();
    (dir, real)
}

fn own() -> OwnPlaces {
    OwnPlaces::default()
}

#[test]
fn a_new_folder_is_created_and_described() {
    let (_dir, real) = folder();
    let entry = create_folder(&real, "Fotos", &own()).unwrap();
    assert_eq!(entry.name, "Fotos");
    assert_eq!(entry.kind, EntryKind::Dir);
    assert!(real.join("Fotos").is_dir());
}

#[test]
fn a_name_that_is_there_is_refused_for_a_new_folder() {
    let (_dir, real) = folder();
    std::fs::write(real.join("Fotos"), "x").unwrap();
    let error = create_folder(&real, "Fotos", &own()).unwrap_err();
    assert_eq!(error.code, FilesErrorCode::Exists);
}

#[test]
fn a_rename_keeps_the_content() {
    let (_dir, real) = folder();
    std::fs::write(real.join("a.txt"), "inhalt").unwrap();
    let entry = rename(&real.join("a.txt"), "b.txt", &own()).unwrap();
    assert_eq!(entry.path, real.join("b.txt").to_string_lossy());
    assert_eq!(
        std::fs::read_to_string(real.join("b.txt")).unwrap(),
        "inhalt"
    );
    assert!(!real.join("a.txt").exists());
}

#[test]
fn a_rename_onto_a_name_that_is_there_is_refused_and_changes_nothing() {
    let (_dir, real) = folder();
    std::fs::write(real.join("a.txt"), "a").unwrap();
    std::fs::write(real.join("b.txt"), "b").unwrap();
    let error = rename(&real.join("a.txt"), "b.txt", &own()).unwrap_err();
    assert_eq!(error.code, FilesErrorCode::Exists);
    assert_eq!(std::fs::read_to_string(real.join("b.txt")).unwrap(), "b");
    assert_eq!(std::fs::read_to_string(real.join("a.txt")).unwrap(), "a");
}

#[test]
fn renaming_to_the_same_name_changes_nothing() {
    let (_dir, real) = folder();
    std::fs::write(real.join("a.txt"), "a").unwrap();
    let entry = rename(&real.join("a.txt"), "a.txt", &own()).unwrap();
    assert_eq!(entry.name, "a.txt");
}

#[test]
fn invalid_names_are_refused() {
    let (_dir, real) = folder();
    std::fs::write(real.join("a.txt"), "a").unwrap();
    for name in ["", " ", ".", "..", "a/b", "a\0b", &"x".repeat(256)] {
        let error = create_folder(&real, name, &own()).unwrap_err();
        assert_eq!(error.code, FilesErrorCode::InvalidName, "{name:?}");
        let error = rename(&real.join("a.txt"), name, &own()).unwrap_err();
        assert_eq!(error.code, FilesErrorCode::InvalidName, "{name:?}");
    }
}

#[test]
fn names_windows_forbids_are_refused_everywhere() {
    // A folder made here may later be copied to a Windows drive or a storage.
    for name in [
        "a:b", "a*b", "a?b", "a\"b", "a<b", "a>b", "a|b", "a\\b", "ab.", "ab ",
    ] {
        assert!(check_name(name).is_err(), "{name:?}");
    }
    assert!(check_name("Urlaub 2026 (Kopie).jpg").is_ok());
    assert!(check_name(".versteckt").is_ok());
}

#[test]
fn nothing_changes_in_haexs_own_places() {
    let (_dir, real) = folder();
    std::fs::create_dir(real.join("eigen")).unwrap();
    std::fs::write(real.join("eigen/a.txt"), "a").unwrap();
    let own = OwnPlaces::new(vec![real.join("eigen")]);
    let error = create_folder(&real.join("eigen"), "neu", &own).unwrap_err();
    assert_eq!(error.code, FilesErrorCode::HaexOwned);
    let error = rename(&real.join("eigen/a.txt"), "b.txt", &own).unwrap_err();
    assert_eq!(error.code, FilesErrorCode::HaexOwned);
    assert!(real.join("eigen/a.txt").exists());
}

#[test]
fn renaming_a_folder_that_holds_haexs_data_is_refused() {
    let (_dir, real) = folder();
    std::fs::create_dir_all(real.join("share/holzi")).unwrap();
    let own = OwnPlaces::new(vec![real.join("share/holzi")]);
    let error = rename(&real.join("share"), "weg", &own).unwrap_err();
    assert_eq!(error.code, FilesErrorCode::HaexOwned);
    assert!(real.join("share/holzi").is_dir());
}
