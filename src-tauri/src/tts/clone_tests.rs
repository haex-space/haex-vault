//! Spec 066 T142: the check of a recording for an own voice (US8 scenario 7). The recordings are
//! made up of tones for speech and quiet noise for pauses, at 16 kHz.

use super::*;

const RATE: u32 = 16_000;

/// A small deterministic noise source, so the tests need no random crate.
struct Noise(u32);

impl Noise {
    /// A sample in `-amplitude..amplitude`.
    fn next(&mut self, amplitude: f32) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        ((self.0 >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0) * amplitude
    }
}

/// `blocks` of (seconds, speech?) with speech as a 220 Hz tone of `amplitude` and pauses as noise
/// of `noise`; noise lies under the speech too.
fn recording(blocks: &[(f64, bool)], amplitude: f32, noise: f32) -> Vec<f32> {
    let mut source = Noise(7);
    let mut out = Vec::new();
    for (secs, speech) in blocks {
        let n = (secs * f64::from(RATE)) as usize;
        for i in 0..n {
            let tone = if *speech {
                amplitude * (i as f32 * 220.0 * std::f32::consts::TAU / RATE as f32).sin()
            } else {
                0.0
            };
            out.push((tone + source.next(noise)).clamp(-1.0, 1.0));
        }
    }
    out
}

/// `count` blocks of `speech` seconds speech and half a second pause, framed by a second of pause.
fn spoken(count: usize, speech: f64) -> Vec<(f64, bool)> {
    let mut blocks = vec![(1.0, false)];
    for _ in 0..count {
        blocks.push((speech, true));
        blocks.push((0.5, false));
    }
    blocks.push((0.5, false));
    blocks
}

#[test]
fn a_clear_recording_of_fifteen_seconds_is_kept_without_its_silent_ends() {
    let samples = recording(&spoken(10, 1.5), 0.3, 0.001);
    let analysis = analyse(&samples, RATE);
    assert!(analysis.ok(), "{:?}", analysis.problems);
    assert!(
        (analysis.speech_secs - 15.0).abs() < 0.1,
        "{}",
        analysis.speech_secs
    );
    // A second of silence in front, 0.2 s of it kept.
    assert_eq!(analysis.keep.start, (0.8 * f64::from(RATE)) as usize);
    assert!(analysis.keep.end < samples.len());
}

#[test]
fn less_than_ten_seconds_of_speech_is_too_short() {
    let analysis = analyse(&recording(&spoken(4, 1.5), 0.3, 0.001), RATE);
    assert_eq!(analysis.problems, vec![Problem::TooShort]);
    assert!((analysis.speech_secs - 6.0).abs() < 0.1);
}

#[test]
fn a_long_recording_is_cut_at_its_last_pause_before_thirty_seconds() {
    // 20 blocks of 2 s speech and 0.5 s pause after a second of pause; the kept part starts at
    // 0.8 s, so the limit lies at 30.8 s, inside the pause from 30.5 s to 31 s.
    let samples = recording(&spoken(20, 2.0), 0.3, 0.001);
    let analysis = analyse(&samples, RATE);
    assert!(analysis.ok(), "{:?}", analysis.problems);
    let kept = analysis.keep.len() as f64 / f64::from(RATE);
    assert!(kept <= MAX_REFERENCE_SECS + 1e-9 && kept > 29.5, "{kept}");
    let end = analysis.keep.end as f64 / f64::from(RATE);
    assert!(end > 30.7 && end <= 30.8 + 1e-9, "{end}");
    // Twelve blocks of speech are in it.
    assert!(
        (analysis.speech_secs - 24.0).abs() < 0.1,
        "{}",
        analysis.speech_secs
    );
    assert!(!analysis.problems.contains(&Problem::TooLong));
}

#[test]
fn the_cut_of_a_long_recording_never_falls_into_speech() {
    // The limit at 30.8 s lies in the middle of speech (blocks of 3 s and 0.5 s pauses: speech
    // from 29.0 s to 32.0 s), so the cut goes back to the pause before it.
    let samples = recording(&spoken(12, 3.0), 0.3, 0.001);
    let analysis = analyse(&samples, RATE);
    let end = analysis.keep.end as f64 / f64::from(RATE);
    assert!(end > 28.7 && end <= 29.0 + 1e-9, "{end}");
    assert!(
        (analysis.speech_secs - 24.0).abs() < 0.1,
        "{}",
        analysis.speech_secs
    );
}

#[test]
fn speech_without_any_pause_is_cut_at_thirty_seconds() {
    // Enough pause around it that the quietest tenth of the frames is the noise.
    let samples = recording(&[(5.0, false), (40.0, true), (1.0, false)], 0.3, 0.001);
    let analysis = analyse(&samples, RATE);
    let kept = analysis.keep.len() as f64 / f64::from(RATE);
    assert!((kept - MAX_REFERENCE_SECS).abs() < 0.03, "{kept}");
}

#[test]
fn an_early_pause_does_not_cut_a_long_recording_down_to_too_little_speech() {
    // One pause after 5 s of speech, then 40 s of speech without any: the pause would keep only
    // 5 s, so the cut goes to the limit and the voice keeps its speech.
    let samples = recording(
        &[
            (5.0, false),
            (5.0, true),
            (0.5, false),
            (40.0, true),
            (1.0, false),
        ],
        0.3,
        0.001,
    );
    let analysis = analyse(&samples, RATE);
    let kept = analysis.keep.len() as f64 / f64::from(RATE);
    assert!((kept - MAX_REFERENCE_SECS).abs() < 0.03, "{kept}");
    assert!(
        !analysis.problems.contains(&Problem::TooShort),
        "{:?}",
        analysis.problems
    );
}

#[test]
fn a_quiet_recording_is_too_quiet() {
    // A tone of 0.02 is about −37 dBFS.
    let analysis = analyse(&recording(&spoken(8, 1.5), 0.02, 0.0002), RATE);
    assert_eq!(analysis.problems, vec![Problem::TooQuiet]);
}

#[test]
fn samples_at_full_scale_are_clipped() {
    let analysis = analyse(&recording(&spoken(8, 1.5), 1.4, 0.001), RATE);
    assert!(
        analysis.problems.contains(&Problem::Clipped),
        "{:?}",
        analysis.problems
    );
}

#[test]
fn noisy_pauses_are_noisy() {
    // Noise of about −20 dBFS in the pauses, speech some 7 dB above it.
    let analysis = analyse(&recording(&spoken(8, 1.5), 0.3, 0.173), RATE);
    assert!(
        analysis.problems.contains(&Problem::Noisy),
        "{:?}",
        analysis.problems
    );
}

#[test]
fn noise_after_the_kept_speech_does_not_make_it_noisy() {
    // Clean speech, then minutes of noise of about −16 dBFS that is cut off with the silent end.
    let mut samples = recording(&spoken(10, 1.5), 0.6, 0.001);
    let clean = samples.len();
    let mut source = Noise(11);
    samples.extend((0..RATE as usize * 220).map(|_| source.next(0.274)));
    let analysis = analyse(&samples, RATE);
    assert!(analysis.keep.end <= clean, "{:?}", analysis.keep);
    assert!(analysis.ok(), "{:?}", analysis.problems);
}

#[test]
fn silence_has_no_speech() {
    let analysis = analyse(&vec![0.0; RATE as usize * 5], RATE);
    assert_eq!(analysis.problems, vec![Problem::TooShort]);
    assert_eq!(analysis.speech_secs, 0.0);
    assert!(analysis.keep.is_empty());
    assert_eq!(analyse(&[], RATE).problems, vec![Problem::TooShort]);
}

#[test]
fn the_live_level_shows_speech_and_silence() {
    let tone = recording(&[(0.1, true)], 0.3, 0.0);
    let (level, speech) = live_level(&tone);
    assert!(speech && level > 0.7, "{level}");
    assert_eq!(live_level(&[0.0; 1600]), (0.0, false));
    assert_eq!(live_level(&[]), (0.0, false));
}

#[test]
fn problems_have_the_names_of_the_contract() {
    let names: Vec<String> = [
        Problem::TooShort,
        Problem::TooLong,
        Problem::TooQuiet,
        Problem::Clipped,
        Problem::Noisy,
    ]
    .iter()
    .map(|p| serde_json::to_string(p).expect("json"))
    .collect();
    assert_eq!(
        names,
        [
            "\"tooShort\"",
            "\"tooLong\"",
            "\"tooQuiet\"",
            "\"clipped\"",
            "\"noisy\""
        ]
    );
}
