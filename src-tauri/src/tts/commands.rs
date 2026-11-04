//! Speech catalog commands (spec 066 T022, contracts/tauri-commands.md): list the catalog, list
//! what is installed, download a model. Which voice a language speaks with is a device preference
//! (`assistant.voice.<sprache>`) written through `set_pref`, not a command here; so is the speech
//! model of a language (`assistant.speechModel.<sprache>`, spec 081), resolved by
//! [`resolve_speech_model`].

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::{HolziError, Result};
use crate::hardware;
use crate::state::AppState;

use super::catalog::{self, TtsCatalogEntry, TtsCatalogEntryWithFit, TtsEngine};
use super::choice::{self, Resolved};
use super::files;
use super::voices::{self, BuiltinVoice, VoiceRef};
use super::TtsError;
use crate::storage::tts_voices::{self, OwnVoice};

/// Whether this build can speak with `engine`.
fn engine_enabled(engine: TtsEngine) -> bool {
    match engine {
        TtsEngine::Pocket => cfg!(feature = "voice"),
        TtsEngine::Qwen3Tts => cfg!(feature = "voice-qwen"),
    }
}

/// The device preference naming the speech model of `language` (spec 081 data-model.md).
pub fn speech_model_pref_key(language: &str) -> String {
    format!("assistant.speechModel.{language}")
}

/// Free memory of a graphics card this build could run Qwen3-TTS on.
fn gpu_free_bytes() -> Option<u64> {
    #[cfg(feature = "voice-qwen")]
    {
        super::qwen::gpu_free_bytes()
    }
    #[cfg(not(feature = "voice-qwen"))]
    {
        None
    }
}

/// The speech models of `language` on this device (spec 081 `tts::choice`): the chosen one from
/// the device preference, the automatic proposal, and the one that speaks now.
pub(crate) async fn resolve_speech_model(app: &AppHandle, language: &str) -> Result<Resolved> {
    let state = app.state::<AppState>();
    let db = crate::state_utils::active_database(&state)?;
    let device = crate::device::commands::resolve_vault_device_uuid(app, &db).await?;
    let key = speech_model_pref_key(language);
    let preference = db
        .read(move |r| {
            crate::storage::preferences::get(
                r,
                crate::storage::preferences::PrefScope::Device(device),
                &key,
            )
        })
        .await?;
    let installed = list_installed_tts_voices(app.clone()).await?;
    let gpu_free_bytes = tauri::async_runtime::spawn_blocking(gpu_free_bytes)
        .await
        .unwrap_or(None);
    Ok(choice::resolve(
        language,
        preference.as_deref(),
        &installed,
        engine_enabled,
        choice::Device { gpu_free_bytes },
        &choice::Unmeasured,
    )?)
}

#[tauri::command]
pub async fn list_tts_catalog() -> Result<Vec<TtsCatalogEntryWithFit>> {
    let hw = hardware::probe_async().await;
    Ok(catalog::entries_with_fit(&hw)?)
}

/// The ids of `entries` whose files are complete; `resolve_dir` returning `None` counts as not
/// installed. Without a Tauri `AppHandle` so the tests can pass plain directories.
pub fn scan_installed(
    entries: &[TtsCatalogEntry],
    resolve_dir: impl Fn(&TtsCatalogEntry) -> Option<PathBuf>,
) -> Vec<String> {
    entries
        .iter()
        .filter(|entry| engine_enabled(entry.engine))
        .filter(|entry| resolve_dir(entry).is_some_and(|dir| files::is_complete_model(&dir, entry)))
        .map(|entry| entry.id.clone())
        .collect()
}

#[tauri::command]
pub async fn list_installed_tts_voices(app: AppHandle) -> Result<Vec<String>> {
    let entries = catalog::entries()?;
    tauri::async_runtime::spawn_blocking(move || {
        scan_installed(entries, |entry| files::resolve_model_dir(&app, entry).ok())
    })
    .await
    .map_err(|e| HolziError::CrdtInit {
        reason: format!("scan installed speech models: {e}"),
    })
}

/// Where a voice comes from (contracts/tauri-commands.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VoiceKind {
    Builtin,
    Clone,
    Model,
}

/// One voice the settings page offers. `installed`, `size_bytes` and `fit` describe the speech
/// model the voice needs: a built-in voice is a recording, the model of its language does the
/// speaking.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceOption {
    #[serde(rename = "ref")]
    pub voice_ref: String,
    pub kind: VoiceKind,
    pub name: String,
    pub language: String,
    /// Catalog entry of the speech model the voice needs.
    pub model_id: String,
    pub installed: bool,
    pub size_bytes: u64,
    pub fit: hardware::Fit,
    pub is_default: bool,
}

/// The built-in voices, then the own voices of the vault (spec 066 FR-049), as options, given
/// which catalog entries are installed and which speech model speaks each language on this device
/// (`model_for`: the chosen one, else the one that speaks now, spec 081). An own voice speaks
/// through the speech model of its language like a built-in one.
pub fn voice_options(
    builtin: &[BuiltinVoice],
    own: &[OwnVoice],
    installed: &[String],
    hw: &hardware::HardwareInfo,
    model_for: impl Fn(&str) -> std::result::Result<&'static TtsCatalogEntry, TtsError>,
) -> std::result::Result<Vec<VoiceOption>, TtsError> {
    let option = |voice_ref: VoiceRef,
                  kind: VoiceKind,
                  name: &str,
                  language: &str,
                  is_default: bool|
     -> std::result::Result<VoiceOption, TtsError> {
        let model = model_for(language)?;
        Ok(VoiceOption {
            voice_ref: voice_ref.to_string(),
            kind,
            name: name.to_owned(),
            language: language.to_owned(),
            model_id: model.id.clone(),
            installed: installed.contains(&model.id),
            size_bytes: model.approx_size_bytes,
            fit: catalog::fit(model, hw),
            is_default,
        })
    };
    let builtin = builtin.iter().map(|voice| {
        option(
            VoiceRef::Builtin(voice.id.clone()),
            VoiceKind::Builtin,
            &voice.name,
            &voice.language,
            voice.default,
        )
    });
    // A voice of a language this build does not speak (a newer device's) is left out.
    let own = own
        .iter()
        .filter(|voice| super::LANGUAGES.contains(&voice.language.as_str()))
        .map(|voice| {
            option(
                VoiceRef::Clone(voice.id.to_string()),
                VoiceKind::Clone,
                &voice.name,
                &voice.language,
                false,
            )
        });
    builtin.chain(own).collect()
}

#[tauri::command]
pub async fn tts_voices_list(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<VoiceOption>> {
    let hw = hardware::probe_async().await;
    let db = crate::state_utils::active_database(&state)?;
    let own = db.read(|r| tts_voices::list(r)).await?;
    let installed = list_installed_tts_voices(app.clone()).await?;
    voice_options_on_device(&app, voices::builtin()?, &own, &installed, &hw).await
}

/// [`voice_options`] with the speech model each language speaks with on this device.
pub(crate) async fn voice_options_on_device(
    app: &AppHandle,
    builtin: &[BuiltinVoice],
    own: &[OwnVoice],
    installed: &[String],
    hw: &hardware::HardwareInfo,
) -> Result<Vec<VoiceOption>> {
    let mut models = Vec::with_capacity(super::LANGUAGES.len());
    for language in super::LANGUAGES {
        let resolved = resolve_speech_model(app, language).await?;
        models.push((language, resolved.chosen.unwrap_or(resolved.speaking)));
    }
    let model_for = |language: &str| {
        models
            .iter()
            .find(|(l, _)| *l == language)
            .map(|(_, entry)| *entry)
            .ok_or_else(|| TtsError::Unavailable {
                reason: format!("no speech model speaks {language}"),
            })
    };
    Ok(voice_options(builtin, own, installed, hw, model_for)?)
}

/// The recording of the own voice `preference` names, if it is one and still in the vault.
pub async fn own_recording(
    state: &AppState,
    preference: Option<&str>,
) -> Result<Option<voices::OwnRecording>> {
    let Some(id) = voices::own_id(preference) else {
        return Ok(None);
    };
    let db = state.database()?;
    db.read(move |r| tts_voices::recording(r, &id)).await
}

/// Speaks a short sample with `voice` (a `VoiceRef` of `tts_voices_list`) at `rate` (spec 066
/// T028, FR-030), downloading the speech model of its language first. A newer sample stops this
/// one. Resolves when the sample has played or was stopped.
#[tauri::command]
pub async fn assistant_tts_preview(
    app: AppHandle,
    state: State<'_, AppState>,
    voice: String,
    rate: f32,
) -> Result<()> {
    let own = own_recording(&state, Some(&voice)).await?;
    let voice = voices::preview_choice(&voice, own)?;
    let rate = super::checked_rate(rate)?;
    #[cfg(feature = "voice")]
    {
        state
            .gate()
            .run(super::preview::run(&app, &state, voice, rate))
            .await?
    }
    #[cfg(not(feature = "voice"))]
    {
        let _ = (app, state, voice, rate);
        Err(TtsError::Unavailable {
            reason: "speech output is not enabled in this build".to_owned(),
        }
        .into())
    }
}

/// Downloads (or repairs) the files of catalog entry `voice_id`, reporting progress as the shared
/// `model-download-progress` of every model download, so the settings show it with the same
/// components as chat and speech recognition models. Idempotent.
#[tauri::command]
pub async fn download_tts_voice(
    app: AppHandle,
    state: State<'_, AppState>,
    voice_id: String,
) -> Result<()> {
    let entry = catalog::get(&voice_id)?;
    state
        .gate()
        .run(ensure_installed(&app, &state, entry))
        .await?
}

/// Makes sure `entry` is installed, downloading what is missing. Shared with
/// `download_assistant_assets`.
pub(crate) async fn ensure_installed(
    app: &AppHandle,
    state: &AppState,
    entry: &'static TtsCatalogEntry,
) -> Result<()> {
    if !engine_enabled(entry.engine) {
        return Err(TtsError::Unavailable {
            reason: format!("{:?} is not enabled in this build", entry.engine),
        }
        .into());
    }
    let _publication_lock =
        crate::models::paths::acquire_model_publication_lock(app, &entry.id, &state.gate().token())
            .await?;
    let app_for_dir = app.clone();
    let dir =
        tauri::async_runtime::spawn_blocking(move || files::resolve_model_dir(&app_for_dir, entry))
            .await
            .map_err(|e| HolziError::CrdtInit {
                reason: format!("resolve speech model directory: {e}"),
            })??;
    files::ensure_model_files(&dir, entry, |received, total| {
        let _ = app.emit(
            crate::models::commands::EVENT_PROGRESS,
            crate::models::commands::ProgressEvent {
                model_id: entry.id.clone(),
                bytes_downloaded: received,
                bytes_total: Some(total),
            },
        );
    })
    .await?;
    Ok(())
}
