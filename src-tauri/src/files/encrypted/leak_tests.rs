//! The SC-001 catalogue: whatever goes into an encrypted folder, nothing of its names, paths,
//! content types or contents is in any object key or body the storage gets (FR-010, FR-011).
//! `RemoteStore::put` and the parts of an upload carry only a key and bytes, so holzi sends no
//! content type and no metadata of its own; the provider answers `application/octet-stream`.

use std::sync::Arc;

use super::ops::create_encrypted_folder;
use super::test_support::{transfer, Scene, BUCKET};
use crate::files::kind::mime_for;
use crate::files::transfer::remote::Side;
use crate::files::transfer::TransferOp;

const SECRET: &str = "GEHEIM-Inhalt-4711";

/// Names with umlauts, spaces and special characters, one per content type of spec 044; each
/// starts upper case, so a random lower-case object key cannot hold it by chance.
const NAMES: [&str; 12] = [
    "Kontoauszug März 2026.pdf",
    "Präsentation (final) #2.txt",
    "Größe&Gewicht.csv",
    "Kundendaten.json",
    "Startseite.html",
    "Foto 😀.png",
    "Bild vom See.jpg",
    "Urlaubsfilm.mp4",
    "Lieblingslied.mp3",
    "Tabelle2026.xml",
    "Zeichnung.svg",
    "Ohne Endung",
];

fn haystack(scene: &Scene) -> Vec<(String, Vec<u8>)> {
    scene
        .keys()
        .into_iter()
        .map(|key| {
            let body = scene.fake.object(BUCKET, &key).unwrap();
            (key, body)
        })
        .collect()
}

fn contains(body: &[u8], needle: &str) -> bool {
    let needle = needle.as_bytes();
    !needle.is_empty() && body.windows(needle.len()).any(|window| window == needle)
}

#[tokio::test(flavor = "multi_thread")]
async fn nothing_readable_reaches_the_storage() {
    let scene = Scene::new();
    let files = scene.files();
    create_encrypted_folder(&files, "/", "Steuer Ünterlagen €")
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let deep = dir.path().join("Ä").join("ö ü").join("ß-Ordner");
    std::fs::create_dir_all(&deep).unwrap();
    std::fs::create_dir_all(dir.path().join("Ä").join("leer (Ordner)")).unwrap();
    for name in NAMES {
        std::fs::write(deep.join(name), format!("{SECRET} {name}")).unwrap();
    }
    transfer(
        TransferOp::Copy,
        Side::Device,
        &[dir.path().join("Ä").to_string_lossy().into_owned()],
        Side::Storage(Arc::new(scene.files())),
        "/Steuer Ünterlagen €",
        None,
        None,
    )
    .await
    .unwrap();
    files
        .create_folder("/Steuer Ünterlagen €", "Leerer Ordner")
        .await
        .unwrap();

    // Everything arrived and reads back by its names.
    let listed = scene
        .fresh_files()
        .list("/Steuer Ünterlagen €/Ä/ö ü/ß-Ordner")
        .await
        .unwrap();
    assert_eq!(listed.len(), NAMES.len());

    let mut needles: Vec<String> = vec![
        SECRET.to_owned(),
        "Steuer".to_owned(),
        "Ünterlagen".to_owned(),
        "Leerer Ordner".to_owned(),
        "leer (Ordner)".to_owned(),
        "ö ü".to_owned(),
        "ß-Ordner".to_owned(),
    ];
    for name in NAMES {
        needles.push(name.to_owned());
        let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
        needles.push(stem.to_owned());
        if let Some(mime) = mime_for(name) {
            needles.push(mime.to_owned());
        }
    }
    for (key, body) in haystack(&scene) {
        for needle in &needles {
            assert!(!key.contains(needle.as_str()), "{needle:?} in a key");
            assert!(!contains(&body, needle), "{needle:?} in the body of {key}");
            assert!(
                !contains(&body, "\"name\""),
                "a sidecar's JSON in clear: {key}"
            );
        }
    }
}

/// FR-027: no log line names anything of an encrypted folder.
#[path = "log_leak_tests.rs"]
mod logs;
