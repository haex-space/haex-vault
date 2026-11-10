//! Checking a recording for an own voice (spec 066 FR-049, US8 scenario 7, research R20, T142).
//!
//! A voice is a reference recording the speech model imitates. Before it is kept, haex measures
//! it in 20-ms frames and names what is wrong: less than 10 s of speech, a median level of the
//! speech frames below −32 dBFS, more than 1 % of the samples at −1 dBFS or above, or less than
//! 10 dB between speech and the pauses. Silence at both ends is cut off, and a recording longer
//! than 30 s is cut at its last pause before 30 s, so a long file still makes a voice. Pocket TTS
//! reads the whole reference (up to [`MAX_REFERENCE_SECS`], `tts::local`), and a reference that
//! stops in the middle of a word disturbs what it speaks: Kyutai's own code lets the prompt end on
//! a pause (`end_on_pause`). Pure: the commands feed it samples from the microphone or a file.

use serde::Serialize;

/// The least speech a voice needs.
pub const MIN_SPEECH_SECS: f64 = 10.0;
/// The longest reference a voice keeps, pauses included; the speech model reads this much
/// (Kyutai's own limit for an audio prompt).
pub const MAX_REFERENCE_SECS: f64 = 30.0;
/// Frame length of the measurement.
const FRAME_SECS: f64 = 0.02;
/// A frame below this level is never speech.
const SPEECH_MIN_DB: f32 = -45.0;
/// A frame counts as speech when it is this much louder than the quietest tenth of the frames.
const SPEECH_ABOVE_FLOOR_DB: f32 = 6.0;
/// Median level of the speech frames below which a recording is too quiet.
const QUIET_DB: f32 = -32.0;
/// A sample at −1 dBFS or above is near full scale.
const CLIP_LEVEL: f32 = 0.891;
/// More than this share of samples near full scale is clipped.
const CLIP_SHARE: f64 = 0.01;
/// Less distance than this between speech and pauses is noisy.
const MIN_SNR_DB: f32 = 10.0;
/// Pauses shorter than this in total say nothing reliable about the noise.
const MIN_PAUSE_SECS: f64 = 0.2;
/// Silence kept before the first and after the last speech frame.
const MARGIN_FRAMES: usize = 10;
/// A cut of a long recording comes after at least this many frames without speech (0.2 s).
const PAUSE_FRAMES: usize = 10;

/// What is wrong with a recording (`CloneCheck.problems`, contracts/tauri-commands.md).
/// `TooLong` stays in the contract but is not reported: longer speech is cut to 30 s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Problem {
    TooShort,
    TooLong,
    TooQuiet,
    Clipped,
    Noisy,
}

/// The measurement of a recording.
#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    /// Speech in the kept part.
    pub speech_secs: f64,
    pub problems: Vec<Problem>,
    /// The samples to keep: silence at both ends cut off, at most [`MAX_REFERENCE_SECS`], ending on
    /// a pause wherever the recording has one.
    pub keep: std::ops::Range<usize>,
}

impl Analysis {
    pub fn ok(&self) -> bool {
        self.problems.is_empty()
    }
}

fn db(rms: f32) -> f32 {
    20.0 * rms.max(1e-9).log10()
}

fn median(values: &mut [f32]) -> f32 {
    values.sort_by(f32::total_cmp);
    values[values.len() / 2]
}

/// Measures mono `samples` at `sample_rate`.
pub fn analyse(samples: &[f32], sample_rate: u32) -> Analysis {
    let frame = ((f64::from(sample_rate) * FRAME_SECS) as usize).max(1);
    let levels: Vec<f32> = samples
        .chunks(frame)
        .map(|chunk| {
            let power = chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32;
            db(power.sqrt())
        })
        .collect();
    if levels.is_empty() {
        return Analysis {
            speech_secs: 0.0,
            problems: vec![Problem::TooShort],
            keep: 0..0,
        };
    }
    let mut sorted = levels.clone();
    sorted.sort_by(f32::total_cmp);
    let floor = sorted[sorted.len() / 10];
    let threshold = SPEECH_MIN_DB.max(floor + SPEECH_ABOVE_FLOOR_DB);
    let is_speech: Vec<bool> = levels.iter().map(|level| *level > threshold).collect();

    let Some(first) = is_speech.iter().position(|s| *s) else {
        return Analysis {
            speech_secs: 0.0,
            problems: vec![Problem::TooShort],
            keep: 0..0,
        };
    };
    let last = is_speech.iter().rposition(|s| *s).unwrap_or(first);
    let start_frame = first.saturating_sub(MARGIN_FRAMES);
    let natural_end = (last + 1 + MARGIN_FRAMES).min(levels.len());
    let end_frame = cut_at_pause(&is_speech, first, start_frame, natural_end);
    let keep = (start_frame * frame)..(end_frame * frame).min(samples.len());
    let speech_secs = is_speech[start_frame..end_frame]
        .iter()
        .filter(|s| **s)
        .count() as f64
        * FRAME_SECS;

    let mut problems = Vec::new();
    if speech_secs < MIN_SPEECH_SECS {
        problems.push(Problem::TooShort);
    }
    let mut speech_levels: Vec<f32> = (start_frame..end_frame)
        .filter(|i| is_speech[*i])
        .map(|i| levels[i])
        .collect();
    let speech_level = median(&mut speech_levels);
    if speech_level < QUIET_DB {
        problems.push(Problem::TooQuiet);
    }
    let kept = &samples[keep.clone()];
    let near_full = kept.iter().filter(|s| s.abs() >= CLIP_LEVEL).count();
    if near_full as f64 > CLIP_SHARE * kept.len() as f64 {
        problems.push(Problem::Clipped);
    }
    // Only the pauses of the kept part: noise after a cut does not reach the voice.
    let mut pauses: Vec<f32> = (start_frame..end_frame)
        .filter(|i| !is_speech[*i])
        .map(|i| levels[i])
        .collect();
    if pauses.len() as f64 * FRAME_SECS >= MIN_PAUSE_SECS
        && speech_level - median(&mut pauses) < MIN_SNR_DB
    {
        problems.push(Problem::Noisy);
    }
    Analysis {
        speech_secs,
        problems,
        keep,
    }
}

/// Where a recording kept from `start` ends: at `natural_end` when that is within
/// [`MAX_REFERENCE_SECS`], else after the last pause of [`PAUSE_FRAMES`] that fits and keeps at
/// least [`MIN_SPEECH_SECS`] of speech, else at the limit itself (speech without such a pause: a
/// cut in a word is better than a voice refused as too short).
fn cut_at_pause(is_speech: &[bool], first: usize, start: usize, natural_end: usize) -> usize {
    let limit = start + (MAX_REFERENCE_SECS / FRAME_SECS).round() as usize;
    if natural_end <= limit {
        return natural_end;
    }
    let min_speech = (MIN_SPEECH_SECS / FRAME_SECS).round() as usize;
    let mut silent = 0;
    let mut speech_frames = 0;
    let mut best = None;
    for (index, speech) in is_speech.iter().enumerate().take(limit).skip(first) {
        if *speech {
            silent = 0;
            speech_frames += 1;
        } else {
            silent += 1;
        }
        if silent >= PAUSE_FRAMES && speech_frames >= min_speech {
            best = Some(index + 1);
        }
    }
    best.unwrap_or(limit)
}

/// The speech of the last `window` of a live recording, for the level meter
/// (`tts-clone-level`): the level from 0 (silence) to 1 (full scale, linear in dB from −60) and
/// whether it would count as speech.
pub fn live_level(window: &[f32]) -> (f32, bool) {
    if window.is_empty() {
        return (0.0, false);
    }
    let power = window.iter().map(|s| s * s).sum::<f32>() / window.len() as f32;
    let level = db(power.sqrt());
    (
        ((level + 60.0) / 60.0).clamp(0.0, 1.0),
        level > SPEECH_MIN_DB,
    )
}

#[cfg(test)]
#[path = "clone_tests.rs"]
mod tests;
