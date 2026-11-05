//! Spec 081 T033: the speed of a speech model on this device.

use super::speed::{self, Meter, Run, SentenceSpeed, Speed, SpeedFile};

fn sentence(compute: f64, audio: f64) -> SentenceSpeed {
    SentenceSpeed {
        parts: vec![[compute, audio]],
    }
}

#[test]
fn the_ratio_is_computing_over_audio_across_all_parts() {
    let mut run = Run::default();
    run.add_answer(
        vec![
            SentenceSpeed {
                parts: vec![[1.0, 2.0], [0.5, 1.0]],
            },
            sentence(1.5, 3.0),
            sentence(3.0, 2.0),
        ],
        1,
    );
    // (1.0 + 0.5 + 1.5 + 3.0) / (2.0 + 1.0 + 3.0 + 2.0) = 6 / 8
    assert_eq!(run.ratio(), Some(0.75));
    assert_eq!(run.speed(), Speed::KeepsUp);
}

#[test]
fn fewer_than_three_sentences_are_not_measured_yet() {
    let mut run = Run::default();
    run.add_answer(vec![sentence(0.5, 1.0), sentence(0.5, 1.0)], 1);
    assert_eq!(run.ratio(), None);
    assert_eq!(run.speed(), Speed::Unknown);
    run.add_answer(vec![sentence(2.0, 1.0)], 2);
    assert_eq!(run.speed(), Speed::Slow, "3 / 3 is not below 1.0");
}

#[test]
fn only_the_last_ten_sentences_count() {
    let mut run = Run::default();
    run.add_answer((0..10).map(|_| sentence(5.0, 1.0)).collect(), 1);
    run.add_answer((0..10).map(|_| sentence(0.5, 1.0)).collect(), 2);
    assert_eq!(run.sentences.len(), 10);
    assert_eq!(run.ratio(), Some(0.5));
}

#[test]
fn three_slow_answers_in_a_row_reach_the_hint_once() {
    let mut run = Run::default();
    let slow = || vec![sentence(2.0, 1.0)];
    assert!(!run.add_answer(slow(), 1));
    assert!(!run.add_answer(slow(), 2));
    assert!(run.add_answer(slow(), 3), "the third slow answer");
    assert!(!run.add_answer(slow(), 4), "only once per streak");
    assert_eq!(run.slow_streak, 4);
}

#[test]
fn an_answer_that_keeps_up_resets_the_streak() {
    let mut run = Run::default();
    run.add_answer(vec![sentence(2.0, 1.0)], 1);
    run.add_answer(vec![sentence(2.0, 1.0)], 2);
    run.add_answer(vec![sentence(0.5, 1.0)], 3);
    assert_eq!(run.slow_streak, 0);
    assert!(!run.add_answer(vec![sentence(2.0, 1.0)], 4));
}

#[test]
fn an_unknown_version_or_a_broken_file_is_no_measurement() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join(speed::FILE_NAME);
    std::fs::write(
        &path,
        r#"{"version":2,"runs":{"cpu":{"sentences":[],"slowStreak":5,"measuredAtMs":1}}}"#,
    )
    .expect("write");
    assert_eq!(speed::read(&path), SpeedFile::default());
    std::fs::write(&path, "{ not json").expect("write");
    assert_eq!(speed::read(&path), SpeedFile::default());
    assert_eq!(
        speed::read(&dir.path().join("missing.json")),
        SpeedFile::default()
    );
}

#[test]
fn the_meter_writes_once_per_answer_per_acceleration() {
    let dir = tempfile::tempdir().expect("dir");
    let meter = Meter::new(dir.path(), "cpu");
    assert!(!meter.answer_over(), "nothing spoken, nothing written");
    assert!(!dir.path().join(speed::FILE_NAME).exists());
    for _ in 0..3 {
        meter.add_sentence(sentence(0.4, 1.0));
    }
    meter.add_sentence(SentenceSpeed::default());
    assert!(!meter.answer_over());
    let run = speed::of_model(dir.path(), "cpu");
    assert_eq!(run.sentences.len(), 3, "the empty sentence is left out");
    assert_eq!(run.speed(), Speed::KeepsUp);
    assert!(run.measured_at_ms > 0);
    assert_eq!(speed::of_model(dir.path(), "gpu"), Run::default());
    assert!(!dir.path().join("speed.json.tmp").exists());
}
