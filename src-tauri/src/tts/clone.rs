//! Checking a recording for an own voice (spec 066 FR-049, US8 scenario 7, research R20, T142).
//!
//! A voice is a reference recording the speech model imitates. Before it is kept, haex measures
//! it in 20-ms frames and names what is wrong: less than 10 s of speech, a median level of the
//! speech frames below −32 dBFS, more than 1 % of the samples at −1 dBFS or above, or less than
//! 10 dB between speech and the pauses. Silence at both ends is cut off, and speech beyond 30 s is
//! cut off as well (research R20: "beschnitten auf 10–30 s Sprache"), so a long file still makes a
//! voice. Pure: the commands feed it samples from the microphone or a file.

use serde::Serialize;

/// The least speech a voice needs.
pub const MIN_SPEECH_SECS: f64 = 10.0;
/// The most speech a voice keeps; more is cut off.
pub const MAX_SPEECH_SECS: f64 = 30.0;
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
    /// Speech in the kept part, at most [`MAX_SPEECH_SECS`].
    pub speech_secs: f64,
    pub problems: Vec<Problem>,
    /// The samples to keep: silence at both ends and speech beyond 30 s cut off.
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
    // Walk the speech frames up to 30 s of speech; the cut lies after the last one counted.
    let max_frames = (MAX_SPEECH_SECS / FRAME_SECS).round() as usize;
    let mut counted = 0;
    let mut last = first;
    for (index, speech) in is_speech.iter().enumerate().skip(first) {
        if *speech {
            counted += 1;
            last = index;
            if counted == max_frames {
                break;
            }
        }
    }
    let cut_at_limit = counted == max_frames;
    let start_frame = first.saturating_sub(MARGIN_FRAMES);
    let end_frame = if cut_at_limit {
        last + 1
    } else {
        (last + 1 + MARGIN_FRAMES).min(levels.len())
    };
    let keep = (start_frame * frame)..(end_frame * frame).min(samples.len());
    let speech_secs = counted as f64 * FRAME_SECS;

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
    let mut pauses: Vec<f32> = levels
        .iter()
        .zip(&is_speech)
        .filter(|(_, speech)| !**speech)
        .map(|(level, _)| *level)
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
