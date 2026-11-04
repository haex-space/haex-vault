//! Spec 081 T007: which speech model speaks a language.

use std::collections::HashMap;

use super::catalog::TtsEngine;
use super::choice::{resolve, Device, Speeds, Unmeasured, QWEN_PRESUMED_GPU_BYTES};

const QWEN: &str = "qwen3-tts-1.7b";

struct Measured(HashMap<&'static str, bool>);

impl Speeds for Measured {
    fn keeps_up(&self, model_id: &str) -> Option<bool> {
        self.0.get(model_id).copied()
    }
}

fn all_engines(_: TtsEngine) -> bool {
    true
}

fn gpu(bytes: u64) -> Device {
    Device {
        gpu_free_bytes: Some(bytes),
    }
}

fn installed(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| (*id).to_owned()).collect()
}

#[test]
fn without_a_choice_pocket_speaks_and_is_proposed_on_a_processor() {
    let r = resolve(
        "de",
        None,
        &installed(&["pocket-de"]),
        all_engines,
        Device::default(),
        &Unmeasured,
    )
    .expect("resolves");
    assert!(r.chosen.is_none());
    assert_eq!(r.proposed.id, "pocket-de");
    assert_eq!(r.speaking.id, "pocket-de");
}

#[test]
fn qwen_is_proposed_on_a_roomy_graphics_card_before_any_measurement() {
    let r = resolve(
        "de",
        None,
        &installed(&["pocket-de"]),
        all_engines,
        gpu(QWEN_PRESUMED_GPU_BYTES),
        &Unmeasured,
    )
    .expect("resolves");
    assert_eq!(r.proposed.id, QWEN);
    // Proposed, not downloaded: Pocket keeps speaking until the user takes it (US5).
    assert_eq!(r.speaking.id, "pocket-de");

    let small = resolve(
        "de",
        None,
        &[],
        all_engines,
        gpu(QWEN_PRESUMED_GPU_BYTES - 1),
        &Unmeasured,
    )
    .expect("resolves");
    assert_eq!(small.proposed.id, "pocket-de");
}

#[test]
fn a_measurement_beats_the_presumption() {
    let slow = Measured(HashMap::from([(QWEN, false)]));
    let r = resolve("de", None, &[], all_engines, gpu(8 << 30), &slow).expect("resolves");
    assert_eq!(r.proposed.id, "pocket-de");

    let fast = Measured(HashMap::from([(QWEN, true)]));
    let r = resolve("de", None, &[], all_engines, Device::default(), &fast).expect("resolves");
    assert_eq!(r.proposed.id, QWEN);
}

#[test]
fn the_chosen_model_speaks_once_it_is_installed() {
    let before = resolve(
        "de",
        Some(QWEN),
        &installed(&["pocket-de"]),
        all_engines,
        Device::default(),
        &Unmeasured,
    )
    .expect("resolves");
    assert_eq!(before.chosen.map(|e| e.id.as_str()), Some(QWEN));
    assert_eq!(before.speaking.id, "pocket-de");

    let after = resolve(
        "de",
        Some(QWEN),
        &installed(&["pocket-de", QWEN]),
        all_engines,
        Device::default(),
        &Unmeasured,
    )
    .expect("resolves");
    assert_eq!(after.speaking.id, QWEN);
    // A choice is kept even where the model is slow (US5 scenario 3).
    assert_eq!(after.proposed.id, "pocket-de");
}

#[test]
fn a_choice_this_build_cannot_speak_counts_as_automatic() {
    let pocket_only = |engine: TtsEngine| engine == TtsEngine::Pocket;
    let r = resolve(
        "de",
        Some(QWEN),
        &installed(&[QWEN, "pocket-de"]),
        pocket_only,
        gpu(8 << 30),
        &Unmeasured,
    )
    .expect("resolves");
    assert!(r.chosen.is_none());
    assert_eq!(r.proposed.id, "pocket-de");
    assert_eq!(r.speaking.id, "pocket-de");
}

#[test]
fn a_choice_of_another_language_is_ignored() {
    let r = resolve(
        "de",
        Some("pocket-en"),
        &[],
        all_engines,
        Device::default(),
        &Unmeasured,
    )
    .expect("resolves");
    assert!(r.chosen.is_none());
}

#[test]
fn qwen_speaks_every_language_and_french_falls_back_to_its_only_pocket_model() {
    for language in super::LANGUAGES {
        let r = resolve(
            language,
            Some(QWEN),
            &installed(&[QWEN]),
            all_engines,
            Device::default(),
            &Unmeasured,
        )
        .expect("resolves");
        assert_eq!(r.speaking.id, QWEN, "{language}");
    }
    let fr =
        resolve("fr", None, &[], all_engines, Device::default(), &Unmeasured).expect("resolves");
    assert_eq!(fr.speaking.id, "pocket-fr-24l");
}

#[test]
fn an_unknown_language_has_no_speech_model() {
    assert!(resolve("nl", None, &[], all_engines, Device::default(), &Unmeasured).is_err());
}
