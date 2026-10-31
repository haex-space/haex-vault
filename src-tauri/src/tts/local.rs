//! Local synthesis (spec 066 T023, research R8): Pocket TTS through sherpa-onnx, one sentence at a
//! time on a dedicated thread, straight into the assistant's mixer.
//!
//! Pocket has no built-in voices: every call gets a short reference recording, whose embedding
//! sherpa keeps in a cache. The assistant cuts answers into sentences itself (FR-022), because
//! sherpa reports progress only at the end of a sentence; so stopping takes effect before the next
//! sentence, and [`Speaker::stop`] also drops what the mixer still holds.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use sherpa_onnx::{
    GenerationConfig, OfflineTts, OfflineTtsConfig, OfflineTtsModelConfig,
    OfflineTtsPocketModelConfig,
};
use tokio_util::sync::CancellationToken;

use crate::audio::mixer::MixerInput;
use crate::audio::resample::resample_linear;
use crate::audio::stream::AudioStream;
use crate::audio::stretch::stretch;
use crate::vault_gate::VaultGate;

use super::files;
use super::TtsError;

/// Range and default of the device preference `assistant.speechRate` (data-model.md).
pub const RATE_MIN: f32 = 0.75;
pub const RATE_MAX: f32 = 1.5;
pub const RATE_DEFAULT: f32 = 1.0;
/// Reference embeddings sherpa keeps, one per voice used recently.
const VOICE_CACHE: i32 = 8;
/// Pocket samples its output; 0.7 is Kyutai's default and what the spike measured.
const TEMPERATURE: &str = "0.7";
/// How often the worker looks for a close while it waits.
const POLL: Duration = Duration::from_millis(50);

/// A voice's reference recording, mono at any rate.
pub struct Reference {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

/// Something that turns one sentence into mono samples at [`Engine::sample_rate`].
pub trait Engine: Send + Sync {
    fn sample_rate(&self) -> u32;
    fn generate(&self, text: &str, reference: &Reference, rate: f32) -> Result<Vec<f32>, TtsError>;
}

/// A loaded Pocket model.
pub struct PocketModel {
    tts: OfflineTts,
    sample_rate: u32,
}

impl PocketModel {
    /// Loads the model in `dir` (files as `files::files_for(Pocket)` names them). Takes about a
    /// second on a desktop, so callers load off the async runtime.
    pub fn load(dir: &Path) -> Result<Self, TtsError> {
        let path = |name: &str| -> Result<Option<String>, TtsError> {
            dir.join(name)
                .to_str()
                .map(|p| Some(p.to_owned()))
                .ok_or_else(|| TtsError::Unavailable {
                    reason: format!("model path {} is not UTF-8", dir.display()),
                })
        };
        let config = OfflineTtsConfig {
            model: OfflineTtsModelConfig {
                pocket: OfflineTtsPocketModelConfig {
                    lm_flow: path(files::POCKET_LM_FLOW)?,
                    lm_main: path(files::POCKET_LM_MAIN)?,
                    encoder: path(files::POCKET_ENCODER)?,
                    decoder: path(files::POCKET_DECODER)?,
                    text_conditioner: path(files::POCKET_TEXT_CONDITIONER)?,
                    vocab_json: path(files::POCKET_VOCAB)?,
                    token_scores_json: path(files::POCKET_TOKEN_SCORES)?,
                    voice_embedding_cache_capacity: VOICE_CACHE,
                },
                num_threads: std::thread::available_parallelism()
                    .map(|n| n.get().min(4) as i32)
                    .unwrap_or(1),
                ..Default::default()
            },
            ..Default::default()
        };
        let tts = OfflineTts::create(&config).ok_or_else(|| TtsError::Unavailable {
            reason: format!("load the speech model in {}", dir.display()),
        })?;
        let sample_rate = u32::try_from(tts.sample_rate()).map_err(|_| TtsError::Unavailable {
            reason: "the speech model reports no sample rate".to_owned(),
        })?;
        Ok(Self { tts, sample_rate })
    }
}

impl Engine for PocketModel {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn generate(&self, text: &str, reference: &Reference, rate: f32) -> Result<Vec<f32>, TtsError> {
        let text = speakable_text(text);
        if text.is_empty() {
            return Ok(Vec::new());
        }
        let config = GenerationConfig {
            reference_audio: Some(reference.samples.clone()),
            reference_sample_rate: i32::try_from(reference.sample_rate).unwrap_or(i32::MAX),
            num_steps: 1,
            extra: Some(
                [("temperature".to_owned(), json!(TEMPERATURE))]
                    .into_iter()
                    .collect(),
            ),
            ..Default::default()
        };
        let audio = self
            .tts
            .generate_with_config(&text, &config, None::<fn(&[f32], f32) -> bool>)
            .ok_or_else(|| TtsError::Failed {
                reason: "the speech model produced nothing".to_owned(),
            })?;
        // Pocket ignores sherpa's `speed`; the rate is applied to its output instead.
        Ok(stretch(
            audio.samples(),
            self.sample_rate,
            rate.clamp(RATE_MIN, RATE_MAX),
        ))
    }
}

/// `text` without the characters sherpa cannot take: a NUL makes it panic, other control
/// characters are read out as noise. Whitespace collapses to single spaces.
pub fn speakable_text(text: &str) -> String {
    text.split(|c: char| c.is_whitespace() || c.is_control())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Where synthesized speech goes: the mixer of a running stream, or a bare mixer in tests.
pub trait SpeechOut: Send + Sync {
    fn mixer(&self) -> &MixerInput;
}

impl SpeechOut for AudioStream {
    fn mixer(&self) -> &MixerInput {
        AudioStream::mixer(self)
    }
}

impl SpeechOut for MixerInput {
    fn mixer(&self) -> &MixerInput {
        self
    }
}

/// One sentence to speak.
pub struct Sentence {
    pub text: String,
    pub engine: Arc<dyn Engine>,
    pub reference: Arc<Reference>,
    pub rate: f32,
}

struct Job {
    generation: u64,
    sentence: Sentence,
}

/// Speaks sentences in order on its own thread. Dropping it ends the thread after the sentence
/// in progress; a vault close ends it too.
pub struct Speaker {
    jobs: Sender<Job>,
    generation: Arc<AtomicU64>,
    out: Arc<dyn SpeechOut>,
    ended: Arc<AtomicBool>,
}

impl Drop for Speaker {
    fn drop(&mut self) {
        self.ended.store(true, Ordering::Release);
    }
}

impl Speaker {
    pub fn start(gate: &VaultGate, out: Arc<dyn SpeechOut>) -> Result<Self, TtsError> {
        let (jobs, queue) = mpsc::channel();
        let generation = Arc::new(AtomicU64::new(0));
        let ended = Arc::new(AtomicBool::new(false));
        let worker = Worker {
            queue,
            generation: Arc::clone(&generation),
            out: Arc::clone(&out),
            closing: gate.token(),
            ended: Arc::clone(&ended),
        };
        gate.spawn_blocking(move || worker.run())
            .map_err(|_| TtsError::Unavailable {
                reason: "the vault is closing".to_owned(),
            })?;
        Ok(Self {
            jobs,
            generation,
            out,
            ended,
        })
    }

    /// Queues `sentence` behind the ones already queued.
    pub fn speak(&self, sentence: Sentence) {
        let generation = self.generation.load(Ordering::Acquire);
        // A send fails only once the worker has ended (vault closing); nothing to speak then.
        let _ = self.jobs.send(Job {
            generation,
            sentence,
        });
    }

    /// Silences speech: what the mixer holds is dropped at once, queued sentences are skipped, and
    /// the sentence being synthesized is thrown away when it is ready.
    pub fn stop(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.out.mixer().stop_speech();
    }
}

struct Worker {
    queue: Receiver<Job>,
    generation: Arc<AtomicU64>,
    out: Arc<dyn SpeechOut>,
    closing: CancellationToken,
    ended: Arc<AtomicBool>,
}

impl Worker {
    fn run(self) {
        loop {
            if self.closing.is_cancelled() {
                return;
            }
            let job = match self.queue.recv_timeout(POLL) {
                Ok(job) => job,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return,
            };
            if self.ended.load(Ordering::Acquire) {
                return;
            }
            if self.is_stale(&job) {
                continue;
            }
            let sentence = &job.sentence;
            let samples =
                match sentence
                    .engine
                    .generate(&sentence.text, &sentence.reference, sentence.rate)
                {
                    Ok(samples) => samples,
                    Err(error) => {
                        log::warn!("speech synthesis failed: {error}");
                        continue;
                    }
                };
            if self.is_stale(&job) || self.closing.is_cancelled() {
                continue;
            }
            let mixer = self.out.mixer();
            let samples =
                resample_linear(&samples, sentence.engine.sample_rate(), mixer.sample_rate());
            // Waits while the mixer's queue is full; returns early on a stop.
            mixer.queue_speech(&samples, || {
                self.is_stale(&job) || self.closing.is_cancelled()
            });
            // Cover a stop racing with the final block enqueue too. The worker has not
            // queued a newer sentence yet, so this only drops the cancelled sentence.
            if self.is_stale(&job) || self.closing.is_cancelled() {
                mixer.stop_speech();
            }
        }
    }

    fn is_stale(&self, job: &Job) -> bool {
        job.generation != self.generation.load(Ordering::Acquire)
    }
}
