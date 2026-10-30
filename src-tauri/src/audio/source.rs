//! Where the microphone signal of a stream comes from and where its output goes (spec 066,
//! research R2). On a device both are the sound card; tests and e2e runs (feature `e2e-audio`)
//! read a WAV script and record the mixer's output instead.

/// What [`Source::read`] found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Read {
    /// Samples were appended.
    Data,
    /// Nothing yet; ask again shortly.
    Idle,
    /// The source has ended and gives nothing more.
    Ended,
}

/// A 16 kHz mono microphone signal.
pub trait Source: Send {
    /// Appends the next samples to `out`.
    fn read(&mut self, out: &mut Vec<f32>) -> Read;
}

/// Takes the mono output of the mixer instead of a speaker.
pub trait Sink: Send {
    fn write(&mut self, samples: &[f32]);
}

#[cfg(any(test, feature = "e2e-audio"))]
pub use script::{RecordingSink, Step, WavScript};

#[cfg(any(test, feature = "e2e-audio"))]
mod script {
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use super::{Read, Sink, Source};
    use crate::audio::resample::resample_linear;
    use crate::audio::AudioError;
    use crate::stt::SAMPLE_RATE_HZ;

    /// 100 ms per read, like a microphone callback.
    const BLOCK: usize = SAMPLE_RATE_HZ as usize / 10;

    /// One step of a WAV script.
    #[derive(Debug, Clone)]
    pub enum Step {
        /// A mono WAV file of any sample rate.
        File(PathBuf),
        /// Silence.
        Pause(Duration),
    }

    /// Plays files with pauses in between as a microphone would hear them.
    pub struct WavScript {
        samples: Vec<f32>,
        position: usize,
        /// With pacing, a block is handed out only once its time has come.
        started: Option<Instant>,
    }

    impl WavScript {
        /// Reads every file up front. `paced` hands the audio out in real time (e2e runs);
        /// without it, as fast as it is read (unit tests).
        pub fn load(steps: &[Step], paced: bool) -> Result<Self, AudioError> {
            let mut samples = Vec::new();
            for step in steps {
                match step {
                    Step::File(path) => {
                        let wave =
                            path.to_str()
                                .and_then(sherpa_onnx::Wave::read)
                                .ok_or_else(|| AudioError::StartFailed {
                                    reason: format!("read WAV script file {}", path.display()),
                                })?;
                        let rate = u32::try_from(wave.sample_rate()).unwrap_or(SAMPLE_RATE_HZ);
                        samples.extend(resample_linear(wave.samples(), rate, SAMPLE_RATE_HZ));
                    }
                    Step::Pause(pause) => {
                        let count = pause.as_secs_f64() * f64::from(SAMPLE_RATE_HZ);
                        samples.resize(samples.len() + count as usize, 0.0);
                    }
                }
            }
            Ok(Self {
                samples,
                position: 0,
                started: paced.then(Instant::now),
            })
        }

        /// The whole script as one signal.
        pub fn samples(&self) -> &[f32] {
            &self.samples
        }
    }

    impl Source for WavScript {
        fn read(&mut self, out: &mut Vec<f32>) -> Read {
            if self.position >= self.samples.len() {
                return Read::Ended;
            }
            if let Some(started) = self.started {
                let due = started.elapsed().as_secs_f64() * f64::from(SAMPLE_RATE_HZ);
                if (self.position as f64) + BLOCK as f64 > due {
                    return Read::Idle;
                }
            }
            let end = (self.position + BLOCK).min(self.samples.len());
            out.extend_from_slice(&self.samples[self.position..end]);
            self.position = end;
            Read::Data
        }
    }

    /// Keeps everything the mixer put out.
    #[derive(Clone, Default)]
    pub struct RecordingSink {
        recorded: Arc<Mutex<Vec<f32>>>,
    }

    impl RecordingSink {
        /// A copy of what was recorded so far.
        pub fn recorded(&self) -> Vec<f32> {
            self.recorded
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
        }
    }

    impl Sink for RecordingSink {
        fn write(&mut self, samples: &[f32]) {
            self.recorded
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .extend_from_slice(samples);
        }
    }
}
