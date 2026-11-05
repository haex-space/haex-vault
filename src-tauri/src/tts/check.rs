//! Checks a generated part before it plays (spec 081 FR-010a, research R6): not far longer than
//! its text allows, not ending after a few words, and actually speech rather than silence or
//! noise. Also trims the silence around a part, so parts follow each other like words after a
//! comma.

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

/// Speech found by its level (research R6): the share of 20 ms frames louder than
/// [`SPEECH_LEVEL`], and zero when those frames are as even as steady noise. Silero VAD, the
/// assistant's pause detection, was tried first and does not fit: it finds speech in stretches
/// between pauses of a live stream and reported about 1.3 s of speech in every part, however long
/// (2026-11-05, `qwen_tts_eval::verdicts`).
pub struct Level;

/// A frame louder than this (RMS, about −40 dBFS) counts as sound.
const SPEECH_LEVEL: f64 = 0.01;
/// Speech rises and falls from syllable to syllable; frames whose loudness varies less than this
/// (standard deviation over mean) are steady noise.
const MIN_VARIATION: f64 = 0.3;
/// Frame length in seconds.
const FRAME_SECS: f64 = 0.02;

impl SpeechShare for Level {
    fn speech_share(&self, samples: &[f32], rate: u32) -> Option<f64> {
        let frame = ((FRAME_SECS * f64::from(rate)) as usize).max(1);
        let levels: Vec<f64> = samples
            .chunks(frame)
            .map(|chunk| {
                let energy: f64 = chunk.iter().map(|s| f64::from(*s) * f64::from(*s)).sum();
                (energy / chunk.len() as f64).sqrt()
            })
            .collect();
        if levels.is_empty() {
            return Some(0.0);
        }
        let loud: Vec<f64> = levels
            .iter()
            .copied()
            .filter(|l| *l > SPEECH_LEVEL)
            .collect();
        if loud.len() < 2 {
            return Some(0.0);
        }
        let mean = loud.iter().sum::<f64>() / loud.len() as f64;
        let variance = loud.iter().map(|l| (l - mean).powi(2)).sum::<f64>() / loud.len() as f64;
        if variance.sqrt() / mean < MIN_VARIATION {
            return Some(0.0);
        }
        Some(loud.len() as f64 / levels.len() as f64)
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
    if has_speech(samples, rate, speech) {
        Verdict::Ok
    } else {
        Verdict::NoSpeech
    }
}

/// Whether `samples` at `rate` are speech rather than silence or noise; without a speech check
/// (or when it cannot tell) everything counts as speech.
pub fn has_speech(samples: &[f32], rate: u32, speech: Option<&dyn SpeechShare>) -> bool {
    speech
        .and_then(|s| s.speech_share(samples, rate))
        .is_none_or(|share| share >= MIN_SPEECH_SHARE)
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
