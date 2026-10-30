use super::imp::{capped_event_payload, TranscriptionResultWire, VoiceState};
use crate::stt::interrupt::InterruptCommand;

#[test]
fn capped_event_payload_keeps_success_and_drops_failure() {
    let payload = capped_event_payload(Ok(TranscriptionResultWire {
        text: "hello".into(),
        interrupt: Some(InterruptCommand::Stop),
    }));
    let payload = payload.expect("successful cap outcome should be emitted");
    assert_eq!(payload.text, "hello");
    assert_eq!(payload.interrupt, Some(InterruptCommand::Stop));

    assert!(capped_event_payload(Err(crate::error::HolziError::NotRecording)).is_none());
}

/// Spec 010: `invalidate_stt_model_cache` must be a successful, panic-free
/// no-op in every feature combination, including a fresh state that never
/// had anything cached — real cache-*population* isn't unit-testable here
/// (it requires either a Tauri `AppHandle`, which this codebase has no
/// mocking convention for, or a real downloaded Whisper model, which needs
/// network — see the `#[ignore]`d tests in `stt/local_tests.rs`).
#[tokio::test]
async fn invalidate_stt_cache_is_a_safe_noop_on_a_fresh_state() {
    let state = VoiceState::new();
    assert!(!state.has_cached_stt_adapter().await);
    state.invalidate_stt_cache().await;
    assert!(!state.has_cached_stt_adapter().await);
}

/// Spec 010 FR-007: a device that never sets `voice.stt_model_id` must
/// resolve to exactly the same default this feature always had.
mod stt_catalog_entry_resolution {
    use super::super::imp::resolve_stt_catalog_entry_from_pref;

    #[test]
    fn missing_preference_falls_back_to_whisper_tiny() {
        assert_eq!(
            resolve_stt_catalog_entry_from_pref(None)
                .expect("default catalog entry")
                .id,
            "whisper-tiny"
        );
    }

    #[test]
    fn empty_preference_falls_back_to_whisper_tiny() {
        assert_eq!(
            resolve_stt_catalog_entry_from_pref(Some(String::new()))
                .expect("default catalog entry")
                .id,
            "whisper-tiny"
        );
    }

    #[test]
    fn unknown_preference_falls_back_to_whisper_tiny() {
        assert_eq!(
            resolve_stt_catalog_entry_from_pref(Some("does-not-exist".to_string()))
                .expect("default catalog entry")
                .id,
            "whisper-tiny"
        );
    }

    #[test]
    fn valid_preference_resolves_to_that_entry() {
        assert_eq!(
            resolve_stt_catalog_entry_from_pref(Some("whisper-base".to_string()))
                .expect("valid catalog entry")
                .id,
            "whisper-base"
        );
    }
}

/// The close releases the microphone and the cached model. With nothing recording and nothing
/// cached it changes nothing, and repeating it is harmless (a real capture needs an audio device).
#[tokio::test]
async fn reset_for_close_is_a_safe_noop_on_a_fresh_state_and_idempotent() {
    let state = VoiceState::new();

    state.reset_for_close().await;
    state.reset_for_close().await;

    assert!(!state.has_cached_stt_adapter().await);
}
