//! Microphone capture (spec 008-voice-control-stt, T006): `cpal`-based
//! start/stop against an in-memory canonical PCM buffer. Since spec 066 a
//! push-to-talk recording reads from the assistant's continuous
//! [`stream::AudioStream`] when that runs, and opens the device itself only
//! when it does not.
//!
//! Capture accumulates raw, native-format interleaved samples (converted to
//! `f32` at the callback boundary only — no resampling/downmixing there)
//! and converts the whole buffer to the canonical contract
//! (`stt::{CanonicalPcm, SAMPLE_RATE_HZ}`, 16 kHz mono) once, in
//! [`Capture::stop`]. A push-to-talk recording is at most
//! [`MAX_RECORDING_SECS`] long, so holding the native-rate buffer
//! momentarily (a few tens of MB, worst case) is simpler and far easier to
//! get right than a stateful streaming resampler that must join
//! consecutive audio callbacks without a discontinuity — and this way the
//! conversion math (`downmix_to_mono`, `resample_linear`) stays two pure,
//! independently testable functions instead of being entangled with the
//! realtime callback.
//!
//! `cpal::Stream` is safe to hold here rather than on a dedicated thread:
//! every backend cpal ships (ALSA, CoreAudio, WASAPI, AAudio, ...) asserts
//! its `Stream` type is `Send + Sync` via cpal's own
//! `assert_stream_send!`/`assert_stream_sync!` macros.

pub mod blocks;
pub mod mixer;
pub mod resample;
pub mod ring;
pub mod source;
pub mod stream;
pub mod stretch;

#[cfg(test)]
mod audio_tests;
#[cfg(test)]
mod mixer_tests;
#[cfg(test)]
mod resample_tests;
#[cfg(test)]
mod ring_tests;
#[cfg(test)]
mod source_tests;
#[cfg(test)]
mod stream_tests;
#[cfg(test)]
mod stretch_tests;
#[cfg(test)]
pub(crate) mod test_feed;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex as StdMutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};

use crate::stt::{CanonicalPcm, SAMPLE_RATE_HZ};
use resample::{downmix_to_mono, resample_linear};
use stream::{AudioStream, SharedBlock};

/// FR-017: hitting this stops capture (not the whole recording session)
/// and transcribes whatever was captured up to the cap.
pub const MAX_RECORDING_SECS: u64 = 60;

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("no microphone input device available")]
    DeviceUnavailable,
    #[error("failed to query input device configuration: {reason}")]
    ConfigUnavailable { reason: String },
    #[error("failed to start capture: {reason}")]
    StartFailed { reason: String },
}

/// Runs `$build` with `$T` set to the Rust sample type of a `cpal::SampleFormat`; an unknown
/// format is an `AudioError::StartFailed`.
macro_rules! with_sample_type {
    ($format:expr, $T:ident => $build:expr) => {
        match $format {
            cpal::SampleFormat::I8 => {
                type $T = i8;
                $build
            }
            cpal::SampleFormat::I16 => {
                type $T = i16;
                $build
            }
            cpal::SampleFormat::I24 => {
                type $T = cpal::I24;
                $build
            }
            cpal::SampleFormat::I32 => {
                type $T = i32;
                $build
            }
            cpal::SampleFormat::I64 => {
                type $T = i64;
                $build
            }
            cpal::SampleFormat::U8 => {
                type $T = u8;
                $build
            }
            cpal::SampleFormat::U16 => {
                type $T = u16;
                $build
            }
            cpal::SampleFormat::U24 => {
                type $T = cpal::U24;
                $build
            }
            cpal::SampleFormat::U32 => {
                type $T = u32;
                $build
            }
            cpal::SampleFormat::U64 => {
                type $T = u64;
                $build
            }
            cpal::SampleFormat::F32 => {
                type $T = f32;
                $build
            }
            cpal::SampleFormat::F64 => {
                type $T = f64;
                $build
            }
            other => Err($crate::audio::AudioError::StartFailed {
                reason: format!("unsupported sample format: {other}"),
            }),
        }
    };
}
pub(crate) use with_sample_type;

/// One running capture. Dropping it (via [`Capture::stop`] or
/// [`Capture::cancel`]) halts the device callback or leaves the stream.
pub struct Capture {
    kind: CaptureKind,
}

enum CaptureKind {
    Device(DeviceCapture),
    Stream(StreamCapture),
}

/// A capture that owns the live `cpal::Stream` of the input device.
struct DeviceCapture {
    stream: cpal::Stream,
    raw_buffer: Arc<StdMutex<Vec<f32>>>,
    channels: u16,
    input_rate: u32,
    capped: Arc<AtomicBool>,
}

/// A capture that collects the blocks of a running [`AudioStream`].
struct StreamCapture {
    blocks: Receiver<SharedBlock>,
    recorded: StdMutex<Vec<f32>>,
    capped: AtomicBool,
}

/// Whether this device has a microphone the assistant could listen on (spec 066 FR-005).
pub fn has_input_device() -> bool {
    cpal::default_host().default_input_device().is_some()
}

impl Capture {
    /// Starts microphone capture immediately on the default input device.
    /// Returns once the stream is confirmed playing (or failed); capture
    /// then continues on cpal's own realtime audio thread.
    pub fn start() -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or(AudioError::DeviceUnavailable)?;
        let supported_config =
            device
                .default_input_config()
                .map_err(|e| AudioError::ConfigUnavailable {
                    reason: e.to_string(),
                })?;

        let channels = supported_config.channels();
        let input_rate = supported_config.sample_rate();
        let sample_format = supported_config.sample_format();
        let stream_config: cpal::StreamConfig = supported_config.into();

        let raw_buffer = Arc::new(StdMutex::new(Vec::new()));
        let capped = Arc::new(AtomicBool::new(false));
        let max_input_samples =
            (MAX_RECORDING_SECS * input_rate as u64) as usize * channels as usize;

        let stream = with_sample_type!(sample_format, T => build_capture_stream::<T>(
            &device,
            &stream_config,
            Arc::clone(&raw_buffer),
            Arc::clone(&capped),
            max_input_samples,
        ))?;

        stream.play().map_err(|e| AudioError::StartFailed {
            reason: e.to_string(),
        })?;

        Ok(Self {
            kind: CaptureKind::Device(DeviceCapture {
                stream,
                raw_buffer,
                channels,
                input_rate,
                capped,
            }),
        })
    }

    /// Records from a running stream instead of opening the device: the
    /// assistant already holds the microphone (spec 066 T020).
    pub fn from_stream(stream: &AudioStream) -> Self {
        let (_, blocks) = stream.subscribe(0.0);
        Self {
            kind: CaptureKind::Stream(StreamCapture {
                blocks,
                recorded: StdMutex::new(Vec::new()),
                capped: AtomicBool::new(false),
            }),
        }
    }

    /// True once the max-duration cap (FR-017) has stopped new samples
    /// from being appended. The stream is still technically running (its
    /// callback just discards further data) until [`Capture::stop`] drops
    /// it — callers poll this to know when to auto-trigger a stop.
    pub fn has_capped(&self) -> bool {
        match &self.kind {
            CaptureKind::Device(device) => device.capped.load(Ordering::Relaxed),
            CaptureKind::Stream(stream) => {
                stream.collect();
                stream.capped.load(Ordering::Relaxed)
            }
        }
    }

    /// Stops capture (dropping the stream halts the device callback) and
    /// converts whatever was captured to the canonical 16 kHz mono
    /// contract.
    pub fn stop(self) -> CanonicalPcm {
        match self.kind {
            CaptureKind::Device(device) => device.stop(),
            CaptureKind::Stream(stream) => {
                stream.collect();
                CanonicalPcm {
                    samples: stream
                        .recorded
                        .into_inner()
                        .unwrap_or_else(|e| e.into_inner()),
                }
            }
        }
    }

    /// Stops capture and discards whatever was captured (cancel path,
    /// FR-016).
    pub fn cancel(self) {
        drop(self);
    }
}

impl DeviceCapture {
    fn stop(self) -> CanonicalPcm {
        drop(self.stream);
        let interleaved = std::mem::take(&mut *self.raw_buffer.lock().unwrap_or_else(|e| {
            // A panic inside the audio callback would poison this — not
            // expected (the callback only does arithmetic and a Vec push),
            // but recovering the last-good buffer is strictly better than
            // propagating a poisoned-lock panic through `stop_voice_recording`.
            e.into_inner()
        }));
        let mono = downmix_to_mono(&interleaved, self.channels);
        CanonicalPcm {
            samples: resample_linear(&mono, self.input_rate, SAMPLE_RATE_HZ),
        }
    }
}

impl StreamCapture {
    /// Moves the blocks that arrived into the recording, up to the cap.
    fn collect(&self) {
        let max = MAX_RECORDING_SECS as usize * SAMPLE_RATE_HZ as usize;
        let mut recorded = self.recorded.lock().unwrap_or_else(|e| e.into_inner());
        while let Ok(block) = self.blocks.try_recv() {
            if recorded.len() >= max {
                continue;
            }
            recorded.extend_from_slice(&block);
            if recorded.len() >= max {
                recorded.truncate(max);
                self.capped.store(true, Ordering::Relaxed);
            }
        }
    }
}

fn build_capture_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    raw_buffer: Arc<StdMutex<Vec<f32>>>,
    capped: Arc<AtomicBool>,
    max_input_samples: usize,
) -> Result<cpal::Stream, AudioError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    device
        .build_input_stream(
            *config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                if capped.load(Ordering::Relaxed) {
                    return;
                }
                let Ok(mut buf) = raw_buffer.lock() else {
                    return;
                };
                buf.extend(data.iter().map(|&s| f32::from_sample(s)));
                if buf.len() >= max_input_samples {
                    buf.truncate(max_input_samples);
                    capped.store(true, Ordering::Relaxed);
                }
            },
            |err| log::warn!("voice capture stream error: {err}"),
            None,
        )
        .map_err(|e| AudioError::StartFailed {
            reason: e.to_string(),
        })
}
