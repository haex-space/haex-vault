//! `assistant` commands for builds without the `voice` feature: registered either way, they
//! answer that the voice assistant is not built in (contracts/tauri-commands.md).

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
