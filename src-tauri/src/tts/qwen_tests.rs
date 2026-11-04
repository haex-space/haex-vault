//! Spec 081 T014–T017: the parts of Qwen3-TTS that need no model files.

use super::catalog::get;
use super::local::Reference;
use super::qwen::generate::{frame_cap, seed_for, FRAMES_PER_SECOND};
use super::qwen::{fingerprint, Placement, QwenModel};

fn reference(samples: Vec<f32>) -> Reference {
    Reference {
        samples,
        sample_rate: 24_000,
        language: "de".to_owned(),
    }
}

#[test]
fn the_frame_cap_grows_with_the_text_and_allows_a_slow_start() {
    assert!(frame_cap(0) as f64 >= 1.5 * FRAMES_PER_SECOND);
    assert!(frame_cap(100) > frame_cap(10));
    // A 100-character part at the measured pace (about 7 s) is far below its cap.
    assert!(frame_cap(100) as f64 > 2.0 * 7.0 * FRAMES_PER_SECOND);
}

#[test]
fn the_same_text_gets_the_same_seed() {
    assert_eq!(seed_for("Guten Morgen."), seed_for("Guten Morgen."));
    assert_ne!(seed_for("Guten Morgen."), seed_for("Guten Abend."));
}

#[test]
fn the_voice_print_key_follows_the_content_not_the_allocation() {
    let a = reference(vec![0.1, 0.2, 0.3]);
    let b = reference(vec![0.1, 0.2, 0.3]);
    let c = reference(vec![0.1, 0.2, 0.4]);
    assert_eq!(fingerprint(&a), fingerprint(&b));
    assert_ne!(fingerprint(&a), fingerprint(&c));
}

#[test]
fn missing_files_are_an_error_not_a_crash() {
    let dir = tempfile::tempdir().expect("temp dir");
    let entry = get("qwen3-tts-1.7b").expect("in the catalog");
    let error = QwenModel::load(dir.path(), entry, Placement::Cpu)
        .err()
        .expect("nothing to load");
    assert!(error.to_string().contains("backbone"), "{error}");
}
