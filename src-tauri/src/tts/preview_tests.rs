//! Spec 066 T028: the voice sample of the settings page.

use std::collections::HashSet;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;

use super::local::{Engine, Reference};
use super::preview::{play, sample};
use super::{TtsError, LANGUAGES};
use crate::audio::mixer::{mixer, MixerRender};

const RATE: u32 = 16_000;
const LEVEL: f32 = 0.5;

/// Says every sentence as `millis` of a constant level; fails on sentence `fail_at`.
struct FakeEngine {
    millis: usize,
    fail_at: Option<usize>,
    heard: Mutex<Vec<(String, f32)>>,
}

impl FakeEngine {
    fn new(millis: usize) -> Self {
        Self {
            millis,
            fail_at: None,
            heard: Mutex::new(Vec::new()),
        }
    }

    fn heard(&self) -> Vec<(String, f32)> {
        self.heard.lock().expect("heard").clone()
    }
}

impl Engine for FakeEngine {
    fn sample_rate(&self) -> u32 {
        RATE
    }

    fn generate(&self, text: &str, _: &Reference, rate: f32) -> Result<Vec<f32>, TtsError> {
        let mut heard = self.heard.lock().expect("heard");
        if self.fail_at == Some(heard.len()) {
            return Err(TtsError::Failed {
                reason: "fake failure".to_owned(),
            });
        }
        heard.push((text.to_owned(), rate));
        Ok(vec![LEVEL; self.millis * RATE as usize / 1000])
    }
}

fn reference() -> Reference {
    Reference {
        samples: vec![0.0; RATE as usize],
        sample_rate: RATE,
    }
}

/// Everything the mixer still holds, as the speaker would get it.
fn rendered(render: &mut MixerRender) -> Vec<f32> {
    let mut out = vec![0.0; 4 * RATE as usize];
    render.render(&mut out);
    out
}

fn spoken(samples: &[f32]) -> usize {
    samples.iter().filter(|&&s| s == LEVEL).count()
}

#[test]
fn every_language_has_a_sample_of_two_sentences_of_its_own() {
    let mut seen = HashSet::new();
    for language in LANGUAGES {
        let sentences = sample(language);
        assert!(sentences.iter().all(|s| !s.trim().is_empty()), "{language}");
        assert!(seen.insert(sentences), "{language} repeats another sample");
    }
    assert_eq!(
        sample("nl"),
        sample("en"),
        "unknown languages speak English"
    );
}

#[test]
fn the_sample_speaks_both_sentences_at_the_rate_and_waits_until_they_played() {
    let engine = FakeEngine::new(50);
    let (input, mut render) = mixer(RATE);
    let started = Instant::now();

    let played = play(
        &engine,
        &reference(),
        "de",
        1.25,
        &input,
        &CancellationToken::new(),
    )
    .expect("plays");

    assert!(played);
    let expected: Vec<_> = sample("de").iter().map(|s| (s.to_string(), 1.25)).collect();
    assert_eq!(engine.heard(), expected);
    // Two sentences of 50 ms with 300 ms between them.
    assert!(started.elapsed() >= Duration::from_millis(400));
    let out = rendered(&mut render);
    assert_eq!(spoken(&out), 2 * 50 * RATE as usize / 1000);
    let first_end = out.iter().position(|&s| s != LEVEL).expect("a gap");
    let second_start = first_end
        + out[first_end..]
            .iter()
            .position(|&s| s == LEVEL)
            .expect("2nd");
    assert_eq!(second_start - first_end, 300 * RATE as usize / 1000);
}

#[test]
fn a_sample_cancelled_before_it_starts_says_nothing() {
    let engine = FakeEngine::new(50);
    let (input, mut render) = mixer(RATE);
    let cancel = CancellationToken::new();
    cancel.cancel();

    let played = play(&engine, &reference(), "en", 1.0, &input, &cancel).expect("returns");

    assert!(!played);
    assert!(engine.heard().is_empty());
    assert_eq!(spoken(&rendered(&mut render)), 0);
}

#[test]
fn a_newer_sample_silences_the_one_playing() {
    let engine = FakeEngine::new(500);
    let (input, mut render) = mixer(RATE);
    let cancel = CancellationToken::new();
    let started = Instant::now();

    let played = std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(50));
            cancel.cancel();
        });
        play(&engine, &reference(), "fr", 1.0, &input, &cancel).expect("returns")
    });

    assert!(!played);
    assert!(started.elapsed() < Duration::from_millis(1_000));
    assert_eq!(
        spoken(&rendered(&mut render)),
        0,
        "queued speech is dropped"
    );
}

#[test]
fn a_failing_sentence_ends_the_sample_and_drops_what_was_queued() {
    let engine = FakeEngine {
        fail_at: Some(1),
        ..FakeEngine::new(50)
    };
    let (input, mut render) = mixer(RATE);

    let result = play(
        &engine,
        &reference(),
        "it",
        1.0,
        &input,
        &CancellationToken::new(),
    );

    assert!(matches!(result, Err(TtsError::Failed { .. })));
    assert_eq!(spoken(&rendered(&mut render)), 0);
}
