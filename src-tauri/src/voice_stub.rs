//! `voice` commands and state for builds without the `voice` feature: the commands are
//! registered either way and answer that voice control is not built in.

use crate::error::{HolziError, Result};

/// State placeholder for builds without voice support. Instance
/// lifecycle commands still receive this state so they can invalidate
/// the cache uniformly across feature combinations.
#[derive(Default)]
pub struct VoiceState;

impl VoiceState {
    pub fn new() -> Self {
        Self
    }

    pub(crate) async fn invalidate_stt_cache(&self) {}

    pub(crate) async fn reset_for_close(&self) {}

    pub(crate) fn cancel_for_close(&self) {}
}

fn voice_disabled() -> HolziError {
    HolziError::InvalidInput {
        reason: "voice control is not enabled in this build".into(),
    }
}

#[tauri::command]
pub async fn start_voice_recording() -> Result<()> {
    Err(voice_disabled())
}

#[tauri::command]
pub async fn stop_voice_recording() -> Result<super::TranscriptionResultWire> {
    Err(voice_disabled())
}

#[tauri::command]
pub async fn cancel_voice_recording() -> Result<()> {
    Err(voice_disabled())
}

/// Successful no-op — there is no cache to invalidate when `voice`
/// isn't compiled in at all, and the frontend must be able to call
/// this unconditionally after a Settings switch (spec 010).
#[tauri::command]
pub async fn invalidate_stt_model_cache() -> Result<()> {
    Ok(())
}
