use std::sync::Arc;

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
    assert!(input.queue_speech(&[0.2; BLOCK]));
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
    assert!(input.queue_speech(&speech));
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
    assert!(input.queue_speech(&[0.8; BLOCK]));
    assert!(input.play_tone(&[0.8; BLOCK]));
    assert!(render(&mut out, BLOCK).iter().all(|&s| s == 1.0));
}

#[test]
fn stop_speech_silences_speech_at_once_but_lets_a_tone_finish() {
    let (input, mut out) = mixer(RATE);
    assert!(input.queue_speech(&[0.5; 20 * BLOCK]));
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
    assert!(input.queue_speech(&[0.5; 4 * BLOCK]));
    input.stop_speech();
    assert!(input.queue_speech(&[0.3; BLOCK]));

    let next = render(&mut out, BLOCK);

    assert!(next.iter().all(|&s| (s - 0.3).abs() < 1e-6));
}

#[test]
fn a_stop_ends_a_producer_that_waits_for_room() {
    let (input, _out) = mixer(RATE);
    let input = Arc::new(input);
    let producer = {
        let input = Arc::clone(&input);
        // Far more than the queue holds while nothing renders: the producer has to wait.
        std::thread::spawn(move || input.queue_speech(&vec![0.1; RATE as usize * 10]))
    };

    // The producer may not have started waiting yet; stopping repeatedly covers both orders.
    while !producer.is_finished() {
        input.stop_speech();
        std::thread::yield_now();
    }

    assert!(!producer.join().expect("producer thread"));
}

#[test]
fn queueing_into_a_mixer_without_render_side_fails() {
    let (input, out) = mixer(RATE);
    drop(out);
    assert!(!input.queue_speech(&[0.1; BLOCK]));
}
