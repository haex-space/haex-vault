//! Listening on the audio stream (spec 066, research R6): pause detection over the 16 kHz mono
//! signal cuts what is said into utterances. Started on a trigger (the wake word), the last
//! [`PREROLL_SECS`] from the ring buffer go in first, so "Hey Haex, wie viele …" said in one
//! breath arrives whole. Pause length, minimum length and the 60 s cut come from
//! [`PauseDetector`].

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;

use super::vad::{PauseDetector, Utterance, VadError};
use crate::audio::ring::PREROLL_SECS;
use crate::audio::stream::AudioStream;
use crate::vault_gate::VaultGate;

/// How often a waiting listener checks whether it should stop.
const POLL: Duration = Duration::from_millis(50);

/// How listening begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// With what is said from now on.
    Now,
    /// With the last [`PREROLL_SECS`] before the trigger.
    OnTrigger,
}

#[derive(Debug, thiserror::Error)]
pub enum ListenError {
    #[error(transparent)]
    Vad(#[from] VadError),
    #[error("the vault is closing")]
    Closing,
}

/// A running listener. Stopping or dropping it ends listening; utterances not taken yet are
/// dropped.
pub struct Listening {
    stop: Arc<AtomicBool>,
    utterances: Receiver<Utterance>,
}

impl Listening {
    /// Utterances in the order they ended. Disconnects once the stream has ended and the last
    /// utterance was delivered.
    pub fn utterances(&self) -> &Receiver<Utterance> {
        &self.utterances
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
    }
}

impl Drop for Listening {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Listens on `stream` with the Silero model at `model` and a pause of `pause_ms` (the device
/// setting `assistant.pauseMs`). Runs on the gate's blocking pool until stopped, the stream ends
/// or the vault closes.
pub fn listen(
    gate: &VaultGate,
    stream: &AudioStream,
    model: &Path,
    pause_ms: u32,
    start: Start,
) -> Result<Listening, ListenError> {
    let mut detector = PauseDetector::new(model, pause_ms)?;
    let preroll_secs = match start {
        Start::Now => 0.0,
        Start::OnTrigger => PREROLL_SECS,
    };
    let (preroll, blocks) = stream.subscribe(preroll_secs);
    let (sender, utterances) = channel();
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = Arc::clone(&stop);
    let closing = gate.token();
    gate.spawn_blocking(move || {
        // `false` once nobody takes utterances any more.
        let deliver = |found: Vec<Utterance>| found.into_iter().all(|u| sender.send(u).is_ok());
        if !deliver(detector.push(&preroll)) {
            return;
        }
        while !stopped.load(Ordering::Acquire) && !closing.is_cancelled() {
            match blocks.recv_timeout(POLL) {
                Ok(block) => {
                    if !deliver(detector.push(&block)) {
                        return;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    deliver(detector.finish());
                    return;
                }
            }
        }
    })
    .map_err(|_| ListenError::Closing)?;
    Ok(Listening { stop, utterances })
}
