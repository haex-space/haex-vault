//! Spec 010: `is_complete_file`/`is_complete_model` guard against a
//! previous run's interrupted/truncated download (or a hand-deleted file)
//! being mistaken for an installed one. No network involved — pure
//! filesystem checks.

use std::path::Path;

use tempfile::tempdir;

use super::catalog::SttEngine;
use super::files::{is_complete_file, is_complete_model, PARAKEET_FILES};

fn write(dir: &Path, filename: &str, contents: &[u8]) {
    std::fs::write(dir.join(filename), contents).unwrap();
}

#[test]
fn missing_file_is_incomplete() {
    let dir = tempdir().unwrap();
    assert!(!is_complete_file(&dir.path().join("config.json")));
}

#[test]
fn zero_byte_file_is_incomplete() {
    let dir = tempdir().unwrap();
    write(dir.path(), "config.json", b"");
    assert!(!is_complete_file(&dir.path().join("config.json")));
}

#[test]
fn nonempty_file_is_complete() {
    let dir = tempdir().unwrap();
    write(dir.path(), "config.json", b"{}");
    assert!(is_complete_file(&dir.path().join("config.json")));
}

#[test]
fn parakeet_needs_every_file_including_the_external_encoder_weights() {
    let dir = tempdir().unwrap();
    for name in PARAKEET_FILES {
        write(dir.path(), name, b"x");
    }
    assert!(is_complete_model(dir.path(), SttEngine::SherpaParakeet));
    std::fs::remove_file(dir.path().join("encoder.int8.onnx.data")).unwrap();
    assert!(!is_complete_model(dir.path(), SttEngine::SherpaParakeet));
}
