//! Spec 066 FR-030: tempo changes keep the pitch.

use super::stretch::stretch;

const RATE: u32 = 24_000;

fn tone(hz: f32, secs: f32) -> Vec<f32> {
    (0..(secs * RATE as f32) as usize)
        .map(|i| 0.5 * (std::f32::consts::TAU * hz * i as f32 / RATE as f32).sin())
        .collect()
}

/// Frequency from rising zero crossings, ignoring the fade-in and fade-out frames.
fn frequency(samples: &[f32]) -> f32 {
    let inner = &samples[RATE as usize / 20..samples.len() - RATE as usize / 20];
    let crossings = inner
        .windows(2)
        .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
        .count();
    crossings as f32 / (inner.len() as f32 / RATE as f32)
}

#[test]
fn rate_one_returns_the_input() {
    let input = tone(220.0, 0.5);
    assert_eq!(stretch(&input, RATE, 1.0), input);
}

#[test]
fn faster_and_slower_change_the_length_but_not_the_pitch() {
    let input = tone(220.0, 1.0);
    for rate in [0.75, 1.25, 1.5] {
        let output = stretch(&input, RATE, rate);
        let expected = input.len() as f32 / rate;
        assert!(
            (output.len() as f32 - expected).abs() <= 1.0,
            "rate {rate}: {} samples",
            output.len()
        );
        let hz = frequency(&output);
        assert!((hz - 220.0).abs() < 4.0, "rate {rate}: {hz} Hz");
    }
}

#[test]
fn a_steady_tone_keeps_its_level() {
    let input = tone(180.0, 1.0);
    for rate in [0.75, 1.5] {
        let output = stretch(&input, RATE, rate);
        let inner = &output[RATE as usize / 20..output.len() - RATE as usize / 20];
        let peak = inner.iter().fold(0.0f32, |p, s| p.max(s.abs()));
        let rms = (inner.iter().map(|s| s * s).sum::<f32>() / inner.len() as f32).sqrt();
        assert!((0.45..=0.55).contains(&peak), "rate {rate}: peak {peak}");
        assert!((0.33..=0.38).contains(&rms), "rate {rate}: rms {rms}");
    }
}

#[test]
fn short_or_empty_input_passes_through() {
    assert!(stretch(&[], RATE, 1.5).is_empty());
    let short = tone(220.0, 0.01);
    assert_eq!(stretch(&short, RATE, 1.5), short);
}
