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
use std::sync::{Arc, Mutex, PoisonError};
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
use super::check::SpeechShare;
use super::files;
use super::speaker::{speak_sentence, Voices};
use super::speed::Meter;
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

    /// How the log names this model, for a failed part (spec 081 FR-010a): its catalog id and
    /// where it runs.
    fn log_name(&self) -> String {
        "a speech model".to_owned()
    }

    /// Where this model computes; it waits while the local chat model computes there (spec 081
    /// FR-016). Pocket TTS runs on the processor.
    fn compute_unit(&self) -> crate::compute::Unit {
        crate::compute::Unit::Processor
    }

    /// Speaks one part whose `words` are already written out (`tts::spoken`) in
    /// `reference.language` (spec 081 FR-010). `attempt` is 0 for the first try of these words; a
    /// retry passes 1, 2, … so a model that samples draws differently (FR-010a). Gives up, returning
    /// nothing, once `cancelled` says so; an engine that cannot stop inside a call checks it only
    /// before it starts.
    fn speak_part(
        &self,
        words: &str,
        reference: &Reference,
        rate: f32,
        attempt: u32,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<f32>, TtsError> {
        let _ = attempt;
        if cancelled() {
            return Ok(Vec::new());
        }
        self.generate(words, reference, rate)
    }
}

/// A loaded Pocket model.
pub struct PocketModel {
    tts: OfflineTts,
    sample_rate: u32,
    /// The language the model speaks; its sentences are written out in it (`tts::spoken`).
    language: String,
    /// The catalog id, for the log.
    id: String,
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
            id: entry.id.clone(),
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

    fn log_name(&self) -> String {
        format!("{} on the processor", self.id)
    }

    fn generate(&self, text: &str, reference: &Reference, rate: f32) -> Result<Vec<f32>, TtsError> {
        // Numbers, times and abbreviations as words: the model reads only letters well.
        let text = super::spoken::spoken(&speakable_text(text), &self.language);
        self.speak_part(&text, reference, rate, 0, &|| false)
    }

    fn speak_part(
        &self,
        text: &str,
        reference: &Reference,
        rate: f32,
        _attempt: u32,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<f32>, TtsError> {
        if text.is_empty() || cancelled() {
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
            .generate_with_config(text, &config, None::<fn(&[f32], f32) -> bool>)
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
    /// Speaks a part the engine failed twice on: the language's Pocket model while Qwen3-TTS
    /// speaks (spec 081 FR-010a).
    pub reserve: Option<Arc<dyn Engine>>,
    /// Finds parts that are silence or noise (spec 081 FR-010a).
    pub speech: Option<Arc<dyn SpeechShare>>,
    /// Measures how fast the engine speaks (spec 081 FR-007).
    pub meter: Option<Arc<Meter>>,
    /// Emits the selected model's fallback notice when Pocket speaks a failed part.
    pub reserve_notice: Option<Arc<dyn Fn() + Send + Sync>>,
}

struct Job {
    generation: u64,
    ticket: u64,
    /// `None` marks the end of an answer in the ordered speaker queue.
    sentence: Option<Sentence>,
}

/// A spoken sentence kept back until the answer can play without gaps (spec 081 FR-017).
struct Held {
    ticket: u64,
    generation: u64,
    /// At the mixer's rate.
    samples: Vec<f32>,
    chars: usize,
}

/// Whether an answer may start playing (spec 081 FR-017): with `ratio` seconds of computing per
/// second of speech, what is still to be computed (`remaining_chars`, estimated at the pace of the
/// sentences held so far) is ready before the held audio has played once the held audio is at
/// least `ratio - 1` times as long as the rest. An empty queue alone does not mean the answer
/// ended: its explicit end marker flushes a short answer that cannot reach the threshold.
pub(crate) fn may_start(
    ratio: f64,
    held_secs: f64,
    held_chars: usize,
    remaining_chars: usize,
    // The worker has consumed the end marker after the answer's last sentence.
    answer_finished: bool,
) -> bool {
    if answer_finished {
        return true;
    }
    // The model may still be producing the next sentence; an empty queue is not a completed
    // answer and cannot be treated as zero seconds of work remaining.
    if remaining_chars == 0 {
        return false;
    }
    if held_chars == 0 {
        return false;
    }
    let remaining_secs = held_secs / held_chars as f64 * remaining_chars as f64;
    held_secs >= (ratio - 1.0) * remaining_secs
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
            pending: VecDeque::new(),
            held: Vec::new(),
            holding: None,
            answer_generation: None,
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
            sentence: Some(sentence),
        });
        ticket
    }

    /// Marks the end of the current answer after its sentences have been queued.
    pub fn finish_answer(&self) {
        let generation = self.generation.load(Ordering::Acquire);
        let _ = self.jobs.send(Job {
            generation,
            ticket: 0,
            sentence: None,
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
    played: Played,
    /// Sentences handed to the mixer and not reported yet: ticket, generation, when it is over.
    sounding: VecDeque<(u64, u64, Instant)>,
    /// When the last queued sentence is over, and the generation it belongs to: a stop drops
    /// what the mixer holds, so a newer generation starts from now.
    sounds_until: Instant,
    sounds_generation: u64,
    /// Sentences taken from the queue early, to see how much of the answer is still to come.
    pending: VecDeque<Job>,
    /// Sentences of an answer kept back until it can play without gaps (FR-017).
    held: Vec<Held>,
    /// The measured ratio while the answer is held; `None` while it plays as it comes.
    holding: Option<f64>,
    /// Keeps an answer from entering hold mode a second time after an underrun released its audio.
    /// Cleared only by that answer's end marker or a stop of its generation.
    answer_generation: Option<u64>,
}

impl Worker {
    fn run(mut self) {
        loop {
            self.drop_stale_held();
            self.report_played();
            if self.closing.is_cancelled() {
                return;
            }
            let job = match self.pending.pop_front() {
                Some(job) => job,
                None => match self.queue.recv_timeout(POLL) {
                    Ok(job) => job,
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => return,
                },
            };
            if self.ended.load(Ordering::Acquire) {
                return;
            }
            if self.is_stale(&job) {
                self.drop_stale_held();
                if job.ticket != 0 {
                    self.silenced(&job);
                }
                continue;
            }
            let Some(sentence) = job.sentence.as_ref() else {
                if self.answer_generation == Some(job.generation) && self.holding.is_some() {
                    self.release();
                }
                if self.answer_generation == Some(job.generation) {
                    self.answer_generation = None;
                }
                continue;
            };
            // A new answer begins when nothing sounds or waits: a model slower than real time
            // holds it back until it can play without gaps (FR-017).
            if self.answer_generation.is_none()
                && self.held.is_empty()
                && self.holding.is_none()
                && (self.sounds_until <= Instant::now() || job.generation != self.sounds_generation)
            {
                self.holding = sentence
                    .meter
                    .as_ref()
                    .and_then(|meter| meter.ratio())
                    .filter(|ratio| *ratio > 1.0);
                if self.holding.is_some() {
                    self.answer_generation = Some(job.generation);
                }
            }
            let hold = self.holding.is_some();
            let mut kept: Vec<f32> = Vec::new();
            let mixer = self.out.mixer();
            let cancelled = || self.is_stale(&job) || self.closing.is_cancelled();
            // Where this sentence's sound ends: after what is queued before it, at the earliest
            // when a part is queued. Parts that come later than the audio before them ran out
            // start when they are queued.
            let mut ends = (job.generation == self.sounds_generation).then_some(self.sounds_until);
            // Speech was flushed elsewhere (a voice sample on the shared mixer): the rest of the
            // sentence is not heard either.
            let mut flushed = false;
            let spoken_parts = Mutex::new(Vec::new());
            let record_part = |part| {
                spoken_parts
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(part)
            };
            let notice = sentence.reserve_notice.as_deref();
            let voices = Voices {
                engine: &*sentence.engine,
                reserve: sentence.reserve.as_deref(),
                reference: &sentence.reference,
                rate: sentence.rate,
                speech: sentence.speech.as_deref(),
                reserve_notice: notice,
                part_spoken: Some(&record_part),
            };
            let spoken =
                speak_sentence(voices, &sentence.text, &cancelled, &mut |samples, rate| {
                    let samples = resample_linear(samples, rate, mixer.sample_rate());
                    if hold {
                        kept.extend_from_slice(&samples);
                        return !cancelled();
                    }
                    // Playback starts once what is queued before has played, at the earliest now;
                    // waiting for room in the mixer's queue does not delay it.
                    let queued_at = Instant::now();
                    // Waits while the mixer's queue is full; returns early on a stop.
                    if !mixer.queue_speech(&samples, cancelled) {
                        flushed = !cancelled();
                        return false;
                    }
                    let length = Duration::from_secs_f64(
                        samples.len() as f64 / f64::from(mixer.sample_rate()),
                    );
                    ends = Some(ends.map_or(queued_at, |e| e.max(queued_at)) + length);
                    !cancelled()
                });
            let stopped = cancelled();
            // Cover a stop racing with the final block enqueue too. The worker has not
            // queued a newer sentence yet, so this only drops the cancelled sentence.
            if stopped {
                self.out.mixer().stop_speech();
                self.drop_stale_held();
                self.silenced(&job);
                continue;
            }
            if flushed {
                self.silenced(&job);
                continue;
            }
            // Parts queued before a failure still play, so the sentence ends with them.
            match spoken {
                Ok(speed) => {
                    if let Some(meter) = &sentence.meter {
                        meter.add_sentence(speed);
                    }
                }
                Err(error) => {
                    if let Some(meter) = &sentence.meter {
                        meter.add_sentence(super::speed::SentenceSpeed {
                            parts: spoken_parts
                                .into_inner()
                                .unwrap_or_else(PoisonError::into_inner),
                        });
                    }
                    log::warn!("speech synthesis failed: {error}");
                }
            }
            if let Some(ratio) = self.holding {
                self.held.push(Held {
                    ticket: job.ticket,
                    generation: job.generation,
                    samples: kept,
                    chars: sentence.text.chars().count(),
                });
                if self.release_due(ratio) {
                    self.release();
                }
                continue;
            }
            match ends {
                Some(until) => self.sounds_until_at(&job, until),
                None => self.silenced(&job),
            }
        }
    }

    fn is_stale(&self, job: &Job) -> bool {
        job.generation != self.generation.load(Ordering::Acquire)
    }

    /// Whether the held answer may start (FR-017): looks at what else is queued of it, through its
    /// explicit end marker. An empty queue can mean the model has not produced the next sentence.
    fn release_due(&mut self, ratio: f64) -> bool {
        while let Ok(job) = self.queue.try_recv() {
            self.pending.push_back(job);
        }
        let current = self.generation.load(Ordering::Acquire);
        let mut remaining_chars = 0;
        let mut answer_finished = false;
        let mut has_pending_sentence = false;
        for job in &self.pending {
            if job.generation != current {
                continue;
            }
            if job.sentence.is_none() {
                answer_finished = !has_pending_sentence;
                break;
            }
            has_pending_sentence = true;
            remaining_chars += job
                .sentence
                .as_ref()
                .map_or(0, |sentence| sentence.text.chars().count());
        }
        let rate = f64::from(self.out.mixer().sample_rate().max(1));
        let held_secs = self.held.iter().map(|h| h.samples.len()).sum::<usize>() as f64 / rate;
        let held_chars = self.held.iter().map(|h| h.chars).sum();
        may_start(
            ratio,
            held_secs,
            held_chars,
            remaining_chars,
            answer_finished,
        )
    }

    /// Plays the held sentences one after another; the rest of the answer plays as it comes.
    fn release(&mut self) {
        self.holding = None;
        let out = Arc::clone(&self.out);
        let rate = f64::from(out.mixer().sample_rate().max(1));
        let current = Arc::clone(&self.generation);
        let closing = self.closing.clone();
        let mut queued = Vec::new();
        for held in std::mem::take(&mut self.held) {
            let cancelled =
                held.generation != current.load(Ordering::Acquire) || closing.is_cancelled();
            if held.samples.is_empty() || cancelled {
                self.silenced_ticket(held.ticket, held.generation);
                continue;
            }
            let queued_at = Instant::now();
            let start = if held.generation == self.sounds_generation {
                self.sounds_until.max(queued_at)
            } else {
                queued_at
            };
            let until = start + Duration::from_secs_f64(held.samples.len() as f64 / rate);
            self.sounds_until_ticket(held.ticket, held.generation, until);
            queued.push(held);
        }
        if queued.is_empty() {
            return;
        }

        // Queue held audio on its own thread. It takes the mixer lock before returning, so the
        // worker can synthesize the next sentence while the feeder fills the mixer, and later
        // sentences cannot overtake the held ones.
        let (started_tx, started_rx) = std::sync::mpsc::sync_channel(0);
        let feeder = std::thread::Builder::new()
            .name("holzi-speech-feeder".to_owned())
            .spawn(move || {
                let mixer = out.mixer();
                let batches = queued
                    .iter()
                    .map(|held| held.samples.as_slice())
                    .collect::<Vec<_>>();
                let mut started = Some(started_tx);
                let first_generation = queued.first().map(|held| held.generation);
                let _ = mixer.queue_speech_batch(
                    &batches,
                    || {
                        first_generation
                            .is_some_and(|generation| generation != current.load(Ordering::Acquire))
                            || closing.is_cancelled()
                    },
                    || {
                        if let Some(started) = started.take() {
                            let _ = started.send(());
                        }
                    },
                );
            });
        match feeder {
            Ok(_) => {
                let _ = started_rx.recv();
            }
            Err(error) => {
                log::warn!("speech playback feeder could not start: {error}");
            }
        }
    }

    /// A stop silences what is held: reported in order, before anything after it.
    fn drop_stale_held(&mut self) {
        let current = self.generation.load(Ordering::Acquire);
        let stale_answer = self
            .answer_generation
            .is_some_and(|generation| generation != current);
        if !stale_answer && self.held.iter().all(|h| h.generation == current) {
            return;
        }
        for held in std::mem::take(&mut self.held) {
            self.silenced_ticket(held.ticket, held.generation);
        }
        self.holding = None;
        self.answer_generation = None;
    }

    /// Notes that `job` sounds until `until`.
    fn sounds_until_at(&mut self, job: &Job, until: Instant) {
        self.sounds_until_ticket(job.ticket, job.generation, until);
    }

    fn sounds_until_ticket(&mut self, ticket: u64, generation: u64, until: Instant) {
        self.sounds_generation = generation;
        self.sounds_until = until;
        self.sounding
            .push_back((ticket, generation, until + PLAYOUT));
        self.report_played();
    }

    /// Notes that `job` was skipped, failed or stopped: it is over with what is queued before it.
    fn silenced(&mut self, job: &Job) {
        self.silenced_ticket(job.ticket, job.generation);
    }

    fn silenced_ticket(&mut self, ticket: u64, generation: u64) {
        let until = self
            .sounding
            .back()
            .map_or_else(Instant::now, |(_, _, until)| *until);
        self.sounding.push_back((ticket, generation, until));
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
