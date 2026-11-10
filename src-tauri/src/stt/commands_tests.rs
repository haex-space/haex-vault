//! Unit tests for the STT command helpers.

#![cfg(feature = "voice")]

use tempfile::tempdir;

use super::commands::{resolve_catalog_entry, scan_installed};
use super::files::PARAKEET_FILES;

#[test]
fn scan_installed_returns_empty_when_no_directories_resolve() {
    let entries = super::catalog::entries().expect("catalog parses");
    assert!(scan_installed(entries, |_| None).is_empty());
}

#[test]
fn scan_installed_requires_every_parakeet_file() {
    let dir = tempdir().unwrap();
    for name in PARAKEET_FILES.iter().take(PARAKEET_FILES.len() - 1) {
        std::fs::write(dir.path().join(name), b"model").unwrap();
    }
    let entry = super::catalog::get("parakeet-primeline").unwrap().unwrap();
    assert!(scan_installed(std::slice::from_ref(entry), |_| Some(dir.path().into())).is_empty());
    std::fs::write(
        dir.path().join(PARAKEET_FILES[PARAKEET_FILES.len() - 1]),
        b"tokens",
    )
    .unwrap();
    let installed = scan_installed(std::slice::from_ref(entry), |_| Some(dir.path().into()));
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0].id, "parakeet-primeline");
}

#[test]
fn resolve_catalog_entry_finds_parakeet_and_rejects_whisper() {
    assert_eq!(
        resolve_catalog_entry("parakeet-primeline").unwrap().id,
        "parakeet-primeline"
    );
    assert!(resolve_catalog_entry("whisper-tiny").is_err());
}
