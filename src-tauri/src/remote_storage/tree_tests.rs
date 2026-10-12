//! The shared logic of servers with real folders (spec 050 research R7–R9): parents of a key, and
//! listings over many levels that look like S3's, without temporary files.

use std::collections::BTreeMap;
use std::sync::Mutex;

use super::tree::{ancestors, is_part, list, parent_of, part_name, DirEntry};
use super::StorageError;

fn file(name: &str, size: u64) -> DirEntry {
    DirEntry {
        name: name.to_owned(),
        is_dir: false,
        size,
        modified: "t".to_owned(),
        etag: format!("e-{name}"),
    }
}

fn folder(name: &str) -> DirEntry {
    DirEntry {
        is_dir: true,
        ..file(name, 0)
    }
}

/// A server with folders `""`, `Fotos/`, `Fotos/2026/`, `Leer/` and a temporary file.
fn server() -> BTreeMap<&'static str, Vec<DirEntry>> {
    BTreeMap::from([
        (
            "",
            vec![
                folder("Fotos"),
                folder("Leer"),
                file("a.txt", 1),
                file(&part_name(), 9),
            ],
        ),
        ("Fotos/", vec![folder("2026"), file("b.jpg", 2)]),
        ("Fotos/2026/", vec![file("c.jpg", 3)]),
        ("Leer/", vec![]),
    ])
}

#[test]
fn parents_of_a_key() {
    assert_eq!(parent_of("a/b/c.txt"), "a/b/");
    assert_eq!(parent_of("c.txt"), "");
    assert_eq!(parent_of("a/b/"), "a/");
    assert_eq!(ancestors("a/b/c.txt"), ["a/", "a/b/"]);
    assert!(ancestors("c.txt").is_empty());
    assert!(is_part(&part_name()));
    assert!(!is_part("holzi-part.txt"));
}

#[tokio::test]
async fn a_listing_shows_folders_as_markers_and_hides_temporary_files() {
    let server = server();
    let read = Mutex::new(Vec::new());
    let found = list("", 100, |dir| {
        read.lock().expect("lock").push(dir.clone());
        let entries = server.get(dir.as_str()).cloned();
        async move { entries.ok_or(StorageError::NotFound) }
    })
    .await
    .expect("list");
    let keys: Vec<(&str, u64)> = found.iter().map(|o| (o.key.as_str(), o.size)).collect();
    assert_eq!(
        keys,
        [
            ("Fotos/", 0),
            ("Fotos/2026/", 0),
            ("Fotos/2026/c.jpg", 3),
            ("Fotos/b.jpg", 2),
            ("Leer/", 0),
            ("a.txt", 1),
        ]
    );
    assert_eq!(read.lock().expect("lock").len(), 4, "every folder once");
}

#[tokio::test]
async fn a_prefix_reads_only_the_folders_it_can_reach() {
    let server = server();
    let read = Mutex::new(Vec::new());
    let found = list("Fotos/b", 100, |dir| {
        read.lock().expect("lock").push(dir.clone());
        let entries = server.get(dir.as_str()).cloned();
        async move { entries.ok_or(StorageError::NotFound) }
    })
    .await
    .expect("list");
    let keys: Vec<&str> = found.iter().map(|o| o.key.as_str()).collect();
    assert_eq!(keys, ["Fotos/b.jpg"], "a prefix may end inside a name");
    assert_eq!(*read.lock().expect("lock"), ["Fotos/"]);
}

#[tokio::test]
async fn more_entries_than_allowed_is_too_large() {
    let server = server();
    let result = list("", 2, |dir| {
        let entries = server.get(dir.as_str()).cloned();
        async move { entries.ok_or(StorageError::NotFound) }
    })
    .await;
    assert_eq!(result.err(), Some(StorageError::TooLarge));
}
