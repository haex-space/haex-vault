//! Spec 066 T142 (command part): saving needs the consent, a name and a language haex speaks, and
//! the check reaches the window in the shape of the contract.

use uuid::Uuid;

use super::*;

#[test]
fn saving_needs_consent_a_name_and_a_known_language() {
    assert_eq!(checked_save("  Anna ", "de", true), Ok("Anna".to_owned()));
    assert_eq!(checked_save("Anna", "de", false), Err("consentRequired"));
    assert_eq!(checked_save("   ", "de", true), Err("nameMissing"));
    assert_eq!(
        checked_save(&"x".repeat(MAX_NAME_CHARS + 1), "de", true),
        Err("nameTooLong")
    );
    assert!(checked_save(&"ü".repeat(MAX_NAME_CHARS), "de", true).is_ok());
    assert_eq!(checked_save("Anna", "pl", true), Err("unknownLanguage"));
}

#[test]
fn the_check_has_the_shape_of_the_contract() {
    let check = CloneCheck {
        session_id: Uuid::nil(),
        speech_secs: 12.5,
        ok: false,
        problems: vec![Problem::Noisy],
    };
    let wire = serde_json::to_value(&check).expect("json");
    assert_eq!(
        wire,
        serde_json::json!({
            "sessionId": Uuid::nil().to_string(),
            "speechSecs": 12.5,
            "ok": false,
            "problems": ["noisy"],
        })
    );
    let level = serde_json::to_value(CloneLevel {
        session_id: Uuid::nil(),
        level: 0.5,
        speech_secs: 1.0,
    })
    .expect("json");
    assert_eq!(level["sessionId"], Uuid::nil().to_string());
    assert_eq!(level["level"], 0.5);
}

#[cfg(feature = "voice")]
#[test]
fn a_recording_is_stored_at_the_rate_of_the_speech_model_at_most() {
    let second = |rate: u32| vec![0.1f32; rate as usize];
    let (samples, rate) = stored_reference(&second(48_000), 48_000);
    assert_eq!((samples.len(), rate), (24_000, STORED_RATE_MAX));
    let (samples, rate) = stored_reference(&second(44_100), 44_100);
    assert_eq!((samples.len(), rate), (24_000, STORED_RATE_MAX));
    // Lower rates stay as they are; the speech model converts them itself.
    let (samples, rate) = stored_reference(&second(16_000), 16_000);
    assert_eq!((samples.len(), rate), (16_000, 16_000));
}
