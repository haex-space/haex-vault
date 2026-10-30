//! The last seconds of the 16 kHz mono microphone signal (spec 066, research R6): what was said
//! just before a trigger, so "Hey Haex, wie viele …" arrives in one piece. Audio before the wake
//! word lives only here and is overwritten after at most [`CAPACITY_SECS`].

use crate::stt::SAMPLE_RATE_HZ;

/// How much of the signal is kept.
pub const CAPACITY_SECS: f32 = 2.0;
/// How much of it goes in front of an utterance that starts on a trigger.
pub const PREROLL_SECS: f32 = 1.5;
const _: () = assert!(PREROLL_SECS <= CAPACITY_SECS);

/// A fixed buffer that overwrites its oldest samples. It never allocates after construction.
pub struct RingBuffer {
    samples: Box<[f32]>,
    /// Where the next sample goes.
    next: usize,
    /// How many samples are valid (up to the capacity).
    filled: usize,
}

impl RingBuffer {
    /// A buffer for `secs` seconds at 16 kHz.
    pub fn with_seconds(secs: f32) -> Self {
        Self::new((secs * SAMPLE_RATE_HZ as f32) as usize)
    }

    pub fn new(capacity: usize) -> Self {
        Self {
            samples: vec![0.0; capacity].into_boxed_slice(),
            next: 0,
            filled: 0,
        }
    }

    pub fn capacity(&self) -> usize {
        self.samples.len()
    }

    pub fn len(&self) -> usize {
        self.filled
    }

    pub fn is_empty(&self) -> bool {
        self.filled == 0
    }

    /// Appends `input`, dropping the oldest samples once the buffer is full.
    pub fn push(&mut self, input: &[f32]) {
        let capacity = self.capacity();
        if capacity == 0 {
            return;
        }
        // Only the newest `capacity` samples of a long input can survive.
        let input = &input[input.len().saturating_sub(capacity)..];
        let first = input.len().min(capacity - self.next);
        self.samples[self.next..self.next + first].copy_from_slice(&input[..first]);
        let rest = &input[first..];
        self.samples[..rest.len()].copy_from_slice(rest);
        self.next = (self.next + input.len()) % capacity;
        self.filled = (self.filled + input.len()).min(capacity);
    }

    /// Appends the newest `count` samples (or all there are) to `out`, oldest first.
    pub fn copy_latest(&self, count: usize, out: &mut Vec<f32>) {
        let count = count.min(self.filled);
        let capacity = self.capacity();
        let start = (self.next + capacity - count) % capacity.max(1);
        if start + count <= capacity {
            out.extend_from_slice(&self.samples[start..start + count]);
        } else {
            out.extend_from_slice(&self.samples[start..]);
            out.extend_from_slice(&self.samples[..start + count - capacity]);
        }
    }

    /// Forgets everything, for example when the stream stops. The samples are zeroed, so no
    /// earlier audio stays in memory.
    pub fn clear(&mut self) {
        self.samples.fill(0.0);
        self.next = 0;
        self.filled = 0;
    }
}
