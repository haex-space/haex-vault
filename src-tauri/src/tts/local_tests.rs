//! Spec 066 T023/T026: sentence-wise synthesis into the mixer, stopping, and closing.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::local::{
    speakable_text, Engine, PocketModel, Reference, Sentence, Speaker, SpeechOut, RATE_DEFAULT,
};
use super::TtsError;
use crate::audio::mixer::{mixer, MixerInput, MixerRender};
use crate::vault_gate::VaultGate;

const ENGINE_RATE: u32 = 16_000;
const MIXER_RATE: u32 = 48_000;
const WAIT: Duration = Duration::from_secs(5);

/// Speaks every sentence as 10 ms per character at the sentence's level, after `delay`.
struct FakeEngine {
    delay: Duration,
    calls: AtomicUsize,
}

impl FakeEngine {
    fn new(delay: Duration) -> Arc<Self> {
        Arc::new(Self {
            delay,
            calls: AtomicUsize::new(0),
        })
    }
}

impl Engine for FakeEngine {
    fn sample_rate(&self) -> u32 {
        ENGINE_RATE
    }

    fn generate(&self, text: &str, _: &Reference, _: f32) -> Result<Vec<f32>, TtsError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(self.delay);
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
            std::thread::sleep(Duration::from_millis(5));
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
    let engine = FakeEngine::new(Duration::ZERO);
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
    let engine = FakeEngine::new(Duration::from_millis(100));
    rig.speaker.speak(sentence(&engine, "0.25"));
    rig.speaker.speak(sentence(&engine, "0.5"));
    rig.speaker.speak(sentence(&engine, "0.75"));
    assert!(!render_until(&mut rig.render, 1).is_empty());
    rig.speaker.stop();
    let mut block = vec![0.0; 1024];
    rig.render.render(&mut block);
    assert!(
        block.iter().all(|s| *s == 0.0),
        "speech stops within a block"
    );
    std::thread::sleep(Duration::from_millis(400));
    assert!(render_until(&mut rig.render, 1).is_empty());
    assert!(
        engine.calls.load(Ordering::SeqCst) <= 2,
        "queued sentences are skipped"
    );

    rig.speaker.speak(sentence(&engine, "0.5"));
    assert!(
        !render_until(&mut rig.render, 1).is_empty(),
        "speaks again after a stop"
    );
}

#[test]
fn a_failing_sentence_does_not_end_the_speaker() {
    let mut rig = rig();
    let engine = FakeEngine::new(Duration::ZERO);
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
    let deadline = Instant::now() + WAIT;
    while Arc::strong_count(&rig.input) > 2 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(Arc::strong_count(&rig.input), 2, "the worker has ended");
    let engine = FakeEngine::new(Duration::ZERO);
    rig.speaker.speak(sentence(&engine, "0.5"));
    assert_eq!(engine.calls.load(Ordering::SeqCst), 0);
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
