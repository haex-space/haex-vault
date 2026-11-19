use std::path::PathBuf;

use super::*;
use crate::files::local::OwnPlaces;
use crate::files::{EntryKind, FilesErrorCode};

fn real_tempdir() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let real = std::fs::canonicalize(dir.path()).unwrap();
    (dir, real)
}

fn by_name<'a>(entries: &'a [crate::files::Entry], name: &str) -> &'a crate::files::Entry {
    entries.iter().find(|e| e.name == name).unwrap()
}

#[test]
fn a_folder_lists_its_files_and_folders() {
    let (_dir, real) = real_tempdir();
    std::fs::write(real.join("notiz.txt"), "Hallo").unwrap();
    std::fs::create_dir(real.join("fotos")).unwrap();
    let entries = list(&real, &OwnPlaces::default()).unwrap();
    assert_eq!(entries.len(), 2);
    let note = by_name(&entries, "notiz.txt");
    assert_eq!(note.kind, EntryKind::File);
    assert_eq!(note.size, Some(5));
    assert_eq!(note.mime.as_deref(), Some("text/plain"));
    assert_eq!(note.path, real.join("notiz.txt").to_string_lossy());
    assert!(note.modified_ms.is_some());
    let folder = by_name(&entries, "fotos");
    assert_eq!(folder.kind, EntryKind::Dir);
    assert_eq!(folder.size, None);
    assert_eq!(folder.mime, None);
}

#[test]
fn dot_files_are_hidden() {
    let (_dir, real) = real_tempdir();
    std::fs::write(real.join(".bashrc"), "").unwrap();
    std::fs::write(real.join("sichtbar"), "").unwrap();
    let entries = list(&real, &OwnPlaces::default()).unwrap();
    assert!(by_name(&entries, ".bashrc").hidden);
    assert!(!by_name(&entries, "sichtbar").hidden);
}

#[cfg(unix)]
#[test]
fn links_are_marked_and_show_their_target_kind() {
    let (_dir, real) = real_tempdir();
    std::fs::create_dir(real.join("ziel")).unwrap();
    std::os::unix::fs::symlink(real.join("ziel"), real.join("verweis")).unwrap();
    std::os::unix::fs::symlink(real.join("fehlt"), real.join("kaputt")).unwrap();
    let entries = list(&real, &OwnPlaces::default()).unwrap();
    let link = by_name(&entries, "verweis");
    assert!(link.symlink);
    assert_eq!(link.kind, EntryKind::Dir);
    let broken = by_name(&entries, "kaputt");
    assert!(broken.symlink);
    assert_eq!(broken.kind, EntryKind::File);
}

#[cfg(unix)]
#[test]
fn a_folder_without_access_is_marked() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, real) = real_tempdir();
    let locked = real.join("gesperrt");
    std::fs::create_dir(&locked).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let entries = list(&real, &OwnPlaces::default()).unwrap();
    let running_as_root = std::fs::read_dir(&locked).is_ok();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    if !running_as_root {
        assert!(by_name(&entries, "gesperrt").no_access);
    }
}

#[test]
fn entries_in_haexs_own_places_are_marked() {
    let (_dir, real) = real_tempdir();
    std::fs::create_dir(real.join("holzi")).unwrap();
    std::fs::create_dir(real.join("anderes")).unwrap();
    let own = OwnPlaces::new(vec![real.join("holzi")]);
    let entries = list(&real, &own).unwrap();
    assert!(by_name(&entries, "holzi").haex_owned);
    assert!(!by_name(&entries, "anderes").haex_owned);
}

#[test]
fn a_missing_folder_is_not_found_and_a_file_is_no_folder() {
    let (_dir, real) = real_tempdir();
    std::fs::write(real.join("datei"), "").unwrap();
    assert_eq!(
        list(&real.join("fehlt"), &OwnPlaces::default())
            .unwrap_err()
            .code,
        FilesErrorCode::NotFound
    );
    assert_eq!(
        list(&real.join("datei"), &OwnPlaces::default())
            .unwrap_err()
            .code,
        FilesErrorCode::InvalidPath
    );
}

#[test]
fn stat_describes_one_entry() {
    let (_dir, real) = real_tempdir();
    std::fs::write(real.join("brief.pdf"), "%PDF").unwrap();
    let entry = stat(&real.join("brief.pdf"), &OwnPlaces::default()).unwrap();
    assert_eq!(entry.name, "brief.pdf");
    assert_eq!(entry.mime.as_deref(), Some("application/pdf"));
    assert_eq!(
        stat(&real.join("fehlt"), &OwnPlaces::default())
            .unwrap_err()
            .code,
        FilesErrorCode::NotFound
    );
}
