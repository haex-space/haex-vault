use super::resample::{downmix_to_mono, resample_linear, StreamResampler};
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
