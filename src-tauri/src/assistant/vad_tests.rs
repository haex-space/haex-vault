use std::path::{Path, PathBuf};

use super::vad::{PauseDetector, Utterance, VadError, MIN_SPEECH_SECS};
use crate::stt::fixture_wav::read_fixture_wav;
use crate::stt::SAMPLE_RATE_HZ;

fn model() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/assistant/models/silero_vad.int8.onnx")
}

fn speech() -> Vec<f32> {
    read_fixture_wav(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src/stt/fixtures/de_sample.wav"))
}

fn silence(secs: f32) -> Vec<f32> {
    vec![0.0; (secs * SAMPLE_RATE_HZ as f32) as usize]
}

/// Feeds `audio` in 100 ms blocks like the microphone stream does, then ends the stream.
fn run(pause_ms: u32, audio: &[f32]) -> Vec<Utterance> {
    let mut detector = PauseDetector::new(&model(), pause_ms).expect("fixture model loads");
    let mut utterances = Vec::new();
    for block in audio.chunks(SAMPLE_RATE_HZ as usize / 10) {
        utterances.extend(detector.push(block));
    }
    utterances.extend(detector.finish());
    utterances
}

#[test]
fn speech_becomes_utterances_inside_the_audio() {
    let mut audio = silence(1.0);
    audio.extend(speech());
    audio.extend(silence(1.5));

    let utterances = run(800, &audio);

    assert!(!utterances.is_empty());
    let min_len = (MIN_SPEECH_SECS * SAMPLE_RATE_HZ as f32) as usize;
    for utterance in &utterances {
        assert!(utterance.samples.len() >= min_len);
        assert!(utterance.start_sample as usize + utterance.samples.len() <= audio.len());
    }
    // The leading second of silence is not part of the first utterance.
    assert!(utterances[0].start_sample >= SAMPLE_RATE_HZ as u64 / 2);
}

#[test]
fn silence_gives_no_utterance() {
    assert!(run(800, &silence(3.0)).is_empty());
}

#[test]
fn a_longer_pause_setting_joins_what_a_short_one_splits() {
    let mut audio = speech();
    audio.extend(silence(1.0));
    audio.extend(speech());
    audio.extend(silence(2.5));

    let split = run(500, &audio).len();
    let joined = run(2000, &audio).len();

    assert!(split >= 2, "a 1 s gap splits at 500 ms, got {split}");
    assert!(
        joined < split,
        "2000 ms joins across the gap: {joined} vs {split}"
    );
}

#[test]
fn finish_returns_running_speech_and_starts_over() {
    let mut detector = PauseDetector::new(&model(), 2000).expect("fixture model loads");
    let pending = detector.push(&speech());
    let tail = detector.finish();
    assert!(!pending.is_empty() || !tail.is_empty());

    let again = detector.push(&silence(1.0));
    assert!(again.is_empty());
    assert!(!detector.in_speech());
}

#[test]
fn a_missing_model_is_an_error_not_a_crash() {
    let missing = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/assistant/models/nope.onnx");
    assert!(matches!(
        PauseDetector::new(&missing, 800),
        Err(VadError::ModelMissing)
    ));
}

#[test]
fn a_corrupt_model_is_an_error_not_a_crash() {
    let dir = tempfile::tempdir().expect("temp dir");
    let corrupt = dir.path().join("silero_vad.onnx");
    std::fs::write(&corrupt, b"not an onnx model").expect("write");
    assert!(matches!(
        PauseDetector::new(&corrupt, 800),
        Err(VadError::ModelLoad)
    ));
}
