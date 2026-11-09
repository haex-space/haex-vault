use std::path::{Path, PathBuf};

use super::fixture_wav::read_fixture_wav;
use super::sherpa::ParakeetAdapter;
use super::{CanonicalPcm, SttAdapter, SttError};

#[test]
fn a_missing_model_is_unavailable_not_a_crash() {
    let empty = tempfile::tempdir().expect("temp dir");
    assert!(matches!(
        ParakeetAdapter::load_from_dir(empty.path()),
        Err(SttError::LocalUnavailable { .. })
    ));
}

/// `HOLZI_PARAKEET_DIR` points at an unpacked `sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8`.
fn model_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOLZI_PARAKEET_DIR").expect("HOLZI_PARAKEET_DIR is set"))
}

fn fixture(name: &str) -> CanonicalPcm {
    CanonicalPcm {
        samples: read_fixture_wav(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src/stt/fixtures")
                .join(name),
        ),
    }
}

#[tokio::test]
#[ignore = "needs the Parakeet model (~490 MB) in HOLZI_PARAKEET_DIR"]
async fn transcribes_german_and_english_fixtures() {
    let adapter = ParakeetAdapter::load_from_dir(&model_dir()).expect("model loads");

    let german = adapter
        .transcribe(&fixture("de_sample.wav"))
        .await
        .expect("German transcribes")
        .to_lowercase();
    assert!(german.contains("librivox"), "{german}");

    let english = adapter
        .transcribe(&fixture("jfk.wav"))
        .await
        .expect("English transcribes")
        .to_lowercase();
    assert!(
        english.contains("ask not what your country can do for you"),
        "{english}"
    );
}

#[tokio::test]
#[ignore = "needs the Parakeet model (~490 MB) in HOLZI_PARAKEET_DIR"]
async fn silence_gives_no_text() {
    let adapter = ParakeetAdapter::load_from_dir(&model_dir()).expect("model loads");
    let silence = CanonicalPcm {
        samples: vec![0.0; 3 * 16_000],
    };
    assert_eq!(adapter.transcribe(&silence).await.expect("runs"), "");
}

#[tokio::test]
#[ignore = "needs the Parakeet model (~490 MB) in HOLZI_PARAKEET_DIR"]
async fn too_little_audio_gives_no_text_instead_of_a_crash() {
    let adapter = ParakeetAdapter::load_from_dir(&model_dir()).expect("model loads");
    for len in [0, 1, 160, super::sherpa::MIN_SAMPLES - 1] {
        let audio = CanonicalPcm {
            samples: vec![0.01; len],
        };
        assert_eq!(adapter.transcribe(&audio).await.expect("no error"), "");
    }
    // From the minimum on the encoder gets frames and runs.
    for len in [super::sherpa::MIN_SAMPLES, super::sherpa::MIN_SAMPLES + 1] {
        let audio = CanonicalPcm {
            samples: vec![0.01; len],
        };
        adapter.transcribe(&audio).await.expect("no error");
    }
}
