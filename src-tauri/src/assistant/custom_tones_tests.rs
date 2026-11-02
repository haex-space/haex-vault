//! Spec 066 FR-045a, T157: own tones are read from every supported format, trimmed, held to their
//! length, matched in loudness, stored and assigned per device.

use std::collections::HashMap;
use std::time::Duration;

use uuid::Uuid;

use super::custom_tones::{
    decode, decode_stored, delete, encode_wav, format_assigned, insert, list, parse_assigned,
    prepare, sound_of, ImportProblem, Sound, MAX_DURATION, MAX_RATE, TARGET_PEAK,
};
use super::tones::ToneId;
use crate::passwords::test_support::open_test_vault;
use crate::storage::query;

/// The built-in "done" tone, stereo at 44.1 kHz with 0.2 s of silence before and after, written
/// by ffmpeg in each format (generated from this repo's own synthesis, no foreign licence).
const FIXTURES: [(&str, &[u8]); 5] = [
    ("wav", include_bytes!("fixtures/tones/done.wav")),
    ("mp3", include_bytes!("fixtures/tones/done.mp3")),
    ("ogg", include_bytes!("fixtures/tones/done.ogg")),
    ("flac", include_bytes!("fixtures/tones/done.flac")),
    ("m4a", include_bytes!("fixtures/tones/done.m4a")),
];

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0f32, |p, s| p.max(s.abs()))
}

fn sine(seconds: f32, rate: u32, level: f32) -> Sound {
    let count = (seconds * rate as f32) as usize;
    Sound {
        samples: (0..count)
            .map(|i| level * (std::f32::consts::TAU * 440.0 * i as f32 / rate as f32).sin())
            .collect(),
        sample_rate: rate,
    }
}

#[test]
fn every_supported_format_becomes_a_trimmed_matched_tone() {
    for (extension, bytes) in FIXTURES {
        let decoded = decode(bytes.to_vec(), Some(extension))
            .unwrap_or_else(|e| panic!("{extension}: {e:?}"));
        assert_eq!(decoded.sample_rate, 44_100, "{extension}");
        assert!(
            decoded.duration() > Duration::from_millis(700),
            "{extension}: {:?}",
            decoded.duration()
        );
        let tone = prepare(decoded, MAX_DURATION).expect(extension);
        let length = tone.duration();
        assert!(
            length > Duration::from_millis(300) && length < Duration::from_millis(500),
            "{extension}: silence trimmed, {length:?} left"
        );
        assert!(
            (peak(&tone.samples) - TARGET_PEAK).abs() < 0.01,
            "{extension}: peak {}",
            peak(&tone.samples)
        );
        assert!(tone.samples[0].abs() < 0.01, "{extension}: fades in");
        assert!(
            tone.samples.last().expect("samples").abs() < 0.01,
            "{extension}: fades out"
        );
    }
}

#[test]
fn a_file_without_readable_audio_is_refused() {
    assert_eq!(
        decode(b"not audio at all, just text".to_vec(), Some("mp3")),
        Err(ImportProblem::Unsupported)
    );
    assert_eq!(decode(Vec::new(), None), Err(ImportProblem::Unsupported));
}

#[test]
fn silence_is_refused() {
    let silent = Sound {
        samples: vec![0.0005; 48_000],
        sample_rate: 48_000,
    };
    assert_eq!(prepare(silent, MAX_DURATION), Err(ImportProblem::Silent));
}

#[test]
fn a_tone_may_last_three_seconds_after_trimming() {
    assert!(prepare(sine(2.9, 48_000, 0.8), MAX_DURATION).is_ok());
    assert_eq!(
        prepare(sine(3.2, 48_000, 0.8), MAX_DURATION),
        Err(ImportProblem::TooLong)
    );
    // Silence around a short sound does not count.
    let mut padded = vec![0.0; 48_000 * 2];
    padded.extend(sine(1.0, 48_000, 0.8).samples);
    padded.extend(vec![0.0; 48_000 * 2]);
    let tone = prepare(
        Sound {
            samples: padded,
            sample_rate: 48_000,
        },
        MAX_DURATION,
    )
    .expect("five seconds with one of sound");
    assert!(tone.duration() <= Duration::from_millis(1_001));
}

#[test]
fn a_high_rate_is_lowered_and_a_quiet_tone_raised() {
    let tone = prepare(sine(0.5, 96_000, 0.05), MAX_DURATION).expect("prepared");
    assert_eq!(tone.sample_rate, MAX_RATE);
    assert!((peak(&tone.samples) - TARGET_PEAK).abs() < 0.01);
}

#[test]
fn a_stored_tone_reads_back_as_it_was_prepared() {
    let tone = prepare(sine(0.4, 24_000, 0.6), MAX_DURATION).expect("prepared");
    let back = decode_stored(&encode_wav(&tone)).expect("decodes");
    assert_eq!(back.sample_rate, 24_000);
    assert_eq!(back.samples.len(), tone.samples.len());
    let worst = back
        .samples
        .iter()
        .zip(&tone.samples)
        .fold(0.0f32, |w, (a, b)| w.max((a - b).abs()));
    assert!(worst < 1e-3, "16-bit rounding only, {worst}");
}

#[test]
fn the_assignment_keeps_known_occasions_and_drops_the_rest() {
    let listening = Uuid::new_v4();
    let working = Uuid::new_v4();
    let mut assigned = HashMap::new();
    assigned.insert(ToneId::Listening, listening);
    assigned.insert(ToneId::Working, working);
    let stored = format_assigned(&assigned);
    assert_eq!(parse_assigned(Some(&stored)), assigned);
    assert_eq!(
        stored,
        format!(r#"{{"listening":"{listening}","working":"{working}"}}"#)
    );

    let messy = format!(r#"{{"listening":"{listening}","laughing":"{working}","done":"nope"}}"#);
    let parsed = parse_assigned(Some(&messy));
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed.get(&ToneId::Listening), Some(&listening));
    assert!(parse_assigned(Some("not json")).is_empty());
    assert!(parse_assigned(None).is_empty());
}

#[test]
fn tones_are_stored_listed_by_name_and_deleted() {
    let (_dir, db) = open_test_vault();
    let gong = prepare(sine(0.3, 48_000, 0.7), MAX_DURATION).expect("prepared");
    let (bird, alarm) = db
        .write(|tx| {
            let bird = insert(tx, "vogel", &gong, 1)?;
            let alarm = insert(tx, "Alarm", &gong, 2)?;
            Ok((bird, alarm))
        })
        .expect("insert");
    assert!(
        (295..=300).contains(&bird.duration_ms),
        "{}",
        bird.duration_ms
    );

    let listed = query::read(&db, |r| list(r)).expect("list");
    let names: Vec<&str> = listed.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["Alarm", "vogel"]);

    let wav = query::read(&db, |r| sound_of(r, bird.id))
        .expect("read")
        .expect("stored");
    assert_eq!(
        decode_stored(&wav).expect("decodes").samples.len(),
        gong.samples.len()
    );

    assert!(db.write(|tx| delete(tx, alarm.id)).expect("delete"));
    assert!(!db.write(|tx| delete(tx, alarm.id)).expect("delete again"));
    assert_eq!(
        query::read(&db, |r| sound_of(r, alarm.id)).expect("read"),
        None
    );
    assert_eq!(query::read(&db, |r| list(r)).expect("list").len(), 1);
}
