//! How fast a speech model speaks on this device (spec 081 FR-007, FR-011, data-model.md "Speed
//! measurement"): computing time per second of speech over the last sentences it actually spoke,
//! kept per model and acceleration in `models/<slug>/speed.json`.
//!
//! The speaker hands each spoken sentence to a [`Meter`]; the assistant closes an answer with
//! [`Meter::answer_over`], which stores the sentences and counts the answers in a row that did not
//! keep up.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// The file next to a model's files.
pub const FILE_NAME: &str = "speed.json";
/// The version this build writes and reads; any other counts as no measurement. Version 2
/// (2026-11-06): computing time no longer includes waiting for the compute unit, and speech no
/// longer computes beside the chat model (FR-016); version 1 measured that slowdown and is dropped.
const VERSION: u32 = 2;
/// Sentences kept per acceleration.
const KEEP_SENTENCES: usize = 10;
/// Below this many sentences the speed is not known yet.
const MIN_SENTENCES: usize = 3;
/// Answers in a row that did not keep up before the hint (FR-011).
pub const SLOW_ANSWERS: u32 = 3;

/// One spoken sentence: its parts as `[compute seconds, audio seconds]`. The computing time of a
/// part includes its retries and, when the reserve spoke it, the reserve's time.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SentenceSpeed {
    pub parts: Vec<[f64; 2]>,
}

impl SentenceSpeed {
    pub(crate) fn sums(&self) -> (f64, f64) {
        self.parts
            .iter()
            .fold((0.0, 0.0), |(c, a), [compute, audio]| {
                (c + compute, a + audio)
            })
    }
}

/// The measurement of one acceleration.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    /// The last [`KEEP_SENTENCES`] sentences, newest last.
    pub sentences: Vec<SentenceSpeed>,
    /// Answers in a row whose own ratio was 1.0 or more.
    pub slow_streak: u32,
    /// When the last answer was measured, Unix milliseconds.
    pub measured_at_ms: u64,
}

/// Whether a model keeps up (FR-006).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Speed {
    KeepsUp,
    Slow,
    Unknown,
}

impl Run {
    /// Seconds of computing per second of speech over the kept sentences; `None` below
    /// [`MIN_SENTENCES`] sentences or without audio.
    pub fn ratio(&self) -> Option<f64> {
        if self.sentences.len() < MIN_SENTENCES {
            return None;
        }
        ratio_of(&self.sentences)
    }

    pub fn speed(&self) -> Speed {
        match self.ratio() {
            None => Speed::Unknown,
            Some(ratio) if ratio < 1.0 => Speed::KeepsUp,
            Some(_) => Speed::Slow,
        }
    }

    /// Adds the sentences of one answer. Returns `true` while the streak of slow answers is at
    /// least [`SLOW_ANSWERS`] long, so a hint that was held back on the third slow answer is asked
    /// again on the next one; the caller offers it once.
    pub fn add_answer(&mut self, sentences: Vec<SentenceSpeed>, now_ms: u64) -> bool {
        let Some(ratio) = ratio_of(&sentences) else {
            return false;
        };
        self.sentences.extend(sentences);
        let surplus = self.sentences.len().saturating_sub(KEEP_SENTENCES);
        self.sentences.drain(..surplus);
        self.measured_at_ms = now_ms;
        if ratio < 1.0 {
            self.slow_streak = 0;
            return false;
        }
        self.slow_streak = self.slow_streak.saturating_add(1);
        self.slow_streak >= SLOW_ANSWERS
    }
}

fn ratio_of(sentences: &[SentenceSpeed]) -> Option<f64> {
    let (compute, audio) = sentences
        .iter()
        .map(SentenceSpeed::sums)
        .fold((0.0, 0.0), |(c, a), (sc, sa)| (c + sc, a + sa));
    (audio > 0.0).then(|| compute / audio)
}

/// `speed.json`: one run per acceleration (`cpu`, `gpu`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeedFile {
    pub version: u32,
    pub runs: BTreeMap<String, Run>,
}

impl Default for SpeedFile {
    fn default() -> Self {
        Self {
            version: VERSION,
            runs: BTreeMap::new(),
        }
    }
}

impl SpeedFile {
    /// The run of `acceleration`; empty when nothing was measured there.
    pub fn run(&self, acceleration: &str) -> Run {
        self.runs.get(acceleration).cloned().unwrap_or_default()
    }
}

/// Reads `path`; a missing or unreadable file or another version counts as no measurement.
pub fn read(path: &Path) -> SpeedFile {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<SpeedFile>(&bytes).ok())
        .filter(|file| file.version == VERSION)
        .unwrap_or_default()
}

/// Writes `file` to `path` through a temporary file and a rename, so a reader never sees half a
/// file.
pub fn write(path: &Path, file: &SpeedFile) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(file).map_err(std::io::Error::other)?;
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, bytes)?;
    std::fs::rename(&temp, path)
}

/// The speed of the model whose files are in `dir`, on `acceleration`.
pub fn of_model(dir: &Path, acceleration: &str) -> Run {
    read(&dir.join(FILE_NAME)).run(acceleration)
}

/// Serializes the read-modify-write of speed files within this process: answers are stored from
/// the blocking pool, so two writes may overlap.
static PERSIST: Mutex<()> = Mutex::new(());

/// Collects the sentences one speech model speaks in a conversation and stores them per answer.
pub struct Meter {
    path: PathBuf,
    acceleration: &'static str,
    answer: Mutex<Vec<SentenceSpeed>>,
}

impl Meter {
    /// Measures the model whose files are in `dir`, running on `acceleration`.
    pub fn new(dir: &Path, acceleration: &'static str) -> Self {
        Self {
            path: dir.join(FILE_NAME),
            acceleration,
            answer: Mutex::new(Vec::new()),
        }
    }

    /// Notes a sentence of the answer being spoken.
    pub fn add_sentence(&self, sentence: SentenceSpeed) {
        if sentence.parts.is_empty() {
            return;
        }
        self.answer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(sentence);
    }

    #[cfg(test)]
    pub fn pending_sentences(&self) -> usize {
        self.answer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// Takes the sentences noted since the last answer ended; the answer they belong to is over.
    pub fn take_answer(&self) -> Vec<SentenceSpeed> {
        std::mem::take(&mut *self.answer.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Stores the sentences of one answer, one write per answer. Returns `true` when the answer
    /// did not keep up and the streak of such answers is at least [`SLOW_ANSWERS`] long.
    pub fn store_answer(&self, sentences: Vec<SentenceSpeed>) -> bool {
        if sentences.is_empty() {
            return false;
        }
        let _persisting = PERSIST.lock().unwrap_or_else(PoisonError::into_inner);
        let mut file = read(&self.path);
        let run = file.runs.entry(self.acceleration.to_owned()).or_default();
        let reached = run.add_answer(sentences, now_ms());
        if let Err(error) = write(&self.path, &file) {
            log::warn!("speech: store the speed measurement: {error}");
        }
        reached
    }

    /// Seconds of computing per second of speech of this model on this acceleration, as measured
    /// so far; `None` while not known (FR-017).
    pub fn ratio(&self) -> Option<f64> {
        read(&self.path).run(self.acceleration).ratio()
    }

    /// Ends the answer: stores what [`Meter::take_answer`] holds.
    pub fn answer_over(&self) -> bool {
        self.store_answer(self.take_answer())
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}
