//! Listening on the audio stream (spec 066, research R6): pause detection over the 16 kHz mono
//! signal cuts what is said into utterances. Started on a trigger (the wake word), the last
//! [`PREROLL_SECS`] from the ring buffer go in first, so "Hey Haex, wie viele …" said in one
//! breath arrives whole. Pause length, minimum length and the 60 s cut come from
//! [`PauseDetector`]. [`transcribe`] turns an utterance into text with the speech recognition
//! model push-to-talk uses (T043).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager};

use super::vad::{PauseDetector, Utterance, VadError};
use super::wakeword::detector::Echo;
use crate::audio::ring::PREROLL_SECS;
use crate::audio::stream::AudioStream;
use crate::stt::{CanonicalPcm, SttError, SAMPLE_RATE_HZ};
use crate::vault_gate::VaultGate;
use crate::voice::VoiceState;

/// How often a waiting listener checks whether it should stop.
const POLL: Duration = Duration::from_millis(50);

/// How listening begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// With what is said from now on.
    Now,
    /// With the last [`PREROLL_SECS`] before the trigger.
    OnTrigger,
}

#[derive(Debug, thiserror::Error)]
pub enum ListenError {
    #[error(transparent)]
    Vad(#[from] VadError),
    #[error("the vault is closing")]
    Closing,
}

/// A running listener. Stopping or dropping it ends listening; utterances not taken yet are
/// dropped.
pub struct Listening {
    stop: Arc<AtomicBool>,
    utterances: Receiver<Utterance>,
}

impl Listening {
    /// Utterances in the order they ended. Disconnects once the stream has ended and the last
    /// utterance was delivered.
    pub fn utterances(&self) -> &Receiver<Utterance> {
        &self.utterances
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
    }
}

impl Drop for Listening {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Listens on `stream` with the Silero model at `model` and a pause of `pause_ms` (the device
/// setting `assistant.pauseMs`). Runs on the gate's blocking pool until stopped, the stream ends
/// or the vault closes.
pub fn listen(
    gate: &VaultGate,
    stream: &AudioStream,
    model: &Path,
    pause_ms: u32,
    start: Start,
) -> Result<Listening, ListenError> {
    let mut detector = PauseDetector::new(model, pause_ms)?;
    let preroll_secs = match start {
        Start::Now => 0.0,
        Start::OnTrigger => PREROLL_SECS,
    };
    let (preroll, blocks) = stream.subscribe(preroll_secs);
    let (sender, utterances) = channel();
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = Arc::clone(&stop);
    let closing = gate.token();
    gate.spawn_blocking(move || {
        // `false` once nobody takes utterances any more.
        let deliver = |found: Vec<Utterance>| found.into_iter().all(|u| sender.send(u).is_ok());
        if !deliver(detector.push(&preroll)) {
            return;
        }
        while !stopped.load(Ordering::Acquire) && !closing.is_cancelled() {
            match blocks.recv_timeout(POLL) {
                Ok(block) => {
                    if !deliver(detector.push(&block)) {
                        return;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    deliver(detector.finish());
                    return;
                }
            }
        }
    })
    .map_err(|_| ListenError::Closing)?;
    Ok(Listening { stop, utterances })
}

/// How loud (RMS of the last [`BARGE_IN_WINDOW`] samples, full scale 1.0) speech over haex must
/// be to count as the user talking over it (FR-016): the cancelled signal keeps a faint rest of
/// haex's voice, the user speaks clearly above it. Measured per platform in T103.
pub const BARGE_IN_MIN_RMS: f32 = 0.005;
/// 250 ms at 16 kHz.
const BARGE_IN_WINDOW: usize = SAMPLE_RATE_HZ as usize / 4;
/// Speech a barge-in collects before it asks whether the voice is enrolled (FR-016a): an
/// embedding of less is unreliable. Barge-in takes longer then, which is the price of ignoring
/// everyone else.
const VERIFY_SAMPLES: usize = SAMPLE_RATE_HZ as usize;

/// A running barge-in watcher. Stopping or dropping it ends watching.
pub struct Watching {
    stop: Arc<AtomicBool>,
}

impl Drop for Watching {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

/// The loudness of `samples` as root mean square.
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// Watches `stream` (with haex's own output cancelled) while haex speaks: once the pause detection
/// hears speech (at least [`super::vad::MIN_SPEECH_SECS`]) and its last 250 ms reach
/// [`BARGE_IN_MIN_RMS`], `heard` is called once and watching ends (FR-016). With `speakers`, the
/// speech must also be an enrolled person's, judged on its first second (FR-016a); anyone else's
/// is ignored and watching goes on.
pub fn watch_barge_in(
    gate: &VaultGate,
    stream: &AudioStream,
    model: &Path,
    speakers: Arc<super::speakers::LiveGate>,
    heard: Box<dyn FnOnce() + Send>,
) -> Result<Watching, ListenError> {
    let mut detector = PauseDetector::new(model, super::vad::PAUSE_MS_MIN)?;
    let (_, blocks) = stream.subscribe(0.0);
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = Arc::clone(&stop);
    let closing = gate.token();
    gate.spawn_blocking(move || {
        let mut recent: Vec<f32> = Vec::with_capacity(2 * BARGE_IN_WINDOW);
        // The speech since it began, for the check of the voice; `None` once it was refused.
        let mut speech: Option<Vec<f32>> = Some(Vec::new());
        while !stopped.load(Ordering::Acquire) && !closing.is_cancelled() {
            match blocks.recv_timeout(POLL) {
                Ok(block) => {
                    detector.push(&block);
                    recent.extend_from_slice(&block);
                    if recent.len() > BARGE_IN_WINDOW {
                        recent.drain(..recent.len() - BARGE_IN_WINDOW);
                    }
                    if !detector.in_speech() {
                        speech = Some(Vec::new());
                        continue;
                    }
                    let Some(said) = speech.as_mut() else {
                        // This stretch of speech is someone not enrolled.
                        continue;
                    };
                    let remaining = VERIFY_SAMPLES.saturating_sub(said.len());
                    said.extend_from_slice(&block[..block.len().min(remaining)]);
                    if rms(&recent) < BARGE_IN_MIN_RMS {
                        continue;
                    }
                    if speakers.required() && said.len() < VERIFY_SAMPLES {
                        continue;
                    }
                    let admitted = speakers.admits(said);
                    if !admitted {
                        log::info!("assistant: a voice not enrolled talked over haex; ignored");
                        speech = None;
                        continue;
                    }
                    if !stopped.load(Ordering::Acquire) {
                        heard();
                    }
                    return;
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    })
    .map_err(|_| ListenError::Closing)?;
    Ok(Watching { stop })
}

/// How long after haex was last audible a block still counts as holding its output: the device's
/// buffer and the room's reverb (FR-010).
const OWN_OUTPUT_TAIL: Duration = Duration::from_millis(300);

/// A running wake word watcher. Stopping or dropping it ends watching.
pub struct WakeWatching {
    stop: Arc<AtomicBool>,
}

impl Drop for WakeWatching {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

/// Waits on `stream` for the wake word (FR-011, T066): the pause detection says when someone
/// speaks, and only then the wake word models run (research R4). `heard` is called once when the
/// detector fires by `rule`; watching ends then.
pub fn watch_wake_word(
    gate: &VaultGate,
    stream: &AudioStream,
    model: &Path,
    models: Arc<super::wakeword::model::Models>,
    rule: super::wakeword::Rule,
    heard: Box<dyn FnOnce() + Send>,
) -> Result<WakeWatching, ListenError> {
    let mut vad = PauseDetector::new(model, super::vad::PAUSE_MS_MIN)?;
    let mut detector =
        super::wakeword::detector::Detector::new(super::wakeword::model::TractScorer(models), rule);
    let (_, blocks) = stream.subscribe(0.0);
    let audible = stream.mixer().audible();
    let own = if stream.cancels_echo() {
        Echo::Cancelled
    } else {
        Echo::Raw
    };
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = Arc::clone(&stop);
    let closing = gate.token();
    gate.spawn_blocking(move || {
        while !stopped.load(Ordering::Acquire) && !closing.is_cancelled() {
            match blocks.recv_timeout(POLL) {
                Ok(block) => {
                    vad.push(&block);
                    let echo = if audible.within(OWN_OUTPUT_TAIL) {
                        own
                    } else {
                        Echo::None
                    };
                    if detector.push(&block, vad.in_speech(), echo) {
                        if !stopped.load(Ordering::Acquire) {
                            heard();
                        }
                        return;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    })
    .map_err(|_| ListenError::Closing)?;
    Ok(WakeWatching { stop })
}

/// What `utterance` said, through the selected speech recognition model (Parakeet).
/// The model is the warm one of `VoiceState` that push-to-talk uses too, never a second copy.
/// The utterance is taken as speech in `language`, the one the conversation is held in.
pub async fn transcribe(
    app: &AppHandle,
    utterance: Utterance,
    language: &str,
) -> Result<String, SttError> {
    let adapter = app.state::<VoiceState>().stt_adapter(app).await?;
    let text = adapter
        .transcribe_in(
            &CanonicalPcm {
                samples: utterance.samples,
            },
            language,
        )
        .await?;
    Ok(text.trim().to_owned())
}
