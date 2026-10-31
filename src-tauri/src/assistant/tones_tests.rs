//! Spec 066 T017: the tone set.

use std::time::Duration;

use super::tones::{
    always_audible, duration, notes, render, repeat_interval, ToneId, SCALE, WORKING_REPEAT,
};

const RATE: u32 = 48_000;

#[test]
fn every_tone_of_the_contract_renders() {
    let wire: Vec<String> = ToneId::ALL
        .iter()
        .map(|tone| serde_json::to_value(tone).expect("serializes"))
        .map(|value| value.as_str().expect("a string").to_owned())
        .collect();
    assert_eq!(
        wire,
        [
            "listening",
            "understood",
            "working",
            "question",
            "done",
            "error",
            "aborted",
            "notUnderstood",
            "conversationEnd",
            "attention",
            "secret",
            "modeOn",
            "modeOff",
            "muted",
            "unmuted"
        ]
    );
    for tone in ToneId::ALL {
        let samples = render(tone, RATE);
        let expected = duration(tone).as_secs_f64() * f64::from(RATE);
        assert!(
            (samples.len() as f64 - expected).abs() <= notes(tone).len() as f64 * 2.0,
            "{tone:?}: {} samples for {expected}",
            samples.len()
        );
        assert!(
            samples.iter().any(|s| s.abs() > 0.05),
            "{tone:?} is audible"
        );
    }
}

#[test]
fn tones_are_short_except_working() {
    for tone in ToneId::ALL {
        assert!(
            duration(tone) < Duration::from_millis(400),
            "{tone:?} lasts {:?}",
            duration(tone)
        );
        let expected = (tone == ToneId::Working).then_some(WORKING_REPEAT);
        assert_eq!(repeat_interval(tone), expected, "{tone:?}");
    }
    assert_eq!(WORKING_REPEAT, Duration::from_secs(2));
}

#[test]
fn every_note_belongs_to_the_family_scale() {
    for tone in ToneId::ALL {
        for note in notes(tone) {
            assert!(SCALE.contains(&note.step), "{tone:?}: step {}", note.step);
            assert!((0.0..=1.0).contains(&note.gain), "{tone:?}");
        }
    }
}

#[test]
fn tones_stay_below_clipping_and_end_quietly() {
    for tone in ToneId::ALL {
        let samples = render(tone, RATE);
        let peak = samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()));
        assert!(peak < 0.9, "{tone:?} peaks at {peak}");
        let tail = &samples[samples.len() - RATE as usize / 1000..];
        assert!(
            tail.iter().all(|s| s.abs() < 0.02),
            "{tone:?} ends without a click"
        );
    }
}

#[test]
fn working_is_quieter_than_the_rest() {
    let peak = |tone| {
        render(tone, RATE)
            .iter()
            .fold(0.0f32, |p, s| p.max(s.abs()))
    };
    let working = peak(ToneId::Working);
    for tone in ToneId::ALL.into_iter().filter(|t| *t != ToneId::Working) {
        assert!(working < peak(tone), "{tone:?}");
    }
}

#[test]
fn every_tone_is_distinguishable() {
    for (i, a) in ToneId::ALL.iter().enumerate() {
        for b in &ToneId::ALL[i + 1..] {
            let steps = |tone: &ToneId| notes(*tone).iter().map(|n| n.step).collect::<Vec<_>>();
            assert_ne!(steps(a), steps(b), "{a:?} and {b:?} have the same melody");
        }
    }
}

#[test]
fn only_listening_and_secret_ignore_the_switch() {
    let always: Vec<_> = ToneId::ALL
        .into_iter()
        .filter(|t| always_audible(*t))
        .collect();
    assert_eq!(always, [ToneId::Listening, ToneId::Secret]);
}

/// Writes every tone, with a second of silence in between, to `HOLZI_TONES_SAVE_DIR/tones.wav`
/// for listening.
#[test]
#[ignore = "writes a WAV for listening, see the doc comment"]
fn save_the_tones_for_listening() {
    let dir = std::env::var("HOLZI_TONES_SAVE_DIR").expect("HOLZI_TONES_SAVE_DIR");
    let mut all = Vec::new();
    for tone in ToneId::ALL {
        all.extend(render(tone, RATE));
        all.resize(all.len() + RATE as usize, 0.0);
    }
    let path = std::path::Path::new(&dir).join("tones.wav");
    assert!(sherpa_onnx::write(
        path.to_str().expect("utf-8"),
        &all,
        RATE as i32
    ));
}
