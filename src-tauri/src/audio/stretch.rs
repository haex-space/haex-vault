//! Changes the tempo of speech without changing its pitch (spec 066 FR-030): WSOLA, waveform
//! similarity overlap-add. Pocket TTS ignores sherpa's `speed`, so the speech rate is applied to
//! its output here.
//!
//! The output is built from Hann-windowed frames overlapping by half. Each next frame is taken
//! near its nominal position in the input (`rate` times the output position), at the offset whose
//! samples best continue the frame before it, so periods line up and no echo or warble appears.

/// Frame length in seconds: a few pitch periods of a low voice.
const FRAME_SECS: f32 = 0.03;
/// How far a frame may move from its nominal position to line up, in seconds.
const SEARCH_SECS: f32 = 0.012;

/// `samples` at `sample_rate`, played `rate` times as fast (2.0 = half as long). A rate of 1.0
/// returns the input unchanged.
pub fn stretch(samples: &[f32], sample_rate: u32, rate: f32) -> Vec<f32> {
    if (rate - 1.0).abs() < 1e-3 || !rate.is_finite() || rate <= 0.0 {
        return samples.to_vec();
    }
    let frame = ((FRAME_SECS * sample_rate as f32) as usize).max(4) & !1;
    let hop = frame / 2;
    let search = (SEARCH_SECS * sample_rate as f32) as usize;
    if samples.len() < frame + 2 * search {
        return samples.to_vec();
    }
    let window: Vec<f32> = (0..frame)
        .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / frame as f32).cos())
        .collect();
    let out_len = (samples.len() as f64 / f64::from(rate)).round() as usize;
    let mut out = vec![0.0f32; out_len + frame];

    // The input position of the previous frame; the next one should continue from `previous + hop`.
    let mut previous = 0usize;
    let mut k = 0usize;
    loop {
        let out_pos = k * hop;
        if out_pos >= out_len {
            break;
        }
        let nominal = (out_pos as f64 * f64::from(rate)) as usize;
        let position = if k == 0 {
            0
        } else {
            best_offset(samples, previous + hop, nominal, search, frame)
        };
        if position + frame > samples.len() {
            break;
        }
        for (i, w) in window.iter().enumerate() {
            out[out_pos + i] += samples[position + i] * w;
        }
        previous = position;
        k += 1;
    }
    out.truncate(out_len);
    out
}

/// The start near `nominal` whose first half-frame correlates best with the input at
/// `continuation` (where the previous frame's audio naturally goes on).
fn best_offset(
    samples: &[f32],
    continuation: usize,
    nominal: usize,
    search: usize,
    frame: usize,
) -> usize {
    let compare = frame / 2;
    let last_start = samples.len().saturating_sub(frame);
    let nominal = nominal.min(last_start);
    if continuation + compare > samples.len() {
        return nominal;
    }
    let target = &samples[continuation..continuation + compare];
    let low = nominal.saturating_sub(search);
    let high = (nominal + search).min(last_start);
    (low..=high)
        .max_by(|&a, &b| {
            let score = |start: usize| -> f32 {
                target
                    .iter()
                    .zip(&samples[start..start + compare])
                    .map(|(x, y)| x * y)
                    .sum()
            };
            score(a).total_cmp(&score(b))
        })
        .unwrap_or(nominal)
}
