//! Spec 081 T027: a sentence in parts, checked, retried and spoken by the reserve; nothing of a
//! failed attempt is heard and no part is skipped.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use super::check::SpeechShare;
use super::local::{Engine, Reference};
use super::speaker::{speak_sentence, Voices};
use super::TtsError;

const RATE: u32 = 1_000;
/// Normal pace of the fake engine: 0.07 s per character, as measured for German.
const SECS_PER_CHAR: f64 = 0.07;

/// What a fake attempt yields.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Outcome {
    Good,
    RunsOn,
    Silent,
    /// Runs on with silence only.
    SilentRunsOn,
    /// The model fails outright.
    Fails,
}

/// Speaks in parts of `part_words`; `script` says what each call yields, in order, then `Good`.
struct Fake {
    part_words: Option<usize>,
    script: Mutex<Vec<Outcome>>,
    calls: Mutex<Vec<(String, u32)>>,
    /// The level the fake writes, so the test can tell engines apart.
    level: f32,
}

impl Fake {
    fn new(part_words: Option<usize>, script: &[Outcome], level: f32) -> Self {
        Self {
            part_words,
            script: Mutex::new(script.to_vec()),
            calls: Mutex::new(Vec::new()),
            level,
        }
    }

    fn calls(&self) -> Vec<(String, u32)> {
        self.calls.lock().expect("calls").clone()
    }
}

impl Engine for Fake {
    fn sample_rate(&self) -> u32 {
        RATE
    }

    fn generate(&self, text: &str, reference: &Reference, rate: f32) -> Result<Vec<f32>, TtsError> {
        self.speak_part(text, reference, rate, 0, &|| false)
    }

    fn part_words(&self) -> Option<usize> {
        self.part_words
    }

    fn speak_part(
        &self,
        words: &str,
        _reference: &Reference,
        _rate: f32,
        attempt: u32,
        _cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<f32>, TtsError> {
        self.calls
            .lock()
            .expect("calls")
            .push((words.to_owned(), attempt));
        let outcome = {
            let mut script = self.script.lock().expect("script");
            if script.is_empty() {
                Outcome::Good
            } else {
                script.remove(0)
            }
        };
        let secs = SECS_PER_CHAR * words.chars().count() as f64;
        let long = ((secs * 4.0 + 5.0) * f64::from(RATE)) as usize;
        Ok(match outcome {
            Outcome::Good => vec![self.level; (secs * f64::from(RATE)) as usize],
            Outcome::RunsOn => vec![self.level; long],
            Outcome::Silent => vec![0.0; (secs * f64::from(RATE)) as usize],
            Outcome::SilentRunsOn => vec![0.0; long],
            Outcome::Fails => {
                return Err(TtsError::Failed {
                    reason: "the fake failed".to_owned(),
                })
            }
        })
    }
}

/// Speech wherever the audio is not silent.
struct Loud;

impl SpeechShare for Loud {
    fn speech_share(&self, samples: &[f32], _rate: u32) -> Option<f64> {
        let loud = samples.iter().filter(|s| s.abs() > 0.01).count();
        Some(loud as f64 / samples.len().max(1) as f64)
    }
}

fn reference() -> Reference {
    Reference {
        samples: vec![0.1; 100],
        sample_rate: RATE,
        language: "de".to_owned(),
    }
}

const LONG: &str =
    "Ich habe heute Morgen mit Jonas über das Budget für das nächste Quartal gesprochen, \
                    und er meint, dass wir die Ausgaben für Werbung um ein Drittel kürzen sollten.";

/// Speaks `text` and returns what reached the output, piece by piece.
fn speak(engine: &Fake, reserve: Option<&Fake>, text: &str) -> Vec<Vec<f32>> {
    try_speak(engine, reserve, text).expect("speaks")
}

fn try_speak(engine: &Fake, reserve: Option<&Fake>, text: &str) -> Result<Vec<Vec<f32>>, TtsError> {
    let reference = reference();
    let voices = Voices {
        engine,
        reserve: reserve.map(|r| r as &dyn Engine),
        reference: &reference,
        rate: 1.0,
        speech: Some(&Loud),
        reserve_notice: None,
        part_spoken: None,
    };
    let mut heard = Vec::new();
    speak_sentence(voices, text, &|| false, &mut |samples, _| {
        heard.push(samples.to_vec());
        true
    })?;
    Ok(heard)
}

#[test]
fn a_long_sentence_plays_in_order_as_parts() {
    let engine = Fake::new(Some(12), &[], 0.5);
    let heard = speak(&engine, None, LONG);
    let calls = engine.calls();
    assert!(calls.len() >= 2, "{calls:?}");
    assert_eq!(heard.len(), calls.len());
    let said: Vec<_> = calls.iter().map(|(words, _)| words.as_str()).collect();
    assert_eq!(
        said.join(" ").split_whitespace().count(),
        LONG.split_whitespace().count()
    );
}

#[test]
fn an_engine_without_parts_speaks_the_whole_sentence() {
    let engine = Fake::new(None, &[], 0.5);
    let heard = speak(&engine, None, LONG);
    assert_eq!(engine.calls().len(), 1);
    assert_eq!(heard.len(), 1);
}

#[test]
fn a_part_that_runs_on_is_made_again_and_never_heard() {
    let engine = Fake::new(Some(40), &[Outcome::RunsOn], 0.5);
    let heard = speak(&engine, None, "Dein Termin ist morgen früh beim Zahnarzt.");
    let calls = engine.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!((calls[0].1, calls[1].1), (0, 1), "the retry draws anew");
    assert_eq!(heard.len(), 1);
    let expected = (SECS_PER_CHAR * calls[1].0.chars().count() as f64 * f64::from(RATE)) as usize;
    assert!(
        heard[0].len() <= expected + 1,
        "only the good attempt plays"
    );
}

#[test]
fn a_part_that_fails_twice_is_split_once_more() {
    let engine = Fake::new(Some(40), &[Outcome::RunsOn, Outcome::Silent], 0.5);
    let text = "Bitte erinnere mich morgen früh an den Termin beim Zahnarzt in der Stadt.";
    let heard = speak(&engine, None, text);
    let calls = engine.calls();
    assert_eq!(calls.len(), 4, "two failures, then two halves: {calls:?}");
    assert_eq!(heard.len(), 1, "the halves play as one part");
    let halves: Vec<_> = calls[2..].iter().map(|(w, _)| w.as_str()).collect();
    assert_eq!(halves.join(" "), calls[0].0);
}

#[test]
fn timings_reach_the_caller_before_a_later_part_fails() {
    let engine = Fake::new(Some(4), &[Outcome::Good, Outcome::Fails], 0.5);
    let reference = reference();
    let mut heard = 0;
    let timings = Mutex::new(Vec::new());
    let record = |timing| timings.lock().expect("timings").push(timing);
    let voices = Voices {
        engine: &engine,
        reserve: None,
        reference: &reference,
        rate: 1.0,
        speech: Some(&Loud),
        reserve_notice: None,
        part_spoken: Some(&record),
    };

    assert!(speak_sentence(
        voices,
        "Bitte erinnere mich morgen früh an den Termin beim Zahnarzt.",
        &|| false,
        &mut |_, _| {
            heard += 1;
            true
        },
    )
    .is_err());
    assert_eq!(heard, 1);
    let timings = timings.into_inner().expect("timings");
    assert_eq!(timings.len(), 1);
    assert!(timings[0][1] > 0.0);
}

#[test]
fn the_reserve_speaks_what_the_engine_cannot() {
    let engine = Fake::new(Some(40), &[Outcome::RunsOn; 8], 0.5);
    let reserve = Fake::new(None, &[], 0.25);
    let heard = speak(
        &engine,
        Some(&reserve),
        "Bitte erinnere mich morgen früh an den Termin.",
    );
    assert_eq!(reserve.calls().len(), 1);
    assert_eq!(heard.len(), 1);
    assert!(
        heard[0].iter().all(|s| *s == 0.25),
        "only the reserve is heard"
    );
}

#[test]
fn without_a_reserve_the_part_still_plays_never_longer_than_its_cap() {
    let engine = Fake::new(Some(40), &[Outcome::RunsOn; 8], 0.5);
    let text = "Bitte erinnere mich morgen früh an den Termin.";
    let heard = speak(&engine, None, text);
    assert_eq!(heard.len(), 1, "no part is skipped");
    let cap = super::check::max_secs(text.chars().count()) * f64::from(RATE);
    assert!(heard[0].len() as f64 <= cap + 1.0);
}

#[test]
fn the_reserve_speaks_when_the_engine_fails_outright() {
    let engine = Fake::new(Some(40), &[Outcome::Fails], 0.5);
    let reserve = Fake::new(None, &[], 0.25);
    let heard = speak(&engine, Some(&reserve), "Dein Termin ist morgen früh.");
    assert_eq!(engine.calls().len(), 1, "an error is not retried");
    assert_eq!(reserve.calls().len(), 1);
    assert!(
        heard[0].iter().all(|s| *s == 0.25),
        "only the reserve is heard"
    );
}

#[test]
fn without_a_reserve_an_engine_failure_is_an_error() {
    let engine = Fake::new(Some(40), &[Outcome::Fails], 0.5);
    assert!(try_speak(&engine, None, "Dein Termin ist morgen früh.").is_err());
}

#[test]
fn without_a_reserve_a_silent_attempt_never_plays() {
    let engine = Fake::new(Some(40), &[Outcome::SilentRunsOn; 8], 0.5);
    let heard = speak(
        &engine,
        None,
        "Bitte erinnere mich morgen früh an den Termin.",
    );
    assert!(heard.is_empty(), "{} pieces", heard.len());
}

#[test]
fn without_a_reserve_the_cap_follows_the_speech_rate() {
    let engine = Fake::new(Some(40), &[Outcome::RunsOn; 8], 0.5);
    let text = "Bitte erinnere mich morgen früh an den Termin.";
    let reference = reference();
    // The fake does not stretch; at half the rate its audio counts as half as long, so the cap
    // in samples doubles.
    let voices = Voices {
        engine: &engine,
        reserve: None,
        reference: &reference,
        rate: 0.5,
        speech: Some(&Loud),
        reserve_notice: None,
        part_spoken: None,
    };
    let mut heard = Vec::new();
    speak_sentence(voices, text, &|| false, &mut |samples, _| {
        heard.push(samples.len());
        true
    })
    .expect("speaks");
    let cap = super::check::max_secs(text.chars().count()) * f64::from(RATE);
    assert_eq!(heard.len(), 1);
    assert!(heard[0] as f64 > cap, "{} against {cap}", heard[0]);
}

#[test]
fn a_stop_between_parts_plays_nothing_more() {
    let engine = Fake::new(Some(12), &[], 0.5);
    let reference = reference();
    let stop = AtomicBool::new(false);
    let voices = Voices {
        engine: &engine,
        reserve: None,
        reference: &reference,
        rate: 1.0,
        speech: Some(&Loud),
        reserve_notice: None,
        part_spoken: None,
    };
    let mut heard = 0;
    speak_sentence(
        voices,
        LONG,
        &|| stop.load(Ordering::SeqCst),
        &mut |_, _| {
            heard += 1;
            stop.store(true, Ordering::SeqCst);
            true
        },
    )
    .expect("stops cleanly");
    assert_eq!(heard, 1);
    assert_eq!(engine.calls().len(), 1);
}

/// Spec 081 FR-016 (operator 2026-11-06): while the local chat model computes on the same unit,
/// nothing is synthesized; once it is done the sentence is spoken, and the wait does not count as
/// computing time in the speed measurement.
#[test]
fn a_sentence_waits_while_the_chat_model_computes_on_its_unit() {
    use crate::compute::{Unit, COMPUTE};
    use std::time::{Duration, Instant};

    let engine = Fake::new(None, &[], 0.5);
    let reference = reference();
    let claim = COMPUTE.claim(&[Unit::Processor]);
    let started = Instant::now();
    let speed = std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let voices = Voices {
                engine: &engine,
                reserve: None,
                reference: &reference,
                rate: 1.0,
                speech: Some(&Loud),
                reserve_notice: None,
                part_spoken: None,
            };
            speak_sentence(voices, "Hallo.", &|| false, &mut |_, _| true)
        });
        std::thread::sleep(Duration::from_millis(150));
        assert!(
            engine.calls().is_empty(),
            "nothing is computed while the chat model computes"
        );
        drop(claim);
        worker.join().expect("joins").expect("speaks")
    });
    assert_eq!(engine.calls().len(), 1);
    assert!(started.elapsed() >= Duration::from_millis(150));
    assert!(
        speed.parts[0][0] < 0.15,
        "the wait is not computing time: {:?}",
        speed.parts
    );
}
