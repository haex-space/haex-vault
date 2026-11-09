//! One sentence from text to audio (spec 081 FR-010, FR-010a, research R5–R7). An engine that
//! speaks in parts (Qwen3-TTS) gets the sentence written out and split; every part is checked
//! before it plays and, when it fails, generated again, split once more, or spoken by the reserve
//! (the language's Pocket model). Nothing of a failed attempt is heard and no part is skipped. An
//! engine without parts (Pocket) speaks the whole sentence as before.
//!
//! The assistant's speaker thread (`tts::local::Speaker`) and the voice sample (`tts::preview`)
//! both speak through [`speak_sentence`]. Before every sentence and every part it waits while the
//! local chat model computes on the same unit (FR-016, `crate::compute`).

use super::check::{has_speech, judge, max_secs, trim_silence, SpeechShare, Verdict};
use super::local::{speakable_text, Engine, Reference};
use super::parts::{self, plain};
use super::speed::SentenceSpeed;
use super::spoken::spoken_protected;
use super::TtsError;

use std::time::{Duration, Instant};

use crate::compute::COMPUTE;

/// What speaks a sentence.
#[derive(Clone, Copy)]
pub struct Voices<'a> {
    pub engine: &'a dyn Engine,
    /// Speaks a part the engine failed twice on (spec 081 FR-010a).
    pub reserve: Option<&'a dyn Engine>,
    pub reference: &'a Reference,
    pub rate: f32,
    /// Finds parts that are silence or noise; without it only the lengths are checked.
    pub speech: Option<&'a dyn SpeechShare>,
    /// Called when the reserve speaks because the chosen model failed a part.
    pub reserve_notice: Option<&'a (dyn Fn() + Send + Sync)>,
    /// Receives each part only after its audio was accepted for playback.
    pub part_spoken: Option<&'a dyn Fn([f64; 2])>,
}

/// Speaks `text`, handing each piece of audio and its sample rate to `emit` as soon as it is
/// ready and checked. Stops when `cancelled` says so or `emit` returns `false`. Returns how long
/// each part took to compute and how long it sounds (spec 081 FR-007); waiting in `emit` does not
/// count.
pub fn speak_sentence(
    voices: Voices<'_>,
    text: &str,
    cancelled: &dyn Fn() -> bool,
    emit: &mut dyn FnMut(&[f32], u32) -> bool,
) -> Result<SentenceSpeed, TtsError> {
    let mut speed = SentenceSpeed::default();
    let unit = voices.engine.compute_unit();
    let Some(max_words) = voices.engine.part_words() else {
        let Some(computing) = COMPUTE.claim_speech(unit, cancelled) else {
            return Ok(speed);
        };
        let started = Instant::now();
        let samples = voices
            .engine
            .generate(text, voices.reference, voices.rate)?;
        drop(computing);
        let rate = voices.engine.sample_rate();
        speed.parts.push(timing(started, &samples, rate));
        if !cancelled() && !samples.is_empty() && emit(&samples, rate) {
            if let Some(part_spoken) = voices.part_spoken {
                part_spoken(speed.parts[0]);
            }
        }
        return Ok(speed);
    };
    let language = voices.reference.language.as_str();
    let words = spoken_protected(&speakable_text(text), language);
    for part in parts::split(&words, language, max_words) {
        let Some((samples, rate, computing_time)) =
            checked_part(voices, &part, language, cancelled)?
        else {
            continue;
        };
        let part_speed = timing_after(computing_time, &samples, rate);
        speed.parts.push(part_speed);
        if cancelled() || !emit(&samples, rate) {
            return Ok(speed);
        }
        if let Some(part_spoken) = voices.part_spoken {
            part_spoken(part_speed);
        }
    }
    Ok(speed)
}

/// `[compute seconds since started, audio seconds]` of a part.
fn timing(started: Instant, samples: &[f32], rate: u32) -> [f64; 2] {
    timing_after(started.elapsed(), samples, rate)
}

/// `[active compute seconds, audio seconds]`; waiting for a compute unit is excluded.
fn timing_after(computing_time: Duration, samples: &[f32], rate: u32) -> [f64; 2] {
    [
        computing_time.as_secs_f64(),
        samples.len() as f64 / f64::from(rate.max(1)),
    ]
}

/// One attempt: its audio and what the check said.
struct Attempt {
    samples: Vec<f32>,
    verdict: Verdict,
}

/// What the engine made of a part in steps 1 to 3 of research R7.
enum Spoken {
    /// Audio that passed its check.
    Passed(Vec<f32>),
    /// Every attempt failed its check.
    Failed,
    Cancelled,
}

/// `part` spoken and checked, following research R7; `None` when cancelled or the part is empty.
fn checked_part(
    voices: Voices<'_>,
    part: &str,
    language: &str,
    cancelled: &dyn Fn() -> bool,
) -> Result<Option<(Vec<f32>, u32, Duration)>, TtsError> {
    let engine = voices.engine;
    let rate = engine.sample_rate();
    let mut failed = Vec::new();
    let Some(computing) = COMPUTE.claim_speech(engine.compute_unit(), cancelled) else {
        return Ok(None);
    };
    let started = Instant::now();
    let result = engine_part(voices, part, language, cancelled, &mut failed);
    let mut computing_time = started.elapsed();
    drop(computing);
    match result {
        Ok(Spoken::Passed(samples)) => return Ok(Some((samples, rate, computing_time))),
        Ok(Spoken::Cancelled) => return Ok(None),
        Ok(Spoken::Failed) => {}
        // The chosen model failed outright: the reserve speaks the part (operator 2026-11-05).
        Err(error) if voices.reserve.is_some() => log::warn!(
            "speech: {} failed on a part of {} characters: {error}",
            engine.log_name(),
            part.chars().count()
        ),
        Err(error) => return Err(error),
    }
    // 4.: the reserve speaks the whole part, unchecked.
    if let Some(reserve) = voices.reserve {
        let Some(computing) = COMPUTE.claim_speech(reserve.compute_unit(), cancelled) else {
            return Ok(None);
        };
        let started = Instant::now();
        let samples =
            reserve.speak_part(&plain(part), voices.reference, voices.rate, 0, cancelled)?;
        computing_time += started.elapsed();
        drop(computing);
        if cancelled() {
            return Ok(None);
        }
        if let Some(reserve_notice) = voices.reserve_notice {
            reserve_notice();
        }
        log::warn!(
            "speech: a part of {} characters was spoken by the reserve {}",
            part.chars().count(),
            reserve.log_name()
        );
        return Ok(Some((samples, reserve.sample_rate(), computing_time)));
    }
    Ok(
        nearest_attempt(voices, part, failed)
            .map(|(samples, rate)| (samples, rate, computing_time)),
    )
}

/// Steps 1 to 3 of research R7: the part, once more with another draw, then split once more with
/// two attempts per half. Failed attempts of the whole part go to `failed`.
fn engine_part(
    voices: Voices<'_>,
    part: &str,
    language: &str,
    cancelled: &dyn Fn() -> bool,
    failed: &mut Vec<Attempt>,
) -> Result<Spoken, TtsError> {
    let rate = voices.engine.sample_rate();
    // 1. and 2.: the part as it is, then once more with another draw.
    for attempt in 0..2 {
        let Some(try_once) = try_part(voices, part, attempt, cancelled)? else {
            return Ok(Spoken::Cancelled);
        };
        if try_once.verdict == Verdict::Ok {
            return Ok(Spoken::Passed(trim_silence(&try_once.samples, rate)));
        }
        log_failure(voices, part, attempt, try_once.verdict);
        failed.push(try_once);
    }
    // 3.: split once more; each half gets two attempts of its own.
    let halves = parts::split(part, language, word_count(part).div_ceil(2).max(1));
    if halves.len() < 2 {
        return Ok(Spoken::Failed);
    }
    let mut joined = Vec::new();
    for half in &halves {
        let mut passed = None;
        for attempt in 0..2 {
            let Some(try_half) = try_part(voices, half, attempt, cancelled)? else {
                return Ok(Spoken::Cancelled);
            };
            if try_half.verdict == Verdict::Ok {
                passed = Some(trim_silence(&try_half.samples, rate));
                break;
            }
            log_failure(voices, half, attempt, try_half.verdict);
        }
        match passed {
            Some(samples) => joined.extend(samples),
            None => return Ok(Spoken::Failed),
        }
    }
    Ok(Spoken::Passed(joined))
}

/// 5.: no reserve at all: the attempt nearest the expected length that has speech, cut at the
/// cap; never skip the part when one has speech (research R7).
fn nearest_attempt(
    voices: Voices<'_>,
    part: &str,
    failed: Vec<Attempt>,
) -> Option<(Vec<f32>, u32)> {
    let rate = voices.engine.sample_rate();
    let spoken = spoken_rate(voices);
    let chars = part.chars().count();
    let expected = 0.07 * chars as f64;
    let best = failed
        .into_iter()
        .filter(|a| {
            a.verdict != Verdict::NoSpeech
                && !a.samples.is_empty()
                && has_speech(&a.samples, spoken, voices.speech)
        })
        .min_by(|a, b| {
            let distance =
                |x: &Attempt| (x.samples.len() as f64 / f64::from(spoken) - expected).abs();
            distance(a).total_cmp(&distance(b))
        });
    let Some(best) = best else {
        log::warn!(
            "speech: a part of {chars} characters has no reserve and no attempt with speech, so \
             it is not heard"
        );
        return None;
    };
    log::warn!("speech: a part of {chars} characters has no reserve and plays its nearest attempt");
    // The cap is in the time the model spoke; the audio is stretched by the speech rate.
    let cap = (max_secs(chars) * f64::from(spoken)) as usize;
    let mut samples = trim_silence(&best.samples, rate);
    samples.truncate(cap);
    Some((samples, rate))
}

/// Speaks `part` once with the engine and judges it; `None` when cancelled.
fn try_part(
    voices: Voices<'_>,
    part: &str,
    attempt: u32,
    cancelled: &dyn Fn() -> bool,
) -> Result<Option<Attempt>, TtsError> {
    let samples = voices.engine.speak_part(
        &plain(part),
        voices.reference,
        voices.rate,
        attempt,
        cancelled,
    )?;
    if cancelled() {
        return Ok(None);
    }
    let verdict = judge(
        &samples,
        spoken_rate(voices),
        part.chars().count(),
        voices.speech,
    );
    Ok(Some(Attempt { samples, verdict }))
}

/// The sample rate at which the engine's stretched audio lasts as long as the model spoke it: the
/// speech rate stretched the audio, the checks judge the length the model gave it.
fn spoken_rate(voices: Voices<'_>) -> u32 {
    let rate = voices.engine.sample_rate();
    ((f64::from(rate) / f64::from(voices.rate.max(0.1))) as u32).max(1)
}

/// How many words `part` has, protected groups counted word by word.
fn word_count(part: &str) -> usize {
    plain(part).split_whitespace().count()
}

/// Notes a failed attempt: the speech model, the part's length and the reason, never its text
/// (FR-010a).
fn log_failure(voices: Voices<'_>, part: &str, attempt: u32, verdict: Verdict) {
    log::info!(
        "speech: attempt {} of {} on a part of {} characters failed its check: {verdict:?}",
        attempt + 1,
        voices.engine.log_name(),
        part.chars().count()
    );
}
