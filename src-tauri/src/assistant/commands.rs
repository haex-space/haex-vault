//! The commands of the voice assistant (spec 066 T044, contracts/tauri-commands.md) and the
//! runtime that drives [`Core`] with the real audio stream, speaker, listener and chat turns.
//!
//! One task per process owns the `Core` and takes everything in order through one inbox: the
//! commands, the utterances of the listener, the outputs of the round and the speaker's reports.
//! It ends with the vault (`VaultGate`), which releases microphone and speaker.
//!
//! Until the wake word (PR D) the microphone is open only during a conversation: "Sprechen"
//! prepares the models and opens it, and the driver closes it after the conversation.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::assets;
use super::conversation::{Timing, PREF_CONTINUE_MINS, PREF_FOLLOW_UP_SECS};
use super::driver::{Core, Input, Ports, StateWire};
use super::listen::{self, Start};
use super::speakable::Clock;
use super::tones::{self, ToneId};
use super::turn::{self, Started, VoiceTurn};
use super::vad::Utterance;
use crate::audio::stream::AudioStream;
use crate::chat::session::ChatState;
use crate::error::{HolziError, Result};
use crate::state::AppState;
use crate::storage::preferences::{self, PrefScope};
use crate::tts::local::{Engine, Reference, Sentence, Speaker, SpeechOut};
use crate::tts::{self, catalog, voices};
use crate::vault_gate::VaultGate;
use crate::voice::VoiceState;

/// Event with the state of the assistant (`AssistantStateWire`).
pub const EVENT_STATE: &str = "assistant-state";
/// Event when something keeps the assistant from working (`{kind, message}`).
pub const EVENT_ERROR: &str = "assistant-error";

const PREF_PAUSE_MS: &str = "assistant.pauseMs";
const PREF_SPEECH_RATE: &str = "assistant.speechRate";
const PREF_TONES_ENABLED: &str = "assistant.tones.enabled";
const PREF_TONES_VOLUME: &str = "assistant.tones.volume";
/// Defaults of data-model.md.
const PAUSE_MS_DEFAULT: u32 = 800;
const TONES_VOLUME_DEFAULT: u8 = 60;

/// The assistant of this process, as managed state.
#[derive(Default)]
pub struct AssistantDriver {
    inbox: Mutex<Option<mpsc::UnboundedSender<Msg>>>,
    state: Arc<Mutex<Option<StateWire>>>,
}

enum Msg {
    Talk {
        language: String,
        clock: Clock,
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
        let ports = RealPorts {
            app: app.clone(),
            gate: gate.clone(),
            inbox: inbox.clone(),
            shared: Arc::clone(&self.state),
            session: 0,
            audio: None,
            listening: None,
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
                    Msg::Talk { language, clock, reply } => {
                        let result = talk(&mut core, &mut ports, language, clock).await;
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

/// "Sprechen": opens microphone and speaker with the models of `language` if no conversation
/// holds them yet, then wakes the conversation.
async fn talk(
    core: &mut Core,
    ports: &mut RealPorts,
    language: String,
    clock: Clock,
) -> Result<()> {
    let language = assets::voice_language(&language).to_owned();
    ports.clock = clock;
    if !core.is_open() {
        let (audio, timing) = open_audio(ports, &language).await?;
        ports.audio = Some(audio);
        ports.language = language.clone();
        core.prepare(&language, timing);
    }
    core.on(Input::Talk, Instant::now(), ports);
    Ok(())
}

/// What a conversation speaks and listens with.
struct Audio {
    stream: Arc<AudioStream>,
    speaker: Speaker,
    engine: Arc<dyn Engine>,
    reference: Arc<Reference>,
    rate: f32,
    tones_enabled: bool,
    vad_model: PathBuf,
    pause_ms: u32,
}

/// The device settings the assistant reads when a conversation starts.
struct Settings {
    timing: Timing,
    pause_ms: u32,
    rate: f32,
    voice: Option<String>,
    tones_enabled: bool,
    tones_volume: u8,
}

async fn read_settings(app: &AppHandle, language: &str) -> Result<Settings> {
    let state = app.state::<AppState>();
    let db = crate::state_utils::active_database(&state)?;
    let device = crate::device::commands::resolve_vault_device_uuid(app, &db).await?;
    let voice_key = format!("assistant.voice.{language}");
    let keys = [
        PREF_FOLLOW_UP_SECS.to_owned(),
        PREF_CONTINUE_MINS.to_owned(),
        PREF_PAUSE_MS.to_owned(),
        PREF_SPEECH_RATE.to_owned(),
        voice_key,
        PREF_TONES_ENABLED.to_owned(),
        PREF_TONES_VOLUME.to_owned(),
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
        timing: Timing::from_prefs(int(0), int(1)),
        pause_ms: int(2).map_or(PAUSE_MS_DEFAULT, |v| v.clamp(0, i64::from(u32::MAX)) as u32),
        rate: values[3]
            .as_deref()
            .and_then(|v| v.trim().parse::<f32>().ok())
            .and_then(|v| tts::checked_rate(v).ok())
            .unwrap_or(tts::RATE_DEFAULT),
        voice: values[4].clone(),
        tones_enabled: values[5]
            .as_deref()
            .is_none_or(|v| !matches!(v.trim(), "false" | "0")),
        tones_volume: int(6).map_or(TONES_VOLUME_DEFAULT, |v| v.clamp(0, 100) as u8),
    })
}

/// Loads what a conversation needs and opens microphone and speaker. Nothing is downloaded here:
/// a missing model is reported, the settings page offers the download (FR-005).
async fn open_audio(ports: &mut RealPorts, language: &str) -> Result<(Audio, Timing)> {
    let app = ports.app.clone();
    let settings = read_settings(&app, language).await?;
    let vad_model = assets::vad_model_path(&app)?;
    let voice_state = app.state::<VoiceState>();
    // Warm the speech recognition model now, not at the first utterance.
    voice_state.stt_adapter(&app).await?;

    let entry = catalog::default_for(language)?.ok_or_else(|| tts::TtsError::Unavailable {
        reason: format!("no speech model speaks {language}"),
    })?;
    let app_for_dir = app.clone();
    let dir = tauri::async_runtime::spawn_blocking(move || {
        tts::files::resolve_model_dir(&app_for_dir, entry)
            .ok()
            .filter(|dir| tts::files::is_complete_model(dir, entry.engine))
    })
    .await
    .map_err(|e| HolziError::SpeechFailed {
        reason: format!("look for the speech model: {e}"),
    })?
    .ok_or_else(|| tts::TtsError::Unavailable {
        reason: format!("the speech model for {language} is not downloaded"),
    })?;
    let engine: Arc<dyn Engine> = voice_state
        .speech_model(&entry.id, dir, &ports.gate)
        .await?;
    let voice = voices::resolve(settings.voice.as_deref(), language)?;
    let reference = Arc::new(voices::reference(voice)?);

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
    voice_state
        .audio_stream()
        .replace(Some(Arc::clone(&stream)));

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
    Ok((
        Audio {
            stream,
            speaker,
            engine,
            reference,
            rate: settings.rate,
            tones_enabled: settings.tones_enabled,
            vad_model,
            pause_ms: settings.pause_ms,
        },
        settings.timing,
    ))
}

struct RunningTurn {
    round: u64,
    cancel: CancellationToken,
}

struct RealPorts {
    app: AppHandle,
    gate: VaultGate,
    inbox: mpsc::UnboundedSender<Msg>,
    shared: Arc<Mutex<Option<StateWire>>>,
    /// Counts audio sessions; reports of an earlier speaker are dropped.
    session: u64,
    audio: Option<Audio>,
    listening: Option<(u64, CancellationToken)>,
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
        let spawned = self.gate.spawn(async move {
            let text = match listen::transcribe(&app, utterance).await {
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
}

impl Ports for RealPorts {
    fn tone(&mut self, tone: ToneId) -> Duration {
        let Some(audio) = &self.audio else {
            return Duration::ZERO;
        };
        tones::play(&self.app, audio.stream.mixer(), tone, audio.tones_enabled);
        if audio.tones_enabled || tones::always_audible(tone) {
            tones::duration(tone)
        } else {
            Duration::ZERO
        }
    }

    fn listen(&mut self, id: u64) {
        self.stop_listening();
        let Some(audio) = &self.audio else {
            return;
        };
        let listening = match listen::listen(
            &self.gate,
            &audio.stream,
            &audio.vad_model,
            audio.pause_ms,
            Start::Now,
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
        self.turn = Some(RunningTurn {
            round,
            cancel: cancel.clone(),
        });
        let app = self.app.clone();
        let inbox = self.inbox.clone();
        let chat_round = Arc::clone(&self.chat_round);
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
                Ok(Started::Running(_)) => chat_round.store(round, Ordering::Release),
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
        if self.chat_round.load(Ordering::Acquire) == turn.round {
            if let Err(error) = crate::chat::commands::abort_turn(&self.app.state::<ChatState>()) {
                self.report_error(&error);
            }
        }
    }

    fn say(&mut self, text: String) -> u64 {
        let Some(audio) = &self.audio else {
            return 0;
        };
        audio.speaker.speak(Sentence {
            text,
            engine: Arc::clone(&audio.engine),
            reference: Arc::clone(&audio.reference),
            rate: audio.rate,
        })
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

    fn close_audio(&mut self) {
        self.stop_listening();
        if let Some(audio) = self.audio.take() {
            audio.speaker.stop();
            self.app.state::<VoiceState>().audio_stream().stop();
            audio.stream.stop();
        }
    }
}

/// The local time of the window: now, `utc_offset_minutes` ahead of UTC.
fn window_clock(utc_offset_minutes: i32) -> Clock {
    let unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    Clock::at(unix, utc_offset_minutes.clamp(-14 * 60, 14 * 60))
}

#[tauri::command]
pub async fn assistant_get_state(driver: State<'_, AssistantDriver>) -> Result<StateWire> {
    Ok(driver.state())
}

/// "Sprechen" in the indicator or the shortcut (FR-011). `language` is the window's language,
/// `utc_offset_minutes` its offset from UTC (dates and times are spoken in local time).
#[tauri::command]
pub async fn assistant_talk(
    app: AppHandle,
    state: State<'_, AppState>,
    driver: State<'_, AssistantDriver>,
    language: String,
    utc_offset_minutes: i32,
) -> Result<()> {
    let gate = state.gate().clone();
    let (reply, replied) = oneshot::channel();
    driver.send(
        &app,
        &gate,
        Msg::Talk {
            language,
            clock: window_clock(utc_offset_minutes),
            reply,
        },
    )?;
    gate.run(replied).await?.map_err(|_| closing())?
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
    Ok(driver.state())
}
