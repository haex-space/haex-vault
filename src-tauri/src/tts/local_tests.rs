//! Spec 066 T023/T026: sentence-wise synthesis into the mixer, stopping, and closing.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use super::local::{
    speakable_text, Engine, PocketModel, Reference, Sentence, Speaker, SpeechOut, RATE_DEFAULT,
};
use super::TtsError;
use crate::audio::mixer::{mixer, MixerInput, MixerRender};
use crate::vault_gate::{DrainOutcome, VaultGate};

const ENGINE_RATE: u32 = 16_000;
const MIXER_RATE: u32 = 48_000;
const WAIT: Duration = Duration::from_secs(5);

/// Speaks every sentence as 10 ms per character, with optional synchronization at synthesis
/// and the final sample-rate lookup before queueing.
struct FakeEngine {
    generating: Option<(mpsc::Sender<()>, Mutex<mpsc::Receiver<()>>)>,
    before_queue: Mutex<Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>>,
    calls: AtomicUsize,
}

impl FakeEngine {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            generating: None,
            before_queue: Mutex::new(None),
            calls: AtomicUsize::new(0),
        })
    }
}

impl Engine for FakeEngine {
    fn sample_rate(&self) -> u32 {
        if let Some((entered, release)) = self.before_queue.lock().expect("queue hook").take() {
            entered.send(()).expect("notify queue hook");
            release.recv_timeout(WAIT).expect("release queue hook");
        }
        ENGINE_RATE
    }

    fn generate(&self, text: &str, _: &Reference, _: f32) -> Result<Vec<f32>, TtsError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some((entered, release)) = &self.generating {
            entered.send(()).expect("notify synthesis");
            release
                .lock()
                .expect("synthesis hook")
                .recv_timeout(WAIT)
                .expect("release synthesis");
        }
        let level: f32 = text.parse().map_err(|_| TtsError::Failed {
            reason: "not a level".to_owned(),
        })?;
        Ok(vec![level; text.len() * ENGINE_RATE as usize / 100])
    }
}

struct Rig {
    speaker: Speaker,
    render: MixerRender,
    input: Arc<MixerInput>,
    gate: VaultGate,
    _runtime: tokio::runtime::Runtime,
}

fn rig() -> Rig {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let gate = VaultGate::with_runtime(runtime.handle().clone());
    let (input, render) = mixer(MIXER_RATE);
    let input = Arc::new(input);
    let out: Arc<dyn SpeechOut> = Arc::clone(&input) as Arc<dyn SpeechOut>;
    let speaker = Speaker::start(&gate, out).expect("speaker starts");
    Rig {
        speaker,
        render,
        input,
        gate,
        _runtime: runtime,
    }
}

fn sentence(engine: &Arc<FakeEngine>, level: &str) -> Sentence {
    Sentence {
        text: level.to_owned(),
        engine: Arc::clone(engine) as Arc<dyn Engine>,
        reference: Arc::new(Reference {
            samples: vec![0.0; 160],
            sample_rate: ENGINE_RATE,
        }),
        rate: RATE_DEFAULT,
    }
}

/// Renders until `count` non-silent samples came out or `WAIT` passes; returns them.
fn render_until(render: &mut MixerRender, count: usize) -> Vec<f32> {
    let deadline = Instant::now() + WAIT;
    let mut heard = Vec::new();
    let mut block = vec![0.0; 1024];
    while heard.len() < count && Instant::now() < deadline {
        render.render(&mut block);
        heard.extend(block.iter().copied().filter(|s| *s != 0.0));
        if block.iter().all(|s| *s == 0.0) {
            std::thread::yield_now();
        }
    }
    heard
}

#[test]
fn speakable_text_drops_control_characters() {
    assert_eq!(speakable_text("Hallo\0 Welt"), "Hallo Welt");
    assert_eq!(
        speakable_text("  eins\n\tzwei\u{7}drei  "),
        "eins zwei drei"
    );
    assert_eq!(speakable_text("\0\n"), "");
}

#[test]
fn sentences_reach_the_mixer_in_order_at_its_rate() {
    let mut rig = rig();
    let engine = FakeEngine::new();
    rig.speaker.speak(sentence(&engine, "0.25"));
    rig.speaker.speak(sentence(&engine, "0.5"));
    // 10 ms per character at 48 kHz: 4 and 3 characters.
    let expected = (4 + 3) * MIXER_RATE as usize / 100;
    let heard = render_until(&mut rig.render, expected);
    assert!(heard.len().abs_diff(expected) <= 2, "{}", heard.len());
    let first = heard
        .iter()
        .take_while(|s| (**s - 0.25).abs() < 1e-3)
        .count();
    assert!(
        first.abs_diff(4 * MIXER_RATE as usize / 100) <= 2,
        "{first}"
    );
    assert!(heard[first..].iter().all(|s| (*s - 0.5).abs() < 1e-3));
}

#[test]
fn stop_drops_playing_and_queued_sentences() {
    let mut rig = rig();
    let (entered, started) = mpsc::channel();
    let (release, proceed) = mpsc::channel();
    let mut engine = FakeEngine::new();
    Arc::get_mut(&mut engine).expect("unique engine").generating =
        Some((entered, Mutex::new(proceed)));
    rig.speaker.speak(sentence(&engine, "0.25"));
    rig.speaker.speak(sentence(&engine, "0.5"));
    rig.speaker.speak(sentence(&engine, "0.75"));
    started.recv_timeout(WAIT).expect("first synthesis starts");
    release.send(()).expect("finish first synthesis");
    started.recv_timeout(WAIT).expect("second synthesis starts");
    assert!(!render_until(&mut rig.render, 1).is_empty());
    rig.speaker.stop();
    let mut block = vec![0.0; 1024];
    rig.render.render(&mut block);
    assert!(
        block.iter().all(|s| *s == 0.0),
        "speech stops within a block"
    );
    release.send(()).expect("finish stopped synthesis");

    rig.speaker.speak(sentence(&engine, "0.5"));
    started.recv_timeout(WAIT).expect("new synthesis starts");
    release.send(()).expect("finish new synthesis");
    let heard = render_until(&mut rig.render, 3 * MIXER_RATE as usize / 100);
    assert!(!heard.is_empty(), "speaks again after a stop");
    assert!(heard.iter().all(|s| (*s - 0.5).abs() < 1e-3));
    assert_eq!(
        engine.calls.load(Ordering::SeqCst),
        3,
        "queued sentence is skipped"
    );
}

#[test]
fn a_failing_sentence_does_not_end_the_speaker() {
    let mut rig = rig();
    let engine = FakeEngine::new();
    rig.speaker.speak(sentence(&engine, "kein Pegel"));
    rig.speaker.speak(sentence(&engine, "0.5"));
    assert!(!render_until(&mut rig.render, 1).is_empty());
}

#[test]
fn closing_the_vault_ends_the_speaker() {
    let rig = rig();
    // The test, the speaker and its worker each hold the mixer input.
    assert_eq!(Arc::strong_count(&rig.input), 3);
    assert!(rig.gate.request_close());
    assert_eq!(
        rig._runtime.block_on(rig.gate.drain()),
        DrainOutcome::Drained
    );
    assert_eq!(Arc::strong_count(&rig.input), 2, "the worker has ended");
    let engine = FakeEngine::new();
    rig.speaker.speak(sentence(&engine, "0.5"));
    assert_eq!(engine.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn stop_between_synthesis_and_queueing_discards_the_old_sentence() {
    let mut rig = rig();
    let (entered, started) = mpsc::channel();
    let (release, proceed) = mpsc::channel();
    let engine = FakeEngine::new();
    *engine.before_queue.lock().expect("queue hook") = Some((entered, proceed));
    // More than the mixer's capacity: missing this stop would block until old speech plays.
    let long_sentence = format!("0.25{}", "0".repeat(200));
    rig.speaker.speak(sentence(&engine, &long_sentence));
    started
        .recv_timeout(WAIT)
        .expect("past the worker's stale check");
    rig.speaker.stop();
    release.send(()).expect("allow queueing after stop");
    rig.speaker.speak(sentence(&engine, "0.5"));

    let heard = render_until(&mut rig.render, 3 * MIXER_RATE as usize / 100);
    assert!(!heard.is_empty(), "the next sentence plays");
    assert!(
        heard.iter().all(|s| (*s - 0.5).abs() < 1e-3),
        "no stopped speech is played"
    );
}

#[test]
fn closing_between_synthesis_and_queueing_does_not_wait_for_playback() {
    let mut rig = rig();
    let (entered, started) = mpsc::channel();
    let (release, proceed) = mpsc::channel();
    let engine = FakeEngine::new();
    *engine.before_queue.lock().expect("queue hook") = Some((entered, proceed));
    rig.speaker
        .speak(sentence(&engine, &format!("0.25{}", "0".repeat(200))));
    started
        .recv_timeout(WAIT)
        .expect("past the worker's closing check");
    rig.gate.request_close();
    release.send(()).expect("allow queueing after close");
    assert_eq!(
        rig._runtime.block_on(rig.gate.drain()),
        DrainOutcome::Drained
    );
    let mut block = [0.0; 1024];
    rig.render.render(&mut block);
    assert!(block.iter().all(|s| *s == 0.0));
}

/// Needs a Pocket model and a reference recording:
/// `HOLZI_TTS_MODEL_DIR=<dir with the files of tts/files.rs> HOLZI_TTS_REFERENCE=<mono wav>`.
/// With `HOLZI_TTS_SAVE_DIR=<dir>` it also writes one sample per rate there to listen to.
#[test]
#[ignore = "needs a Pocket model and a reference recording, see the doc comment"]
fn pocket_speaks_and_follows_the_rate() {
    let dir = PathBuf::from(std::env::var("HOLZI_TTS_MODEL_DIR").expect("HOLZI_TTS_MODEL_DIR"));
    let wav = std::env::var("HOLZI_TTS_REFERENCE").expect("HOLZI_TTS_REFERENCE");
    let wave = sherpa_onnx::Wave::read(&wav).expect("reference decodes");
    let reference = Reference {
        samples: wave.samples().to_vec(),
        sample_rate: u32::try_from(wave.sample_rate()).expect("rate"),
    };
    let model = PocketModel::load(&dir).expect("model loads");
    assert_eq!(model.sample_rate(), 24_000);
    let text = "Du hast morgen um halb zehn einen Termin beim Zahnarzt.";
    let seconds = |rate: f32| -> f64 {
        (0..3)
            .map(|_| {
                model
                    .generate(text, &reference, rate)
                    .expect("speaks")
                    .len()
            })
            .sum::<usize>() as f64
            / 3.0
            / 24_000.0
    };
    if let Ok(save) = std::env::var("HOLZI_TTS_SAVE_DIR") {
        for rate in [0.75, 1.0, 1.5] {
            let samples = model.generate(text, &reference, rate).expect("speaks");
            let path = PathBuf::from(&save).join(format!("rate-{rate}.wav"));
            assert!(sherpa_onnx::write(
                path.to_str().expect("utf-8"),
                &samples,
                24_000
            ));
        }
    }
    let normal = seconds(1.0);
    let fast = seconds(1.5);
    let slow = seconds(0.75);
    println!("rate 0.75: {slow:.2} s, 1.0: {normal:.2} s, 1.5: {fast:.2} s");
    assert!(normal > 1.0, "{normal}");
    assert!(fast < normal * 0.85, "faster rate gives shorter speech");
    assert!(slow > normal * 1.1, "slower rate gives longer speech");
    assert!(model
        .generate("\0", &reference, 1.0)
        .expect("no panic")
        .is_empty());
}
