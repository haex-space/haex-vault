use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::Duration;

use super::source::RecordingSink;
use super::stream::{AudioStream, Output, SharedBlock, StreamSlot};
use super::test_feed::feed;
use super::{Capture, MAX_RECORDING_SECS};
use crate::stt::SAMPLE_RATE_HZ;
use crate::vault_gate::VaultGate;

const WAIT: Duration = Duration::from_secs(5);

// Fields drop in order: the stream ends before the runtime, which waits for its worker.
struct Rig {
    gate: VaultGate,
    mic: Sender<Vec<f32>>,
    stream: AudioStream,
    speaker: RecordingSink,
    /// A subscriber that tells the test when the worker has published a block.
    published: Receiver<SharedBlock>,
    _runtime: tokio::runtime::Runtime,
}

fn rig(speaker_rate: u32) -> Rig {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let gate = VaultGate::with_runtime(runtime.handle().clone());
    let (mic, source) = feed();
    let speaker = RecordingSink::default();
    let stream = AudioStream::start(
        &gate,
        Box::new(source),
        Output::Sink {
            sink: Box::new(speaker.clone()),
            sample_rate: speaker_rate,
        },
    )
    .expect("stream starts");
    let (_, published) = stream.subscribe(0.0);
    Rig {
        gate,
        mic,
        stream,
        speaker,
        published,
        _runtime: runtime,
    }
}

impl Rig {
    /// Speaks `samples` and waits until the stream has passed them on.
    fn say(&self, samples: Vec<f32>) {
        self.mic.send(samples).expect("stream is running");
        self.published.recv_timeout(WAIT).expect("block published");
    }
}

fn ramp(from: usize, to: usize) -> Vec<f32> {
    (from..to).map(|i| i as f32 / 100_000.0).collect()
}

#[test]
fn every_subscriber_gets_every_block() {
    let rig = rig(SAMPLE_RATE_HZ);
    let (_, late) = rig.stream.subscribe(0.0);
    rig.say(ramp(0, 1_600));
    rig.say(ramp(1_600, 3_200));

    let mut got = Vec::new();
    for _ in 0..2 {
        got.extend_from_slice(&late.recv_timeout(WAIT).expect("block"));
    }
    assert_eq!(got, ramp(0, 3_200));
}

#[test]
fn a_subscription_starts_with_the_preroll_and_continues_without_a_gap() {
    let rig = rig(SAMPLE_RATE_HZ);
    rig.say(ramp(0, 16_000));
    rig.say(ramp(16_000, 32_000));

    let (preroll, blocks) = rig.stream.subscribe(1.5);
    rig.say(ramp(32_000, 40_000));

    let mut got = preroll;
    got.extend_from_slice(&blocks.recv_timeout(WAIT).expect("block"));
    assert_eq!(got, ramp(8_000, 40_000));
}

#[test]
fn the_speaker_gets_as_much_output_as_the_microphone_got_input() {
    let rig = rig(48_000);
    assert!(rig.stream.mixer().queue_speech(&[0.5; 24_000], || false));

    rig.say(vec![0.0; 16_000]);

    let out = rig.speaker.recorded();
    assert_eq!(out.len(), 48_000);
    assert!(out[..24_000].iter().all(|&s| s == 0.5));
    assert!(out[24_000..].iter().all(|&s| s == 0.0));
}

#[test]
fn stopping_ends_every_subscription() {
    let rig = rig(SAMPLE_RATE_HZ);
    rig.stream.stop();
    assert_eq!(
        rig.published.recv_timeout(WAIT),
        Err(RecvTimeoutError::Disconnected)
    );
    let (_, after) = rig.stream.subscribe(1.0);
    assert_eq!(
        after.recv_timeout(WAIT),
        Err(RecvTimeoutError::Disconnected)
    );
}

#[test]
fn closing_the_vault_ends_the_stream() {
    let rig = rig(SAMPLE_RATE_HZ);
    assert!(rig.gate.request_close());
    assert_eq!(
        rig.published.recv_timeout(WAIT),
        Err(RecvTimeoutError::Disconnected)
    );
}

#[test]
fn a_closing_vault_refuses_a_new_stream() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let gate = VaultGate::with_runtime(runtime.handle().clone());
    gate.request_close();
    let (_mic, source) = feed();
    let started = AudioStream::start(
        &gate,
        Box::new(source),
        Output::Sink {
            sink: Box::new(RecordingSink::default()),
            sample_rate: SAMPLE_RATE_HZ,
        },
    );
    assert!(started.is_err());
}

#[test]
fn push_to_talk_records_from_the_running_stream() {
    let rig = rig(SAMPLE_RATE_HZ);
    rig.say(ramp(0, 1_600));
    let capture = Capture::from_stream(&rig.stream);
    // Register the barrier after capture so a receive here proves that the
    // capture subscriber has already received the block as well.
    let (_, capture_barrier) = rig.stream.subscribe(0.0);
    rig.say(ramp(1_600, 4_800));
    capture_barrier
        .recv_timeout(WAIT)
        .expect("capture block published");

    assert!(!capture.has_capped());
    assert_eq!(capture.stop().samples, ramp(1_600, 4_800));
}

#[test]
fn push_to_talk_from_the_stream_stops_at_the_cap() {
    let rig = rig(SAMPLE_RATE_HZ);
    let capture = Capture::from_stream(&rig.stream);
    let max = MAX_RECORDING_SECS as usize * SAMPLE_RATE_HZ as usize;
    for _ in 0..=MAX_RECORDING_SECS {
        rig.say(vec![0.1; SAMPLE_RATE_HZ as usize]);
    }

    assert!(capture.has_capped());
    assert_eq!(capture.stop().samples.len(), max);
}

#[test]
fn a_new_stream_in_the_slot_stops_the_old_one() {
    let rig = rig(SAMPLE_RATE_HZ);
    let slot = StreamSlot::default();
    slot.replace(Some(Arc::new(rig.stream)));
    assert!(slot.current().is_some());

    slot.stop();

    assert!(slot.current().is_none());
    assert_eq!(
        rig.published.recv_timeout(WAIT),
        Err(RecvTimeoutError::Disconnected)
    );
}
