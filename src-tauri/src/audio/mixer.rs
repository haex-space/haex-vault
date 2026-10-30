//! Speech output and tones mixed into one mono signal for the speaker (spec 066, research R2).
//!
//! [`MixerRender`] runs in the output callback: it only takes queued blocks and reads atomics.
//! [`MixerInput`] queues speech and tones from any thread and stops speech. On request, what the
//! render writes also goes out as the reference signal that the echo cancellation (research R9)
//! compares the microphone with.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::TryRecvError;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::blocks::{block_queue, Block, BlockReceiver, BlockSender};

/// Samples per queued block.
const BLOCK_LEN: usize = 1024;
/// Blocks per queue: about 1.4 s of speech at 48 kHz, enough to bridge a slow sentence start.
const QUEUE_BLOCKS: usize = 64;
/// How long a producer waits before it looks for a free block again.
const WAIT: Duration = Duration::from_millis(5);

struct Shared {
    /// Speech blocks below this index are dropped (set by [`MixerInput::stop_speech`]).
    flush_below: AtomicU64,
    /// The index the next speech block gets.
    next_speech: AtomicU64,
    /// Tone volume as `f32` bits, 0.0–1.0.
    tone_gain: AtomicU32,
}

/// Queues audio for the mixer. Every method may be called from any thread.
pub struct MixerInput {
    sample_rate: u32,
    speech: Mutex<BlockSender>,
    tones: Mutex<BlockSender>,
    shared: Arc<Shared>,
}

/// Renders the mix. Owned by the output callback (or, in tests and e2e runs, the stream worker).
pub struct MixerRender {
    speech: BlockReceiver,
    tones: BlockReceiver,
    current_speech: Option<(Block, usize)>,
    current_tone: Option<(Block, usize)>,
    reference: Option<BlockSender>,
    shared: Arc<Shared>,
}

/// A mixer for `sample_rate`: the input side and the render side.
pub fn mixer(sample_rate: u32) -> (MixerInput, MixerRender) {
    let shared = Arc::new(Shared {
        flush_below: AtomicU64::new(0),
        next_speech: AtomicU64::new(0),
        tone_gain: AtomicU32::new(1.0f32.to_bits()),
    });
    let (speech_tx, speech_rx) = block_queue(QUEUE_BLOCKS, BLOCK_LEN);
    let (tones_tx, tones_rx) = block_queue(QUEUE_BLOCKS, BLOCK_LEN);
    (
        MixerInput {
            sample_rate,
            speech: Mutex::new(speech_tx),
            tones: Mutex::new(tones_tx),
            shared: Arc::clone(&shared),
        },
        MixerRender {
            speech: speech_rx,
            tones: tones_rx,
            current_speech: None,
            current_tone: None,
            reference: None,
            shared,
        },
    )
}

impl MixerInput {
    /// The rate queued samples must have.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Queues mono speech at [`Self::sample_rate`], waiting while the queue is full. Returns
    /// `false` when speech was stopped meanwhile (the rest is not queued) or the mixer is gone.
    pub fn queue_speech(&self, samples: &[f32]) -> bool {
        let speech = self.speech.lock().unwrap_or_else(|e| e.into_inner());
        let stopped_at = self.shared.flush_below.load(Ordering::Acquire);
        queue(
            &speech,
            samples,
            || self.shared.flush_below.load(Ordering::Acquire) != stopped_at,
            || self.shared.next_speech.fetch_add(1, Ordering::AcqRel),
        )
    }

    /// Queues a mono tone at [`Self::sample_rate`]. Tones play to the end, even when speech
    /// stops. Returns `false` when the mixer is gone.
    pub fn play_tone(&self, samples: &[f32]) -> bool {
        let tones = self.tones.lock().unwrap_or_else(|e| e.into_inner());
        queue(&tones, samples, || false, || 0)
    }

    /// Silences speech within one rendered block: everything queued so far is dropped. A tone
    /// that is playing finishes.
    pub fn stop_speech(&self) {
        let queued = self.shared.next_speech.load(Ordering::Acquire);
        self.shared.flush_below.store(queued, Ordering::Release);
    }

    /// Tone volume in percent, as the device setting `assistant.tones.volume` (0–100).
    pub fn set_tone_volume(&self, percent: u8) {
        let gain = f32::from(percent.min(100)) / 100.0;
        self.shared
            .tone_gain
            .store(gain.to_bits(), Ordering::Release);
    }
}

/// Splits `samples` into blocks and sends them, polling for free blocks.
fn queue(
    sender: &BlockSender,
    samples: &[f32],
    stopped: impl Fn() -> bool,
    next_index: impl Fn() -> u64,
) -> bool {
    for chunk in samples.chunks(BLOCK_LEN) {
        let mut block = loop {
            if stopped() {
                return false;
            }
            match sender.take() {
                Ok(block) => break block,
                Err(TryRecvError::Empty) => std::thread::sleep(WAIT),
                Err(TryRecvError::Disconnected) => return false,
            }
        };
        block.samples.extend_from_slice(chunk);
        block.index = next_index();
        if !sender.send(block) {
            return false;
        }
    }
    true
}

impl MixerRender {
    /// From now on, also passes everything rendered to the returned receiver: the reference
    /// signal, exactly what went to the speaker. Blocks the receiver does not take in time are
    /// dropped. Call it before the render moves into the output callback.
    pub fn reference(&mut self) -> BlockReceiver {
        let (sender, receiver) = block_queue(QUEUE_BLOCKS, BLOCK_LEN);
        self.reference = Some(sender);
        receiver
    }

    /// Fills `out` with the mix (mono) and passes the same samples on as reference signal.
    /// Neither allocates nor locks.
    pub fn render(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        self.drop_stopped_speech();
        mix_from(&self.speech, &mut self.current_speech, out, 1.0);
        let tone_gain = f32::from_bits(self.shared.tone_gain.load(Ordering::Acquire));
        mix_from(&self.tones, &mut self.current_tone, out, tone_gain);
        for sample in out.iter_mut() {
            *sample = sample.clamp(-1.0, 1.0);
        }
        self.send_reference(out);
    }

    /// Drops the speech blocks a stop request covers, including the one that is playing.
    fn drop_stopped_speech(&mut self) {
        let flush_below = self.shared.flush_below.load(Ordering::Acquire);
        if matches!(&self.current_speech, Some((block, _)) if block.index < flush_below) {
            if let Some((block, _)) = self.current_speech.take() {
                self.speech.recycle(block);
            }
        }
        if self.current_speech.is_some() {
            return;
        }
        // Blocks arrive in order, so the first one the stop does not cover ends the search.
        while let Ok(block) = self.speech.try_recv() {
            if block.index < flush_below {
                self.speech.recycle(block);
            } else {
                self.current_speech = Some((block, 0));
                break;
            }
        }
    }

    fn send_reference(&self, rendered: &[f32]) {
        let Some(reference) = &self.reference else {
            return;
        };
        for chunk in rendered.chunks(BLOCK_LEN) {
            let Ok(mut block) = reference.take() else {
                // Nobody takes the reference right now; drop it rather than wait.
                return;
            };
            block.samples.extend_from_slice(chunk);
            reference.send(block);
        }
    }
}

/// Adds the queued samples of one source to `out`, block after block.
fn mix_from(
    receiver: &BlockReceiver,
    current: &mut Option<(Block, usize)>,
    out: &mut [f32],
    gain: f32,
) {
    let mut written = 0;
    while written < out.len() {
        let (block, position) = match current.take() {
            Some(playing) => playing,
            None => match receiver.try_recv() {
                Ok(block) => (block, 0),
                Err(_) => return,
            },
        };
        let available = block.samples.len() - position;
        let count = available.min(out.len() - written);
        for (target, sample) in out[written..written + count]
            .iter_mut()
            .zip(&block.samples[position..position + count])
        {
            *target += sample * gain;
        }
        written += count;
        if position + count < block.samples.len() {
            *current = Some((block, position + count));
        } else {
            receiver.recycle(block);
        }
    }
}
