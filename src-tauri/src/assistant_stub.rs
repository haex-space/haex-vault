//! `assistant` commands for builds without the `voice` feature: registered either way, they
//! answer that the voice assistant is not built in (contracts/tauri-commands.md).
//!
//! The word list is compiled either way: the stop words of spec 008 and Parakeet's language
//! candidates read it (research R22).

#[path = "assistant/lexicon.rs"]
pub mod lexicon;

#[path = "assistant/transcript.rs"]
pub mod transcript;

pub mod assets {
    use crate::error::{HolziError, Result};

    fn voice_disabled() -> HolziError {
        HolziError::InvalidInput {
            reason: "the voice assistant is not enabled in this build".into(),
        }
    }

    #[tauri::command]
    pub async fn assistant_readiness(_language: String) -> Result<()> {
        Err(voice_disabled())
    }

    #[tauri::command]
    pub async fn download_assistant_assets(_language: String) -> Result<()> {
        Err(voice_disabled())
    }
}

pub mod tones {
    use crate::error::Result;

    #[tauri::command]
    pub async fn assistant_tone_preview(
        _tone: String,
        _volume: u8,
        _custom_id: Option<String>,
    ) -> Result<()> {
        Err(super::voice_disabled())
    }
}

pub mod custom_tones {
    use crate::error::Result;
    use crate::files::picked::PickedFile;

    #[tauri::command]
    pub async fn assistant_tones_list() -> Result<()> {
        Err(super::voice_disabled())
    }

    #[tauri::command]
    pub async fn assistant_tone_import(_file: PickedFile) -> Result<()> {
        Err(super::voice_disabled())
    }

    #[tauri::command]
    pub async fn assistant_tone_assign(_tone: String, _custom_id: Option<String>) -> Result<()> {
        Err(super::voice_disabled())
    }

    #[tauri::command]
    pub async fn assistant_tone_delete(_custom_id: String) -> Result<()> {
        Err(super::voice_disabled())
    }
}

fn voice_disabled() -> crate::error::HolziError {
    crate::error::HolziError::InvalidInput {
        reason: "the voice assistant is not enabled in this build".into(),
    }
}

pub mod commands {
    use crate::error::{HolziError, Result};

    fn voice_disabled() -> HolziError {
        HolziError::InvalidInput {
            reason: "the voice assistant is not enabled in this build".into(),
        }
    }

    /// Managed either way so `lib.rs` registers it the same in every build.
    #[derive(Default)]
    pub struct AssistantDriver {
        _private: (),
    }

    #[tauri::command]
    pub async fn assistant_get_state() -> Result<()> {
        Err(voice_disabled())
    }

    #[tauri::command]
    pub async fn assistant_set_enabled(
        _enabled: bool,
        _language: String,
        _utc_offset_minutes: i32,
    ) -> Result<()> {
        Err(voice_disabled())
    }

    #[tauri::command]
    pub async fn assistant_apply_speech_model(_language: String) -> Result<()> {
        Err(voice_disabled())
    }

    #[tauri::command]
    pub async fn assistant_end_conversation() -> Result<()> {
        Err(voice_disabled())
    }

    #[tauri::command]
    pub async fn assistant_set_muted(_muted: bool) -> Result<()> {
        Err(voice_disabled())
    }
}

pub mod wakeword {
    use crate::error::Result;

    #[tauri::command]
    pub async fn assistant_wake_word_model() -> Result<()> {
        Err(super::voice_disabled())
    }

    #[tauri::command]
    pub async fn assistant_wake_word_model_download() -> Result<()> {
        Err(super::voice_disabled())
    }
}

pub mod speakers {
    use crate::error::Result;

    #[tauri::command]
    pub async fn assistant_speakers_list() -> Result<()> {
        Err(super::voice_disabled())
    }

    #[tauri::command]
    pub async fn assistant_speaker_model() -> Result<()> {
        Err(super::voice_disabled())
    }

    #[tauri::command]
    pub async fn assistant_speaker_model_download() -> Result<()> {
        Err(super::voice_disabled())
    }

    #[tauri::command]
    pub async fn assistant_speaker_add_sentence(
        _session_id: uuid::Uuid,
        _generation: u64,
    ) -> Result<()> {
        Err(super::voice_disabled())
    }

    #[tauri::command]
    pub async fn assistant_speaker_enrolment_reset() -> Result<u64> {
        Err(super::voice_disabled())
    }

    #[tauri::command]
    pub async fn assistant_speaker_save(_name: String) -> Result<()> {
        Err(super::voice_disabled())
    }

    #[tauri::command]
    pub async fn assistant_speaker_rename(_id: uuid::Uuid, _name: String) -> Result<()> {
        Err(super::voice_disabled())
    }

    #[tauri::command]
    pub async fn assistant_speaker_delete(_id: uuid::Uuid) -> Result<()> {
        Err(super::voice_disabled())
    }
}
