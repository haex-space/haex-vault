//! The shared logic of servers with real folders (spec 050 research R7–R9): parents of a key, and
//! listings of one and of many levels that look like S3's, without temporary files and with the old
//! ones found for deleting, and names that differ only in case.

use std::collections::BTreeMap;
use std::sync::Mutex;

use super::tree::{
    ancestors, case_clash, is_part, level, list, parent_of, part_name, DirEntry, PART_PREFIX,
};
use super::StorageError;

fn file(name: &str, size: u64) -> DirEntry {
    DirEntry {
        name: name.to_owned(),
        is_dir: false,
        size,
        modified: "2026-10-06T10:00:00.000Z".to_owned(),
        etag: format!("e-{name}"),
    }
}

fn folder(name: &str) -> DirEntry {
    DirEntry {
        is_dir: true,
        ..file(name, 0)
    }
}

/// 2026-10-07T10:00:00Z: a day after the time of every entry above, plus one second.
const NOW: i64 = 1_791_367_201_000;

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
    let (found, _) = list("", 100, NOW, |dir| {
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
    let (found, _) = list("Fotos/b", 100, NOW, |dir| {
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
    let result = list("", 2, NOW, |dir| {
        let entries = server.get(dir.as_str()).cloned();
        async move { entries.ok_or(StorageError::NotFound) }
    })
    .await;
    assert_eq!(result.err(), Some(StorageError::TooLarge));
}

fn part(name_tail: &str, modified: &str) -> DirEntry {
    DirEntry {
        modified: modified.to_owned(),
        ..file(&format!("{PART_PREFIX}{name_tail}"), 9)
    }
}

#[tokio::test]
async fn temporary_files_older_than_a_day_are_found_for_deleting() {
    let server = BTreeMap::from([
        (
            "",
            vec![
                folder("Fotos"),
                part("alt", "2026-10-06T10:00:00.000Z"),
                part("frisch", "2026-10-07T09:00:00.000Z"),
                part("ohne-zeit", ""),
            ],
        ),
        ("Fotos/", vec![part("auch-alt", "2026-10-01T10:00:00.000Z")]),
    ]);
    let (found, stale) = list("", 100, NOW, |dir| {
        let entries = server.get(dir.as_str()).cloned();
        async move { entries.ok_or(StorageError::NotFound) }
    })
    .await
    .expect("list");
    assert_eq!(
        found.iter().map(|o| o.key.as_str()).collect::<Vec<_>>(),
        ["Fotos/"],
        "no temporary file is listed"
    );
    let mut stale = stale;
    stale.sort();
    assert_eq!(
        stale,
        [
            format!("{PART_PREFIX}alt"),
            format!("Fotos/{PART_PREFIX}auch-alt")
        ],
        "a fresh one and one without a time stay"
    );
}

#[test]
fn one_level_shows_folders_as_prefixes_and_finds_old_temporary_files() {
    let entries = vec![
        file("b.jpg", 2),
        folder("2026"),
        file("a.jpg", 1),
        part("alt", "2026-10-01T10:00:00.000Z"),
        part("frisch", "2026-10-07T09:00:00.000Z"),
    ];
    let (listing, stale) = level("Fotos/", entries.clone(), 100, NOW).expect("level");
    assert_eq!(listing.prefixes, ["Fotos/2026/"]);
    assert_eq!(
        listing
            .objects
            .iter()
            .map(|o| (o.key.as_str(), o.size))
            .collect::<Vec<_>>(),
        [("Fotos/a.jpg", 1), ("Fotos/b.jpg", 2)]
    );
    assert_eq!(stale, [format!("Fotos/{PART_PREFIX}alt")]);
    assert_eq!(
        level("Fotos/", entries, 2, NOW).err(),
        Some(StorageError::TooLarge)
    );
}

#[tokio::test]
async fn a_walk_reads_at_most_four_folders_at_once() {
    let wide: Vec<DirEntry> = (0..10).map(|n| folder(&format!("d{n}"))).collect();
    let running = std::sync::atomic::AtomicUsize::new(0);
    let most = std::sync::atomic::AtomicUsize::new(0);
    let (found, _) = list("", 100, NOW, |dir| {
        let entries = if dir.is_empty() {
            wide.clone()
        } else {
            Vec::new()
        };
        let (running, most) = (&running, &most);
        async move {
            let now = running.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            most.fetch_max(now, std::sync::atomic::Ordering::SeqCst);
            tokio::task::yield_now().await;
            running.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            Ok(entries)
        }
    })
    .await
    .expect("list");
    assert_eq!(found.len(), 10);
    let most = most.load(std::sync::atomic::Ordering::SeqCst);
    assert!((2..=4).contains(&most), "{most} folders at once");
}

#[test]
fn a_name_that_differs_only_in_case_clashes() {
    let names = ["Fotos", "notiz.txt", "Urlaub"];
    assert_eq!(case_clash(names, "fotos"), Some("Fotos"));
    assert_eq!(case_clash(names, "NOTIZ.TXT"), Some("notiz.txt"));
    assert_eq!(
        case_clash(names, "Fotos"),
        None,
        "the same name is no clash"
    );
    assert_eq!(case_clash(names, "Bilder"), None);
    assert_eq!(case_clash(["Straße"], "STRASSE"), None);
    assert_eq!(case_clash(["Ärger"], "ärger"), Some("Ärger"));
}

#[tokio::test]
async fn a_folder_that_is_not_there_lists_as_empty() {
    let server = server();
    let (found, _) = list("Unbekannt/x", 100, NOW, |dir| {
        let entries = server.get(dir.as_str()).cloned();
        async move { entries.ok_or(StorageError::NotFound) }
    })
    .await
    .expect("as S3 lists an unknown prefix");
    assert!(found.is_empty());
}
