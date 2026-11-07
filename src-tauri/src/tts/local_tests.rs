//! Spec 066 T023/T026: sentence-wise synthesis into the mixer, stopping, and closing.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use super::local::{
    speakable_text, Engine, LanguageVoice, LanguageVoices, Passage, PocketModel, Reference,
    Sentence, Speaker, SpeechOut, MAX_REFERENCE_LEN, RATE_DEFAULT,
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
            language: "de".to_owned(),
        }),
        rate: RATE_DEFAULT,
        reserve: None,
        speech: None,
        meter: None,
        reserve_notice: None,
        passage: None,
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
fn dropping_speaker_finishes_current_sentence_and_skips_queued_ones() {
    let mut rig = rig();
    let (entered, started) = mpsc::channel();
    let (release, proceed) = mpsc::channel();
    let mut engine = FakeEngine::new();
    Arc::get_mut(&mut engine).expect("unique engine").generating =
        Some((entered, Mutex::new(proceed)));
    rig.speaker.speak(sentence(&engine, "0.25"));
    rig.speaker.speak(sentence(&engine, "0.5"));
    started.recv_timeout(WAIT).expect("first synthesis starts");
    drop(rig.speaker);
    release.send(()).expect("finish current synthesis");

    let deadline = Instant::now() + WAIT;
    while Arc::strong_count(&rig.input) != 1 && Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert_eq!(Arc::strong_count(&rig.input), 1, "worker has ended");
    assert_eq!(
        engine.calls.load(Ordering::SeqCst),
        1,
        "queued sentence skipped"
    );
    let heard = render_until(&mut rig.render, 4 * MIXER_RATE as usize / 100);
    assert!(heard.len().abs_diff(4 * MIXER_RATE as usize / 100) <= 2);
    assert!(heard.iter().all(|sample| (*sample - 0.25).abs() < 1e-3));
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
    // The test, the speaker, the synthesis worker and the playback feeder hold the mixer input.
    assert_eq!(Arc::strong_count(&rig.input), 4);
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
        language: "de".to_owned(),
    };
    let entry = super::catalog::pocket_for("de")
        .expect("parses")
        .expect("German model");
    let model = PocketModel::load(&dir, entry).expect("model loads");
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

fn reporting_rig() -> (Rig, mpsc::Receiver<u64>) {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let gate = VaultGate::with_runtime(runtime.handle().clone());
    let (input, render) = mixer(MIXER_RATE);
    let input = Arc::new(input);
    let out: Arc<dyn SpeechOut> = Arc::clone(&input) as Arc<dyn SpeechOut>;
    let (report, played) = mpsc::channel();
    let report = Mutex::new(report);
    let speaker = Speaker::start_reporting(
        &gate,
        out,
        Arc::new(move |ticket| {
            let _ = report.lock().expect("report").send(ticket);
        }),
    )
    .expect("speaker starts");
    (
        Rig {
            speaker,
            render,
            input,
            gate,
            _runtime: runtime,
        },
        played,
    )
}

#[test]
fn every_sentence_is_reported_in_order_once_it_has_sounded() {
    let (rig, played) = reporting_rig();
    let engine = FakeEngine::new();
    let started = Instant::now();
    let tickets = [
        rig.speaker.speak(sentence(&engine, "0.25")),
        rig.speaker.speak(sentence(&engine, "not a level")),
        rig.speaker.speak(sentence(&engine, "0.5")),
    ];
    assert_eq!(tickets, [1, 2, 3]);
    let reported: Vec<u64> = (0..3)
        .map(|_| played.recv_timeout(WAIT).expect("reported"))
        .collect();
    assert_eq!(reported, [1, 2, 3], "a failed sentence is reported too");
    // "0.25" and "0.5" sound for 40 ms and 30 ms.
    assert!(started.elapsed() >= Duration::from_millis(70));
}

#[test]
fn a_stop_reports_what_it_silenced_at_once() {
    let (rig, played) = reporting_rig();
    // Only the renderer moves out: the rest of the rig drops speaker first, runtime last.
    let mut render = rig.render;
    let speaker = &rig.speaker;
    // Plays like a device: the mixer drops stopped speech only while it renders.
    let playing = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let still_playing = Arc::clone(&playing);
    let device = std::thread::spawn(move || {
        let mut block = vec![0.0; 1024];
        while still_playing.load(Ordering::Acquire) {
            render.render(&mut block);
            std::thread::sleep(Duration::from_millis(5));
        }
    });
    let engine = FakeEngine::new();
    // 300 characters: three seconds of speech each.
    let long = format!("0.5{}", "0".repeat(297));
    speaker.speak(sentence(&engine, &long));
    speaker.speak(sentence(&engine, &long));
    std::thread::sleep(Duration::from_millis(200));
    let stopped = Instant::now();
    speaker.stop();
    assert_eq!(played.recv_timeout(WAIT).expect("first"), 1);
    assert_eq!(played.recv_timeout(WAIT).expect("second"), 2);
    assert!(
        stopped.elapsed() < Duration::from_secs(1),
        "not after the three seconds the speech would have lasted"
    );

    // Speech after the stop is timed from now, not behind the silenced sentences.
    let again = Instant::now();
    assert_eq!(speaker.speak(sentence(&engine, "0.25")), 3);
    assert_eq!(played.recv_timeout(WAIT).expect("third"), 3);
    assert!(again.elapsed() < Duration::from_secs(1));
    playing.store(false, Ordering::Release);
    device.join().expect("device thread");
}

#[test]
fn sherpa_reads_every_reference_a_voice_keeps() {
    // An own voice keeps up to 30 s ending on a pause; sherpa would cut it at 10 s, mid-word.
    let len: f64 = MAX_REFERENCE_LEN.parse().expect("a number");
    assert!((len - super::clone::MAX_REFERENCE_SECS).abs() < 1e-9);
}

/// Spec 081 T034: every sentence the speaker spoke reaches the meter before it is reported as
/// played, so the assistant can close the answer then; one write per answer.
#[test]
fn the_meter_gets_every_spoken_sentence() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let gate = VaultGate::with_runtime(runtime.handle().clone());
    let (input, mut render) = mixer(MIXER_RATE);
    let out: Arc<dyn SpeechOut> = Arc::new(input);
    let dir = tempfile::tempdir().expect("dir");
    let meter = Arc::new(super::speed::Meter::new(dir.path(), "cpu"));
    let (played, reports) = mpsc::channel();
    let played = Mutex::new(played);
    let report_meter = Arc::clone(&meter);
    let speaker = Speaker::start_reporting(
        &gate,
        out,
        Arc::new(move |ticket| {
            let _ = played
                .lock()
                .expect("played")
                .send((ticket, report_meter.pending_sentences()));
        }),
    )
    .expect("speaker starts");
    let engine = FakeEngine::new();
    for level in ["0.25", "0.5", "0.75"] {
        let mut measured = sentence(&engine, level);
        measured.meter = Some(Arc::clone(&meter));
        speaker.speak(measured);
    }
    render_until(&mut render, (4 + 3 + 4) * MIXER_RATE as usize / 100);
    for expected in 1..=3 {
        let (ticket, count) = reports.recv_timeout(WAIT).expect("played");
        assert_eq!(ticket, expected);
        assert!(count >= expected as usize, "report {expected} saw {count}");
    }
    meter.answer_over();
    let run = super::speed::of_model(dir.path(), "cpu");
    assert_eq!(run.sentences.len(), 3);
    // 10 ms per character: 0.04 s, 0.03 s and 0.04 s of audio.
    let audio: f64 = run.sentences.iter().map(|s| s.parts[0][1]).sum();
    assert!((audio - 0.11).abs() < 1e-6, "{audio}");
}

/// Spec 081 FR-017: what the answer may hold before it starts.
#[test]
fn an_answer_starts_once_the_rest_can_be_computed_in_time() {
    use super::local::may_start;
    // Nothing more to compute: start at once, whatever the speed.
    assert!(may_start(3.0, 0.0, 0, 0, true));
    // Twice as slow as real time: the held audio must be as long as the rest.
    assert!(!may_start(2.0, 1.0, 10, 20, true), "1 s held, 2 s to come");
    assert!(may_start(2.0, 2.0, 20, 20, true), "2 s held, 2 s to come");
    // 1.35 (the processor measurement of Qwen3-TTS): a third of the rest is enough.
    assert!(may_start(1.35, 1.0, 10, 28, true));
    assert!(!may_start(1.35, 1.0, 10, 30, true));
    // Nothing held yet: wait for the first sentence.
    assert!(!may_start(2.0, 0.0, 0, 5, true));
    // An empty queue can mean the model is still producing the next sentence.
    assert!(!may_start(2.0, 1.0, 10, 0, false));
    // While the chat model writes, the rest is unknown: no amount held is enough.
    assert!(!may_start(2.0, 100.0, 1000, 5, false));
    // A model that keeps up starts with its first sentence, written to the end or not.
    assert!(may_start(0.5, 0.2, 10, 0, false));
    assert!(may_start(1.0, 0.2, 10, 50, false));
}

/// Spec 081 FR-017 (operator 2026-11-06): a speech model slower than real time keeps the first
/// sentences back until the rest can be computed before they have played, then plays them one
/// after another.
#[test]
fn a_slow_model_holds_the_answer_until_it_can_play_without_gaps() {
    let mut rig = rig();
    let (entered, synthesizing) = mpsc::channel();
    let (release, proceed) = mpsc::channel();
    let engine = Arc::new(FakeEngine {
        generating: Some((entered, Mutex::new(proceed))),
        before_queue: Mutex::new(None),
        calls: AtomicUsize::new(0),
    });
    // Measured twice as slow as real time.
    let dir = tempfile::tempdir().expect("dir");
    let meter = Arc::new(super::speed::Meter::new(dir.path(), "cpu"));
    let slow = super::speed::SentenceSpeed {
        parts: vec![[2.0, 1.0]],
    };
    meter.store_answer(vec![slow.clone(), slow.clone(), slow]);
    let measured = |level: &str| {
        let mut s = sentence(&engine, level);
        s.meter = Some(Arc::clone(&meter));
        s
    };
    // 0.04 s, then 0.03 s, then 0.04 s of audio (10 ms per character).
    for level in ["0.25", "0.5", "0.75"] {
        rig.speaker.speak(measured(level));
    }
    rig.speaker.finish_answer();

    synthesizing.recv_timeout(WAIT).expect("first sentence");
    release.send(()).expect("first");
    synthesizing.recv_timeout(WAIT).expect("second sentence");
    // After the first: 0.04 s held, 0.07 s to come at ratio 2 — it waits.
    let mut block = vec![0.0; 4 * MIXER_RATE as usize / 100];
    rig.render.render(&mut block);
    assert!(block.iter().all(|s| *s == 0.0), "nothing plays yet");

    release.send(()).expect("second");
    synthesizing.recv_timeout(WAIT).expect("third sentence");
    // After the second: 0.07 s held, 0.04 s to come — both play, back to back.
    let first_two = (4 + 3) * MIXER_RATE as usize / 100;
    let heard = render_until(&mut rig.render, first_two);
    assert_eq!(heard.len(), first_two, "the held sentences play");
    assert!((heard[0] - 0.25).abs() < 1e-3);
    assert!((heard[first_two - 1] - 0.5).abs() < 1e-3);

    release.send(()).expect("third");
    let last = render_until(&mut rig.render, 4 * MIXER_RATE as usize / 100);
    assert!(!last.is_empty(), "the rest plays as it comes");
    assert!(last.iter().all(|s| (*s - 0.75).abs() < 1e-3));
}

#[test]
fn a_model_that_keeps_up_plays_the_first_sentence_at_once() {
    let mut rig = rig();
    let engine = FakeEngine::new();
    let dir = tempfile::tempdir().expect("dir");
    let meter = Arc::new(super::speed::Meter::new(dir.path(), "cpu"));
    let fast = super::speed::SentenceSpeed {
        parts: vec![[0.3, 1.0]],
    };
    meter.store_answer(vec![fast.clone(), fast.clone(), fast]);
    let mut first = sentence(&engine, "0.25");
    first.meter = Some(Arc::clone(&meter));
    rig.speaker.speak(first);
    let heard = render_until(&mut rig.render, 4 * MIXER_RATE as usize / 100);
    assert!(!heard.is_empty());
}

/// Spec 081 FR-017 (operator 2026-11-06, second pass): a measured slow model waits for the end of
/// the answer, because text the chat model has not written yet cannot be computed in advance; the
/// pauses between paragraphs came from starting before that.
#[test]
fn a_slow_model_waits_for_the_end_of_the_answer() {
    let mut rig = rig();
    let engine = FakeEngine::new();
    let dir = tempfile::tempdir().expect("dir");
    let meter = Arc::new(super::speed::Meter::new(dir.path(), "cpu"));
    let slow = super::speed::SentenceSpeed {
        parts: vec![[2.0, 1.0]],
    };
    meter.store_answer(vec![slow.clone(), slow.clone(), slow]);
    for level in ["0.25", "0.5"] {
        let mut s = sentence(&engine, level);
        s.meter = Some(Arc::clone(&meter));
        rig.speaker.speak(s);
    }
    // Both are computed long before this; the answer may go on.
    std::thread::sleep(Duration::from_millis(200));
    let mut block = vec![0.0; 7 * MIXER_RATE as usize / 100];
    rig.render.render(&mut block);
    assert!(
        block.iter().all(|s| *s == 0.0),
        "nothing plays while the answer is written"
    );

    rig.speaker.finish_answer();
    let both = (4 + 3) * MIXER_RATE as usize / 100;
    let heard = render_until(&mut rig.render, both);
    assert_eq!(heard.len(), both, "the answer plays once it is written");
    assert!((heard[0] - 0.25).abs() < 1e-3);
    assert!((heard[both - 1] - 0.5).abs() < 1e-3);
}

/// Spec 081 FR-017: a model not measured yet is held until its first sentence shows its pace; one
/// that keeps up then plays at once, without waiting for the end of the answer.
#[test]
fn an_unmeasured_model_that_keeps_up_plays_its_first_sentence() {
    let mut rig = rig();
    let engine = FakeEngine::new();
    let dir = tempfile::tempdir().expect("dir");
    let meter = Arc::new(super::speed::Meter::new(dir.path(), "cpu"));
    let mut first = sentence(&engine, "0.25");
    first.meter = Some(Arc::clone(&meter));
    rig.speaker.speak(first);
    let heard = render_until(&mut rig.render, 4 * MIXER_RATE as usize / 100);
    assert_eq!(heard.len(), 4 * MIXER_RATE as usize / 100);
}

/// Spec 081 FR-017: a model not measured yet that turns out slower than real time is held by the
/// pace of its first sentences, through the end of the answer.
#[test]
fn an_unmeasured_slow_model_is_held_by_its_own_pace() {
    let mut rig = rig();
    let (entered, synthesizing) = mpsc::channel();
    let (release, proceed) = mpsc::channel();
    let engine = Arc::new(FakeEngine {
        generating: Some((entered, Mutex::new(proceed))),
        before_queue: Mutex::new(None),
        calls: AtomicUsize::new(0),
    });
    let dir = tempfile::tempdir().expect("dir");
    let meter = Arc::new(super::speed::Meter::new(dir.path(), "cpu"));
    for level in ["0.25", "0.5"] {
        let mut s = sentence(&engine, level);
        s.meter = Some(Arc::clone(&meter));
        rig.speaker.speak(s);
    }
    // 0.04 s and 0.03 s of audio, each after at least 0.1 s of computing.
    for _ in 0..2 {
        synthesizing.recv_timeout(WAIT).expect("sentence");
        std::thread::sleep(Duration::from_millis(100));
        release.send(()).expect("computed");
    }
    std::thread::sleep(Duration::from_millis(100));
    let mut block = vec![0.0; 7 * MIXER_RATE as usize / 100];
    rig.render.render(&mut block);
    assert!(
        block.iter().all(|s| *s == 0.0),
        "a slow model waits for the end"
    );

    rig.speaker.finish_answer();
    let both = (4 + 3) * MIXER_RATE as usize / 100;
    assert_eq!(render_until(&mut rig.render, both).len(), both);
}

/// Hands out the English voice of `engine`, and nothing for any other language.
struct EnglishOnly(Arc<FakeEngine>);

impl LanguageVoices for EnglishOnly {
    fn voice(&self, language: &str) -> Option<Arc<LanguageVoice>> {
        (language == "en").then(|| {
            Arc::new(LanguageVoice {
                engine: Arc::clone(&self.0) as Arc<dyn Engine>,
                reserve: None,
                reference: Arc::new(Reference {
                    samples: vec![0.0; 160],
                    sample_rate: ENGINE_RATE,
                    language: "en".to_owned(),
                }),
                speech: None,
            })
        })
    }
}

/// Spec 066 FR-022d: a sentence in another language speaks with that language's voice; without
/// one it keeps its own.
#[test]
fn a_passage_in_another_language_speaks_with_its_voice() {
    let mut rig = rig();
    let german = FakeEngine::new();
    let english = FakeEngine::new();
    let voices: Arc<dyn LanguageVoices> = Arc::new(EnglishOnly(Arc::clone(&english)));
    let mut quoted = sentence(&german, "0.25");
    quoted.passage = Some(Passage {
        language: "en".to_owned(),
        voices: Arc::clone(&voices),
    });
    let mut unknown = sentence(&german, "0.5");
    unknown.passage = Some(Passage {
        language: "fr".to_owned(),
        voices: Arc::clone(&voices),
    });
    let mut own = sentence(&german, "0.75");
    own.passage = Some(Passage {
        language: "de".to_owned(),
        voices,
    });
    rig.speaker.speak(quoted);
    rig.speaker.speak(unknown);
    rig.speaker.speak(own);
    let expected = (4 + 3 + 4) * MIXER_RATE as usize / 100;
    let heard = render_until(&mut rig.render, expected);
    assert!(heard.len().abs_diff(expected) <= 2, "{}", heard.len());
    assert_eq!(english.calls.load(Ordering::SeqCst), 1);
    assert_eq!(german.calls.load(Ordering::SeqCst), 2);
}
