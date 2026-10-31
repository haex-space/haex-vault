use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use super::listen::{listen, Listening, Start};
use super::vad::{Utterance, MAX_SPEECH_SECS};
use crate::audio::source::{RecordingSink, Step, WavScript};
use crate::audio::stream::{AudioStream, Output, SharedBlock};
use crate::audio::test_feed::feed;
use crate::stt::SAMPLE_RATE_HZ;
use crate::vault_gate::VaultGate;

const WAIT: Duration = Duration::from_secs(30);
const RATE: usize = SAMPLE_RATE_HZ as usize;

fn model() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/assistant/models/silero_vad.int8.onnx")
}

/// The German fixture through the WAV source, as a script would play it.
fn speech() -> Vec<f32> {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/stt/fixtures/de_sample.wav");
    WavScript::load(&[Step::File(file)], false)
        .expect("fixture loads")
        .samples()
        .to_vec()
}

fn silence(secs: f32) -> Vec<f32> {
    vec![0.0; (secs * RATE as f32) as usize]
}

// Fields drop in order: the stream ends before the runtime, which waits for its workers.
struct Rig {
    gate: VaultGate,
    mic: Option<Sender<Vec<f32>>>,
    stream: AudioStream,
    published: Receiver<SharedBlock>,
    _runtime: tokio::runtime::Runtime,
}

fn rig() -> Rig {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let gate = VaultGate::with_runtime(runtime.handle().clone());
    let (mic, source) = feed();
    let stream = AudioStream::start(
        &gate,
        Box::new(source),
        Output::Sink {
            sink: Box::new(RecordingSink::default()),
            sample_rate: SAMPLE_RATE_HZ,
        },
    )
    .expect("stream starts");
    let (_, published) = stream.subscribe(0.0);
    Rig {
        gate,
        mic: Some(mic),
        stream,
        published,
        _runtime: runtime,
    }
}

impl Rig {
    /// Speaks `audio` in 100 ms blocks, like the microphone, and waits until the stream has
    /// passed every block on.
    fn say(&self, audio: &[f32]) {
        for block in audio.chunks(RATE / 10) {
            self.mic
                .as_ref()
                .expect("microphone open")
                .send(block.to_vec())
                .expect("stream is running");
            self.published.recv_timeout(WAIT).expect("block published");
        }
    }

    fn listen(&self, pause_ms: u32, start: Start) -> Listening {
        listen(&self.gate, &self.stream, &model(), pause_ms, start).expect("listener starts")
    }

    /// Ends the microphone and collects every utterance the listener still delivers.
    fn finish(mut self, listening: &Listening) -> Vec<Utterance> {
        self.mic = None;
        let mut utterances = Vec::new();
        loop {
            match listening.utterances().recv_timeout(WAIT) {
                Ok(utterance) => utterances.push(utterance),
                Err(RecvTimeoutError::Disconnected) => return utterances,
                Err(RecvTimeoutError::Timeout) => panic!("listener did not finish"),
            }
        }
    }
}

fn heard(pause_ms: u32, audio: &[f32]) -> Vec<Utterance> {
    let rig = rig();
    let listening = rig.listen(pause_ms, Start::Now);
    rig.say(audio);
    rig.finish(&listening)
}

fn total_secs(utterances: &[Utterance]) -> f32 {
    utterances.iter().map(|u| u.samples.len()).sum::<usize>() as f32 / RATE as f32
}

#[test]
fn an_utterance_ends_after_the_pause_setting() {
    let mut audio = speech();
    audio.extend(silence(1.0));
    audio.extend(speech());
    audio.extend(silence(2.5));

    let short = heard(500, &audio).len();
    let long = heard(2_000, &audio).len();

    assert!(
        short >= 2,
        "a 1 s gap ends an utterance at 500 ms, got {short}"
    );
    assert!(
        long < short,
        "2000 ms waits across the gap: {long} vs {short}"
    );
}

#[test]
fn a_sound_shorter_than_250_ms_is_no_utterance() {
    let speech = speech();
    let mut audio = silence(1.0);
    audio.extend_from_slice(&speech[2 * RATE..2 * RATE + RATE * 15 / 100]);
    audio.extend(silence(1.5));

    assert!(heard(800, &audio).is_empty());
}

#[test]
fn speech_longer_than_60_seconds_is_cut_and_delivered() {
    let speech = speech();
    let mut audio = Vec::new();
    while audio.len() < 66 * RATE {
        audio.extend_from_slice(&speech);
    }
    audio.extend(silence(2.5));

    let utterances = heard(2_000, &audio);

    // sherpa keeps a short lead-in before the detected start (about 0.5 s), so a cut utterance
    // is slightly longer than the limit itself.
    let max = ((MAX_SPEECH_SECS + 1.0) * RATE as f32) as usize;
    let lengths: Vec<f32> = utterances
        .iter()
        .map(|u| u.samples.len() as f32 / RATE as f32)
        .collect();
    assert!(utterances.len() >= 2, "cut into pieces: {lengths:?}");
    assert!(
        utterances.iter().all(|u| u.samples.len() <= max),
        "{lengths:?}"
    );
    assert!(
        total_secs(&utterances) > 50.0,
        "the cut speech is delivered, got {} s",
        total_secs(&utterances)
    );
}

#[test]
fn listening_on_a_trigger_starts_with_what_was_said_just_before() {
    let speech = speech();
    let (before, after) = speech.split_at(RATE);

    let run = |start: Start| {
        let rig = rig();
        rig.say(&silence(0.5));
        rig.say(before);
        let listening = rig.listen(2_000, start);
        rig.say(after);
        rig.say(&silence(2.5));
        total_secs(&rig.finish(&listening))
    };
    let now = run(Start::Now);
    let on_trigger = run(Start::OnTrigger);

    assert!(
        on_trigger - now > 0.5,
        "the pre-roll adds the second before the trigger: {on_trigger} s vs {now} s"
    );
}

#[test]
fn stopping_the_listener_ends_its_utterances() {
    let rig = rig();
    let listening = rig.listen(800, Start::Now);
    listening.stop();
    assert_eq!(
        listening.utterances().recv_timeout(WAIT),
        Err(RecvTimeoutError::Disconnected)
    );
}

#[test]
fn a_missing_model_is_an_error() {
    let rig = rig();
    let missing = model().with_file_name("nope.onnx");
    assert!(listen(&rig.gate, &rig.stream, &missing, 800, Start::Now).is_err());
}
