//! Spec 081 T025: checking a part before it plays.

use super::check::{judge, max_secs, min_secs, trim_silence, SpeechShare, Verdict};

const RATE: u32 = 24_000;

struct Share(f64);

impl SpeechShare for Share {
    fn speech_share(&self, _samples: &[f32], _rate: u32) -> Option<f64> {
        Some(self.0)
    }
}

fn audio(secs: f64) -> Vec<f32> {
    vec![0.2; (secs * f64::from(RATE)) as usize]
}

#[test]
fn a_part_at_the_normal_pace_is_ok() {
    // 60 characters at 0.07 s per character.
    assert_eq!(judge(&audio(4.2), RATE, 60, Some(&Share(0.9))), Verdict::Ok);
}

#[test]
fn a_part_that_reaches_its_cap_ran_on() {
    let cap = max_secs(60);
    assert_eq!(
        judge(&audio(cap), RATE, 60, Some(&Share(0.9))),
        Verdict::TooLong
    );
    assert_eq!(judge(&audio(cap + 3.0), RATE, 60, None), Verdict::TooLong);
}

#[test]
fn a_part_that_ends_after_a_few_words_is_too_short() {
    assert_eq!(
        judge(&audio(min_secs(60) * 0.8), RATE, 60, Some(&Share(0.9))),
        Verdict::TooShort
    );
}

#[test]
fn silence_or_noise_is_no_speech() {
    assert_eq!(
        judge(&audio(4.2), RATE, 60, Some(&Share(0.1))),
        Verdict::NoSpeech
    );
    // Without a detector only the lengths count.
    assert_eq!(judge(&audio(4.2), RATE, 60, None), Verdict::Ok);
}

#[test]
fn trimming_keeps_a_short_pause_around_the_speech() {
    let mut samples = vec![0.0; RATE as usize];
    samples.extend(vec![0.3; RATE as usize]);
    samples.extend(vec![0.0; RATE as usize]);
    let trimmed = trim_silence(&samples, RATE);
    let keep = (0.15 * f64::from(RATE)) as usize;
    assert_eq!(trimmed.len(), RATE as usize + 2 * keep);
    assert!(trim_silence(&vec![0.0; 100], RATE).is_empty());
}

/// Spec 081 research R6: speech rises and falls, silence and steady noise do not count.
#[test]
fn the_level_check_tells_speech_from_silence_and_steady_noise() {
    use super::check::Level;
    // Syllables of changing loudness: 120 ms sounding, 80 ms quiet, for two seconds.
    let loudness = [0.1, 0.3, 0.2, 0.4, 0.15];
    let speech: Vec<f32> = (0..2 * RATE as usize)
        .map(|i| {
            let in_syllable = (i % 4_800) < 2_880;
            let wave = ((i as f32) * 0.05).sin();
            if in_syllable {
                loudness[(i / 4_800) % loudness.len()] * wave
            } else {
                0.002 * wave
            }
        })
        .collect();
    let share = Level.speech_share(&speech, RATE).expect("measures");
    assert!(share > 0.5, "{share}");

    let silence = vec![0.0; 2 * RATE as usize];
    assert_eq!(Level.speech_share(&silence, RATE), Some(0.0));

    let hiss: Vec<f32> = (0..2 * RATE as usize)
        .map(|i| 0.1 * ((i as f32) * 1.7).sin())
        .collect();
    assert_eq!(Level.speech_share(&hiss, RATE), Some(0.0), "steady noise");
}
