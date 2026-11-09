//! Own voices (spec 066 FR-049, FR-050, FR-053, contracts/tauri-commands.md, T146): record a voice
//! or read it from an audio file, check it ([`super::clone`]), hear a sample, and keep it with the
//! user's confirmation that it is their voice or the speaker consented; delete it again.
//!
//! One recording session at a time: a new one ends the last. A recording runs on the microphone
//! only between `tts_clone_record_start` and `tts_clone_record_stop`, i.e. while the dialog is
//! open, and reads from the assistant's stream when that runs (as push-to-talk does). Closing the
//! vault ends it. Recording, reading files and samples need the `voice` feature; listing and
//! deleting own voices work in every build, since they sync to every device.
//!
//! None of these commands is an action, an agent tool or an extension API (FR-050, T144): only the
//! user makes or deletes a voice, from the settings page.

use serde::Serialize;
use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::error::{HolziError, Result};
use crate::files::picked::PickedFile;
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::storage::tts_voices;

use super::clone::Problem;
use super::commands::VoiceOption;
use super::voices::VoiceRef;
use super::LANGUAGES;

/// Event with the level of a running recording (`{sessionId, level, speechSecs}`).
pub const EVENT_CLONE_LEVEL: &str = "tts-clone-level";
/// The longest name of an own voice.
pub const MAX_NAME_CHARS: usize = 80;
/// The largest audio file read for a voice.
pub const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;
/// Stored recordings have at most this rate, the speech model's own (research R20: 30 s at
/// 24 kHz ≈ 1.4 MB).
pub const STORED_RATE_MAX: u32 = 24_000;

/// The kept part of a recording as it is stored: above [`STORED_RATE_MAX`] brought down to it
/// through a low-pass (the speech model imitates the sound of its reference, folded-back highs
/// included), at or below it as it is; the speech model converts a lower rate itself.
#[cfg(feature = "voice")]
pub fn stored_reference(kept: &[f32], rate: u32) -> (Vec<f32>, u32) {
    if rate > STORED_RATE_MAX {
        (
            crate::audio::resample::resample_filtered(kept, rate, STORED_RATE_MAX),
            STORED_RATE_MAX,
        )
    } else {
        (kept.to_vec(), rate)
    }
}

/// The answer of `tts_clone_record_stop` and `tts_clone_from_file`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneCheck {
    pub session_id: Uuid,
    pub speech_secs: f64,
    pub ok: bool,
    pub problems: Vec<Problem>,
}

/// Payload of [`EVENT_CLONE_LEVEL`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneLevel {
    pub session_id: Uuid,
    /// 0 (silence) to 1 (full scale).
    pub level: f32,
    /// Speech heard so far, roughly.
    pub speech_secs: f64,
}

/// The `sessionId` of `tts_clone_record_start`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneSession {
    pub session_id: Uuid,
}

/// The checked input of `tts_clone_save`: the trimmed name. The error is the reason the window
/// names (`consentRequired`, `nameMissing`, `nameTooLong`, `unknownLanguage`).
pub fn checked_save(
    name: &str,
    language: &str,
    consent: bool,
) -> std::result::Result<String, &'static str> {
    if !consent {
        return Err("consentRequired");
    }
    let name = name.trim();
    if name.is_empty() {
        return Err("nameMissing");
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err("nameTooLong");
    }
    if !LANGUAGES.contains(&language) {
        return Err("unknownLanguage");
    }
    Ok(name.to_owned())
}

fn invalid(reason: &str) -> HolziError {
    HolziError::InvalidInput {
        reason: reason.to_owned(),
    }
}

#[cfg(not(feature = "voice"))]
fn no_voice() -> HolziError {
    super::TtsError::Unavailable {
        reason: "speech output is not enabled in this build".to_owned(),
    }
    .into()
}

#[cfg(feature = "voice")]
pub use imp::CloneSessions;
/// The enrolment of a person (spec 066 FR-016a) records its sentences with these sessions.
#[cfg(feature = "voice")]
pub(crate) use imp::{end as end_session, recording_of};

#[cfg(feature = "voice")]
mod imp {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use tauri::{AppHandle, Emitter, Manager};
    use uuid::Uuid;

    use super::{CloneCheck, CloneLevel, EVENT_CLONE_LEVEL};
    use crate::audio::Capture;
    use crate::error::{HolziError, Result};
    use crate::state::AppState;
    use crate::tts::clone::{analyse, live_level};
    use crate::voice::VoiceState;

    /// How often a running recording reports its level.
    const LEVEL_EVERY: Duration = Duration::from_millis(100);

    type SharedCapture = Arc<Mutex<Option<Capture>>>;

    enum Stage {
        Recording(SharedCapture),
        /// A file is read and checked; it holds the slot so an older import cannot take it.
        Reading,
        Ready {
            samples: Vec<f32>,
            sample_rate: u32,
            ok: bool,
        },
    }

    struct Session {
        id: Uuid,
        stage: Stage,
    }

    /// The one recording session of this process.
    #[derive(Default)]
    pub struct CloneSessions(Mutex<Option<Session>>);

    impl CloneSessions {
        fn lock(&self) -> std::sync::MutexGuard<'_, Option<Session>> {
            self.0.lock().unwrap_or_else(|e| e.into_inner())
        }

        /// Makes `next` the one session. A recording it replaces ends here: its microphone goes
        /// at once and its level task stops at the next tick.
        fn replace(&self, next: Session) {
            let previous = self.lock().replace(next);
            release(previous);
        }

        /// Ends session `id` unless a newer one replaced it.
        fn end(&self, id: Uuid) {
            let ended = {
                let mut sessions = self.lock();
                if sessions.as_ref().is_some_and(|s| s.id == id) {
                    sessions.take()
                } else {
                    None
                }
            };
            release(ended);
        }
    }

    /// Drops the capture of a recording session. Takes the capture's lock only after the
    /// registry's is released, as the level task does.
    fn release(session: Option<Session>) {
        if let Some(Session {
            stage: Stage::Recording(shared),
            ..
        }) = session
        {
            drop(shared.lock().unwrap_or_else(|e| e.into_inner()).take());
        }
    }

    fn not_a_session() -> HolziError {
        super::invalid("unknownSession")
    }

    pub async fn record_start(app: &AppHandle, state: &AppState) -> Result<Uuid> {
        crate::voice::ensure_microphone(app).await?;
        // The device itself at its own rate, even while the assistant listens: its stream carries
        // 16 kHz, and the speech model would imitate the missing highs. Only a device that cannot
        // be opened twice falls back to that stream.
        let capture = match Capture::start() {
            Ok(capture) => capture,
            Err(e) => match app.state::<VoiceState>().audio_stream().current() {
                Some(stream) => {
                    log::info!("own voice: record from the running stream: {e}");
                    Capture::from_stream(&stream)
                }
                None => {
                    log::warn!("own voice: open the microphone: {e}");
                    return Err(HolziError::DeviceUnavailable);
                }
            },
        };
        let id = Uuid::new_v4();
        let shared: SharedCapture = Arc::new(Mutex::new(Some(capture)));
        // A session that was still recording ends here, its microphone with it.
        app.state::<CloneSessions>().replace(Session {
            id,
            stage: Stage::Recording(shared.clone()),
        });
        let token = state.gate().token();
        let handle = app.clone();
        let started = state.gate().spawn(async move {
            let app = handle;
            let mut speech_secs = 0.0;
            let mut last_recorded = 0.0;
            loop {
                tokio::select! {
                    () = token.cancelled() => {
                        // The vault closes: the microphone goes at once.
                        drop(shared.lock().unwrap_or_else(|e| e.into_inner()).take());
                        return;
                    }
                    () = tokio::time::sleep(LEVEL_EVERY) => {}
                }
                let (tail, recorded) = {
                    let guard = shared.lock().unwrap_or_else(|e| e.into_inner());
                    match &*guard {
                        Some(capture) => capture.tail(LEVEL_EVERY.as_secs_f32()),
                        None => return,
                    }
                };
                // Past the recording cap nothing new arrives: no level, no more speech.
                let grew = recorded > last_recorded;
                last_recorded = recorded;
                let (level, speech) = if grew {
                    live_level(&tail)
                } else {
                    (0.0, false)
                };
                if speech {
                    speech_secs += LEVEL_EVERY.as_secs_f64();
                }
                let _ = app.emit(
                    EVENT_CLONE_LEVEL,
                    CloneLevel {
                        session_id: id,
                        level,
                        speech_secs,
                    },
                );
            }
        });
        if let Err(error) = started {
            // The vault closes: the recording ends before it began.
            app.state::<CloneSessions>().end(id);
            return Err(error);
        }
        Ok(id)
    }

    /// Checks `samples` off the async runtime and keeps the session ready for sample and save.
    async fn finish(
        app: &AppHandle,
        state: &AppState,
        id: Uuid,
        samples: Vec<f32>,
        sample_rate: u32,
    ) -> Result<CloneCheck> {
        let ((samples, stored_rate), analysis) = state
            .gate()
            .spawn_blocking(move || {
                let analysis = analyse(&samples, sample_rate);
                let kept = &samples[analysis.keep.clone()];
                (super::stored_reference(kept, sample_rate), analysis)
            })?
            .await
            .map_err(|e| HolziError::SpeechFailed {
                reason: format!("check the recording: {e}"),
            })?;
        let check = CloneCheck {
            session_id: id,
            speech_secs: analysis.speech_secs,
            ok: analysis.ok(),
            problems: analysis.problems.clone(),
        };
        let registry = app.state::<CloneSessions>();
        let mut sessions = registry.lock();
        // A newer session may have started meanwhile; this one is then gone.
        if sessions.as_ref().is_some_and(|s| s.id == id) {
            *sessions = Some(Session {
                id,
                stage: Stage::Ready {
                    samples,
                    sample_rate: stored_rate,
                    ok: check.ok,
                },
            });
        }
        Ok(check)
    }

    pub async fn record_stop(app: &AppHandle, state: &AppState, id: Uuid) -> Result<CloneCheck> {
        let shared = {
            let registry = app.state::<CloneSessions>();
            let sessions = registry.lock();
            match sessions.as_ref() {
                Some(Session {
                    id: current,
                    stage: Stage::Recording(shared),
                }) if *current == id => shared.clone(),
                _ => return Err(not_a_session()),
            }
        };
        let capture = shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .ok_or_else(not_a_session)?;
        let (samples, rate) = state
            .gate()
            .spawn_blocking(move || capture.stop_native())?
            .await
            .map_err(|e| HolziError::SpeechFailed {
                reason: format!("stop the recording: {e}"),
            })?;
        finish(app, state, id, samples, rate).await
    }

    pub async fn from_file(
        app: &AppHandle,
        state: &AppState,
        bytes: Vec<u8>,
        extension: Option<String>,
    ) -> Result<CloneCheck> {
        use crate::assistant::custom_tones::decode;
        let id = Uuid::new_v4();
        // A recording that still runs ends: one session at a time. The slot names this import, so
        // an older one that finishes later cannot take it.
        let registry = app.state::<CloneSessions>();
        registry.replace(Session {
            id,
            stage: Stage::Reading,
        });
        let decoded = match state
            .gate()
            .spawn_blocking(move || decode(bytes, extension.as_deref()))
        {
            Ok(task) => task.await.map_err(|e| HolziError::SpeechFailed {
                reason: format!("read the audio file: {e}"),
            }),
            Err(error) => Err(error),
        }
        .and_then(|decoded| decoded.map_err(|_| super::invalid("unsupported")));
        let sound = match decoded {
            Ok(sound) => sound,
            Err(error) => {
                registry.end(id);
                return Err(error);
            }
        };
        finish(app, state, id, sound.samples, sound.sample_rate).await
    }

    /// The checked recording of session `id` as a WAV, and whether its check passed.
    pub fn recording_of(app: &AppHandle, id: Uuid) -> Result<(Vec<u8>, u32, bool)> {
        use crate::assistant::custom_tones::{encode_wav, Sound};
        let registry = app.state::<CloneSessions>();
        let sessions = registry.lock();
        match sessions.as_ref() {
            Some(Session {
                id: current,
                stage:
                    Stage::Ready {
                        samples,
                        sample_rate,
                        ok,
                    },
            }) if *current == id && !samples.is_empty() => {
                let wav = encode_wav(&Sound {
                    samples: samples.clone(),
                    sample_rate: *sample_rate,
                });
                Ok((wav, *sample_rate, *ok))
            }
            _ => Err(not_a_session()),
        }
    }

    /// Ends session `id` once its voice is kept.
    pub fn end(app: &AppHandle, id: Uuid) {
        app.state::<CloneSessions>().end(id);
    }
}

/// Starts recording a voice from the microphone; the level arrives as [`EVENT_CLONE_LEVEL`].
#[tauri::command]
pub async fn tts_clone_record_start(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<CloneSession> {
    #[cfg(feature = "voice")]
    {
        let session_id = state
            .gate()
            .run(async { imp::record_start(&app, &state).await })
            .await??;
        Ok(CloneSession { session_id })
    }
    #[cfg(not(feature = "voice"))]
    {
        let _ = (app, state);
        Err(no_voice())
    }
}

/// Ends the recording of `session_id` and checks it.
#[tauri::command]
pub async fn tts_clone_record_stop(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: Uuid,
) -> Result<CloneCheck> {
    #[cfg(feature = "voice")]
    {
        state
            .gate()
            .run(async { imp::record_stop(&app, &state, session_id).await })
            .await?
    }
    #[cfg(not(feature = "voice"))]
    {
        let _ = (app, state, session_id);
        Err(no_voice())
    }
}

/// Cancels a recording session without keeping its audio (`SpeakerEnrolDialog` on close or start over).
#[tauri::command]
pub async fn tts_clone_record_cancel(app: AppHandle, session_id: Uuid) -> Result<()> {
    #[cfg(feature = "voice")]
    {
        imp::end(&app, session_id);
        Ok(())
    }
    #[cfg(not(feature = "voice"))]
    {
        let _ = (app, session_id);
        Err(no_voice())
    }
}

/// Reads a voice from the chosen audio file (MP3, WAV, OGG, FLAC, M4A; its first 30 seconds) and
/// checks it. Takes the picked file like the import of an own tone, so it works with Android's
/// content URIs too (the contract's `{path}`).
#[tauri::command]
pub async fn tts_clone_from_file(
    app: AppHandle,
    state: State<'_, AppState>,
    file: PickedFile,
) -> Result<CloneCheck> {
    #[cfg(feature = "voice")]
    {
        use crate::files::picked;
        let name = picked::display_name(&app, &file);
        let bytes = picked::read(&app, &file, MAX_FILE_BYTES)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(invalid("tooLarge"));
        }
        let extension = name
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase());
        state
            .gate()
            .run(async { imp::from_file(&app, &state, bytes, extension).await })
            .await?
    }
    #[cfg(not(feature = "voice"))]
    {
        let _ = (app, state, file);
        Err(no_voice())
    }
}

/// Speaks a sample in `language` with the checked recording of `session_id`, at `rate` (default
/// 1.0), downloading the speech model of the language first.
#[tauri::command]
pub async fn tts_clone_preview(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: Uuid,
    language: String,
    rate: Option<f32>,
) -> Result<()> {
    if !LANGUAGES.contains(&language.as_str()) {
        return Err(invalid("unknownLanguage"));
    }
    let rate = super::checked_rate(rate.unwrap_or(super::RATE_DEFAULT))?;
    #[cfg(feature = "voice")]
    {
        let (wav, _, _) = imp::recording_of(&app, session_id)?;
        let voice = super::voices::Chosen::Own {
            id: session_id.to_string(),
            language,
            wav,
        };
        state
            .gate()
            .run(super::preview::run(&app, &state, voice, rate))
            .await?
    }
    #[cfg(not(feature = "voice"))]
    {
        let _ = (app, state, session_id, rate);
        Err(no_voice())
    }
}

/// Keeps the checked recording of `session_id` as an own voice of the vault. Refused without
/// `consent` (FR-050) or when the check found a problem.
#[tauri::command]
pub async fn tts_clone_save(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: Uuid,
    name: String,
    language: String,
    consent: bool,
) -> Result<VoiceOption> {
    let name = checked_save(&name, &language, consent).map_err(invalid)?;
    #[cfg(feature = "voice")]
    {
        let (wav, sample_rate, ok) = imp::recording_of(&app, session_id)?;
        if !ok {
            return Err(invalid("checkFailed"));
        }
        let now = crate::passwords::clock::unix_millis(std::time::SystemTime::now());
        let db = active_database(&state)?;
        let stored = db
            .write(move |tx| {
                tts_voices::insert(
                    tx,
                    tts_voices::NewVoice {
                        name: &name,
                        language: &language,
                        wav: &wav,
                        sample_rate,
                        consent_confirmed_at: now,
                    },
                    now,
                )
            })
            .await?;
        imp::end(&app, session_id);
        let hw = crate::hardware::probe_async().await;
        let installed = super::commands::list_installed_tts_voices(app.clone()).await?;
        super::commands::voice_options_on_device(&app, &[], &[stored], &installed, &hw)
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| invalid("unknownLanguage"))
    }
    #[cfg(not(feature = "voice"))]
    {
        let _ = (app, state, session_id, name);
        Err(no_voice())
    }
}

/// Deletes an own voice on every device (FR-050); devices that spoke with it fall back to the
/// default of its language. Deleting a voice that is already gone is not an error.
#[tauri::command]
pub async fn tts_voice_delete(state: State<'_, AppState>, voice: String) -> Result<()> {
    match VoiceRef::parse(&voice) {
        Some(VoiceRef::Clone(id)) => {
            let id = Uuid::parse_str(&id).map_err(|_| invalid("unknownVoice"))?;
            let db = active_database(&state)?;
            db.write(move |tx| tts_voices::delete(tx, id)).await?;
            Ok(())
        }
        Some(VoiceRef::Builtin(_)) => Err(invalid("builtinVoice")),
        Some(VoiceRef::Model { .. }) => Err(super::TtsError::Unavailable {
            reason: "imported models arrive with PR K part 2".to_owned(),
        }
        .into()),
        None => Err(invalid("unknownVoice")),
    }
}

/// Removes recordings no voice has used for a week, once when a vault opens. A failure is logged
/// and never blocks opening.
pub fn start_after_open(state: &AppState) {
    let Ok(db) = state.database() else {
        return;
    };
    let started = state.gate().spawn(async move {
        let now = crate::passwords::clock::unix_millis(std::time::SystemTime::now());
        match db.write(move |tx| tts_voices::prune(tx, now)).await {
            Ok(0) => {}
            Ok(deleted) => log::info!("own voices: removed {deleted} unused recordings"),
            Err(error) => log::warn!("own voices: the clean-up of recordings failed: {error}"),
        }
    });
    if let Err(error) = started {
        log::warn!("own voices: the clean-up could not start: {error}");
    }
}

#[cfg(test)]
#[path = "clone_commands_tests.rs"]
mod tests;
