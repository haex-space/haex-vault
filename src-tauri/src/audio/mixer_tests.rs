use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use super::blocks::BlockReceiver;
use super::mixer::mixer;

const RATE: u32 = 48_000;
const BLOCK: usize = 480;

fn render(render: &mut super::mixer::MixerRender, frames: usize) -> Vec<f32> {
    let mut out = vec![0.0; frames];
    render.render(&mut out);
    out
}

fn drain(reference: &BlockReceiver) -> Vec<f32> {
    let mut samples = Vec::new();
    while let Ok(block) = reference.try_recv() {
        samples.extend_from_slice(&block.samples);
        reference.recycle(block);
    }
    samples
}

#[test]
fn speech_and_tones_mix_with_their_own_volume() {
    let (input, mut out) = mixer(RATE);
    input.set_tone_volume(50);
    assert!(input.queue_speech(&[0.2; BLOCK], || false));
    assert!(input.play_tone(&[0.4; BLOCK]));

    let mixed = render(&mut out, BLOCK);

    for sample in mixed {
        assert!((sample - 0.4).abs() < 1e-6, "{sample}");
    }
}

#[test]
fn a_tone_at_volume_zero_is_silent() {
    let (input, mut out) = mixer(RATE);
    input.set_tone_volume(0);
    assert!(input.play_tone(&[0.9; BLOCK]));
    assert!(render(&mut out, BLOCK).iter().all(|&s| s == 0.0));
}

#[test]
fn the_reference_is_what_was_written() {
    let (input, mut out) = mixer(RATE);
    let reference = out.reference();
    let speech: Vec<f32> = (0..3 * BLOCK).map(|i| (i as f32 / 100.0).sin()).collect();
    assert!(input.queue_speech(&speech, || false));
    assert!(input.play_tone(&[0.25; BLOCK]));

    let mut written = Vec::new();
    for _ in 0..4 {
        written.extend(render(&mut out, BLOCK));
    }

    assert_eq!(drain(&reference), written);
}

#[test]
fn loud_input_is_clipped_to_full_scale() {
    let (input, mut out) = mixer(RATE);
    assert!(input.queue_speech(&[0.8; BLOCK], || false));
    assert!(input.play_tone(&[0.8; BLOCK]));
    assert!(render(&mut out, BLOCK).iter().all(|&s| s == 1.0));
}

#[test]
fn stop_speech_silences_speech_at_once_but_lets_a_tone_finish() {
    let (input, mut out) = mixer(RATE);
    assert!(input.queue_speech(&[0.5; 20 * BLOCK], || false));
    assert!(input.play_tone(&[0.25; 3 * BLOCK]));

    let first = render(&mut out, BLOCK);
    assert!(first.iter().all(|&s| (s - 0.75).abs() < 1e-6));

    input.stop_speech();

    // The very next block has only the tone left …
    let second = render(&mut out, BLOCK);
    assert!(second.iter().all(|&s| (s - 0.25).abs() < 1e-6));
    // … the tone plays to its end, and then it is quiet.
    let third = render(&mut out, BLOCK);
    assert!(third.iter().all(|&s| (s - 0.25).abs() < 1e-6));
    let after = render(&mut out, 4 * BLOCK);
    assert!(after.iter().all(|&s| s == 0.0));
}

#[test]
fn speech_queued_after_a_stop_plays() {
    let (input, mut out) = mixer(RATE);
    assert!(input.queue_speech(&[0.5; 4 * BLOCK], || false));
    input.stop_speech();
    assert!(input.queue_speech(&[0.3; BLOCK], || false));

    let next = render(&mut out, BLOCK);

    assert!(next.iter().all(|&s| (s - 0.3).abs() < 1e-6));
}

#[test]
fn a_stop_ends_a_producer_that_waits_for_room() {
    for cancel_job in [false, true] {
        let (input, out) = mixer(RATE);
        let input = Arc::new(input);
        let cancelled = Arc::new(AtomicBool::new(false));
        let (waiting, entered) = mpsc::channel();
        let (finished, done) = mpsc::channel();
        let producer = {
            let input = Arc::clone(&input);
            let cancelled = Arc::clone(&cancelled);
            std::thread::spawn(move || {
                let checks = AtomicUsize::new(0);
                let queued = input.queue_speech(&vec![0.1; RATE as usize * 10], || {
                    // The first 64 blocks fill the queue; the next check precedes waiting.
                    if checks.fetch_add(1, Ordering::Relaxed) == 64 {
                        waiting.send(()).expect("notify full queue");
                    }
                    cancelled.load(Ordering::Acquire)
                });
                finished.send(queued).expect("notify completion");
            })
        };
        entered
            .recv_timeout(Duration::from_secs(5))
            .expect("queue fills");
        if cancel_job {
            cancelled.store(true, Ordering::Release);
        } else {
            input.stop_speech();
        }
        let result = done.recv_timeout(Duration::from_secs(5));
        // Unblock the producer even if a regression makes it ignore cancellation.
        drop(out);
        producer.join().expect("producer thread");
        assert!(!result.expect("producer stops without playback"));
    }
}

#[test]
fn queueing_into_a_mixer_without_render_side_fails() {
    let (input, out) = mixer(RATE);
    drop(out);
    assert!(!input.queue_speech(&[0.1; BLOCK], || false));
}
