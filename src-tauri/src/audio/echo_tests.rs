//! Spec 066 T099 (FR-016): haex's own output is taken out of what the microphone hears.

use std::sync::{Arc, Mutex};

use super::blocks::block_queue;
use super::echo::{Aec3, EchoCanceller, EchoStage, FRAME};

/// A deterministic, speech-like signal: noise from a fixed generator, low-passed, with a syllable
/// envelope of 4 Hz.
fn voice(seconds: f32, seed: u64) -> Vec<f32> {
    let len = (seconds * 16_000.0) as usize;
    let mut state = seed;
    let mut low = 0.0f32;
    (0..len)
        .map(|i| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let white = ((state >> 33) as f32 / (1u64 << 31) as f32) * 2.0 - 1.0;
            low += 0.3 * (white - low);
            let syllable = (std::f32::consts::PI * 4.0 * i as f32 / 16_000.0)
                .sin()
                .abs();
            0.4 * low * syllable
        })
        .collect()
}

/// The room between speaker and microphone: 30 ms late, quieter, with two weaker reflections.
fn room(reference: &[f32]) -> Vec<f32> {
    let taps = [(480, 0.5f32), (560, 0.2), (720, 0.08)];
    (0..reference.len())
        .map(|i| {
            taps.iter()
                .filter(|(delay, _)| i >= *delay)
                .map(|(delay, gain)| reference[i - delay] * gain)
                .sum()
        })
        .collect()
}

fn energy(samples: &[f32]) -> f64 {
    samples.iter().map(|s| f64::from(*s) * f64::from(*s)).sum()
}

/// Runs `reference` and `mic` through AEC3 frame by frame; returns the cancelled microphone.
fn cancel(reference: &[f32], mic: &[f32]) -> Vec<f32> {
    let mut aec = Aec3::new();
    let mut out = Vec::with_capacity(mic.len());
    for (far, near) in reference
        .as_chunks::<FRAME>()
        .0
        .iter()
        .zip(mic.as_chunks::<FRAME>().0)
    {
        aec.render(far);
        let mut frame = near.to_vec();
        aec.capture(&mut frame);
        out.extend_from_slice(&frame);
    }
    out
}

#[test]
fn haex_speaking_alone_is_taken_out_of_the_microphone() {
    let reference = voice(8.0, 7);
    let mic = room(&reference);
    let out = cancel(&reference, &mic);
    // After the canceller has found the room (the first 3 s), at least 20 dB of the echo is gone.
    let settled = 3 * 16_000;
    let before = energy(&mic[settled..]);
    let after = energy(&out[settled..]);
    let erle_db = 10.0 * (before / after.max(1e-12)).log10();
    assert!(erle_db >= 20.0, "echo reduced by only {erle_db:.1} dB");
}

#[test]
fn the_user_talking_while_haex_is_silent_passes() {
    let silence = vec![0.0; 4 * 16_000];
    let user = voice(4.0, 99);
    let out = cancel(&silence, &user);
    let kept = energy(&out[16_000..]) / energy(&user[16_000..]);
    assert!(
        kept > 0.5,
        "only {:.0} % of the user's voice is left",
        kept * 100.0
    );
}

#[test]
fn the_user_talking_over_haex_stays_audible() {
    let reference = voice(8.0, 7);
    let user = voice(8.0, 99);
    let echo = room(&reference);
    let mic: Vec<f32> = echo.iter().zip(&user).map(|(e, u)| e + u).collect();
    let out = cancel(&reference, &mic);
    // In the last 3 s the user is still there: the canceller does not suppress them along with
    // the echo. (Its band filters delay the signal, so this compares loudness, not samples.)
    let tail = 5 * 16_000;
    let kept = energy(&out[tail..]) / energy(&user[tail..]);
    assert!(
        kept > 0.3,
        "only {:.0} % of the user's voice is left",
        kept * 100.0
    );
}

/// Subtracts the reference it was given, frame by frame, and remembers what it saw.
#[derive(Default)]
struct Subtract {
    pending: Vec<Vec<f32>>,
    seen: Arc<Mutex<(usize, usize)>>,
}

impl EchoCanceller for Subtract {
    fn render(&mut self, frame: &[f32]) {
        assert_eq!(frame.len(), FRAME);
        self.seen.lock().unwrap().0 += 1;
        self.pending.push(frame.to_vec());
    }

    fn capture(&mut self, frame: &mut [f32]) {
        assert_eq!(frame.len(), FRAME);
        self.seen.lock().unwrap().1 += 1;
        if !self.pending.is_empty() {
            let far = self.pending.remove(0);
            for (s, f) in frame.iter_mut().zip(far) {
                *s -= f;
            }
        }
    }
}

#[test]
fn the_stage_brings_reference_and_microphone_to_whole_frames() {
    let seen = Arc::new(Mutex::new((0, 0)));
    let canceller = Subtract {
        pending: Vec::new(),
        seen: Arc::clone(&seen),
    };
    let (sender, receiver) = block_queue(8, 1024);
    let mut stage = EchoStage::new(Box::new(canceller), receiver, 48_000);

    // 30 ms of a constant reference at 48 kHz, in uneven blocks.
    for len in [700, 1_000, 1_180] {
        let mut block = sender.take().expect("free block");
        block.samples.extend(std::iter::repeat_n(0.5, len));
        sender.send(block);
    }
    // 25 ms of microphone at 16 kHz in two pieces: two whole frames come out, 5 ms wait.
    let mut out = Vec::new();
    stage.process(&[0.5; 250], &mut out);
    stage.process(&[0.5; 150], &mut out);
    assert_eq!(out.len(), 2 * FRAME);
    assert_eq!(seen.lock().unwrap().1, 2);
    assert!(seen.lock().unwrap().0 >= 2, "reference frames handed on");
    // The middle of the frames cancels to (almost) nothing; the resampler's start may differ.
    assert!(out[40..].iter().all(|s| s.abs() < 0.05), "{:?}", &out[..8]);

    stage.process(&[0.5; 80], &mut out);
    assert_eq!(out.len(), 3 * FRAME);
}
