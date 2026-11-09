//! Echo cancellation on the assistant's stream (spec 066 FR-016, research R9): what haex plays is
//! taken out of what the microphone hears, so the user can talk while haex speaks.
//!
//! The mixer passes everything it renders on as the reference signal ([`MixerRender::reference`]
//! (super::mixer::MixerRender::reference)). The stream worker hands it to an [`EchoStage`] before
//! each microphone block; the stage brings both to 16 kHz mono in 10 ms frames and runs them
//! through an [`EchoCanceller`]. Subscribers (listening, the ring buffer) only ever see the
//! cancelled signal. [`Aec3`] is WebRTC's AEC3 as the pure Rust port `sonora`; it estimates the
//! delay between speaker and microphone itself.

use sonora::config::EchoCanceller as Aec3Settings;
use sonora::{AudioProcessing, Config, StreamConfig};

use super::blocks::BlockReceiver;
use super::resample::StreamResampler;
use crate::stt::SAMPLE_RATE_HZ;

/// One frame: 10 ms at 16 kHz.
pub const FRAME: usize = SAMPLE_RATE_HZ as usize / 100;
/// What the canceller is told about the delay from the reference to the microphone before it has
/// measured it: the output and input buffers of a usual device.
const DELAY_HINT_MS: i32 = 60;

/// Takes the speaker's signal out of the microphone's, frame by frame.
pub trait EchoCanceller: Send {
    /// The next 10 ms of what went to the speaker, 16 kHz mono.
    fn render(&mut self, frame: &[f32]);
    /// The next 10 ms of the microphone, 16 kHz mono; haex's own output is removed in place.
    fn capture(&mut self, frame: &mut [f32]);
}

/// WebRTC's AEC3 through `sonora`, on 16 kHz mono; no noise suppression or gain control, so speech
/// recognition hears the voice as it is.
pub struct Aec3 {
    apm: AudioProcessing,
    out: Vec<f32>,
}

impl Aec3 {
    pub fn new() -> Self {
        let config = Config {
            echo_canceller: Some(Aec3Settings::default()),
            ..Default::default()
        };
        let stream = StreamConfig::new(SAMPLE_RATE_HZ, 1);
        let apm = AudioProcessing::builder()
            .config(config)
            .capture_config(stream)
            .render_config(stream)
            .build();
        Self {
            apm,
            out: vec![0.0; FRAME],
        }
    }
}

impl Default for Aec3 {
    fn default() -> Self {
        Self::new()
    }
}

impl EchoCanceller for Aec3 {
    fn render(&mut self, frame: &[f32]) {
        if let Err(error) = self
            .apm
            .process_render_f32(&[frame], &mut [&mut self.out[..]])
        {
            log::debug!("echo cancellation: render frame: {error:?}");
        }
    }

    fn capture(&mut self, frame: &mut [f32]) {
        // A clamped hint is still applied; the canceller measures the real delay itself.
        let _ = self.apm.set_stream_delay_ms(DELAY_HINT_MS);
        match self
            .apm
            .process_capture_f32(&[&*frame], &mut [&mut self.out[..]])
        {
            Ok(()) => frame.copy_from_slice(&self.out),
            Err(error) => log::debug!("echo cancellation: capture frame: {error:?}"),
        }
    }
}

/// The reference signal and the microphone joined for an [`EchoCanceller`] in the stream worker.
pub struct EchoStage {
    canceller: Box<dyn EchoCanceller>,
    reference: BlockReceiver,
    resampler: StreamResampler,
    /// Reference at 16 kHz not yet handed on as a whole frame.
    rendered: Vec<f32>,
    /// Microphone samples not yet handed on as a whole frame.
    captured: Vec<f32>,
}

impl EchoStage {
    /// `reference` is the mixer's reference at `output_rate`.
    pub fn new(
        canceller: Box<dyn EchoCanceller>,
        reference: BlockReceiver,
        output_rate: u32,
    ) -> Self {
        Self {
            canceller,
            reference,
            resampler: StreamResampler::new(1, output_rate, SAMPLE_RATE_HZ),
            rendered: Vec::new(),
            captured: Vec::new(),
        }
    }

    /// Takes the reference rendered so far, then cancels the echo in `mic` (16 kHz mono). `out`
    /// gets the cancelled whole frames; a rest shorter than a frame waits for the next block.
    pub fn process(&mut self, mic: &[f32], out: &mut Vec<f32>) {
        while let Ok(block) = self.reference.try_recv() {
            self.resampler.process(&block.samples, &mut self.rendered);
            self.reference.recycle(block);
        }
        let whole = self.rendered.len() - self.rendered.len() % FRAME;
        for frame in self.rendered[..whole].as_chunks::<FRAME>().0 {
            self.canceller.render(frame);
        }
        self.rendered.drain(..whole);

        self.captured.extend_from_slice(mic);
        let whole = self.captured.len() - self.captured.len() % FRAME;
        for frame in self.captured[..whole].as_chunks_mut::<FRAME>().0 {
            self.canceller.capture(frame);
            out.extend_from_slice(frame);
        }
        self.captured.drain(..whole);
    }
}
