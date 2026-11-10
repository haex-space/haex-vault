use super::resample::{downmix_to_mono, resample_filtered, resample_linear, StreamResampler};
use crate::stt::SAMPLE_RATE_HZ;

/// Two seconds of a stereo signal: a slow sine on the left, a faster one on the right.
fn stereo(rate: u32) -> Vec<f32> {
    let frames = 2 * rate as usize;
    (0..frames)
        .flat_map(|i| {
            let t = i as f32 / rate as f32;
            [
                (t * 440.0 * std::f32::consts::TAU).sin(),
                (t * 1_000.0 * std::f32::consts::TAU).sin() * 0.5,
            ]
        })
        .collect()
}

/// Feeds `interleaved` in blocks whose sizes cycle through `sizes` (in frames).
fn streamed(interleaved: &[f32], rate: u32, sizes: &[usize]) -> Vec<f32> {
    let mut resampler = StreamResampler::new(2, rate, SAMPLE_RATE_HZ);
    let mut out = Vec::new();
    let mut position = 0;
    for size in sizes.iter().cycle() {
        if position >= interleaved.len() {
            break;
        }
        let end = (position + size * 2).min(interleaved.len());
        resampler.process(&interleaved[position..end], &mut out);
        position = end;
    }
    out
}

fn assert_matches_one_shot(rate: u32, sizes: &[usize]) {
    let input = stereo(rate);
    let one_shot = resample_linear(&downmix_to_mono(&input, 2), rate, SAMPLE_RATE_HZ);
    let stream = streamed(&input, rate, sizes);

    // The stream holds back at most the last sample, which needs a frame that never comes.
    assert!(
        one_shot.len() - stream.len() <= 1,
        "{} vs {}",
        one_shot.len(),
        stream.len()
    );
    for (i, (a, b)) in one_shot.iter().zip(&stream).enumerate() {
        assert!((a - b).abs() < 1e-5, "sample {i}: {a} vs {b}");
    }
}

#[test]
fn stereo_44_1_khz_in_any_block_size_gives_the_one_shot_result() {
    assert_matches_one_shot(44_100, &[441, 1, 1_024, 37, 4_096]);
}

#[test]
fn stereo_48_khz_in_any_block_size_gives_the_one_shot_result() {
    assert_matches_one_shot(48_000, &[480, 3, 2_048, 999]);
}

#[test]
fn the_position_carries_across_single_frame_blocks() {
    assert_matches_one_shot(48_000, &[1]);
}

#[test]
fn an_empty_block_changes_nothing() {
    let mut resampler = StreamResampler::new(2, 48_000, SAMPLE_RATE_HZ);
    let mut out = Vec::new();
    resampler.process(&[], &mut out);
    assert!(out.is_empty());
}

/// One second of a sine of `freq` Hz at `rate`.
fn sine(freq: f32, rate: u32) -> Vec<f32> {
    (0..rate)
        .map(|i| (i as f32 / rate as f32 * freq * std::f32::consts::TAU).sin())
        .collect()
}

/// The RMS of the middle half, away from the edges of the kernel.
fn middle_rms(samples: &[f32]) -> f32 {
    let middle = &samples[samples.len() / 4..samples.len() * 3 / 4];
    (middle.iter().map(|s| s * s).sum::<f32>() / middle.len() as f32).sqrt()
}

#[test]
fn filtered_resampling_keeps_speech_frequencies() {
    // An own voice's recording (spec 066 FR-049): 48 kHz from the microphone down to 24 kHz.
    let out = resample_filtered(&sine(1_000.0, 48_000), 48_000, 24_000);
    assert_eq!(out.len(), 24_000);
    let rms = middle_rms(&out);
    assert!(
        (rms - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.01,
        "{rms}"
    );
}

#[test]
fn filtered_resampling_removes_what_lies_above_the_new_nyquist_frequency() {
    // 15 kHz cannot exist at 24 kHz; without a low-pass it folds back to 9 kHz.
    let input = sine(15_000.0, 48_000);
    let filtered = middle_rms(&resample_filtered(&input, 48_000, 24_000));
    let linear = middle_rms(&resample_linear(&input, 48_000, 24_000));
    assert!(filtered < 0.01, "{filtered}");
    assert!(linear > 0.1, "{linear}");
}

#[test]
fn filtered_resampling_handles_odd_ratios_and_equal_rates() {
    let out = resample_filtered(&sine(440.0, 44_100), 44_100, 24_000);
    assert_eq!(out.len(), 24_000);
    let rms = middle_rms(&out);
    assert!(
        (rms - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.01,
        "{rms}"
    );
    let same = sine(440.0, 24_000);
    assert_eq!(resample_filtered(&same, 24_000, 24_000), same);
}
