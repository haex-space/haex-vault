//! Spec 066 T017: the tone set.

use std::time::Duration;

use super::tones::{
    always_audible, duration, notes, render, repeat_interval, Timbre, ToneId, SCALE, WORKING_REPEAT,
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
            assert!(SCALE.contains(&note.to), "{tone:?}: glides to {}", note.to);
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

/// Colours in order and the overall direction (up, level, down) of `tone`.
fn shape(tone: ToneId) -> (Vec<Timbre>, std::cmp::Ordering) {
    let notes = notes(tone);
    let timbres = notes.iter().map(|n| n.timbre).collect();
    let first = notes.first().expect("a note").step;
    let last = notes.last().expect("a note").to;
    (timbres, last.cmp(&first))
}

#[test]
fn every_tone_is_distinguishable() {
    // Not only the melody differs: no two tones share their colours and their direction, so
    // "listening" and "understood" can be told apart without comparing pitches.
    for (i, a) in ToneId::ALL.iter().enumerate() {
        for b in &ToneId::ALL[i + 1..] {
            assert_ne!(shape(*a), shape(*b), "{a:?} and {b:?} have the same shape");
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
    // And each tone on its own, named by its wire name, for comparing them one by one.
    for tone in ToneId::ALL {
        let name = serde_json::to_value(tone).expect("tone name");
        let path = std::path::Path::new(&dir).join(format!("{}.wav", name.as_str().expect("text")));
        assert!(sherpa_onnx::write(
            path.to_str().expect("utf-8"),
            &render(tone, RATE),
            RATE as i32
        ));
    }
}

#[test]
fn a_tone_sample_sounds_at_the_chosen_volume_and_waits_for_its_end() {
    use crate::audio::mixer::mixer;
    use tokio_util::sync::CancellationToken;

    let (input, mut render_side) = mixer(RATE);
    let started = std::time::Instant::now();
    assert!(super::tones::sample(
        &input,
        &render(ToneId::Done, RATE),
        50,
        &CancellationToken::new()
    ));
    assert!(started.elapsed() >= duration(ToneId::Done));

    let mut out = vec![0.0; render(ToneId::Done, RATE).len()];
    render_side.render(&mut out);
    let full = render(ToneId::Done, RATE);
    let peak = |s: &[f32]| s.iter().fold(0.0_f32, |m, x| m.max(x.abs()));
    assert!((peak(&out) - peak(&full) * 0.5).abs() < 1e-3, "half volume");
}

#[test]
fn a_newer_sample_ends_the_wait_for_a_tone() {
    use crate::audio::mixer::mixer;
    use tokio_util::sync::CancellationToken;

    let (input, _render_side) = mixer(RATE);
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(!super::tones::sample(
        &input,
        &render(ToneId::Done, RATE),
        60,
        &cancel
    ));
}

#[test]
fn tones_arrive_from_the_window_by_their_wire_name() {
    for tone in ToneId::ALL {
        let wire = serde_json::to_value(tone).expect("serializes");
        let back: ToneId = serde_json::from_value(wire).expect("deserializes");
        assert_eq!(back, tone);
    }
}
