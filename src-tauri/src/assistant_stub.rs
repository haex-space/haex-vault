//! `assistant` commands for builds without the `voice` feature: registered either way, they
//! answer that the voice assistant is not built in (contracts/tauri-commands.md).
//!
//! The word list is compiled either way: the stop words of spec 008 and Whisper's language
//! candidates read it (research R22).

#[path = "assistant/lexicon.rs"]
pub mod lexicon;

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
    use crate::error::{HolziError, Result};

    #[tauri::command]
    pub async fn assistant_tone_preview(_tone: String, _volume: u8) -> Result<()> {
        Err(HolziError::InvalidInput {
            reason: "the voice assistant is not enabled in this build".into(),
        })
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
    pub async fn assistant_talk(_language: String, _utc_offset_minutes: i32) -> Result<()> {
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
