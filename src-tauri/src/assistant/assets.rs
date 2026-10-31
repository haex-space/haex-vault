//! What the assistant needs before its mode can be switched on (spec 066 FR-005, T025), and the
//! models that come without a choice.
//!
//! The pause detection model ships in the binary (212 KB) and is written next to the downloaded
//! models on first use. The default wake word model follows with PR D; until it exists the
//! readiness check reports it missing, which is what FR-005 asks for.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::error::{HolziError, Result};
use crate::state::AppState;
use crate::tts;

/// Silero VAD, int8 (`models/README.md`).
pub const SILERO_VAD: &[u8] = include_bytes!("models/silero_vad.int8.onnx");
const SILERO_VAD_FILE: &str = "silero_vad.int8.onnx";
/// `models::paths` slug the pause detection model is written to.
const SILERO_VAD_SLUG: &str = "silero-vad";

/// `Missing` of contracts/tauri-commands.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Missing {
    SttModel,
    VadModel,
    WakeWordModel,
    Voice,
    ChatModel,
    Microphone,
}

/// What is in place on this device.
#[derive(Debug, Clone, Copy, Default)]
pub struct Facts {
    pub stt_model: bool,
    pub vad_model: bool,
    pub wake_word_model: bool,
    pub voice: bool,
    pub chat_model: bool,
    pub microphone: bool,
}

/// Everything that is not in place, in the order of [`Missing`].
pub fn missing(facts: &Facts) -> Vec<Missing> {
    [
        (facts.stt_model, Missing::SttModel),
        (facts.vad_model, Missing::VadModel),
        (facts.wake_word_model, Missing::WakeWordModel),
        (facts.voice, Missing::Voice),
        (facts.chat_model, Missing::ChatModel),
        (facts.microphone, Missing::Microphone),
    ]
    .into_iter()
    .filter(|(present, _)| !present)
    .map(|(_, item)| item)
    .collect()
}

/// The language whose voice the assistant needs: `language` if haex speaks it, else English.
pub fn voice_language(language: &str) -> &'static str {
    let primary = language.split(['-', '_']).next().unwrap_or_default();
    tts::LANGUAGES
        .into_iter()
        .find(|l| l.eq_ignore_ascii_case(primary))
        .unwrap_or("en")
}

/// Writes the pause detection model into `dir` unless an identical copy is there; returns its
/// path.
pub fn write_vad_model(dir: &Path) -> std::io::Result<PathBuf> {
    let path = dir.join(SILERO_VAD_FILE);
    if std::fs::read(&path).is_ok_and(|bytes| bytes == SILERO_VAD) {
        return Ok(path);
    }
    std::fs::create_dir_all(dir)?;
    let mut part = tempfile::NamedTempFile::new_in(dir)?;
    std::io::Write::write_all(&mut part, SILERO_VAD)?;
    part.persist(&path).map_err(|error| error.error)?;
    Ok(path)
}

/// The pause detection model's path on this device, written there if needed.
pub fn vad_model_path(app: &AppHandle) -> Result<PathBuf> {
    let dir = crate::models::paths::slug_dir(app, SILERO_VAD_SLUG)?;
    write_vad_model(&dir).map_err(|e| HolziError::InvalidInput {
        reason: format!("write the pause detection model: {e}"),
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct Readiness {
    pub missing: Vec<Missing>,
}

/// `language` is the window's language; it decides which voice is needed.
#[tauri::command]
pub async fn assistant_readiness(
    app: AppHandle,
    state: State<'_, AppState>,
    language: String,
) -> Result<Readiness> {
    let stt = crate::voice::resolve_stt_catalog_entry(&app).await?;
    let voice = tts::catalog::default_for(voice_language(&language))?;
    let app_for_disk = app.clone();
    let (stt_model, voice, vad_model) = tauri::async_runtime::spawn_blocking(move || {
        let stt_model = crate::stt::files::resolve_model_dir(&app_for_disk, stt)
            .is_ok_and(|dir| crate::stt::files::is_complete_model(&dir, stt.engine));
        let voice = voice.is_some_and(|entry| {
            tts::files::resolve_model_dir(&app_for_disk, entry)
                .is_ok_and(|dir| tts::files::is_complete_model(&dir, entry.engine))
        });
        (stt_model, voice, vad_model_path(&app_for_disk).is_ok())
    })
    .await
    .map_err(|e| HolziError::InvalidInput {
        reason: format!("check the assistant's models: {e}"),
    })?;
    let chat_model = crate::chat::default_model::resolve_default_model(app.clone(), state)
        .await
        .is_ok_and(|resolved| resolved.model_id.is_some());
    let facts = Facts {
        stt_model,
        vad_model,
        // PR D ships the default wake word model.
        wake_word_model: false,
        voice,
        chat_model,
        microphone: crate::audio::has_input_device(),
    };
    Ok(Readiness {
        missing: missing(&facts),
    })
}

/// Downloads what the assistant can fetch by itself: the selected speech recognition model and
/// the voice of `language`. Progress of the voice arrives as `assistant-download-progress`.
#[tauri::command]
pub async fn download_assistant_assets(
    app: AppHandle,
    state: State<'_, AppState>,
    language: String,
) -> Result<()> {
    let stt = crate::voice::resolve_stt_catalog_entry(&app).await?;
    crate::stt::commands::download_stt_model(app.clone(), state.clone(), stt.id.clone()).await?;
    if let Some(voice) = tts::catalog::default_for(voice_language(&language))? {
        state
            .gate()
            .run(tts::commands::ensure_installed(&app, &state, voice))
            .await??;
    }
    Ok(())
}
