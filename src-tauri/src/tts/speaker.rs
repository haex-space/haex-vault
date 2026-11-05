//! One sentence from text to audio (spec 081 FR-010, FR-010a, research R5–R7). An engine that
//! speaks in parts (Qwen3-TTS) gets the sentence written out and split; every part is checked
//! before it plays and, when it fails, generated again, split once more, or spoken by the reserve
//! (the language's Pocket model). Nothing of a failed attempt is heard and no part is skipped. An
//! engine without parts (Pocket) speaks the whole sentence as before.
//!
//! The assistant's speaker thread (`tts::local::Speaker`) and the voice sample (`tts::preview`)
//! both speak through [`speak_sentence`].

use super::check::{judge, max_secs, trim_silence, SpeechShare, Verdict};
use super::local::{speakable_text, Engine, Reference};
use super::parts::{self, plain};
use super::spoken::spoken_protected;
use super::TtsError;

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
}

/// Speaks `text`, handing each piece of audio and its sample rate to `emit` as soon as it is
/// ready and checked. Stops when `cancelled` says so or `emit` returns `false`.
pub fn speak_sentence(
    voices: Voices<'_>,
    text: &str,
    cancelled: &dyn Fn() -> bool,
    emit: &mut dyn FnMut(&[f32], u32) -> bool,
) -> Result<(), TtsError> {
    let Some(max_words) = voices.engine.part_words() else {
        let samples = voices
            .engine
            .generate(text, voices.reference, voices.rate)?;
        if !cancelled() && !samples.is_empty() {
            emit(&samples, voices.engine.sample_rate());
        }
        return Ok(());
    };
    let language = voices.reference.language.as_str();
    let words = spoken_protected(&speakable_text(text), language);
    for part in parts::split(&words, language, max_words) {
        if cancelled() {
            return Ok(());
        }
        let Some((samples, rate)) = checked_part(voices, &part, language, cancelled)? else {
            continue;
        };
        if cancelled() || !emit(&samples, rate) {
            return Ok(());
        }
    }
    Ok(())
}

/// One attempt: its audio and what the check said.
struct Attempt {
    samples: Vec<f32>,
    verdict: Verdict,
}

/// `part` spoken and checked, following research R7; `None` when cancelled or the part is empty.
fn checked_part(
    voices: Voices<'_>,
    part: &str,
    language: &str,
    cancelled: &dyn Fn() -> bool,
) -> Result<Option<(Vec<f32>, u32)>, TtsError> {
    let engine = voices.engine;
    let rate = engine.sample_rate();
    let mut failed = Vec::new();
    // 1. and 2.: the part as it is, then once more with another draw.
    for attempt in 0..2 {
        let try_once = try_part(voices, part, attempt, cancelled)?;
        let Some(try_once) = try_once else {
            return Ok(None);
        };
        if try_once.verdict == Verdict::Ok {
            return Ok(Some((trim_silence(&try_once.samples, rate), rate)));
        }
        log_failure(part, attempt, try_once.verdict);
        failed.push(try_once);
    }
    // 3.: split once more; each half gets two attempts of its own.
    let halves = parts::split(part, language, word_count(part).div_ceil(2).max(1));
    if halves.len() > 1 {
        let mut joined = Vec::new();
        let mut all_ok = true;
        for half in &halves {
            let mut ok = None;
            for attempt in 0..2 {
                let Some(try_half) = try_part(voices, half, attempt, cancelled)? else {
                    return Ok(None);
                };
                if try_half.verdict == Verdict::Ok {
                    ok = Some(trim_silence(&try_half.samples, rate));
                    break;
                }
                log_failure(half, attempt, try_half.verdict);
            }
            match ok {
                Some(samples) => joined.extend(samples),
                None => {
                    all_ok = false;
                    break;
                }
            }
        }
        if all_ok {
            return Ok(Some((joined, rate)));
        }
    }
    // 4.: the reserve speaks the whole part, unchecked.
    if let Some(reserve) = voices.reserve {
        let samples =
            reserve.speak_part(&plain(part), voices.reference, voices.rate, 0, cancelled)?;
        if cancelled() {
            return Ok(None);
        }
        log::warn!(
            "speech: a part of {} characters was spoken by the reserve",
            part.chars().count()
        );
        return Ok(Some((samples, reserve.sample_rate())));
    }
    // 5.: no reserve at all: the attempt nearest the expected length that has speech, cut at the
    // cap; never skip the part (research R7).
    let chars = part.chars().count();
    let expected = 0.07 * chars as f64;
    let best = failed
        .into_iter()
        .filter(|a| a.verdict != Verdict::NoSpeech && !a.samples.is_empty())
        .min_by(|a, b| {
            let distance =
                |x: &Attempt| (x.samples.len() as f64 / f64::from(rate) - expected).abs();
            distance(a).total_cmp(&distance(b))
        });
    log::warn!("speech: a part of {chars} characters has no reserve and plays its nearest attempt");
    Ok(best.map(|a| {
        let cap = (max_secs(chars) * f64::from(rate)) as usize;
        let mut samples = trim_silence(&a.samples, rate);
        samples.truncate(cap);
        (samples, rate)
    }))
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
    // The speaking rate stretched the audio; judge it at the length the model spoke it.
    let rate = voices.engine.sample_rate();
    let spoken_rate = (f64::from(rate) / f64::from(voices.rate.max(0.1))) as u32;
    let verdict = judge(
        &samples,
        spoken_rate.max(1),
        part.chars().count(),
        voices.speech,
    );
    Ok(Some(Attempt { samples, verdict }))
}

/// How many words `part` has, protected groups counted word by word.
fn word_count(part: &str) -> usize {
    plain(part).split_whitespace().count()
}

/// Notes a failed attempt: its length and the reason, never its text (FR-010a).
fn log_failure(part: &str, attempt: u32, verdict: Verdict) {
    log::info!(
        "speech: attempt {} of a part of {} characters failed its check: {verdict:?}",
        attempt + 1,
        part.chars().count()
    );
}
