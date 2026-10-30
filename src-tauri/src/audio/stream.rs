//! The continuous audio stream of the voice assistant (spec 066, research R2): one input and one
//! output, open while the assistant runs.
//!
//! The input callback hands raw device blocks to a worker over [`blocks`](super::blocks); the
//! worker converts them to 16 kHz mono, keeps the last seconds in a [`RingBuffer`] and passes
//! every block to the subscribers (push-to-talk, pause detection). The output callback renders
//! the [`mixer`](super::mixer). Neither callback locks or allocates; the worker runs through
//! `VaultGate`, so closing the vault ends it.
//!
//! Tests and e2e runs replace the device with a [`Source`] and a [`Sink`]; the worker then
//! renders as much output per input block as the block lasts, so both sides keep the same time.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};
use tokio_util::sync::CancellationToken;

use super::blocks::{block_queue, BlockReceiver, BlockSender};
use super::mixer::{mixer, MixerInput, MixerRender};
use super::resample::StreamResampler;
use super::ring::{RingBuffer, CAPACITY_SECS};
use super::source::{Read, Sink, Source};
use super::AudioError;
use crate::stt::SAMPLE_RATE_HZ;
use crate::vault_gate::VaultGate;

/// How long the worker waits when no audio is there yet.
const POLL: Duration = Duration::from_millis(10);
/// Raw input blocks: 64 × 4096 samples hold about 2.7 s of 48 kHz stereo, plenty of slack for a
/// worker that falls behind for a moment. Overruns drop the newest audio.
const INPUT_BLOCKS: usize = 64;
const INPUT_BLOCK_LEN: usize = 4096;
/// Frames the output callback renders at a time.
const OUTPUT_FRAMES: usize = 4096;

/// Where the mixed output goes.
pub enum Output {
    /// The default output device.
    Device,
    /// A sink at `sample_rate` instead of a device (tests, e2e runs).
    Sink {
        sink: Box<dyn Sink>,
        sample_rate: u32,
    },
}

/// A block of the 16 kHz mono signal as subscribers get it.
pub type SharedBlock = Arc<[f32]>;

struct Taps {
    ring: RingBuffer,
    listeners: Vec<Sender<SharedBlock>>,
    ended: bool,
}

/// A running stream. Stopping it (or dropping it) releases microphone and speaker at once, even
/// while subscribers still hold the stream.
pub struct AudioStream {
    input_device: Mutex<Option<cpal::Stream>>,
    output_device: Mutex<Option<cpal::Stream>>,
    mixer: MixerInput,
    taps: Arc<Mutex<Taps>>,
    stop: Arc<AtomicBool>,
}

impl AudioStream {
    /// Opens the default input and output device.
    pub fn open_devices(gate: &VaultGate) -> Result<Self, AudioError> {
        let (input_device, source) = open_input()?;
        let (output_device, mixer_input) = open_output()?;
        Self::run(
            gate,
            Box::new(source),
            None,
            mixer_input,
            Some(input_device),
            Some(output_device),
        )
    }

    /// Runs on `source` and `output` instead of the input device.
    pub fn start(
        gate: &VaultGate,
        source: Box<dyn Source>,
        output: Output,
    ) -> Result<Self, AudioError> {
        match output {
            Output::Device => {
                let (output_device, mixer_input) = open_output()?;
                Self::run(gate, source, None, mixer_input, None, Some(output_device))
            }
            Output::Sink { sink, sample_rate } => {
                let (mixer_input, render) = mixer(sample_rate);
                let pump = SinkPump::new(render, sink, sample_rate);
                Self::run(gate, source, Some(pump), mixer_input, None, None)
            }
        }
    }

    fn run(
        gate: &VaultGate,
        source: Box<dyn Source>,
        pump: Option<SinkPump>,
        mixer: MixerInput,
        input_device: Option<cpal::Stream>,
        output_device: Option<cpal::Stream>,
    ) -> Result<Self, AudioError> {
        let taps = Arc::new(Mutex::new(Taps {
            ring: RingBuffer::with_seconds(CAPACITY_SECS),
            listeners: Vec::new(),
            ended: false,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let worker = Worker {
            source,
            pump,
            taps: Arc::clone(&taps),
            stop: Arc::clone(&stop),
            closing: gate.token(),
        };
        gate.spawn_blocking(move || worker.run())
            .map_err(|_| AudioError::StartFailed {
                reason: "the vault is closing".to_owned(),
            })?;
        Ok(Self {
            input_device: Mutex::new(input_device),
            output_device: Mutex::new(output_device),
            mixer,
            taps,
            stop,
        })
    }

    /// Queues speech and tones.
    pub fn mixer(&self) -> &MixerInput {
        &self.mixer
    }

    /// Subscribes to the 16 kHz mono signal. Returns the last `preroll_secs` (at most
    /// [`CAPACITY_SECS`]) and every block from then on, without a gap or overlap between them.
    /// The receiver disconnects when the stream ends.
    pub fn subscribe(&self, preroll_secs: f32) -> (Vec<f32>, Receiver<SharedBlock>) {
        let (sender, receiver) = channel();
        let mut preroll = Vec::new();
        let mut taps = self.taps.lock().unwrap_or_else(|e| e.into_inner());
        taps.ring.copy_latest(
            (preroll_secs.max(0.0) * SAMPLE_RATE_HZ as f32) as usize,
            &mut preroll,
        );
        if !taps.ended {
            taps.listeners.push(sender);
        }
        (preroll, receiver)
    }

    /// Stops the stream: releases the devices now and ends the worker within a few milliseconds.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
        self.input_device
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        self.output_device
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
    }
}

impl Drop for AudioStream {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The stream while it runs, shared by push-to-talk and the assistant.
#[derive(Default)]
pub struct StreamSlot(Mutex<Option<Arc<AudioStream>>>);

impl StreamSlot {
    pub fn current(&self) -> Option<Arc<AudioStream>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Puts `stream` in place of the running one, which stops.
    pub fn replace(&self, stream: Option<Arc<AudioStream>>) {
        let previous = std::mem::replace(
            &mut *self.0.lock().unwrap_or_else(|e| e.into_inner()),
            stream,
        );
        if let Some(previous) = previous {
            previous.stop();
        }
    }

    /// Stops the running stream, if any, for example when the vault closes.
    pub fn stop(&self) {
        self.replace(None);
    }
}

struct Worker {
    source: Box<dyn Source>,
    pump: Option<SinkPump>,
    taps: Arc<Mutex<Taps>>,
    stop: Arc<AtomicBool>,
    closing: CancellationToken,
}

impl Worker {
    fn run(mut self) {
        let mut block = Vec::with_capacity(SAMPLE_RATE_HZ as usize);
        while !self.stop.load(Ordering::Acquire) && !self.closing.is_cancelled() {
            block.clear();
            match self.source.read(&mut block) {
                Read::Data => {
                    // Output first: a subscriber that sees the block also sees its output.
                    if let Some(pump) = self.pump.as_mut() {
                        pump.advance(block.len());
                    }
                    self.publish(&block);
                }
                Read::Idle => std::thread::sleep(POLL),
                Read::Ended => break,
            }
        }
        let mut taps = self.taps.lock().unwrap_or_else(|e| e.into_inner());
        taps.ended = true;
        taps.listeners.clear();
        taps.ring.clear();
    }

    fn publish(&self, block: &[f32]) {
        let mut taps = self.taps.lock().unwrap_or_else(|e| e.into_inner());
        taps.ring.push(block);
        if taps.listeners.is_empty() {
            return;
        }
        let shared: SharedBlock = Arc::from(block);
        taps.listeners
            .retain(|listener| listener.send(Arc::clone(&shared)).is_ok());
    }
}

/// Renders the mixer into a sink, as much as the input that arrived lasts.
struct SinkPump {
    render: MixerRender,
    sink: Box<dyn Sink>,
    sample_rate: u32,
    input_samples: u64,
    output_samples: u64,
    scratch: Vec<f32>,
}

impl SinkPump {
    fn new(render: MixerRender, sink: Box<dyn Sink>, sample_rate: u32) -> Self {
        Self {
            render,
            sink,
            sample_rate,
            input_samples: 0,
            output_samples: 0,
            scratch: Vec::new(),
        }
    }

    fn advance(&mut self, input: usize) {
        self.input_samples += input as u64;
        let due = self.input_samples * u64::from(self.sample_rate) / u64::from(SAMPLE_RATE_HZ);
        let count = (due - self.output_samples) as usize;
        self.scratch.resize(count, 0.0);
        self.render.render(&mut self.scratch);
        self.sink.write(&self.scratch);
        self.output_samples = due;
    }
}

/// The microphone as a [`Source`]: raw blocks from the input callback, converted to 16 kHz mono.
struct DeviceSource {
    blocks: BlockReceiver,
    resampler: StreamResampler,
}

impl Source for DeviceSource {
    fn read(&mut self, out: &mut Vec<f32>) -> Read {
        let mut read = Read::Idle;
        loop {
            match self.blocks.try_recv() {
                Ok(block) => {
                    self.resampler.process(&block.samples, out);
                    self.blocks.recycle(block);
                    read = Read::Data;
                }
                Err(TryRecvError::Empty) => return read,
                Err(TryRecvError::Disconnected) if read == Read::Data => return read,
                Err(TryRecvError::Disconnected) => return Read::Ended,
            }
        }
    }
}

fn open_input() -> Result<(cpal::Stream, DeviceSource), AudioError> {
    let device = cpal::default_host()
        .default_input_device()
        .ok_or(AudioError::DeviceUnavailable)?;
    let supported = device
        .default_input_config()
        .map_err(|e| AudioError::ConfigUnavailable {
            reason: e.to_string(),
        })?;
    let channels = supported.channels();
    let rate = supported.sample_rate();
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let (sender, receiver) = block_queue(INPUT_BLOCKS, INPUT_BLOCK_LEN);
    let stream = super::with_sample_type!(format, T => build_input::<T>(&device, &config, sender, channels))?;
    stream.play().map_err(|e| AudioError::StartFailed {
        reason: e.to_string(),
    })?;
    Ok((
        stream,
        DeviceSource {
            blocks: receiver,
            resampler: StreamResampler::new(channels, rate, SAMPLE_RATE_HZ),
        },
    ))
}

fn build_input<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sender: BlockSender,
    channels: u16,
) -> Result<cpal::Stream, AudioError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    // Whole frames per block, so the worker never splits a frame.
    let chunk = (INPUT_BLOCK_LEN / channels.max(1) as usize) * channels.max(1) as usize;
    device
        .build_input_stream(
            *config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                for samples in data.chunks(chunk) {
                    // No free block: the worker is behind, this audio is lost.
                    let Ok(mut block) = sender.take() else {
                        return;
                    };
                    block
                        .samples
                        .extend(samples.iter().map(|&s| f32::from_sample(s)));
                    sender.send(block);
                }
            },
            |err| log::warn!("voice stream input error: {err}"),
            None,
        )
        .map_err(|e| AudioError::StartFailed {
            reason: e.to_string(),
        })
}

fn open_output() -> Result<(cpal::Stream, MixerInput), AudioError> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or(AudioError::DeviceUnavailable)?;
    let supported = device
        .default_output_config()
        .map_err(|e| AudioError::ConfigUnavailable {
            reason: e.to_string(),
        })?;
    let channels = supported.channels();
    let format = supported.sample_format();
    let (input, render) = mixer(supported.sample_rate());
    let config: cpal::StreamConfig = supported.into();
    let stream = super::with_sample_type!(format, T => build_output::<T>(&device, &config, render, channels))?;
    stream.play().map_err(|e| AudioError::StartFailed {
        reason: e.to_string(),
    })?;
    Ok((stream, input))
}

fn build_output<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut render: MixerRender,
    channels: u16,
) -> Result<cpal::Stream, AudioError>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = channels.max(1) as usize;
    let mut mono = vec![0.0f32; OUTPUT_FRAMES];
    device
        .build_output_stream(
            *config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                for frames in data.chunks_mut(OUTPUT_FRAMES * channels) {
                    let mono = &mut mono[..frames.len() / channels];
                    render.render(mono);
                    for (frame, &sample) in frames.chunks_exact_mut(channels).zip(mono.iter()) {
                        frame.fill(T::from_sample(sample));
                    }
                }
            },
            |err| log::warn!("voice stream output error: {err}"),
            None,
        )
        .map_err(|e| AudioError::StartFailed {
            reason: e.to_string(),
        })
}
