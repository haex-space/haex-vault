//! Local synthesis (spec 066 T023, research R8): Pocket TTS through sherpa-onnx, one sentence at a
//! time on a dedicated thread, straight into the assistant's mixer.
//!
//! Pocket has no built-in voices: every call gets a short reference recording, whose embedding
//! sherpa keeps in a cache. The assistant cuts answers into sentences itself (FR-022), because
//! sherpa reports progress only at the end of a sentence; so stopping takes effect before the next
//! sentence, and [`Speaker::stop`] also drops what the mixer still holds.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

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

use super::catalog::TtsCatalogEntry;
use super::files;
use super::TtsError;

pub use super::{RATE_DEFAULT, RATE_MAX, RATE_MIN};
/// Reference embeddings sherpa keeps, one per voice used recently.
const VOICE_CACHE: i32 = 8;
/// Pocket's sampling temperature when a catalog entry names none: what the spike measured (spec
/// 066). Entries carry their own since spec 081 FR-015.
const TEMPERATURE: &str = "0.7";
/// The longest reference sherpa reads, in seconds: the whole of every reference
/// ([`super::clone::MAX_REFERENCE_SECS`]; the built-in voices are 8–15 s).
pub(crate) const MAX_REFERENCE_LEN: &str = "30";
/// How often the worker looks for a close while it waits.
const POLL: Duration = Duration::from_millis(50);
/// The device still plays its own buffer after the mixer handed a sentence on; a sentence counts
/// as played this much after its samples ran out.
const PLAYOUT: Duration = Duration::from_millis(150);

/// A voice's reference recording, mono at any rate, and the language the voice speaks.
pub struct Reference {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    /// The voice's language; a model that speaks several (Qwen3-TTS, spec 081) speaks in it.
    pub language: String,
}

/// Something that turns one sentence into mono samples at [`Engine::sample_rate`].
pub trait Engine: Send + Sync {
    fn sample_rate(&self) -> u32;
    fn generate(&self, text: &str, reference: &Reference, rate: f32) -> Result<Vec<f32>, TtsError>;

    /// The most words one call should speak; the speaker splits longer sentences into parts
    /// (spec 081 FR-010). `None`: whole sentences.
    fn part_words(&self) -> Option<usize> {
        None
    }

    /// [`Engine::generate`] that gives up as soon as `cancelled` says so; an engine that cannot
    /// stop inside a call checks it only before it starts.
    fn generate_cancellable(
        &self,
        text: &str,
        reference: &Reference,
        rate: f32,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<f32>, TtsError> {
        if cancelled() {
            return Ok(Vec::new());
        }
        self.generate(text, reference, rate)
    }
}

/// A loaded Pocket model.
pub struct PocketModel {
    tts: OfflineTts,
    sample_rate: u32,
    /// The language the model speaks; its sentences are written out in it (`tts::spoken`).
    language: String,
    /// Sampling temperature of the catalog entry (spec 081 FR-015).
    temperature: String,
}

impl PocketModel {
    /// Loads the model of `entry` from `dir` (files as `files::files_for` names them). Takes about
    /// a second on a desktop, so callers load off the async runtime.
    pub fn load(dir: &Path, entry: &TtsCatalogEntry) -> Result<Self, TtsError> {
        let language = entry
            .languages
            .first()
            .ok_or_else(|| TtsError::Unavailable {
                reason: format!("{} names no language", entry.id),
            })?;
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
        Ok(Self {
            tts,
            sample_rate,
            language: language.clone(),
            temperature: entry
                .temperature
                .clone()
                .unwrap_or_else(|| TEMPERATURE.to_owned()),
        })
    }
}

impl Engine for PocketModel {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn generate(&self, text: &str, reference: &Reference, rate: f32) -> Result<Vec<f32>, TtsError> {
        // Numbers, times and abbreviations as words: the model reads only letters well.
        let text = super::spoken::spoken(&speakable_text(text), &self.language);
        if text.is_empty() {
            return Ok(Vec::new());
        }
        let config = GenerationConfig {
            reference_audio: Some(reference.samples.clone()),
            reference_sample_rate: i32::try_from(reference.sample_rate).unwrap_or(i32::MAX),
            num_steps: 1,
            extra: Some(
                [
                    ("temperature".to_owned(), json!(self.temperature)),
                    // sherpa reads only the first 10 s by default and cuts mid-word; Kyutai reads
                    // up to 30 s, and every reference ends on a pause within that (`tts::clone`).
                    (
                        "max_reference_audio_len".to_owned(),
                        json!(MAX_REFERENCE_LEN),
                    ),
                ]
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
    ticket: u64,
    sentence: Sentence,
}

/// Told the ticket of every sentence once it has sounded, was skipped by a stop or failed, in the
/// order [`Speaker::speak`] handed the tickets out.
pub type Played = Arc<dyn Fn(u64) + Send + Sync>;

/// Speaks sentences in order on its own thread. Dropping it ends the thread after the sentence
/// in progress; a vault close ends it too.
pub struct Speaker {
    jobs: Sender<Job>,
    generation: Arc<AtomicU64>,
    tickets: AtomicU64,
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
        Self::start_reporting(gate, out, Arc::new(|_| {}))
    }

    /// Like [`start`](Self::start), and tells `played` when each sentence is over, so the
    /// assistant knows when haex has finished speaking (it listens only then).
    pub fn start_reporting(
        gate: &VaultGate,
        out: Arc<dyn SpeechOut>,
        played: Played,
    ) -> Result<Self, TtsError> {
        let (jobs, queue) = mpsc::channel();
        let generation = Arc::new(AtomicU64::new(0));
        let ended = Arc::new(AtomicBool::new(false));
        let worker = Worker {
            queue,
            generation: Arc::clone(&generation),
            out: Arc::clone(&out),
            closing: gate.token(),
            ended: Arc::clone(&ended),
            played,
            sounding: VecDeque::new(),
            sounds_until: Instant::now(),
            sounds_generation: 0,
        };
        gate.spawn_blocking(move || worker.run())
            .map_err(|_| TtsError::Unavailable {
                reason: "the vault is closing".to_owned(),
            })?;
        Ok(Self {
            jobs,
            generation,
            tickets: AtomicU64::new(0),
            out,
            ended,
        })
    }

    /// Queues `sentence` behind the ones already queued. Returns its ticket: 1 for the first
    /// sentence of this speaker, one more for every further one.
    pub fn speak(&self, sentence: Sentence) -> u64 {
        let generation = self.generation.load(Ordering::Acquire);
        let ticket = self.tickets.fetch_add(1, Ordering::AcqRel) + 1;
        // A send fails only once the worker has ended (vault closing); nothing to speak then.
        let _ = self.jobs.send(Job {
            generation,
            ticket,
            sentence,
        });
        ticket
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
    played: Played,
    /// Sentences handed to the mixer and not reported yet: ticket, generation, when it is over.
    sounding: VecDeque<(u64, u64, Instant)>,
    /// When the last queued sentence is over, and the generation it belongs to: a stop drops
    /// what the mixer holds, so a newer generation starts from now.
    sounds_until: Instant,
    sounds_generation: u64,
}

impl Worker {
    fn run(mut self) {
        loop {
            self.report_played();
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
                self.silenced(&job);
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
                        self.silenced(&job);
                        continue;
                    }
                };
            if self.is_stale(&job) || self.closing.is_cancelled() {
                self.silenced(&job);
                continue;
            }
            let mixer = self.out.mixer();
            let samples =
                resample_linear(&samples, sentence.engine.sample_rate(), mixer.sample_rate());
            // Playback starts once what is queued before has played, at the earliest now; waiting
            // for room in the mixer's queue does not delay it.
            let queued_at = Instant::now();
            // Waits while the mixer's queue is full; returns early on a stop.
            mixer.queue_speech(&samples, || {
                self.is_stale(&job) || self.closing.is_cancelled()
            });
            // Cover a stop racing with the final block enqueue too. The worker has not
            // queued a newer sentence yet, so this only drops the cancelled sentence.
            if self.is_stale(&job) || self.closing.is_cancelled() {
                mixer.stop_speech();
                self.silenced(&job);
                continue;
            }
            let length =
                Duration::from_secs_f64(samples.len() as f64 / f64::from(mixer.sample_rate()));
            self.sounds(&job, queued_at, length);
        }
    }

    fn is_stale(&self, job: &Job) -> bool {
        job.generation != self.generation.load(Ordering::Acquire)
    }

    /// Notes that `job`, handed to the mixer at `queued_at`, sounds for `length` after what is
    /// queued before it.
    fn sounds(&mut self, job: &Job, queued_at: Instant, length: Duration) {
        if job.generation != self.sounds_generation {
            self.sounds_generation = job.generation;
            self.sounds_until = queued_at;
        }
        self.sounds_until = self.sounds_until.max(queued_at) + length;
        self.sounding
            .push_back((job.ticket, job.generation, self.sounds_until + PLAYOUT));
        self.report_played();
    }

    /// Notes that `job` was skipped, failed or stopped: it is over with what is queued before it.
    fn silenced(&mut self, job: &Job) {
        let until = self
            .sounding
            .back()
            .map_or_else(Instant::now, |(_, _, until)| *until);
        self.sounding.push_back((job.ticket, job.generation, until));
        self.report_played();
    }

    /// Reports, in order, the sentences that are over or were silenced by a stop.
    fn report_played(&mut self) {
        let now = Instant::now();
        let current = self.generation.load(Ordering::Acquire);
        while let Some(&(ticket, generation, until)) = self.sounding.front() {
            if until > now && generation == current {
                return;
            }
            self.sounding.pop_front();
            (self.played)(ticket);
        }
    }
}
