//! `LocalWhisperAdapter` against a fixed, checked-in audio fixture with a
//! known transcript (T010, spec 008 research.md §7 "Testing"). No real
//! microphone is involved. Downloading the real model *is* — see the
//! `#[ignore]` reason below, a direct consequence of the operator's
//! 2026-09-17 decision to fetch the bundled model on first use rather than
//! bundle it in the installer (see the module doc on `local.rs`).

use std::path::Path;

use super::catalog::SttCatalogEntry;
use super::files::ensure_model_files;
use super::fixture_wav::read_fixture_wav;
use super::local::LocalWhisperAdapter;
use super::{CanonicalPcm, SttAdapter};

/// Cross-checked against `librosa.filters.mel(sr=16000, n_fft=400,
/// n_mels=80, norm="slaney", htk=False)` — the values Whisper's
/// preprocessing was trained against.
#[test]
fn mel_filterbank_matches_known_shape_and_support() {
    let n_mels = 80;
    let n_freqs = 400 / 2 + 1;
    let filters = super::local::mel_filterbank(16_000, 400, n_mels);
    assert_eq!(filters.len(), n_mels * n_freqs);
    assert!(filters.iter().all(|&w| w.is_finite() && w >= 0.0));

    let row0 = &filters[0..n_freqs];
    let first_nonzero = row0.iter().position(|&w| w > 0.0).unwrap();
    assert!(first_nonzero <= 1);

    let row_last = &filters[(n_mels - 1) * n_freqs..n_mels * n_freqs];
    let last_nonzero = row_last.iter().rposition(|&w| w > 0.0).unwrap();
    assert!(last_nonzero >= n_freqs - 20);
}

fn whisper_tiny_entry() -> SttCatalogEntry {
    super::catalog::get("whisper-tiny")
        .expect("catalog should parse")
        .expect("whisper-tiny catalog entry")
        .clone()
}

#[tokio::test]
#[ignore = "downloads the real whisper-tiny model from Hugging Face on first \
            run (~75MB) and runs real CPU inference — a direct consequence \
            of downloading the bundled model on first use instead of \
            shipping it in the installer. Run explicitly with \
            `cargo test --features llm-cpu -- --ignored transcribes_known_fixture`."]
async fn transcribes_known_fixture() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/stt/fixtures/jfk.wav");
    let pcm = CanonicalPcm {
        samples: read_fixture_wav(&fixture),
    };

    let model_dir = std::env::temp_dir().join("holzi-whisper-tiny-test-cache");
    std::fs::create_dir_all(&model_dir).unwrap();
    ensure_model_files(&model_dir, &whisper_tiny_entry(), |_| {})
        .await
        .expect("failed to fetch whisper-tiny fixture cache");

    let adapter =
        LocalWhisperAdapter::load_from_dir(&model_dir).expect("failed to load whisper-tiny");
    let text = adapter
        .transcribe(&pcm)
        .await
        .expect("transcription failed")
        .to_lowercase();

    // The JFK inaugural excerpt ("ask not what your country can do for
    // you, ask what you can do for your country") — checking for the
    // distinctive content words rather than an exact string match, since
    // greedy decoding without punctuation/casing normalization can differ
    // in capitalization or minor phrasing from the canonical transcript.
    assert!(text.contains("country"), "unexpected transcript: {text}");
    assert!(text.contains("ask"), "unexpected transcript: {text}");
}

#[tokio::test]
#[ignore = "downloads the real whisper-tiny model from Hugging Face on first \
            run (~75MB) and runs real CPU inference — a direct consequence \
            of downloading the bundled model on first use instead of \
            shipping it in the installer. Run explicitly with \
            `cargo test --features llm-cpu -- --ignored transcribes_german_fixture`."]
async fn transcribes_german_fixture() {
    // 9.5s clip trimmed from a public-domain LibriVox German reading
    // (grimm_maerchen_1_librivox, archive.org) — lands on the standard
    // German LibriVox disclaimer read before the story itself ("Alle
    // LibriVox-Aufnahmen sind... öffentlichem Besitz..."), not the fairy
    // tale text, but that's immaterial here: it's unambiguous German.
    // Added 2026-09-17 as a regression test for `detect_language`
    // misidentifying spoken German as English when scored against
    // Whisper's full 99-language set (see `SUPPORTED_LANGUAGES` in
    // `local.rs`).
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/stt/fixtures/de_sample.wav");
    let pcm = CanonicalPcm {
        samples: read_fixture_wav(&fixture),
    };

    let model_dir = std::env::temp_dir().join("holzi-whisper-tiny-test-cache");
    std::fs::create_dir_all(&model_dir).unwrap();
    ensure_model_files(&model_dir, &whisper_tiny_entry(), |_| {})
        .await
        .expect("failed to fetch whisper-tiny fixture cache");

    let adapter =
        LocalWhisperAdapter::load_from_dir(&model_dir).expect("failed to load whisper-tiny");
    let text = adapter
        .transcribe(&pcm)
        .await
        .expect("transcription failed")
        .to_lowercase();

    assert!(text.contains("librivox"), "unexpected transcript: {text}");
    assert!(
        text.contains("öffentlichem"),
        "unexpected transcript: {text}"
    );
}
