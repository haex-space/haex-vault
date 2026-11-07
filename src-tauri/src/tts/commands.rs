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

/// Whether this build can speak with `engine` on this processor. Qwen3-TTS runs on llama.cpp, so it
/// needs the extensions ggml was built for, like a chat model (spec 070 R3).
fn engine_enabled(engine: TtsEngine) -> bool {
    match engine {
        TtsEngine::Pocket => cfg!(feature = "voice"),
        #[cfg(feature = "voice-qwen")]
        TtsEngine::Qwen3Tts => crate::llm::llamacpp::cpu::missing_features().is_empty(),
        #[cfg(not(feature = "voice-qwen"))]
        TtsEngine::Qwen3Tts => false,
    }
}

/// Whether `engine` may speak without a choice (spec 081 FR-012): only Pocket TTS, on every
/// platform (operator 2026-11-06). The onboarding offers it, so the assistant speaks right away;
/// Qwen3-TTS (2.3 GB, slower than real time on a processor) is the person's own choice.
pub fn engine_may_default(engine: TtsEngine) -> bool {
    match engine {
        TtsEngine::Pocket => true,
        TtsEngine::Qwen3Tts => false,
    }
}

/// The device preference naming the speech model of `language` (spec 081 data-model.md).
pub fn speech_model_pref_key(language: &str) -> String {
    format!("assistant.speechModel.{language}")
}

/// What speaks a conversation or a sample (spec 081).
#[cfg(feature = "voice")]
pub(crate) struct SpeechEngines {
    pub engine: std::sync::Arc<dyn super::local::Engine>,
    /// The language's Pocket model while the engine speaks in parts (FR-010a).
    pub reserve: Option<std::sync::Arc<dyn super::local::Engine>>,
    /// The catalog entry of `engine` and its directory.
    pub entry: &'static TtsCatalogEntry,
    pub dir: PathBuf,
    /// The entry asked for did not load and the language's Pocket model speaks instead.
    pub load_failed: bool,
}

/// The engine that speaks `entry` (installed in `dir`) in `language`, and its reserve: the
/// language's Pocket model while the engine speaks in parts (spec 081 FR-010a). When `entry` does
/// not load, the language's Pocket model speaks instead (operator 2026-11-05). Never downloads;
/// [`ensure_installed_with_reserve`] does that.
#[cfg(feature = "voice")]
pub(crate) async fn load_speech_engines(
    app: &AppHandle,
    gate: &crate::vault_gate::VaultGate,
    language: &str,
    entry: &'static TtsCatalogEntry,
    dir: PathBuf,
) -> Result<SpeechEngines> {
    let voice = app.state::<crate::voice::VoiceState>();
    let engine = match voice.speech_model(entry, dir.clone(), gate).await {
        Ok(engine) => engine,
        Err(error) => {
            if reserve_needed(entry, language)?.is_some() {
                if let Some((pocket, pocket_entry, pocket_dir)) =
                    installed_pocket(app, gate, language).await?
                {
                    log::warn!(
                        "speech: {} did not load, the Pocket model of {language} speaks: {error}",
                        entry.id
                    );
                    return Ok(SpeechEngines {
                        engine: pocket,
                        reserve: None,
                        entry: pocket_entry,
                        dir: pocket_dir,
                        load_failed: true,
                    });
                }
            }
            return Err(error);
        }
    };
    let reserve = match engine.part_words() {
        Some(_) => installed_pocket(app, gate, language)
            .await?
            .map(|(pocket, _, _)| pocket),
        None => None,
    };
    Ok(SpeechEngines {
        engine,
        reserve,
        entry,
        dir,
        load_failed: false,
    })
}

/// The language's Pocket model, loaded beside the speech model, with its entry and directory;
/// `None` when it is not installed or does not load.
#[cfg(feature = "voice")]
async fn installed_pocket(
    app: &AppHandle,
    gate: &crate::vault_gate::VaultGate,
    language: &str,
) -> Result<
    Option<(
        std::sync::Arc<dyn super::local::Engine>,
        &'static TtsCatalogEntry,
        PathBuf,
    )>,
> {
    let Some(entry) = catalog::pocket_for(language)? else {
        return Ok(None);
    };
    let app_for_dir = app.clone();
    let dir = tauri::async_runtime::spawn_blocking(move || {
        files::resolve_model_dir(&app_for_dir, entry)
            .ok()
            .filter(|dir| files::is_complete_model(dir, entry))
    })
    .await
    .map_err(|e| HolziError::SpeechFailed {
        reason: format!("look for the Pocket model of {language}: {e}"),
    })?;
    let Some(dir) = dir else {
        log::warn!("speech: the Pocket model of {language} is not installed");
        return Ok(None);
    };
    let voice = app.state::<crate::voice::VoiceState>();
    match voice.reserve_model(entry, dir.clone(), gate).await {
        Ok(pocket) => Ok(Some((pocket, entry, dir))),
        Err(error) => {
            log::warn!("speech: load the Pocket model of {language}: {error}");
            Ok(None)
        }
    }
}

/// Installs `entry` and the Pocket model of `language`, which is always there as the reserve
/// (spec 081 FR-010a, operator 2026-11-05).
pub(crate) async fn ensure_installed_with_reserve(
    app: &AppHandle,
    state: &AppState,
    entry: &'static TtsCatalogEntry,
    language: &str,
) -> Result<()> {
    if let Some(reserve) = reserve_needed(entry, language)? {
        ensure_installed(app, state, reserve).await?;
    }
    ensure_installed(app, state, entry).await
}

/// The Pocket model of `language` that `entry` brings as its reserve; none when `entry` is that
/// Pocket model.
pub fn reserve_needed(
    entry: &TtsCatalogEntry,
    language: &str,
) -> std::result::Result<Option<&'static TtsCatalogEntry>, TtsError> {
    Ok(catalog::pocket_for(language)?.filter(|pocket| pocket.id != entry.id))
}

/// The device preference of the speaking rate (spec 066 FR-030).
pub const PREF_SPEECH_RATE: &str = "assistant.speechRate";

/// The device preference naming the voice of `language` (spec 066 FR-029).
pub fn voice_pref_key(language: &str) -> String {
    format!("assistant.voice.{language}")
}

/// The voice chosen for `language` and the speaking rate, from the device preferences.
#[cfg(feature = "voice")]
pub(crate) async fn voice_settings(
    app: &AppHandle,
    language: &str,
) -> Result<(Option<String>, f32)> {
    let voice = device_pref(app, &voice_pref_key(language)).await?;
    let rate = device_pref(app, PREF_SPEECH_RATE)
        .await?
        .and_then(|v| v.trim().parse::<f32>().ok())
        .and_then(|v| super::checked_rate(v).ok())
        .unwrap_or(super::RATE_DEFAULT);
    Ok((voice, rate))
}

/// The speech models of `language` that speak now, as [`resolve_speech_model`] picks them, loaded:
/// the one that speaks without a download (spec 081 FR-012, FR-014) and its reserve. Never
/// downloads; a missing model is an error the settings offer the download for (FR-005).
#[cfg(feature = "voice")]
pub(crate) async fn load_installed_speech_engines(
    app: &AppHandle,
    gate: &crate::vault_gate::VaultGate,
    language: &str,
) -> Result<SpeechEngines> {
    Ok(load_installed(app, gate, language).await?.1)
}

/// [`load_installed_speech_engines`] with the resolution it started from.
#[cfg(feature = "voice")]
pub(crate) async fn load_installed(
    app: &AppHandle,
    gate: &crate::vault_gate::VaultGate,
    language: &str,
) -> Result<(Resolved, SpeechEngines)> {
    let resolved = resolve_speech_model(app, language).await?;
    let entry = resolved.speaking;
    let app_for_dir = app.clone();
    let dir = tauri::async_runtime::spawn_blocking(move || {
        files::resolve_model_dir(&app_for_dir, entry)
            .ok()
            .filter(|dir| files::is_complete_model(dir, entry))
    })
    .await
    .map_err(|e| HolziError::SpeechFailed {
        reason: format!("look for the speech model: {e}"),
    })?
    .ok_or_else(|| TtsError::Unavailable {
        reason: format!("the speech model for {language} is not downloaded"),
    })?;
    let engines = load_speech_engines(app, gate, language, entry, dir).await?;
    Ok((resolved, engines))
}

/// The speech models of `language` on this device (spec 081 `tts::choice`): the chosen one from
/// the device preference, the default, and the one that speaks now.
pub(crate) async fn resolve_speech_model(app: &AppHandle, language: &str) -> Result<Resolved> {
    let preference = device_pref(app, &speech_model_pref_key(language)).await?;
    let installed = list_installed_tts_voices(app.clone()).await?;
    Ok(choice::resolve(
        language,
        preference.as_deref(),
        &installed,
        engine_enabled,
        engine_may_default,
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
        models.push((language, resolved.wanted()));
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

/// Reads an answer of the agent aloud (spec 066 FR-022c): in the language it is written in, with
/// the speech model and voice chosen for that language. A newer reading or voice sample stops this
/// one. Resolves `true` once everything has sounded, `false` when it was stopped.
#[tauri::command]
pub async fn chat_message_speak(
    app: AppHandle,
    state: State<'_, AppState>,
    text: String,
    ui_language: String,
    utc_offset_minutes: i32,
) -> Result<bool> {
    #[cfg(feature = "voice")]
    {
        state
            .gate()
            .run(super::read_aloud::run(
                &app,
                &state,
                &text,
                &ui_language,
                utc_offset_minutes,
            ))
            .await?
    }
    #[cfg(not(feature = "voice"))]
    {
        let _ = (app, state, text, ui_language, utc_offset_minutes);
        Err(TtsError::Unavailable {
            reason: "speech output is not enabled in this build".to_owned(),
        }
        .into())
    }
}

/// Stops reading aloud (and a voice sample); `chat_message_speak` then resolves `false`.
#[tauri::command]
pub async fn chat_message_speak_stop(app: AppHandle) -> Result<()> {
    #[cfg(feature = "voice")]
    app.state::<crate::voice::VoiceState>().stop_preview();
    #[cfg(not(feature = "voice"))]
    let _ = app;
    Ok(())
}

/// Downloads (or repairs) the files of catalog entry `voice_id`, reporting progress as the shared
/// `model-download-progress` of every model download, so the settings show it with the same
/// components as chat and speech recognition models. Idempotent. With `language`, a model that
/// needs a reserve for it also gets that (spec 081 FR-010a).
#[tauri::command]
pub async fn download_tts_voice(
    app: AppHandle,
    state: State<'_, AppState>,
    voice_id: String,
    language: Option<String>,
) -> Result<()> {
    let entry = catalog::get(&voice_id)?;
    match language {
        Some(language) => {
            state
                .gate()
                .run(ensure_installed_with_reserve(
                    &app, &state, entry, &language,
                ))
                .await?
        }
        None => {
            state
                .gate()
                .run(ensure_installed(&app, &state, entry))
                .await?
        }
    }
}

/// Deletes the files of speech model `model_id` on this device (spec 081, operator 2026-11-06),
/// with its speed measurement. A Pocket model is refused: it is its language's reserve and always
/// stays installed (FR-010a). A preference naming the deleted model stays; the language's Pocket
/// model speaks until the model is downloaded again (US5). Idempotent.
#[tauri::command]
pub async fn delete_tts_model(
    app: AppHandle,
    state: State<'_, AppState>,
    model_id: String,
) -> Result<()> {
    let entry = catalog::get(&model_id)?;
    if !may_delete(entry.engine) {
        return Err(TtsError::Invalid {
            reason: format!("{} stays installed as its language's reserve", entry.id),
        }
        .into());
    }
    state.gate().run(remove_model(&app, &state, entry)).await?
}

/// Whether the files of a model of `engine` may be deleted: every model but Pocket, the reserve of
/// its language (FR-010a).
pub fn may_delete(engine: TtsEngine) -> bool {
    engine != TtsEngine::Pocket
}

async fn remove_model(
    app: &AppHandle,
    state: &AppState,
    entry: &'static TtsCatalogEntry,
) -> Result<()> {
    let _publication_lock =
        crate::models::paths::acquire_model_publication_lock(app, &entry.id, &state.gate().token())
            .await?;
    #[cfg(feature = "voice")]
    app.state::<crate::voice::VoiceState>()
        .forget_speech_model(&entry.id)
        .await;
    let app_for_dir = app.clone();
    let dir =
        tauri::async_runtime::spawn_blocking(move || files::resolve_model_dir(&app_for_dir, entry))
            .await
            .map_err(|e| HolziError::SpeechFailed {
                reason: format!("resolve speech model directory: {e}"),
            })??;
    match tokio::fs::remove_dir_all(&dir).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(HolziError::Io {
            reason: format!("remove {}: {e}", dir.display()),
        }),
    }
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

/// Event when a slow speech model could make way for a faster one (spec 081 FR-011).
pub const EVENT_SPEECH_MODEL_HINT: &str = "tts-speech-model-hint";
/// Event when Pocket TTS speaks instead of the chosen model (spec 081 contracts).
pub const EVENT_SPEECH_MODEL_FALLBACK: &str = "tts-speech-model-fallback";
/// Device preference that silences the hint for good (spec 081 data-model.md).
pub const PREF_SPEECH_MODEL_HINT: &str = "assistant.speechModelHint";

/// Payload of [`EVENT_SPEECH_MODEL_HINT`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechModelHint {
    pub language: String,
    pub slow_model_id: String,
    pub faster_model_id: String,
}

/// Why Pocket TTS speaks instead of the chosen model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FallbackReason {
    /// Not enough memory to load it (with the graphics card ledger, spec 081 T044).
    Memory,
    NotInstalled,
    LoadFailed,
    /// The selected model failed speech checks and the reserve spoke the part.
    PartFailed,
}

/// Payload of [`EVENT_SPEECH_MODEL_FALLBACK`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechModelFallback {
    pub reason: FallbackReason,
    pub model_id: String,
}

/// Tells the window that Pocket TTS speaks instead of `chosen`.
pub fn emit_fallback(app: &AppHandle, reason: FallbackReason, chosen: &TtsCatalogEntry) {
    let _ = app.emit(
        EVENT_SPEECH_MODEL_FALLBACK,
        SpeechModelFallback {
            reason,
            model_id: chosen.id.clone(),
        },
    );
}

/// The hint is offered at most once per app run (spec 081 T035).
static HINT_OFFERED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Stores the speed of the answer (`sentences`, taken from the meter when the answer ended) for
/// `model` and, when the model did not keep up in the third answer in a row or later, offers the
/// language's Pocket model once (FR-011). haex never switches by itself.
pub async fn answer_spoken(
    app: &AppHandle,
    meter: std::sync::Arc<super::speed::Meter>,
    sentences: Vec<super::speed::SentenceSpeed>,
    model: &'static TtsCatalogEntry,
    language: &str,
) {
    let reached = tauri::async_runtime::spawn_blocking(move || meter.store_answer(sentences))
        .await
        .unwrap_or(false);
    if !reached {
        return;
    }
    match speech_model_hint(app, model, language).await {
        Ok(Some(hint)) => {
            let _ = app.emit(EVENT_SPEECH_MODEL_HINT, hint);
        }
        Ok(None) => {}
        Err(error) => log::warn!("speech: offer a faster speech model: {error}"),
    }
}

/// The hint for `model` in `language`, unless the language's Pocket model is the model itself,
/// is not installed, does not keep up either, the hint was dismissed, or it was offered in this
/// run already.
async fn speech_model_hint(
    app: &AppHandle,
    model: &'static TtsCatalogEntry,
    language: &str,
) -> Result<Option<SpeechModelHint>> {
    if HINT_OFFERED.load(std::sync::atomic::Ordering::Acquire) {
        return Ok(None);
    }
    let Some(pocket) = reserve_needed(model, language)? else {
        return Ok(None);
    };
    let app_for_dir = app.clone();
    // `None`: Pocket is not (completely) here, so switching to it would fail.
    let pocket_speed = tauri::async_runtime::spawn_blocking(move || {
        let dir = files::resolve_model_dir(&app_for_dir, pocket).ok()?;
        files::is_complete_model(&dir, pocket).then(|| super::speed::of_model(&dir, "cpu").speed())
    })
    .await
    .unwrap_or(None);
    if pocket_speed.is_none_or(|speed| speed == super::speed::Speed::Slow) {
        return Ok(None);
    }
    if device_pref(app, PREF_SPEECH_MODEL_HINT).await?.as_deref() == Some("dismissed") {
        return Ok(None);
    }
    if HINT_OFFERED.swap(true, std::sync::atomic::Ordering::AcqRel) {
        return Ok(None);
    }
    Ok(Some(SpeechModelHint {
        language: language.to_owned(),
        slow_model_id: model.id.clone(),
        faster_model_id: pocket.id.clone(),
    }))
}

/// A device preference of this vault.
async fn device_pref(app: &AppHandle, key: &str) -> Result<Option<String>> {
    let state = app.state::<AppState>();
    let db = crate::state_utils::active_database(&state)?;
    let device = crate::device::commands::resolve_vault_device_uuid(app, &db).await?;
    let key = key.to_owned();
    db.read(move |r| {
        crate::storage::preferences::get(
            r,
            crate::storage::preferences::PrefScope::Device(device),
            &key,
        )
    })
    .await
}

/// One speech model the settings page offers for a language (spec 081
/// contracts/tauri-commands.md).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechModelOption {
    pub id: String,
    pub name: String,
    pub engine: TtsEngine,
    pub languages: Vec<String>,
    pub license: String,
    pub size_bytes: u64,
    pub installed: bool,
    pub fit: hardware::Fit,
    pub speed: super::speed::Speed,
    pub ratio: Option<f64>,
    pub acceleration: &'static str,
    pub chosen: bool,
    pub is_default: bool,
}

/// The speech models that speak `language` in this build, best-sounding first, with what is
/// installed, how each fits and how fast each spoke here (`run_of`: the measurement of an entry on
/// its acceleration).
pub fn speech_model_options(
    language: &str,
    resolved: &Resolved,
    installed: &[String],
    hw: &hardware::HardwareInfo,
    run_of: impl Fn(&TtsCatalogEntry) -> super::speed::Run,
) -> std::result::Result<Vec<SpeechModelOption>, TtsError> {
    Ok(catalog::for_language(language)?
        .into_iter()
        .filter(|entry| engine_enabled(entry.engine))
        .map(|entry| {
            let run = run_of(entry);
            SpeechModelOption {
                id: entry.id.clone(),
                name: entry.name.clone(),
                engine: entry.engine,
                languages: entry.languages.clone(),
                license: entry.license.clone(),
                size_bytes: entry.approx_size_bytes,
                installed: installed.contains(&entry.id),
                fit: catalog::fit(entry, hw),
                speed: run.speed(),
                ratio: run.ratio(),
                // Both engines run on the processor until the graphics card ledger (T044).
                acceleration: "cpu",
                chosen: resolved.chosen.is_some_and(|c| c.id == entry.id),
                is_default: resolved.default.id == entry.id,
            }
        })
        .collect())
}

/// The speech models of `language` for the settings page (spec 081 T010, T036).
#[tauri::command]
pub async fn tts_speech_models(app: AppHandle, language: String) -> Result<Vec<SpeechModelOption>> {
    let hw = hardware::probe_async().await;
    let resolved = resolve_speech_model(&app, &language).await?;
    let installed = list_installed_tts_voices(app.clone()).await?;
    let app_for_dirs = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        speech_model_options(&language, &resolved, &installed, &hw, |entry| {
            files::resolve_model_dir(&app_for_dirs, entry)
                .map(|dir| super::speed::of_model(&dir, "cpu"))
                .unwrap_or_default()
        })
    })
    .await
    .map_err(|e| HolziError::CrdtInit {
        reason: format!("list speech models: {e}"),
    })?
    .map_err(Into::into)
}
