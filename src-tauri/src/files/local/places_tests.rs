use std::path::{Path, PathBuf};

use super::*;
use crate::files::local::resolve;

fn real_tempdir() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let real = std::fs::canonicalize(dir.path()).unwrap();
    (dir, real)
}

#[test]
fn a_path_inside_an_own_place_is_own() {
    let (_dir, real) = real_tempdir();
    let own = OwnPlaces::new(vec![real.join("holzi")]);
    assert!(own.contains(&real.join("holzi")));
    assert!(own.contains(&real.join("holzi").join("instances").join("a.db")));
    assert!(!own.contains(&real.join("haex-notes")));
    assert!(!own.contains(&real));
}

#[test]
fn a_tree_that_holds_an_own_place_touches_it() {
    let (_dir, real) = real_tempdir();
    let own = OwnPlaces::new(vec![real.join("data").join("holzi")]);
    assert!(own.touches(&real.join("data"), true));
    assert!(!own.touches(&real.join("data"), false));
    assert!(!own.touches(&real.join("other"), true));
}

#[cfg(unix)]
#[test]
fn a_link_into_an_own_place_resolves_into_it() {
    let (_dir, real) = real_tempdir();
    std::fs::create_dir_all(real.join("holzi").join("instances")).unwrap();
    std::os::unix::fs::symlink(real.join("holzi"), real.join("innocent")).unwrap();
    let own = OwnPlaces::new(vec![real.join("holzi")]);
    let target = resolve(&real.join("innocent").join("instances")).unwrap();
    assert!(own.contains(&target));
    assert!(own.contains_resolved(&real.join("innocent")));
}

#[test]
fn dot_dot_out_of_a_known_place_does_not_escape_the_check() {
    let (_dir, real) = real_tempdir();
    std::fs::create_dir_all(real.join("home").join("docs")).unwrap();
    std::fs::create_dir_all(real.join("holzi")).unwrap();
    let own = OwnPlaces::new(vec![real.join("holzi")]);
    let spelled = real
        .join("home")
        .join("docs")
        .join("..")
        .join("..")
        .join("holzi");
    assert!(own.contains(&resolve(&spelled).unwrap()));
}

#[cfg(any(windows, target_os = "macos"))]
#[test]
fn letter_case_does_not_hide_an_own_place() {
    let own = OwnPlaces::new(vec![PathBuf::from("/Users/Anna/Library/holzi")]);
    assert!(own.contains(Path::new("/users/anna/library/HOLZI/instances")));
}

#[cfg(not(any(windows, target_os = "macos")))]
#[test]
fn letter_case_counts_where_the_file_system_counts_it() {
    let own = OwnPlaces::new(vec![PathBuf::from("/home/anna/.local/share/holzi")]);
    assert!(!own.contains(Path::new("/home/anna/.local/share/Holzi")));
}

#[test]
fn only_known_places_that_exist_are_offered() {
    let (_dir, real) = real_tempdir();
    std::fs::create_dir(real.join("Pictures")).unwrap();
    let places = existing_places(vec![
        ("pictures", Some(real.join("Pictures"))),
        ("videos", Some(real.join("Videos"))),
        ("desktop", None),
    ]);
    assert_eq!(
        places,
        vec![Place {
            name: "pictures",
            path: real.join("Pictures")
        }]
    );
}
