//! Spec 066 T032: every voice and speech model is credited on the page "Lizenzen".

use super::credits::{credits, CreditKind};
use super::{catalog, voices};

#[test]
fn every_voice_and_speech_model_is_credited_once() {
    let all = credits().expect("credits");
    let named = |kind: CreditKind| -> Vec<String> {
        all.iter()
            .filter(|c| c.kind == kind)
            .map(|c| c.name.clone())
            .collect()
    };
    let voice_names: Vec<_> = voices::builtin()
        .expect("voices")
        .iter()
        .map(|v| v.name.clone())
        .collect();
    assert_eq!(named(CreditKind::Voice), voice_names);
    let model_names: Vec<_> = catalog::entries()
        .expect("catalog")
        .iter()
        .map(|e| e.name.clone())
        .collect();
    assert_eq!(named(CreditKind::SpeechModel), model_names);
    let recognition: Vec<_> = crate::stt::catalog::entries()
        .expect("stt catalog")
        .iter()
        .filter(|e| e.attribution.is_some())
        .map(|e| e.name.clone())
        .collect();
    assert!(
        !recognition.is_empty(),
        "parakeet-primeline asks for a credit"
    );
    assert_eq!(named(CreditKind::RecognitionModel), recognition);
}

#[test]
fn every_credit_names_its_licence_author_and_source() {
    for credit in credits().expect("credits") {
        assert!(!credit.attribution.trim().is_empty(), "{}", credit.name);
        assert!(credit.source.starts_with("https://"), "{}", credit.name);
        assert!(
            ["CC0-1.0", "CC-BY-4.0"].contains(&credit.license.as_str())
                || credit.kind == CreditKind::RecognitionModel,
            "{}: {}",
            credit.name,
            credit.license
        );
        assert_eq!(
            credit.language.is_some(),
            credit.kind != CreditKind::RecognitionModel,
            "{}",
            credit.name
        );
    }
    let wire = serde_json::to_value(&credits().expect("credits")[0]).expect("serializes");
    for key in [
        "kind",
        "name",
        "language",
        "license",
        "attribution",
        "source",
    ] {
        assert!(wire.get(key).is_some(), "{key}");
    }
    assert_eq!(wire["kind"], "voice");
}
