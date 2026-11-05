//! Checks a generated part before it plays (spec 081 FR-010a, research R6): not far longer than
//! its text allows, not ending after a few words, and actually speech rather than silence or
//! noise. Also trims the silence around a part, so parts follow each other like words after a
//! comma.

use std::path::{Path, PathBuf};

use crate::assistant::vad::PauseDetector;
use crate::audio::resample::resample_filtered;
use crate::stt::SAMPLE_RATE_HZ;

/// Start allowance of a part, in seconds (research R6).
// ponytail: 1.5 s, 0.16 s and 0.035 s per character are from the R23 sentences (about 0.07 s per
// character of German); revisit with the 200-sentence run of SC-006.
const START_SECS: f64 = 1.5;
/// Twice the measured pace per character: more counts as running on.
const MAX_SECS_PER_CHAR: f64 = 0.16;
/// Half the measured pace per character: less counts as ending early.
const MIN_SECS_PER_CHAR: f64 = 0.035;
/// Below this share of speech a part is silence or noise.
const MIN_SPEECH_SHARE: f64 = 0.4;
/// Pause the VAD may find inside one part without ending it, in ms.
const VAD_PAUSE_MS: u32 = 500;
/// Silence kept before and after a part, in seconds.
const KEEP_SILENCE_SECS: f64 = 0.15;
/// Amplitude below which a sample counts as silence when trimming.
const SILENCE_LEVEL: f32 = 0.01;

/// The longest a part of `chars` characters may be, in seconds.
pub fn max_secs(chars: usize) -> f64 {
    START_SECS + MAX_SECS_PER_CHAR * chars as f64
}

/// The shortest a part of `chars` characters may be, in seconds.
pub fn min_secs(chars: usize) -> f64 {
    MIN_SECS_PER_CHAR * chars as f64
}

/// What a check found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Ok,
    /// As long as the cap or longer: the model ran on.
    TooLong,
    /// Shorter than half the normal pace: the model stopped after a few words.
    TooShort,
    /// Too little speech: silence or noise.
    NoSpeech,
}

/// Measures how much of some audio is speech.
pub trait SpeechShare: Send + Sync {
    /// The share of `samples` (mono at `rate`) that is speech, 0.0 to 1.0; `None` when it cannot
    /// tell.
    fn speech_share(&self, samples: &[f32], rate: u32) -> Option<f64>;
}

/// Silero VAD, the assistant's pause detection model, as a [`SpeechShare`]. Every check gets a
/// detector of its own: reusing one after `finish` crashed inside sherpa-onnx after about a hundred
/// parts (2026-11-05, `qwen_tts_eval::two_hundred_sentences`), and building one takes milliseconds.
pub struct Silero {
    model: PathBuf,
}

impl Silero {
    pub fn new(model: &Path) -> Self {
        Self {
            model: model.to_path_buf(),
        }
    }
}

impl SpeechShare for Silero {
    fn speech_share(&self, samples: &[f32], rate: u32) -> Option<f64> {
        if samples.is_empty() {
            return Some(0.0);
        }
        let mut detector = PauseDetector::new(&self.model, VAD_PAUSE_MS).ok()?;
        let pcm = resample_filtered(samples, rate, SAMPLE_RATE_HZ);
        let mut utterances = detector.push(&pcm);
        utterances.extend(detector.finish());
        let speech: usize = utterances.iter().map(|u| u.samples.len()).sum();
        Some((speech as f64 / pcm.len() as f64).min(1.0))
    }
}

/// Judges a part of `chars` characters that came out as `samples` at `rate`.
pub fn judge(
    samples: &[f32],
    rate: u32,
    chars: usize,
    speech: Option<&dyn SpeechShare>,
) -> Verdict {
    let secs = samples.len() as f64 / f64::from(rate.max(1));
    // The generator stops at the cap; reaching it (within one frame) means it ran on.
    if secs + 0.08 >= max_secs(chars) {
        return Verdict::TooLong;
    }
    if secs < min_secs(chars) {
        return Verdict::TooShort;
    }
    match speech.and_then(|s| s.speech_share(samples, rate)) {
        Some(share) if share < MIN_SPEECH_SHARE => Verdict::NoSpeech,
        _ => Verdict::Ok,
    }
}

/// `samples` with the silence before and after cut down to [`KEEP_SILENCE_SECS`].
pub fn trim_silence(samples: &[f32], rate: u32) -> Vec<f32> {
    let keep = (KEEP_SILENCE_SECS * f64::from(rate)) as usize;
    let Some(first) = samples.iter().position(|s| s.abs() >= SILENCE_LEVEL) else {
        return Vec::new();
    };
    let last = samples
        .iter()
        .rposition(|s| s.abs() >= SILENCE_LEVEL)
        .unwrap_or(first);
    let start = first.saturating_sub(keep);
    let end = (last + 1 + keep).min(samples.len());
    samples[start..end].to_vec()
}
