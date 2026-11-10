//! Spec 066 T022: which files make a speech model complete and where they come from.

use tempfile::tempdir;

use super::catalog::{get, CatalogFile, TtsCatalogEntry};
use super::files::{
    ensure_model_files, file_url, files_for, is_complete_model, pending_files, verify_file, Pending,
};

#[tokio::test]
async fn a_model_is_complete_only_with_every_file() {
    let dir = tempdir().expect("temp dir");
    let entry = get("pocket-de").expect("known");
    let files = files_for(entry);
    for (i, name) in files.iter().enumerate() {
        assert!(!is_complete_model(dir.path(), entry), "{i}");
        let contents: &[u8] = if name.ends_with(".json") { b"{}" } else { b"x" };
        std::fs::write(dir.path().join(name), contents).expect("write");
    }
    assert!(is_complete_model(dir.path(), entry));
    let mut progress = Vec::new();
    ensure_model_files(dir.path(), entry, |done, total| {
        progress.push((done, total))
    })
    .await
    .expect("complete model needs no downloads");
    assert_eq!(progress, vec![(9, entry.approx_size_bytes)]);
}

#[test]
fn a_truncated_json_file_makes_the_model_incomplete() {
    let dir = tempdir().expect("temp dir");
    let entry = get("pocket-de").expect("known");
    for name in files_for(entry) {
        let contents: &[u8] = if name.ends_with(".json") { b"{}" } else { b"x" };
        std::fs::write(dir.path().join(name), contents).expect("write");
    }
    std::fs::write(dir.path().join("vocab.json"), b"{\"a\":").expect("write");
    assert!(!is_complete_model(dir.path(), entry));
}

#[test]
fn files_come_from_the_entry_folder_at_the_pinned_revision() {
    let entry = get("pocket-de").expect("known");
    assert_eq!(
        file_url(entry, "encoder.onnx"),
        format!(
            "https://huggingface.co/haex-space/pocket-tts-sherpa-onnx/resolve/{}/de/encoder.onnx",
            entry.hf_revision
        )
    );
}

#[test]
fn only_complete_models_count_as_installed() {
    use super::catalog::entries;
    use super::commands::scan_installed;

    let complete = tempdir().expect("temp dir");
    for name in files_for(get("pocket-de").expect("known")) {
        let contents: &[u8] = if name.ends_with(".json") { b"{}" } else { b"x" };
        std::fs::write(complete.path().join(name), contents).expect("write");
    }
    let empty = tempdir().expect("temp dir");
    let installed = scan_installed(entries().expect("parses"), |entry| {
        match entry.id.as_str() {
            "pocket-de" => Some(complete.path().to_path_buf()),
            "pocket-en" => Some(empty.path().to_path_buf()),
            _ => None,
        }
    });
    let expected: &[&str] = if cfg!(feature = "voice") {
        &["pocket-de"]
    } else {
        &[]
    };
    assert_eq!(installed, expected);
}

/// An entry that pins its files, as Qwen3-TTS does (spec 081 FR-003).
fn hashed_entry(contents: &[u8]) -> TtsCatalogEntry {
    let mut entry = get("pocket-de").expect("known").clone();
    entry.hf_folder = String::new();
    entry.files = Some(vec![CatalogFile {
        name: "model.gguf".to_owned(),
        bytes: contents.len() as u64,
        sha256: crate::models::hash::sha256_bytes_hex(contents),
    }]);
    entry
}

#[test]
fn a_pinned_file_counts_only_after_its_check() {
    let dir = tempdir().expect("temp dir");
    let entry = hashed_entry(b"weights");
    let path = dir.path().join("model.gguf");
    std::fs::write(&path, b"weights").expect("write");
    assert!(!is_complete_model(dir.path(), &entry), "not checked yet");
    let hash = entry.files.as_ref().expect("files")[0].sha256.clone();
    verify_file(&path, "model.gguf", &hash).expect("matches");
    assert!(is_complete_model(dir.path(), &entry));
}

#[test]
fn a_damaged_pinned_file_is_deleted() {
    let dir = tempdir().expect("temp dir");
    let entry = hashed_entry(b"weights");
    let path = dir.path().join("model.gguf");
    std::fs::write(&path, b"damaged").expect("write");
    let hash = entry.files.as_ref().expect("files")[0].sha256.clone();
    assert!(verify_file(&path, "model.gguf", &hash).is_err());
    assert!(!path.exists());
    assert!(!is_complete_model(dir.path(), &entry));
}

#[tokio::test]
async fn a_written_but_unchecked_file_is_checked_not_fetched() {
    let dir = tempdir().expect("temp dir");
    let entry = hashed_entry(b"weights");
    std::fs::write(dir.path().join("model.gguf"), b"weights").expect("write");
    ensure_model_files(dir.path(), &entry, |_, _| {})
        .await
        .expect("checks the file without a download");
    assert!(is_complete_model(dir.path(), &entry));
}

#[test]
fn a_file_checked_against_an_earlier_pin_is_fetched_again() {
    let dir = tempdir().expect("temp dir");
    let entry = hashed_entry(b"new weights");
    let path = dir.path().join("model.gguf");
    std::fs::write(&path, b"old weights").expect("write");
    std::fs::write(dir.path().join("model.gguf.sha256"), "an earlier hash").expect("marker");
    let (done, pending) = pending_files(dir.path(), &entry);
    assert_eq!(done, 0);
    assert_eq!(pending, vec![("model.gguf".to_owned(), Pending::Fetch)]);
    assert!(
        !dir.path().join("model.gguf.sha256").exists(),
        "the stale marker is gone"
    );
}

#[test]
fn a_written_file_without_a_marker_is_only_checked() {
    let dir = tempdir().expect("temp dir");
    let entry = hashed_entry(b"weights");
    std::fs::write(dir.path().join("model.gguf"), b"weights").expect("write");
    let (_, pending) = pending_files(dir.path(), &entry);
    assert_eq!(
        pending,
        vec![("model.gguf".to_owned(), Pending::Check { bytes: 7 })]
    );
}

#[test]
fn files_of_the_repository_root_have_no_folder() {
    let entry = hashed_entry(b"weights");
    assert_eq!(
        file_url(&entry, "model.gguf"),
        format!(
            "https://huggingface.co/haex-space/pocket-tts-sherpa-onnx/resolve/{}/model.gguf",
            entry.hf_revision
        )
    );
}
