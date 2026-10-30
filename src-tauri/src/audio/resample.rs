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
