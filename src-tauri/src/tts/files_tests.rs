//! Spec 066 T022: which files make a speech model complete and where they come from.

use tempfile::tempdir;

use super::catalog::{get, TtsEngine};
use super::files::{ensure_model_files, file_url, files_for, is_complete_model};

#[tokio::test]
async fn a_model_is_complete_only_with_every_file() {
    let dir = tempdir().expect("temp dir");
    let files = files_for(TtsEngine::Pocket);
    for (i, name) in files.iter().enumerate() {
        assert!(!is_complete_model(dir.path(), TtsEngine::Pocket), "{i}");
        let contents: &[u8] = if name.ends_with(".json") { b"{}" } else { b"x" };
        std::fs::write(dir.path().join(name), contents).expect("write");
    }
    assert!(is_complete_model(dir.path(), TtsEngine::Pocket));
    let entry = get("pocket-de").expect("known");
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
    for name in files_for(TtsEngine::Pocket) {
        let contents: &[u8] = if name.ends_with(".json") { b"{}" } else { b"x" };
        std::fs::write(dir.path().join(name), contents).expect("write");
    }
    std::fs::write(dir.path().join("vocab.json"), b"{\"a\":").expect("write");
    assert!(!is_complete_model(dir.path(), TtsEngine::Pocket));
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
    for name in files_for(TtsEngine::Pocket) {
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
