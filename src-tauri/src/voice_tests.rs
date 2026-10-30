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

/// A device that never sets `voice.stt_model_id` gets the default of its kind:
/// Parakeet on the desktop (spec 066), the smallest model on a phone (spec 043
/// FR-030).
mod stt_catalog_entry_resolution {
    use super::super::imp::resolve_stt_catalog_entry_from_pref;
    use crate::platform::ModelPresets;

    fn resolve(pref: Option<&str>, presets: ModelPresets) -> String {
        resolve_stt_catalog_entry_from_pref(pref.map(str::to_owned), presets)
            .expect("catalog entry")
            .id
            .clone()
    }

    #[test]
    fn missing_empty_or_unknown_preference_falls_back_to_parakeet_on_the_desktop() {
        for pref in [None, Some(""), Some("does-not-exist")] {
            assert_eq!(resolve(pref, ModelPresets::Desktop), "parakeet-primeline");
        }
    }

    #[test]
    fn missing_empty_or_unknown_preference_falls_back_to_whisper_tiny_on_a_phone() {
        for pref in [None, Some(""), Some("does-not-exist")] {
            assert_eq!(resolve(pref, ModelPresets::Phone), "whisper-tiny");
        }
    }

    #[test]
    fn valid_preference_resolves_to_that_entry() {
        assert_eq!(
            resolve(Some("whisper-base"), ModelPresets::Desktop),
            "whisper-base"
        );
        assert_eq!(
            resolve(Some("parakeet-primeline"), ModelPresets::Phone),
            "parakeet-primeline"
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
