//! Converting device audio to the canonical 16 kHz mono signal (`stt::SAMPLE_RATE_HZ`): once for
//! a whole push-to-talk buffer (spec 008) and block by block for the continuous stream (spec 066,
//! research R2). Both use the same linear interpolation, so a stream gives the same samples as
//! converting everything at once.

/// Averages `channels` interleaved channels down to mono. A no-op copy
/// when the device is already mono.
pub(crate) fn downmix_to_mono(interleaved: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    let channels = channels as usize;
    interleaved
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// Linear-interpolation resampling from `from_rate` to `to_rate`. Adequate
/// for speech capture feeding a speech model — not intended as a
/// high-fidelity audio resampler.
pub(crate) fn resample_linear(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if input.is_empty() || from_rate == to_rate {
        return input.to_vec();
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = ((input.len() as f64) / ratio).round() as usize;
    let last = input.len() - 1;
    (0..out_len)
        .map(|i| {
            let src_pos = i as f64 * ratio;
            let idx = (src_pos.floor() as usize).min(last);
            let frac = (src_pos - idx as f64) as f32;
            let a = input[idx];
            let b = input[(idx + 1).min(last)];
            a + (b - a) * frac
        })
        .collect()
}

/// Zero crossings of the sinc on each side of [`resample_filtered`]'s kernel.
const SINC_ZEROS: f64 = 16.0;
/// Where [`resample_filtered`]'s low-pass sits, as a share of the lower Nyquist frequency.
const SINC_CUTOFF: f64 = 0.95;

/// Resampling through a Hann-windowed sinc low-pass below the lower of both Nyquist frequencies,
/// for audio that is heard again: an own voice's reference recording (spec 066 FR-049), which the
/// speech model imitates sound and all. [`resample_linear`] has no low-pass, so going down from
/// 48 kHz it folds everything above the new Nyquist frequency back into the audible range.
pub(crate) fn resample_filtered(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if input.is_empty() || from_rate == to_rate || from_rate == 0 || to_rate == 0 {
        return input.to_vec();
    }
    let ratio = f64::from(from_rate) / f64::from(to_rate);
    // Cut-off in cycles per input sample, and the kernel's half width in input samples.
    let cutoff = 0.5 * SINC_CUTOFF * (1.0 / ratio).min(1.0);
    let half_width = SINC_ZEROS / (2.0 * cutoff);
    let out_len = (input.len() as f64 / ratio).round() as usize;
    let last = input.len() as isize - 1;
    (0..out_len)
        .map(|i| {
            let centre = i as f64 * ratio;
            let low = (centre - half_width).ceil() as isize;
            let high = (centre + half_width).floor() as isize;
            let mut sum = 0.0f64;
            for k in low.max(0)..=high.min(last) {
                let distance = centre - k as f64;
                let x = 2.0 * cutoff * distance;
                let sinc = if x.abs() < 1e-12 {
                    1.0
                } else {
                    (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x)
                };
                let window = 0.5 + 0.5 * (std::f64::consts::PI * distance / half_width).cos();
                sum += f64::from(input[k as usize]) * 2.0 * cutoff * sinc * window;
            }
            sum as f32
        })
        .collect()
}

/// [`downmix_to_mono`] and [`resample_linear`] for a stream of interleaved blocks of any size.
/// It remembers the last frame and how far it got, so consecutive blocks join without a seam.
/// An output sample is produced once both frames around its position have arrived, so the stream
/// trails the one-shot conversion by at most one sample.
pub struct StreamResampler {
    channels: usize,
    ratio: f64,
    /// Output samples produced so far.
    produced: u64,
    /// Input frames of earlier blocks.
    consumed: u64,
    /// The last frame of the previous block, mono.
    previous: f32,
    mono: Vec<f32>,
}

impl StreamResampler {
    pub fn new(channels: u16, from_rate: u32, to_rate: u32) -> Self {
        Self {
            channels: channels.max(1) as usize,
            ratio: from_rate as f64 / to_rate as f64,
            produced: 0,
            consumed: 0,
            previous: 0.0,
            mono: Vec::new(),
        }
    }

    /// Converts one interleaved block and appends the result to `out`.
    pub fn process(&mut self, interleaved: &[f32], out: &mut Vec<f32>) {
        self.mono.clear();
        self.mono.extend(
            interleaved
                .chunks_exact(self.channels)
                .map(|frame| frame.iter().sum::<f32>() / self.channels as f32),
        );
        let frames = self.mono.len();
        if frames == 0 {
            return;
        }
        loop {
            // The position of the next output sample, counted from the first frame of this block;
            // -1 is the last frame of the previous block. Computed from the totals rather than
            // added up, so it matches `resample_linear` exactly however long the stream runs.
            let position = self.produced as f64 * self.ratio - self.consumed as f64;
            let index = position.floor();
            if index + 1.0 >= frames as f64 {
                break;
            }
            let frac = (position - index) as f32;
            let index = index as isize;
            let a = if index < 0 {
                self.previous
            } else {
                self.mono[index as usize]
            };
            let b = self.mono[(index + 1) as usize];
            out.push(a + (b - a) * frac);
            self.produced += 1;
        }
        self.previous = self.mono[frames - 1];
        self.consumed += frames as u64;
    }
}
