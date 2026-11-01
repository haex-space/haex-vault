//! Voice control Tauri commands (spec 008-voice-control-stt, T009/T013):
//! start/stop/cancel a recording and dispatch the completed buffer to the
//! selected local STT adapter (Whisper or Parakeet). Registered unconditionally in
//! `lib.rs::run()`'s `generate_handler!` list; a `--no-default-features`
//! (no `voice`) build compiles the stub variants below instead of failing
//! to register the command names at all — matching the existing
//! `#[cfg(feature = "llm-cpu")]` pattern in `chat::model_loading::load_model_inner`.
//!
//! US3 (choosing an external transcription provider) is not implemented
//! yet: every recording dispatches to the bundled local adapter
//! unconditionally. US2 (the "stop"/"halt"/"abbrechen" fast path
//! cancelling an in-progress assistant turn) is also not implemented yet —
//! `match_interrupt` still runs on the final transcript so the wire shape
//! matches the contract, but nothing consumes `interrupt` to actually
//! cancel anything, and the frontend does not yet suppress `text` for it
//! (T019/T020).

#[cfg(feature = "voice")]
mod imp {
    use std::sync::{Arc, Mutex};

    use serde::Serialize;
    use tauri::{AppHandle, Emitter, Manager, State};
    use tokio::sync::watch;
    use tokio::sync::Mutex as AsyncMutex;
    use tokio_util::sync::CancellationToken;

    use crate::audio::stream::StreamSlot;
    use crate::error::{HolziError, Result};
    use crate::stt::interrupt::{match_interrupt, InterruptCommand};
    use crate::vault_gate::VaultGate;

    /// How often the cap-watcher polls `Capture::has_capped()` after
    /// `start_voice_recording`. Coarse on purpose — FR-017 only needs the
    /// cap to fire "soon after" 60s, not to the millisecond.
    const CAP_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);

    #[derive(Debug, Clone, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct TranscriptionResultWire {
        pub text: String,
        pub interrupt: Option<InterruptCommand>,
    }

    type StopOutcome = std::result::Result<TranscriptionResultWire, String>;

    enum VoiceSlot {
        Idle,
        Recording(crate::audio::Capture),
        Transcribing(watch::Receiver<Option<StopOutcome>>),
    }

    /// Per-process voice pipeline state — at most one active recording or
    /// pending transcription at a time (contracts/tauri-commands.md
    /// `start_voice_recording`).
    pub struct VoiceState {
        slot: Mutex<VoiceSlot>,
        /// The selected local model (Whisper or Parakeet, spec 066 T134),
        /// loaded once on first use and kept warm — reloading it (a
        /// multi-second operation) per transcription would blow SC-001's
        /// 5-second budget. Push-to-talk and the voice assistant share it.
        adapter: AsyncMutex<Option<Arc<dyn crate::stt::SttAdapter>>>,
        /// The assistant's continuous audio stream while it runs (spec 066 T020).
        stream: StreamSlot,
        /// The speech model last spoken with and its catalog id (spec 066 T028), kept warm like
        /// the STT adapter: loading it takes about a second.
        speech: AsyncMutex<Option<(String, Arc<crate::tts::local::PocketModel>)>>,
        /// Stops the voice sample that is playing, if any.
        preview: Mutex<Option<CancellationToken>>,
    }

    impl VoiceState {
        pub fn new() -> Self {
            Self {
                slot: Mutex::new(VoiceSlot::Idle),
                adapter: AsyncMutex::new(None),
                stream: StreamSlot::default(),
                speech: AsyncMutex::new(None),
                preview: Mutex::new(None),
            }
        }

        /// The speech model `id` in `dir`, loaded on first use and kept until another model is
        /// asked for or the vault closes.
        pub(crate) async fn speech_model(
            &self,
            id: &str,
            dir: std::path::PathBuf,
            gate: &VaultGate,
        ) -> Result<Arc<crate::tts::local::PocketModel>> {
            let mut slot = self.speech.lock().await;
            if let Some((loaded, model)) = &*slot {
                if loaded == id {
                    return Ok(Arc::clone(model));
                }
            }
            // Free the old model before the new one takes its memory.
            *slot = None;
            let model = gate
                .spawn_blocking(move || crate::tts::local::PocketModel::load(&dir))?
                .await
                .map_err(|e| HolziError::SpeechFailed {
                    reason: format!("load the speech model: {e}"),
                })??;
            let model = Arc::new(model);
            *slot = Some((id.to_owned(), Arc::clone(&model)));
            Ok(model)
        }

        /// Stops the voice sample that is playing and returns the token of the next one, a child
        /// of `closing` so a vault close stops it too.
        pub(crate) fn begin_preview(&self, closing: &CancellationToken) -> CancellationToken {
            let token = closing.child_token();
            let previous = self
                .preview
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .replace(token.clone());
            if let Some(previous) = previous {
                previous.cancel();
            }
            token
        }

        /// Clears the cached STT adapter so the next
        /// `resolve_local_adapter` call reloads instead of reusing a stale
        /// instance (spec 010's `invalidate_stt_model_cache`); switching the
        /// model drops the old one.
        pub(crate) async fn invalidate_stt_cache(&self) {
            *self.adapter.lock().await = None;
        }

        /// Stops a recording that is still running, if any. Idempotent.
        fn cancel_recording(&self) {
            let mut slot = self.slot.lock().unwrap_or_else(|e| e.into_inner());
            if let VoiceSlot::Recording(_) = &*slot {
                let VoiceSlot::Recording(capture) = std::mem::replace(&mut *slot, VoiceSlot::Idle)
                else {
                    unreachable!()
                };
                capture.cancel();
            }
        }

        /// For the close (spec 013): releases the microphone and frees the cached Whisper model
        /// while the process finishes ending.
        pub(crate) async fn reset_for_close(&self) {
            self.cancel_recording();
            self.stream.stop();
            self.invalidate_stt_cache().await;
            *self.speech.lock().await = None;
        }

        /// Cancels an active capture synchronously during close phase 1, so no microphone stream
        /// can survive while the gate drains the remaining tracked work.
        pub(crate) fn cancel_for_close(&self) {
            self.cancel_recording();
            self.stream.stop();
        }

        /// The audio stream push-to-talk and the assistant share.
        pub fn audio_stream(&self) -> &StreamSlot {
            &self.stream
        }

        /// Test-only: whether the STT adapter cache currently holds a
        /// loaded instance.
        #[cfg(test)]
        pub(crate) async fn has_cached_stt_adapter(&self) -> bool {
            self.adapter.lock().await.is_some()
        }
    }

    impl Default for VoiceState {
        fn default() -> Self {
            Self::new()
        }
    }

    /// Asks for the microphone before a recording (spec 043 FR-029): on Android the system asks
    /// the person the first time and answers from then on; elsewhere it is always granted here.
    async fn ensure_microphone(app: &AppHandle) -> Result<()> {
        use tauri_plugin_holzi_android::{HolziAndroidExt, Permission, PermissionState};
        let app = app.clone();
        // The request blocks until the person answers the system's question.
        let state = tauri::async_runtime::spawn_blocking(move || {
            app.holzi_android()
                .request_permission(Permission::Microphone)
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|state| state)
        .map_err(|reason| {
            log::warn!("microphone permission: {reason}");
            HolziError::DeviceUnavailable
        })?;
        match state {
            PermissionState::Granted => Ok(()),
            _ => Err(HolziError::MicrophoneDenied),
        }
    }

    #[tauri::command]
    pub async fn start_voice_recording(app: AppHandle, voice: State<'_, VoiceState>) -> Result<()> {
        ensure_microphone(&app).await?;
        let app_for_watcher = app.clone();
        app.state::<VaultGate>()
            .run(async move {
                let mut slot = voice.slot.lock().unwrap_or_else(|e| e.into_inner());
                if !matches!(&*slot, VoiceSlot::Idle) {
                    return Err(HolziError::AlreadyRecording);
                }
                // While the assistant holds the microphone, record from its stream.
                let capture = match voice.stream.current() {
                    Some(stream) => crate::audio::Capture::from_stream(&stream),
                    None => crate::audio::Capture::start().map_err(|e| match e {
                        crate::audio::AudioError::DeviceUnavailable => {
                            HolziError::DeviceUnavailable
                        }
                        // cpal does not surface a distinct OS-permission-denied error
                        // uniformly across desktop backends; Android asks before this
                        // (`ensure_microphone`).
                        crate::audio::AudioError::ConfigUnavailable { .. }
                        | crate::audio::AudioError::StartFailed { .. } => {
                            HolziError::DeviceUnavailable
                        }
                    })?,
                };
                *slot = VoiceSlot::Recording(capture);
                drop(slot);
                spawn_cap_watcher(app_for_watcher);
                Ok(())
            })
            .await?
    }

    #[tauri::command]
    pub async fn stop_voice_recording(
        app: AppHandle,
        voice: State<'_, VoiceState>,
    ) -> Result<TranscriptionResultWire> {
        app.state::<VaultGate>().run(do_stop(&app, &voice)).await?
    }

    /// Spec 010: clears the warm-cached Whisper adapter so the next
    /// `resolve_local_adapter` call picks up a change to the
    /// `voice.stt_model_id` preference instead of continuing to serve the
    /// previously-loaded tier for the rest of the process lifetime.
    /// Registered in every feature combination (mirrors
    /// `start_voice_recording`'s registration discipline) so the frontend
    /// can call it unconditionally after a Settings switch; a build
    /// without `voice` has no cache to clear and is a successful no-op.
    #[tauri::command]
    pub async fn invalidate_stt_model_cache(voice: State<'_, VoiceState>) -> Result<()> {
        voice.invalidate_stt_cache().await;
        Ok(())
    }

    #[tauri::command]
    pub async fn cancel_voice_recording(voice: State<'_, VoiceState>) -> Result<()> {
        // Idempotent: cancelling while idle or already transcribing is not
        // an error (contracts/tauri-commands.md).
        voice.cancel_recording();
        Ok(())
    }

    /// Shared by the manual `stop_voice_recording` command and the
    /// max-duration cap watcher. The first caller to find `Recording` owns
    /// the transition to `Transcribing` and spawns the actual work;
    /// anyone else (including a concurrent caller here) observes
    /// `Transcribing` and awaits the same coalesced result instead of
    /// starting a second transcription or returning `NotRecording`
    /// (data-model.md "Backend-seitig wird der Puffer... atomar...").
    async fn do_stop(app: &AppHandle, voice: &VoiceState) -> Result<TranscriptionResultWire> {
        let mut rx = {
            let mut slot = voice.slot.lock().unwrap_or_else(|e| e.into_inner());
            match std::mem::replace(&mut *slot, VoiceSlot::Idle) {
                VoiceSlot::Idle => return Err(HolziError::NotRecording),
                VoiceSlot::Recording(capture) => {
                    let pcm = capture.stop();
                    let (tx, rx) = watch::channel(None);
                    *slot = VoiceSlot::Transcribing(rx.clone());
                    let app_task = app.clone();
                    app.state::<VaultGate>().spawn(async move {
                        let voice_task = app_task.state::<VoiceState>();
                        let result = run_transcription(&app_task, &voice_task, pcm).await;
                        let _ = tx.send(Some(result));
                        *voice_task.slot.lock().unwrap_or_else(|e| e.into_inner()) =
                            VoiceSlot::Idle;
                    })?;
                    rx
                }
                VoiceSlot::Transcribing(rx) => {
                    *slot = VoiceSlot::Transcribing(rx.clone());
                    rx
                }
            }
        };
        loop {
            if let Some(result) = rx.borrow().clone() {
                return result.map_err(|reason| HolziError::TranscriptionFailed { reason });
            }
            if rx.changed().await.is_err() {
                return Err(HolziError::TranscriptionFailed {
                    reason: "voice pipeline closed unexpectedly".into(),
                });
            }
        }
    }

    /// Polls the max-duration cap (FR-017), runs the same coalesced stop path
    /// a manual `stop_voice_recording` call would, and emits the completed
    /// result as the event payload. Emitting the result after `do_stop`
    /// completes lets the renderer consume it without issuing a second stop
    /// request after the slot has returned to `Idle`.
    fn spawn_cap_watcher(app: AppHandle) {
        // A closing gate refuses the watcher, and there is nothing left to cap then.
        let gate = app.state::<VaultGate>().inner().clone();
        let _ = gate.spawn(async move {
            loop {
                tokio::time::sleep(CAP_POLL_INTERVAL).await;
                let voice = app.state::<VoiceState>();
                let capped = {
                    let slot = voice.slot.lock().unwrap_or_else(|e| e.into_inner());
                    match &*slot {
                        VoiceSlot::Recording(capture) => {
                            if capture.has_capped() {
                                true
                            } else {
                                continue;
                            }
                        }
                        _ => return,
                    }
                };
                if capped {
                    let outcome = do_stop(&app, &voice).await;
                    let payload = capped_event_payload(outcome);
                    let _ = app.emit("voice-recording-capped", payload);
                    return;
                }
            }
        });
    }

    /// Converts the completed cap-watcher outcome into the optional event
    /// payload consumed by the renderer. Failed transcriptions use `None` so
    /// the renderer can enter its generic error state without retrying stop.
    pub(crate) fn capped_event_payload(
        outcome: crate::error::Result<TranscriptionResultWire>,
    ) -> Option<TranscriptionResultWire> {
        outcome.ok()
    }

    /// Resolves the active transcription adapter and runs it. US3 (external
    /// provider selection) is not implemented — always the bundled local
    /// adapter for now.
    async fn run_transcription(
        app: &AppHandle,
        voice: &VoiceState,
        pcm: crate::stt::CanonicalPcm,
    ) -> StopOutcome {
        let adapter = resolve_local_adapter(app, voice)
            .await
            .map_err(|e| e.to_string())?;
        let text = adapter.transcribe(&pcm).await.map_err(|e| e.to_string())?;
        let interrupt = match_interrupt(&text);
        Ok(TranscriptionResultWire { text, interrupt })
    }

    /// Device-scoped preference naming which STT catalog tier is active
    /// (spec 010). Missing/empty/unknown-id all fall back to the smallest
    /// built-in tier — see [`resolve_stt_catalog_entry`].
    const STT_MODEL_PREF_KEY: &str = "voice.stt_model_id";

    /// Reads this device's [`STT_MODEL_PREF_KEY`] from the active vault.
    /// Missing preferences return `Ok(None)`; infrastructure failures retain
    /// their cause so a broken active-vault/device lookup is diagnosable.
    async fn read_stt_model_pref(
        app: &AppHandle,
    ) -> std::result::Result<Option<String>, crate::stt::SttError> {
        let state = app.state::<crate::state::AppState>();
        let db = crate::state_utils::active_database(&state).map_err(|e| {
            crate::stt::SttError::LocalUnavailable {
                reason: format!("resolve active vault for STT preference: {e}"),
            }
        })?;
        let device_uuid = crate::device::commands::resolve_vault_device_uuid(app, &db)
            .await
            .map_err(|e| crate::stt::SttError::LocalUnavailable {
                reason: format!("resolve device for STT preference: {e}"),
            })?;
        let scope = crate::storage::preferences::PrefScope::Device(device_uuid);
        let key = STT_MODEL_PREF_KEY.to_string();
        db.read(move |r| crate::storage::preferences::get(r, scope, &key))
            .await
            .map_err(|e| crate::stt::SttError::LocalUnavailable {
                reason: format!("read STT preference: {e}"),
            })
    }

    /// Resolves a raw preference read (possibly missing/empty/unknown) to
    /// the STT catalog entry it should mean, falling back to the device's
    /// default (`stt::catalog::default_entry`). Pure/`AppHandle`-free on
    /// purpose, split out from [`resolve_stt_catalog_entry`] so the fallback
    /// rules are directly unit-testable (`voice_tests.rs`) without a Tauri
    /// `AppHandle` — this codebase has no mocking convention for that type.
    pub(crate) fn resolve_stt_catalog_entry_from_pref(
        pref: Option<String>,
        presets: crate::platform::ModelPresets,
    ) -> std::result::Result<&'static crate::stt::catalog::SttCatalogEntry, crate::stt::SttError>
    {
        match pref.filter(|id| !id.is_empty()) {
            Some(id) => match crate::stt::catalog::get(&id)? {
                Some(entry)
                    if match entry.engine {
                        crate::stt::catalog::SttEngine::CandleWhisper => cfg!(feature = "llm-cpu"),
                        crate::stt::catalog::SttEngine::SherpaParakeet => cfg!(feature = "voice"),
                    } =>
                {
                    Ok(entry)
                }
                Some(_) => crate::stt::catalog::default_entry(presets),
                None => crate::stt::catalog::default_entry(presets),
            },
            None => crate::stt::catalog::default_entry(presets),
        }
    }

    /// Resolves the active STT catalog entry for this device: the
    /// [`STT_MODEL_PREF_KEY`] preference if it names a known catalog
    /// entry, otherwise the device's default.
    pub(crate) async fn resolve_stt_catalog_entry(
        app: &AppHandle,
    ) -> std::result::Result<&'static crate::stt::catalog::SttCatalogEntry, crate::stt::SttError>
    {
        resolve_stt_catalog_entry_from_pref(
            read_stt_model_pref(app).await?,
            crate::platform::capabilities().model_presets,
        )
    }

    /// Loads the selected local model on first use and keeps it warm
    /// (`voice.adapter`) — reloading it per transcription would blow
    /// SC-001's 5-second budget. The catalog entry's engine decides which
    /// adapter loads (spec 066 T134).
    async fn resolve_local_adapter(
        app: &AppHandle,
        voice: &VoiceState,
    ) -> std::result::Result<Arc<dyn crate::stt::SttAdapter>, crate::stt::SttError> {
        let mut guard = voice.adapter.lock().await;
        if let Some(adapter) = guard.as_ref() {
            return Ok(Arc::clone(adapter));
        }
        let entry = resolve_stt_catalog_entry(app).await?;
        let adapter: Arc<dyn crate::stt::SttAdapter> = match entry.engine {
            crate::stt::catalog::SttEngine::CandleWhisper => load_whisper(app, entry).await?,
            crate::stt::catalog::SttEngine::SherpaParakeet => {
                Arc::new(crate::stt::sherpa::ParakeetAdapter::load(app, entry).await?)
            }
        };
        *guard = Some(Arc::clone(&adapter));
        Ok(adapter)
    }

    #[cfg(feature = "llm-cpu")]
    async fn load_whisper(
        app: &AppHandle,
        entry: &crate::stt::catalog::SttCatalogEntry,
    ) -> std::result::Result<Arc<dyn crate::stt::SttAdapter>, crate::stt::SttError> {
        Ok(Arc::new(
            crate::stt::local::LocalWhisperAdapter::load(app, entry).await?,
        ))
    }

    /// Whisper runs on candle, which only `llm-cpu` builds compile in.
    #[cfg(not(feature = "llm-cpu"))]
    async fn load_whisper(
        _app: &AppHandle,
        _entry: &crate::stt::catalog::SttCatalogEntry,
    ) -> std::result::Result<Arc<dyn crate::stt::SttAdapter>, crate::stt::SttError> {
        Err(crate::stt::SttError::LocalUnavailable {
            reason: "Whisper is not enabled in this build".into(),
        })
    }
}

#[cfg(all(test, feature = "voice"))]
#[path = "voice_tests.rs"]
mod voice_tests;

#[cfg(feature = "voice")]
pub(crate) use imp::resolve_stt_catalog_entry;
#[cfg(feature = "voice")]
pub use imp::{
    cancel_voice_recording, invalidate_stt_model_cache, start_voice_recording,
    stop_voice_recording, TranscriptionResultWire, VoiceState,
};

#[cfg(not(feature = "voice"))]
#[path = "voice_stub.rs"]
mod stub;

#[cfg(not(feature = "voice"))]
pub use stub::{
    cancel_voice_recording, invalidate_stt_model_cache, start_voice_recording,
    stop_voice_recording, VoiceState,
};

#[cfg(not(feature = "voice"))]
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionResultWire {
    pub text: String,
    pub interrupt: Option<()>,
}
