//! The commands of the voice assistant (spec 066 T044, contracts/tauri-commands.md) and the
//! runtime that drives [`Core`] with the real audio stream, speaker, listener and chat turns.
//!
//! One task per process owns the `Core` and takes everything in order through one inbox: the
//! commands, the utterances of the listener, the outputs of the round and the speaker's reports.
//! It ends with the vault (`VaultGate`), which releases microphone and speaker.
//!
//! The assistant is switched on and off by one toggle (`assistant_set_enabled`, operator
//! 2026-11-02): on prepares the models and opens the microphone, off closes it. With the wake word
//! model installed haex then waits for "Hex" / "Hey Hex"; without it, on starts a conversation at
//! once and the assistant switches itself off after a pause.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::assets;
use super::conversation::{Timing, PREF_CONTINUE_MINS};
use super::driver::{Core, Input, Note, Ports, StateWire};
use super::listen::{self, Start};
use super::passage::PassageLanguage;
use super::tones::{self, ToneId};
use super::turn::{self, Started, VoiceTurn};
use super::vad::Utterance;
use crate::audio::stream::AudioStream;
use crate::chat::session::ChatState;
use crate::error::{HolziError, Result};
use crate::local_time::Clock;
use crate::state::AppState;
use crate::storage::preferences::{self, PrefScope};
use crate::storage::voice_transcript::{self, EntryKind, NewEntry, TranscriptEntry};
use crate::tts::commands::PREF_SPEECH_RATE;
use crate::tts::local::{Engine, LanguageVoices, Passage, Reference, Sentence, Speaker, SpeechOut};
use crate::tts::{self, voices};
use crate::vault_gate::VaultGate;
use crate::voice::VoiceState;

/// Event with the state of the assistant (`AssistantStateWire`).
pub const EVENT_STATE: &str = "assistant-state";
/// Event when something keeps the assistant from working (`{kind, message}`).
pub const EVENT_ERROR: &str = "assistant-error";
/// Event with every transcript entry once it is stored (`TranscriptEntryWire`).
pub const EVENT_TRANSCRIPT: &str = "assistant-transcript";

/// The toggle and the mute of this device, kept across a restart and restored after unlock by the
/// window (FR-002, data-model.md): it knows the language and the clock a conversation needs.
pub const PREF_ENABLED: &str = "assistant.enabled";
pub const PREF_MUTED: &str = "assistant.muted";
const PREF_PAUSE_MS: &str = "assistant.pauseMs";
const PREF_TONES_ENABLED: &str = "assistant.tones.enabled";
const PREF_TONES_VOLUME: &str = "assistant.tones.volume";
/// Defaults of data-model.md.
const PAUSE_MS_DEFAULT: u32 = 1500;
const TONES_VOLUME_DEFAULT: u8 = 60;

/// The assistant of this process, as managed state.
#[derive(Default)]
pub struct AssistantDriver {
    inbox: Mutex<Option<mpsc::UnboundedSender<Msg>>>,
    state: Arc<Mutex<Option<StateWire>>>,
}

enum Msg {
    SetEnabled {
        enabled: bool,
        language: String,
        clock: Clock,
        reply: oneshot::Sender<Result<()>>,
    },
    RefreshSpeechModel {
        language: String,
        reply: oneshot::Sender<Result<()>>,
    },
    Input(Input),
    /// The speaker of audio session `session` finished a sentence.
    Played {
        session: u64,
        ticket: u64,
    },
    /// Listener `listener` cut an utterance; it still has to be transcribed.
    Utterance {
        listener: u64,
        utterance: Utterance,
    },
    /// Answers once everything sent before is handled.
    Flush(oneshot::Sender<()>),
}

impl AssistantDriver {
    pub fn state(&self) -> StateWire {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .unwrap_or_else(StateWire::off)
    }

    /// The inbox of the running task, started on first use.
    fn inbox(&self, app: &AppHandle, gate: &VaultGate) -> Result<mpsc::UnboundedSender<Msg>> {
        let mut slot = self.inbox.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(inbox) = slot.as_ref().filter(|inbox| !inbox.is_closed()) {
            return Ok(inbox.clone());
        }
        let (inbox, messages) = mpsc::unbounded_channel();
        let (notes, to_write) = mpsc::unbounded_channel();
        gate.spawn(write_notes(app.clone(), to_write))?;
        let ports = RealPorts {
            app: app.clone(),
            gate: gate.clone(),
            inbox: inbox.clone(),
            notes,
            shared: Arc::clone(&self.state),
            session: 0,
            audio: None,
            listening: None,
            watching: None,
            wake_watching: None,
            turn: None,
            chat_round: Arc::new(AtomicU64::new(0)),
            language: "en".to_owned(),
            clock: Clock::at(0, 0),
        };
        let core = Core::new("en", Timing::default(), Instant::now());
        gate.spawn(run(core, ports, messages, gate.token()))?;
        *slot = Some(inbox.clone());
        Ok(inbox)
    }

    fn send(&self, app: &AppHandle, gate: &VaultGate, msg: Msg) -> Result<()> {
        self.inbox(app, gate)?.send(msg).map_err(|_| closing())
    }

    /// Sends `input` and waits until it is handled.
    async fn handle(&self, app: &AppHandle, gate: &VaultGate, input: Input) -> Result<()> {
        self.send(app, gate, Msg::Input(input))?;
        let (done, handled) = oneshot::channel();
        self.send(app, gate, Msg::Flush(done))?;
        handled.await.map_err(|_| closing())
    }
}

/// `{kind, message}` of `assistant-error`; `kind` is the error's wire kind.
fn error_payload(error: &HolziError) -> serde_json::Value {
    let kind = serde_json::to_value(error)
        .ok()
        .and_then(|value| value.get("kind").cloned())
        .unwrap_or_else(|| serde_json::Value::from("unknown"));
    serde_json::json!({ "kind": kind, "message": error.to_string() })
}

fn closing() -> HolziError {
    HolziError::InvalidInput {
        reason: "the vault is closing".into(),
    }
}

/// Emits the selected model's fallback notice at most once per conversation.
fn make_fallback_notice(
    app: &AppHandle,
    chosen: &'static tts::catalog::TtsCatalogEntry,
    sent: &Arc<AtomicBool>,
) -> Arc<dyn Fn(tts::commands::FallbackReason) + Send + Sync> {
    let app = app.clone();
    let sent = Arc::clone(sent);
    Arc::new(move |reason| {
        if !sent.swap(true, Ordering::AcqRel) {
            tts::commands::emit_fallback(&app, reason, chosen);
        }
    })
}

async fn run(
    mut core: Core,
    mut ports: RealPorts,
    mut messages: mpsc::UnboundedReceiver<Msg>,
    closing: CancellationToken,
) {
    loop {
        let wakeup = core.next_wakeup();
        let sleep = async {
            match wakeup {
                Some(at) => tokio::time::sleep_until(at.into()).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            _ = closing.cancelled() => {
                core.on(Input::VaultClosed, Instant::now(), &mut ports);
                return;
            }
            _ = sleep => core.on(Input::Tick, Instant::now(), &mut ports),
            msg = messages.recv() => {
                let Some(msg) = msg else {
                    core.on(Input::VaultClosed, Instant::now(), &mut ports);
                    return;
                };
                match msg {
                    Msg::SetEnabled { enabled, language, clock, reply } => {
                        let result = if enabled {
                            switch_on(&mut core, &mut ports, language, clock).await
                        } else {
                            core.on(Input::SwitchOff, Instant::now(), &mut ports);
                            Ok(())
                        };
                        let _ = reply.send(result);
                    }
                    Msg::RefreshSpeechModel { language, reply } => {
                        let result = ports.refresh_speech_model(&language).await;
                        let _ = reply.send(result);
                    }
                    Msg::Input(input) => core.on(input, Instant::now(), &mut ports),
                    Msg::Played { session, ticket } => {
                        if session == ports.session {
                            core.on(Input::Played(ticket), Instant::now(), &mut ports);
                        }
                    }
                    Msg::Utterance { listener, utterance } => ports.transcribe(listener, utterance),
                    Msg::Flush(done) => {
                        let _ = done.send(());
                    }
                }
            }
        }
    }
}

/// Switching on: opens microphone and speaker with the models of `language` unless they are open,
/// then switches the conversation on.
async fn switch_on(
    core: &mut Core,
    ports: &mut RealPorts,
    language: String,
    clock: Clock,
) -> Result<()> {
    let language = assets::voice_language(&language).to_owned();
    ports.clock = clock;
    if !core.is_open() {
        let (audio, timing) = open_audio(ports, &language).await?;
        core.set_barge_in(audio.stream.cancels_echo());
        core.set_wake_word(audio.wake.is_some());
        ports.audio = Some(audio);
        ports.language = language.clone();
        core.prepare(&language, timing);
    }
    core.on(Input::SwitchOn, Instant::now(), ports);
    Ok(())
}

/// What a conversation speaks and listens with.
struct Audio {
    stream: Arc<AudioStream>,
    speaker: Speaker,
    /// The language of each sentence of the answer being spoken (FR-022d).
    passage: PassageLanguage,
    /// The voices of other languages for passages, loaded when first needed (FR-022d).
    other_voices: Arc<dyn LanguageVoices>,
    /// The live enrolled-speaker policy (FR-016a), including changes from other devices.
    speakers: Arc<super::speakers::LiveGate>,
    speaker_changes_cancel: CancellationToken,
    engine: Arc<dyn Engine>,
    /// The language's Pocket model while the engine speaks in parts (spec 081 FR-010a).
    reserve: Option<Arc<dyn Engine>>,
    /// Checks that a generated part is speech (spec 081 FR-010a).
    speech: Option<Arc<dyn tts::check::SpeechShare>>,
    /// Measures how fast the engine speaks; each answer is stored once it is over (spec 081
    /// FR-007).
    meters: Vec<(
        Arc<tts::speed::Meter>,
        &'static tts::catalog::TtsCatalogEntry,
    )>,
    model: &'static tts::catalog::TtsCatalogEntry,
    /// Shares the once-per-conversation fallback notification guard.
    fallback_sent: Arc<AtomicBool>,
    reference: Arc<Reference>,
    rate: f32,
    tones_enabled: bool,
    vad_model: PathBuf,
    pause_ms: u32,
    /// The wake word models when installed (FR-011, PR D); without them a conversation starts as
    /// soon as the assistant is switched on.
    wake: Option<Arc<super::wakeword::model::Models>>,
    wake_rule: super::wakeword::Rule,
}

impl Drop for Audio {
    fn drop(&mut self) {
        self.speaker_changes_cancel.cancel();
    }
}

fn follow_speaker_gate(
    app: AppHandle,
    mut changes: tokio::sync::broadcast::Receiver<Arc<Vec<String>>>,
    speakers: Arc<super::speakers::LiveGate>,
    stop: CancellationToken,
) {
    let gate = app.state::<AppState>().gate().clone();
    if let Err(error) = gate.spawn(async move {
        loop {
            let refresh = tokio::select! {
                () = stop.cancelled() => return,
                event = changes.recv() => match event {
                    Ok(tables) => tables.iter().any(|table| {
                        table == "haex_assistant_speakers" || table == "preferences"
                    }),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => true,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                },
            };
            if refresh {
                speakers.replace(super::speakers::gate(&app).await);
            }
        }
    }) {
        log::warn!("assistant: speaker gate changes will not be followed: {error}");
    }
}

/// The device settings the assistant reads when a conversation starts.
struct Settings {
    timing: Timing,
    pause_ms: u32,
    rate: f32,
    voice: Option<String>,
    tones_enabled: bool,
    tones_volume: u8,
    sensitivity: super::wakeword::Sensitivity,
}

async fn read_settings(app: &AppHandle, language: &str) -> Result<Settings> {
    let state = app.state::<AppState>();
    let db = crate::state_utils::active_database(&state)?;
    let device = crate::device::commands::resolve_vault_device_uuid(app, &db).await?;
    let voice_key = tts::commands::voice_pref_key(language);
    let keys = [
        PREF_CONTINUE_MINS.to_owned(),
        PREF_PAUSE_MS.to_owned(),
        PREF_SPEECH_RATE.to_owned(),
        voice_key,
        PREF_TONES_ENABLED.to_owned(),
        PREF_TONES_VOLUME.to_owned(),
        super::wakeword::PREF_SENSITIVITY.to_owned(),
    ];
    let values = db
        .read(move |r| {
            keys.iter()
                .map(|key| preferences::get(r, PrefScope::Device(device), key))
                .collect::<std::result::Result<Vec<_>, _>>()
        })
        .await?;
    let int = |i: usize| {
        values[i]
            .as_deref()
            .and_then(|v| v.trim().parse::<i64>().ok())
    };
    Ok(Settings {
        timing: Timing::from_prefs(int(0)),
        pause_ms: int(1).map_or(PAUSE_MS_DEFAULT, |v| v.clamp(0, i64::from(u32::MAX)) as u32),
        rate: values[2]
            .as_deref()
            .and_then(|v| v.trim().parse::<f32>().ok())
            .and_then(|v| tts::checked_rate(v).ok())
            .unwrap_or(tts::RATE_DEFAULT),
        voice: values[3].clone(),
        tones_enabled: values[4]
            .as_deref()
            .is_none_or(|v| !matches!(v.trim(), "false" | "0")),
        tones_volume: int(5).map_or(TONES_VOLUME_DEFAULT, |v| v.clamp(0, 100) as u8),
        sensitivity: super::wakeword::Sensitivity::parse(values[6].as_deref()),
    })
}

/// Loads what a conversation needs and opens microphone and speaker. Nothing is downloaded here:
/// a missing model is reported, the settings page offers the download (FR-005).
async fn open_audio(ports: &mut RealPorts, language: &str) -> Result<(Audio, Timing)> {
    let app = ports.app.clone();
    let settings = read_settings(&app, language).await?;
    let vad_model = assets::vad_model_path(&app)?;
    let wake = load_wake_word(&app, &ports.gate).await;
    let voice_state = app.state::<VoiceState>();
    // Warm the speech recognition model now, not at the first utterance.
    voice_state.stt_adapter(&app).await?;

    // The chosen speech model when it is installed, else the one that speaks without a choice
    // (spec 081 FR-012, FR-014).
    // Qwen3-TTS speaks in parts: its reserve and the check for parts without speech (spec 081
    // FR-010a).
    let (resolved, engines) = tts::commands::load_installed(&app, &ports.gate, language).await?;
    let entry = resolved.speaking;
    let fallback_sent = Arc::new(AtomicBool::new(false));
    // Pocket speaks instead of the model the user chose: say why, once per conversation (spec
    // 081 contracts/tauri-commands.md).
    let fallback_notice = resolved
        .chosen
        .map(|chosen| make_fallback_notice(&app, chosen, &fallback_sent));
    if let (Some(chosen), Some(notify)) = (resolved.chosen, fallback_notice.as_ref()) {
        let reason = if engines.load_failed {
            Some(tts::commands::FallbackReason::LoadFailed)
        } else if chosen.id != entry.id {
            Some(tts::commands::FallbackReason::NotInstalled)
        } else {
            None
        };
        if let Some(reason) = reason {
            notify(reason);
        }
    }
    let (engine, reserve) = (engines.engine, engines.reserve);
    // Pocket and Qwen3-TTS run on the processor until the graphics card ledger (spec 081 T044).
    let meter = Arc::new(tts::speed::Meter::new(&engines.dir, "cpu"));
    let model = engines.entry;
    let speech = engine
        .part_words()
        .map(|_| Arc::new(tts::check::Level) as Arc<dyn tts::check::SpeechShare>);
    // An own voice (spec 066 FR-049) speaks while it is in the vault; otherwise the built-in
    // default of the language.
    let own =
        tts::commands::own_recording(&app.state::<AppState>(), settings.voice.as_deref()).await?;
    let voice = voices::choose(settings.voice.as_deref(), language, own)?;
    let reference = Arc::new(voice.reference()?);

    // Subscribe before loading the initial policy so a concurrent local or synced change cannot be
    // missed between the read and the start of the watcher.
    let state = app.state::<AppState>();
    let changes = state.vault_changes().subscribe();
    let initial_speakers = super::speakers::gate(&app).await?;
    let speakers = Arc::new(super::speakers::LiveGate::new(initial_speakers));
    let speaker_changes_cancel = CancellationToken::new();

    crate::voice::ensure_microphone(&app).await?;
    let gate = ports.gate.clone();
    let stream = ports
        .gate
        .spawn_blocking(move || AudioStream::open_devices(&gate))?
        .await
        .map_err(|e| HolziError::SpeechFailed {
            reason: format!("open the audio devices: {e}"),
        })?
        .map_err(|e| {
            log::warn!("assistant: open the audio devices: {e}");
            HolziError::DeviceUnavailable
        })?;
    let stream = Arc::new(stream);
    stream.mixer().set_tone_volume(settings.tones_volume);
    ports.session += 1;
    let session = ports.session;
    let inbox = ports.inbox.clone();
    let speaker = Speaker::start_reporting(
        &ports.gate,
        Arc::clone(&stream) as Arc<dyn SpeechOut>,
        Arc::new(move |ticket| {
            let _ = inbox.send(Msg::Played { session, ticket });
        }),
    )?;
    voice_state
        .audio_stream()
        .replace(Some(Arc::clone(&stream)));
    follow_speaker_gate(
        app.clone(),
        changes,
        Arc::clone(&speakers),
        speaker_changes_cancel.clone(),
    );
    Ok((
        Audio {
            stream,
            speaker,
            passage: PassageLanguage::new(language),
            other_voices: tts::languages::AppVoices::new(app.clone()),
            speakers,
            speaker_changes_cancel,
            engine,
            reserve,
            speech,
            meters: vec![(meter, model)],
            model,
            reference,
            rate: settings.rate,
            tones_enabled: settings.tones_enabled,
            fallback_sent,
            vad_model,
            pause_ms: settings.pause_ms,
            wake,
            wake_rule: settings.sensitivity.rule(),
        },
        settings.timing,
    ))
}

/// The wake word models when both files are installed; a model that does not load is reported
/// and the assistant works as without it.
async fn load_wake_word(
    app: &AppHandle,
    gate: &VaultGate,
) -> Option<Arc<super::wakeword::model::Models>> {
    use super::wakeword::{model::Models, CLASSIFIER, EMBEDDING};
    if !super::wakeword::is_installed(app) {
        return None;
    }
    let (embedding, classifier) = (EMBEDDING.path(app).ok()?, CLASSIFIER.path(app).ok()?);
    let loaded = gate
        .spawn_blocking(move || Models::load(&embedding, &classifier))
        .ok()?
        .await;
    match loaded {
        Ok(Ok(models)) => Some(models),
        Ok(Err(error)) => {
            log::warn!("assistant: the wake word models did not load: {error}");
            None
        }
        Err(error) => {
            log::warn!("assistant: loading the wake word models stopped: {error}");
            None
        }
    }
}

struct RunningTurn {
    round: u64,
    cancel: CancellationToken,
    abort_requested: Arc<std::sync::atomic::AtomicBool>,
}

struct RealPorts {
    app: AppHandle,
    gate: VaultGate,
    inbox: mpsc::UnboundedSender<Msg>,
    /// Transcript entries for [`write_notes`], in order.
    notes: mpsc::UnboundedSender<NewEntry>,
    shared: Arc<Mutex<Option<StateWire>>>,
    /// Counts audio sessions; reports of an earlier speaker are dropped.
    session: u64,
    audio: Option<Audio>,
    listening: Option<(u64, CancellationToken)>,
    /// The barge-in watcher while haex speaks (FR-016).
    watching: Option<listen::Watching>,
    /// The wake word watcher while haex is "bereit" (FR-011).
    wake_watching: Option<listen::WakeWatching>,
    turn: Option<RunningTurn>,
    /// The round whose chat turn has started, so an abort never hits a turn of a window.
    chat_round: Arc<AtomicU64>,
    language: String,
    clock: Clock,
}

impl RealPorts {
    fn report_error(&self, error: &HolziError) {
        log::warn!("assistant: {error}");
        let _ = self.app.emit(EVENT_ERROR, error_payload(error));
    }

    /// Transcribes an utterance of `listener` and hands the text back to the driver.
    fn transcribe(&self, listener: u64, utterance: Utterance) {
        let app = self.app.clone();
        let inbox = self.inbox.clone();
        let language = self.language.clone();
        let Some(audio) = self.audio.as_ref() else {
            return;
        };
        let speakers = Arc::clone(&audio.speakers);
        let blocking = self.gate.clone();
        let spawned = self.gate.spawn(async move {
            // Someone not enrolled changes nothing (FR-016a): no request, no stop word, no
            // answer; the listener goes on.
            let samples = utterance.samples.clone();
            let admitted = match blocking.spawn_blocking(move || speakers.admits(&samples)) {
                Ok(task) => task.await.unwrap_or(false),
                Err(_) => return,
            };
            if !admitted {
                log::info!("assistant: an utterance of a voice not enrolled was ignored");
                return;
            }
            let text = match listen::transcribe(&app, utterance, &language).await {
                Ok(text) => text,
                Err(error) => {
                    log::warn!("assistant: transcription failed: {error}");
                    String::new()
                }
            };
            let _ = inbox.send(Msg::Input(Input::Heard { listener, text }));
        });
        if let Err(error) = spawned {
            self.report_error(&error);
        }
    }

    /// Applies a saved model choice to the next sentence in an open conversation.
    async fn refresh_speech_model(&mut self, language: &str) -> Result<()> {
        if self.audio.is_none() || self.language != language {
            return Ok(());
        }
        let app = self.app.clone();
        let (resolved, engines) = tts::commands::load_installed(&app, &self.gate, language).await?;
        let entry = resolved.speaking;
        let Some(audio) = self.audio.as_mut() else {
            return Ok(());
        };
        let chosen = resolved.chosen;
        if let Some(chosen) = chosen {
            let notify = make_fallback_notice(&app, chosen, &audio.fallback_sent);
            let reason = if engines.load_failed {
                Some(tts::commands::FallbackReason::LoadFailed)
            } else if chosen.id != entry.id {
                Some(tts::commands::FallbackReason::NotInstalled)
            } else {
                None
            };
            if let Some(reason) = reason {
                notify(reason);
            }
        }
        if !audio
            .meters
            .iter()
            .any(|(_, model)| model.id == engines.entry.id)
        {
            let meter = Arc::new(tts::speed::Meter::new(&engines.dir, "cpu"));
            audio.meters.push((meter, engines.entry));
        }
        audio.model = engines.entry;
        audio.engine = engines.engine;
        audio.reserve = engines.reserve;
        audio.speech = audio
            .engine
            .part_words()
            .map(|_| Arc::new(tts::check::Level) as Arc<dyn tts::check::SpeechShare>);
        Ok(())
    }

    /// Starts listener `id` from `start` (FR-016: after a barge-in with what was said just before).
    fn listen_from(&mut self, id: u64, start: Start) {
        self.stop_listening();
        let Some(audio) = &self.audio else {
            return;
        };
        let listening = match listen::listen(
            &self.gate,
            &audio.stream,
            &audio.vad_model,
            audio.pause_ms,
            start,
        ) {
            Ok(listening) => listening,
            Err(error) => {
                self.report_error(&HolziError::SpeechFailed {
                    reason: format!("listen: {error}"),
                });
                return;
            }
        };
        let stop = CancellationToken::new();
        let stopped = stop.clone();
        let inbox = self.inbox.clone();
        // Hands each utterance to the driver; dropping `listening` stops the pause detection.
        let spawned = self.gate.spawn_blocking(move || loop {
            if stopped.is_cancelled() {
                return;
            }
            match listening
                .utterances()
                .recv_timeout(Duration::from_millis(50))
            {
                Ok(utterance) => {
                    if inbox
                        .send(Msg::Utterance {
                            listener: id,
                            utterance,
                        })
                        .is_err()
                    {
                        return;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
        });
        match spawned {
            Ok(_) => self.listening = Some((id, stop)),
            Err(error) => self.report_error(&error),
        }
    }

    /// Queues `text`; the meter hears the sentences of answers only (spec 081 FR-007).
    fn say_with(&mut self, text: String, answer: bool) -> u64 {
        let Some(audio) = self.audio.as_mut() else {
            return 0;
        };
        // An answer may quote a passage in another language (FR-022d); haex's own lines stay in
        // the conversation's language.
        let passage = answer.then(|| Passage {
            language: audio.passage.of(&text).to_owned(),
            voices: Arc::clone(&audio.other_voices),
        });
        audio.speaker.speak(Sentence {
            text,
            engine: Arc::clone(&audio.engine),
            reference: Arc::clone(&audio.reference),
            rate: audio.rate,
            reserve: audio.reserve.clone(),
            speech: audio.speech.clone(),
            meter: if answer {
                audio
                    .meters
                    .iter()
                    .find(|(_, model)| model.id == audio.model.id)
                    .map(|(meter, _)| Arc::clone(meter))
            } else {
                None
            },
            reserve_notice: audio.engine.part_words().map(|_| {
                let app = self.app.clone();
                let chosen = audio.model;
                let sent = Arc::clone(&audio.fallback_sent);
                Arc::new(move || {
                    make_fallback_notice(&app, chosen, &sent)(
                        tts::commands::FallbackReason::PartFailed,
                    );
                }) as Arc<dyn Fn() + Send + Sync>
            }),
            passage,
        })
    }
}

impl Ports for RealPorts {
    fn tone(&mut self, tone: ToneId) -> Duration {
        let Some(audio) = &self.audio else {
            return Duration::ZERO;
        };
        tones::play(&self.app, audio.stream.mixer(), tone, audio.tones_enabled)
    }

    fn listen(&mut self, id: u64) {
        self.listen_from(id, Start::Now);
    }

    fn listen_after_barge_in(&mut self, id: u64) {
        self.listen_from(id, Start::OnTrigger);
    }

    fn watch_barge_in(&mut self, id: u64) {
        self.stop_watching();
        let Some(audio) = &self.audio else {
            return;
        };
        let inbox = self.inbox.clone();
        match listen::watch_barge_in(
            &self.gate,
            &audio.stream,
            &audio.vad_model,
            Arc::clone(&audio.speakers),
            Box::new(move || {
                let _ = inbox.send(Msg::Input(Input::BargeIn { watcher: id }));
            }),
        ) {
            Ok(watching) => self.watching = Some(watching),
            Err(error) => self.report_error(&HolziError::SpeechFailed {
                reason: format!("watch for barge-in: {error}"),
            }),
        }
    }

    fn stop_watching(&mut self) {
        self.watching = None;
    }

    fn watch_wake_word(&mut self, id: u64) {
        self.stop_wake_word();
        let Some(audio) = &self.audio else {
            return;
        };
        let Some(models) = audio.wake.clone() else {
            return;
        };
        let inbox = self.inbox.clone();
        match listen::watch_wake_word(
            &self.gate,
            &audio.stream,
            &audio.vad_model,
            models,
            audio.wake_rule,
            Box::new(move || {
                let _ = inbox.send(Msg::Input(Input::WakeWord { watcher: id }));
            }),
        ) {
            Ok(watching) => self.wake_watching = Some(watching),
            Err(error) => self.report_error(&HolziError::SpeechFailed {
                reason: format!("wait for the wake word: {error}"),
            }),
        }
    }

    fn stop_wake_word(&mut self) {
        self.wake_watching = None;
    }

    fn stop_listening(&mut self) {
        if let Some((_, stop)) = self.listening.take() {
            stop.cancel();
        }
    }

    fn start_turn(
        &mut self,
        round: u64,
        thread_id: Option<Uuid>,
        text: String,
        delegate_notice_due: bool,
    ) {
        let cancel = self.gate.token().child_token();
        let abort_requested = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.turn = Some(RunningTurn {
            round,
            cancel: cancel.clone(),
            abort_requested: Arc::clone(&abort_requested),
        });
        let app = self.app.clone();
        let inbox = self.inbox.clone();
        let chat_round = Arc::clone(&self.chat_round);
        let requested = Arc::clone(&abort_requested);
        let round_cancel = cancel.clone();
        let turn = VoiceTurn {
            thread_id,
            text,
            language: self.language.clone(),
            clock: self.clock,
            delegate_notice_due,
            cancel,
        };
        let sink_inbox = inbox.clone();
        let sink: turn::Sink = Arc::new(move |output| {
            let _ = sink_inbox.send(Msg::Input(Input::Turn { round, output }));
        });
        let spawned = self.gate.spawn(async move {
            match turn::start(&app, turn, sink).await {
                Ok(Started::Running(_)) => {
                    chat_round.store(round, Ordering::Release);
                    // End can arrive after chat::start_turn creates its generation but before
                    // this task publishes that fact to the actor. In that window abort_turn
                    // cancels the VoiceTurn token but cannot yet safely abort the global chat
                    // generation. Recheck here, claiming the abort once across both paths.
                    if round_cancel.is_cancelled()
                        && requested
                            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                            .is_ok()
                    {
                        if let Err(error) =
                            crate::chat::commands::abort_turn(&app.state::<ChatState>())
                        {
                            log::warn!("assistant: could not abort a just-started round: {error}");
                        }
                    }
                }
                Ok(Started::GaveUp | Started::Cancelled) => {}
                Err(error) => {
                    log::warn!("assistant: the round could not start: {error}");
                    let _ = app.emit(EVENT_ERROR, error_payload(&error));
                    let _ = inbox.send(Msg::Input(Input::TurnFailed { round }));
                }
            }
        });
        if let Err(error) = spawned {
            self.report_error(&error);
            let _ = self.inbox.send(Msg::Input(Input::TurnFailed { round }));
        }
    }

    fn abort_turn(&mut self) {
        let Some(turn) = self.turn.take() else {
            return;
        };
        turn.cancel.cancel();
        if self.chat_round.load(Ordering::Acquire) == turn.round
            && turn
                .abort_requested
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            if let Err(error) = crate::chat::commands::abort_turn(&self.app.state::<ChatState>()) {
                self.report_error(&error);
            }
        }
    }

    fn say(&mut self, text: String) -> u64 {
        self.say_with(text, false)
    }

    fn say_answer(&mut self, text: String) -> u64 {
        self.say_with(text, true)
    }

    fn finish_answer(&mut self) {
        if let Some(audio) = self.audio.as_mut() {
            audio.speaker.finish_answer();
            audio.passage.reset();
        }
    }

    fn stop_output(&mut self) {
        if let Some(audio) = &self.audio {
            audio.speaker.stop();
        }
    }

    fn publish(&mut self, state: &StateWire) {
        *self.shared.lock().unwrap_or_else(PoisonError::into_inner) = Some(state.clone());
        let _ = self.app.emit(EVENT_STATE, state);
    }

    fn record(&mut self, thread_id: Uuid, note: Note) {
        // The answer is over and every sentence of it sounded or was stopped: store how fast it
        // was spoken (spec 081 FR-007, FR-011).
        if note.kind == EntryKind::Answer {
            if let Some(audio) = &self.audio {
                let app = self.app.clone();
                let language = self.language.clone();
                let answers = audio
                    .meters
                    .iter()
                    .map(|(meter, model)| (Arc::clone(meter), meter.take_answer(), *model))
                    .filter(|(_, sentences, _)| !sentences.is_empty())
                    .collect::<Vec<_>>();
                let spawned = self.gate.spawn(async move {
                    for (meter, sentences, model) in answers {
                        tts::commands::answer_spoken(&app, meter, sentences, model, &language)
                            .await;
                    }
                });
                if let Err(error) = spawned {
                    log::warn!("assistant: store the speech speed: {error}");
                }
            }
        }
        let _ = self.notes.send(NewEntry {
            thread_id,
            spoken_at: unix_ms(note.at),
            speaker: note.speaker,
            kind: note.kind,
            text: note.text,
            spoken_until: note.spoken_until,
            outcome: note.outcome,
            ref_id: note.ref_id,
        });
    }

    fn close_audio(&mut self) {
        self.stop_listening();
        self.watching = None;
        self.wake_watching = None;
        if let Some(audio) = self.audio.take() {
            audio.speaker.stop();
            self.app.state::<VoiceState>().audio_stream().stop();
            audio.stream.stop();
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// `at` as Unix milliseconds.
fn unix_ms(at: Instant) -> i64 {
    now_ms() - Instant::now().saturating_duration_since(at).as_millis() as i64
}

/// `TranscriptEntryWire` of contracts/tauri-commands.md: a stored entry; `final` is always true
/// for now, text that is still being spoken is not sent yet.
#[derive(Clone, serde::Serialize)]
struct TranscriptEntryWire {
    #[serde(flatten)]
    entry: TranscriptEntry,
    #[serde(rename = "final")]
    is_final: bool,
}

/// Stores transcript entries one after the other, so `seq` follows the order of the
/// conversation, and shows each to the window.
async fn write_notes(app: AppHandle, mut notes: mpsc::UnboundedReceiver<NewEntry>) {
    while let Some(entry) = notes.recv().await {
        let state = app.state::<AppState>();
        let stored = async {
            let db = crate::state_utils::active_database(&state)?;
            let now = now_ms();
            db.write(move |tx| voice_transcript::append(tx, entry, now))
                .await
        }
        .await;
        match stored {
            Ok(entry) => {
                let _ = app.emit(
                    EVENT_TRANSCRIPT,
                    TranscriptEntryWire {
                        entry,
                        is_final: true,
                    },
                );
            }
            Err(error) => log::warn!("assistant: could not store a transcript entry: {error}"),
        }
    }
}

#[tauri::command]
pub async fn assistant_get_state(driver: State<'_, AssistantDriver>) -> Result<StateWire> {
    Ok(driver.state())
}

/// The toggle of the assistant (FR-001, operator 2026-11-02). On loads the models of the
/// conversation language (FR-002a: the one chosen on this device's assistant page, else the
/// vault's interface language; `language`, the window's, only when neither is known; nothing is
/// downloaded) and opens the microphone;
/// off ends a conversation, aborts a running round and closes the microphone.
/// `utc_offset_minutes` is the window's offset from UTC, for dates and times spoken in local time.
#[tauri::command]
pub async fn assistant_set_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    driver: State<'_, AssistantDriver>,
    enabled: bool,
    language: String,
    utc_offset_minutes: i32,
) -> Result<StateWire> {
    let gate = state.gate().clone();
    let language = if enabled {
        conversation_language(&app, &state, language).await?
    } else {
        language
    };
    let (reply, replied) = oneshot::channel();
    driver.send(
        &app,
        &gate,
        Msg::SetEnabled {
            enabled,
            language,
            clock: Clock::now(utc_offset_minutes),
            reply,
        },
    )?;
    gate.run(replied).await?.map_err(|_| closing())??;
    // Switched off, the driver forgets the mute too.
    remember(&app, &state, PREF_ENABLED, enabled).await;
    if !enabled {
        remember(&app, &state, PREF_MUTED, false).await;
    }
    Ok(driver.state())
}

/// The language of a spoken conversation on this device (FR-002a): its choice on the assistant
/// page, else the interface language. A failed read falls back to the window's `language` like a
/// typed turn does (`chat::send`), so it never keeps the assistant from starting.
pub(super) async fn conversation_language(
    app: &AppHandle,
    state: &State<'_, AppState>,
    language: String,
) -> Result<String> {
    let db = crate::state_utils::active_database(state)?;
    let device = crate::device::commands::resolve_vault_device_uuid(app, &db).await?;
    match db
        .read(move |r| crate::chat::reply_language::read_spoken(r, device))
        .await
    {
        Ok(fixed) => Ok(fixed.map_or(language, |fixed| fixed.code().to_owned())),
        Err(error) => {
            log::warn!("assistant: could not read the conversation language: {error}");
            Ok(language)
        }
    }
}

/// Stores `value` under `key` for this device. A failure is logged: the switch itself stands, it
/// is only not restored after the next unlock.
async fn remember(app: &AppHandle, state: &State<'_, AppState>, key: &'static str, value: bool) {
    let stored = async {
        let db = crate::state_utils::active_database(state)?;
        let device = crate::device::commands::resolve_vault_device_uuid(app, &db).await?;
        write_switch(&db, device, key, value).await
    }
    .await;
    if let Err(error) = stored {
        log::warn!("assistant: could not store {key}: {error}");
    }
}

/// Writes a switch of the assistant as the device preference `key` (`"true"` / `"false"`).
pub(super) async fn write_switch(
    db: &crate::vault_gate::VaultDb,
    device: Uuid,
    key: &'static str,
    value: bool,
) -> Result<()> {
    let value = if value { "true" } else { "false" };
    db.write(move |tx| {
        preferences::insert_or_update(tx, PrefScope::Device(device), key, value).map(|_| ())
    })
    .await?;
    Ok(())
}

/// Applies a saved speech-model preference to an open conversation at its next sentence.
#[tauri::command]
pub async fn assistant_apply_speech_model(
    app: AppHandle,
    state: State<'_, AppState>,
    driver: State<'_, AssistantDriver>,
    language: String,
) -> Result<()> {
    let gate = state.gate().clone();
    let (reply, replied) = oneshot::channel();
    driver.send(&app, &gate, Msg::RefreshSpeechModel { language, reply })?;
    gate.run(replied).await?.map_err(|_| closing())??;
    Ok(())
}

/// "Gespräch beenden" (FR-014, FR-039): ends the conversation, also a running round.
#[tauri::command]
pub async fn assistant_end_conversation(
    app: AppHandle,
    state: State<'_, AppState>,
    driver: State<'_, AssistantDriver>,
) -> Result<()> {
    let gate = state.gate().clone();
    gate.run(driver.handle(&app, &gate, Input::End)).await?
}

/// Mute on or off (US9). Muted, haex neither listens nor speaks; answers show in the window.
#[tauri::command]
pub async fn assistant_set_muted(
    app: AppHandle,
    state: State<'_, AppState>,
    driver: State<'_, AssistantDriver>,
    muted: bool,
) -> Result<StateWire> {
    let gate = state.gate().clone();
    gate.run(driver.handle(&app, &gate, Input::Mute(muted)))
        .await??;
    remember(&app, &state, PREF_MUTED, muted).await;
    Ok(driver.state())
}
