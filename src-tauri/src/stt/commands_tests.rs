//! Unit tests for the STT command handlers (spec 010). `list_stt_catalog`/
//! `stt_recommend_tiers` just delegate to `stt::catalog`, already covered by
//! `stt/catalog_tests.rs` — this file covers the engine-backed logic that
//! isn't: `scan_installed` (behind `list_installed_stt_models`) and
//! `resolve_catalog_entry`'s error mapping (behind `download_stt_model`).
//! Both are `AppHandle`-free helpers precisely so they can be unit tested
//! here — this codebase has no Tauri `AppHandle` mocking convention.

#![cfg(any(feature = "llm-cpu", feature = "voice"))]

use tempfile::tempdir;

use super::commands::{resolve_catalog_entry, scan_installed};

const CONFIG_FILENAME: &str = "config.json";
const TOKENIZER_FILENAME: &str = "tokenizer.json";
const WEIGHTS_FILENAME: &str = "model.safetensors";

#[test]
fn scan_installed_returns_empty_when_no_directories_resolve() {
    let entries = super::catalog::entries().expect("catalog should parse");
    let installed = scan_installed(entries, |_entry| None);
    assert!(installed.is_empty());
}

#[test]
fn scan_installed_skips_an_entry_with_zero_byte_files() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join(CONFIG_FILENAME), b"{}").unwrap();
    std::fs::write(dir.path().join(TOKENIZER_FILENAME), b"{}").unwrap();
    std::fs::write(dir.path().join(WEIGHTS_FILENAME), b"").unwrap(); // truncated — zero bytes

    let entries = super::catalog::entries().expect("catalog should parse");
    let target = &entries[0];
    let installed = scan_installed(std::slice::from_ref(target), |_| {
        Some(dir.path().to_path_buf())
    });
    assert!(installed.is_empty());
}

#[test]
fn scan_installed_skips_an_entry_with_a_missing_file() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join(CONFIG_FILENAME), b"{}").unwrap();
    std::fs::write(dir.path().join(TOKENIZER_FILENAME), b"{}").unwrap();
    // WEIGHTS_FILENAME intentionally never written.

    let entries = super::catalog::entries().expect("catalog should parse");
    let target = &entries[0];
    let installed = scan_installed(std::slice::from_ref(target), |_| {
        Some(dir.path().to_path_buf())
    });
    assert!(installed.is_empty());
}

#[test]
fn scan_installed_includes_an_entry_with_all_three_files_present() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join(CONFIG_FILENAME), b"{}").unwrap();
    std::fs::write(dir.path().join(TOKENIZER_FILENAME), b"{}").unwrap();
    std::fs::write(dir.path().join(WEIGHTS_FILENAME), b"weights").unwrap();

    let entries = super::catalog::entries().expect("catalog should parse");
    let target = &entries[0];
    let installed = scan_installed(std::slice::from_ref(target), |_| {
        Some(dir.path().to_path_buf())
    });
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0].id, target.id);
}

#[test]
fn scan_installed_checks_every_entry_independently() {
    let complete = tempdir().unwrap();
    std::fs::write(complete.path().join(CONFIG_FILENAME), b"{}").unwrap();
    std::fs::write(complete.path().join(TOKENIZER_FILENAME), b"{}").unwrap();
    std::fs::write(complete.path().join(WEIGHTS_FILENAME), b"weights").unwrap();
    let incomplete = tempdir().unwrap();
    std::fs::write(incomplete.path().join(CONFIG_FILENAME), b"{}").unwrap();

    let entries = super::catalog::entries().expect("catalog should parse");
    assert!(
        entries.len() >= 2,
        "catalog must have at least two tiers for this test"
    );
    let installed = scan_installed(&entries[0..2], |entry| {
        if entry.id == entries[0].id {
            Some(complete.path().to_path_buf())
        } else {
            Some(incomplete.path().to_path_buf())
        }
    });
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0].id, entries[0].id);
}

#[test]
fn resolve_catalog_entry_finds_a_known_id() {
    let entry = resolve_catalog_entry("whisper-tiny").expect("whisper-tiny is in the catalog");
    assert_eq!(entry.id, "whisper-tiny");
}

#[test]
fn resolve_catalog_entry_reports_not_found_for_an_unknown_id() {
    let err =
        resolve_catalog_entry("whisper-does-not-exist").expect_err("unknown id must be rejected");
    match err {
        crate::error::HolziError::CatalogEntryNotFound { id } => {
            assert_eq!(id, "whisper-does-not-exist");
        }
        other => panic!("expected CatalogEntryNotFound, got {other:?}"),
    }
}
