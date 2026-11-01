use std::path::{Path, PathBuf};
use std::time::Duration;

use super::mixer::mixer;
use super::source::{Read, RecordingSink, Sink, Source, Step, WavScript};
use crate::stt::fixture_wav::read_fixture_wav;
use crate::stt::SAMPLE_RATE_HZ;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/stt/fixtures/de_sample.wav")
}

fn read_all(source: &mut dyn Source) -> Vec<f32> {
    let mut out = Vec::new();
    while source.read(&mut out) == Read::Data {}
    out
}

#[test]
fn a_wav_script_gives_its_files_with_the_pauses_in_between() {
    let mut script = WavScript::load(
        &[
            Step::File(fixture()),
            Step::Pause(Duration::from_millis(500)),
            Step::File(fixture()),
        ],
        false,
    )
    .expect("script loads");

    let file = read_fixture_wav(&fixture());
    let mut expected = file.clone();
    expected.extend(std::iter::repeat_n(0.0, SAMPLE_RATE_HZ as usize / 2));
    expected.extend(&file);

    let got = read_all(&mut script);
    assert_eq!(got.len(), expected.len());
    for (i, (a, b)) in got.iter().zip(&expected).enumerate() {
        assert!((a - b).abs() < 1e-4, "sample {i}: {a} vs {b}");
    }
    assert_eq!(script.read(&mut Vec::new()), Read::Ended);
}

#[test]
fn a_paced_script_waits_for_the_time_of_its_audio() {
    let mut script = WavScript::load(&[Step::File(fixture())], true).expect("script loads");
    assert_eq!(script.read(&mut Vec::new()), Read::Idle);
}

#[test]
fn a_missing_file_is_an_error() {
    let missing = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/stt/fixtures/nope.wav");
    assert!(WavScript::load(&[Step::File(missing)], false).is_err());
}

#[test]
fn the_recording_sink_keeps_what_the_mixer_put_out() {
    let (input, mut render) = mixer(SAMPLE_RATE_HZ);
    let speech: Vec<f32> = (0..2_000).map(|i| (i as f32 / 50.0).sin() * 0.5).collect();
    assert!(input.queue_speech(&speech, || false));

    let sink = RecordingSink::default();
    let mut writer: Box<dyn Sink> = Box::new(sink.clone());
    let mut rendered = Vec::new();
    for _ in 0..3 {
        let mut block = vec![0.0; 1_000];
        render.render(&mut block);
        writer.write(&block);
        rendered.extend(block);
    }

    assert_eq!(sink.recorded(), rendered);
    assert_eq!(&sink.recorded()[..speech.len()], &speech[..]);
}
