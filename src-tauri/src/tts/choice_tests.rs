//! Spec 081 T007, T041: which speech model speaks a language.

use super::catalog::TtsEngine;
use super::choice::resolve;

const QWEN: &str = "qwen3-tts-1.7b";

fn all_engines(_: TtsEngine) -> bool {
    true
}

fn pocket_only(engine: TtsEngine) -> bool {
    engine == TtsEngine::Pocket
}

fn installed(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| (*id).to_owned()).collect()
}

#[test]
fn qwen_is_the_default_and_pocket_speaks_until_it_is_downloaded() {
    let before = resolve(
        "de",
        None,
        &installed(&["pocket-de"]),
        all_engines,
        all_engines,
    )
    .expect("resolves");
    assert!(before.chosen.is_none());
    assert_eq!(before.default.id, QWEN);
    assert_eq!(before.wanted().id, QWEN);
    assert_eq!(before.speaking.id, "pocket-de");

    let after = resolve(
        "de",
        None,
        &installed(&["pocket-de", QWEN]),
        all_engines,
        all_engines,
    )
    .expect("resolves");
    assert_eq!(after.speaking.id, QWEN);
}

#[test]
fn a_build_without_qwen_defaults_to_pocket() {
    let r = resolve(
        "de",
        Some(QWEN),
        &installed(&[QWEN, "pocket-de"]),
        pocket_only,
        all_engines,
    )
    .expect("resolves");
    assert!(
        r.chosen.is_none(),
        "a choice this build cannot speak counts as none"
    );
    assert_eq!(r.default.id, "pocket-de");
    assert_eq!(r.speaking.id, "pocket-de");
}

#[test]
fn the_chosen_model_speaks_once_it_is_installed() {
    let r = resolve(
        "de",
        Some("pocket-de"),
        &installed(&["pocket-de", QWEN]),
        all_engines,
        all_engines,
    )
    .expect("resolves");
    assert_eq!(r.chosen.map(|e| e.id.as_str()), Some("pocket-de"));
    assert_eq!(r.wanted().id, "pocket-de");
    assert_eq!(
        r.speaking.id, "pocket-de",
        "the user's choice wins over the default"
    );

    let missing = resolve("de", Some(QWEN), &[], all_engines, all_engines).expect("resolves");
    assert_eq!(missing.wanted().id, QWEN);
    assert_eq!(missing.speaking.id, "pocket-de");
}

#[test]
fn a_choice_of_another_language_is_ignored() {
    let r = resolve("de", Some("pocket-en"), &[], all_engines, all_engines).expect("resolves");
    assert!(r.chosen.is_none());
}

#[test]
fn qwen_speaks_every_language_and_french_falls_back_to_its_only_pocket_model() {
    for language in super::LANGUAGES {
        let r = resolve(
            language,
            None,
            &installed(&[QWEN]),
            all_engines,
            all_engines,
        )
        .expect("resolves");
        assert_eq!(r.speaking.id, QWEN, "{language}");
    }
    let fr = resolve("fr", None, &[], all_engines, all_engines).expect("resolves");
    assert_eq!(fr.speaking.id, "pocket-fr-24l");
}

#[test]
fn an_unknown_language_has_no_speech_model() {
    assert!(resolve("nl", None, &[], all_engines, all_engines).is_err());
}

/// Spec 081 FR-010a (operator 2026-11-05): the language's Pocket model is always installed beside
/// another speech model as its reserve; a Pocket model is its own.
#[test]
fn every_other_speech_model_brings_the_languages_pocket_model() {
    use super::catalog::get;
    use super::commands::reserve_needed;
    let qwen = get(QWEN).expect("in the catalog");
    for language in super::LANGUAGES {
        let reserve = reserve_needed(qwen, language)
            .expect("parses")
            .expect("a reserve");
        assert_eq!(reserve.engine, TtsEngine::Pocket);
        assert!(reserve.speaks(language), "{language}");
    }
    let pocket = get("pocket-de").expect("in the catalog");
    assert!(reserve_needed(pocket, "de").expect("parses").is_none());
}

/// Spec 081 FR-012 (operator 2026-11-05): where Qwen3-TTS may not be the default (Android), Pocket
/// speaks without a choice, and Qwen3-TTS can still be chosen.
#[test]
fn where_qwen_may_not_be_the_default_pocket_is_and_qwen_stays_selectable() {
    let r = resolve(
        "de",
        None,
        &installed(&["pocket-de", QWEN]),
        all_engines,
        pocket_only,
    )
    .expect("resolves");
    assert_eq!(r.default.id, "pocket-de");
    assert_eq!(r.speaking.id, "pocket-de");
    let chosen = resolve(
        "de",
        Some(QWEN),
        &installed(&["pocket-de", QWEN]),
        all_engines,
        pocket_only,
    )
    .expect("resolves");
    assert_eq!(chosen.chosen.map(|e| e.id.as_str()), Some(QWEN));
    assert_eq!(chosen.speaking.id, QWEN);
}

#[test]
fn pocket_is_the_default_everywhere_and_qwen_only_by_choice() {
    use super::commands::engine_may_default;
    assert!(engine_may_default(TtsEngine::Pocket));
    assert!(
        !engine_may_default(TtsEngine::Qwen3Tts),
        "operator 2026-11-06: Pocket by default, Qwen3-TTS by choice"
    );
}

/// Operator 2026-11-06: installed speech models can be deleted again, except the Pocket models, the
/// reserve of their language (FR-010a).
#[test]
fn every_speech_model_but_pocket_may_be_deleted() {
    use super::commands::may_delete;
    assert!(may_delete(TtsEngine::Qwen3Tts));
    assert!(!may_delete(TtsEngine::Pocket));
}
