use super::ring::{RingBuffer, CAPACITY_SECS, PREROLL_SECS};
use crate::stt::SAMPLE_RATE_HZ;

fn ramp(from: usize, to: usize) -> Vec<f32> {
    (from..to).map(|i| i as f32).collect()
}

#[test]
fn holds_at_most_two_seconds_of_16_khz_mono() {
    let ring = RingBuffer::with_seconds(CAPACITY_SECS);
    assert_eq!(ring.capacity(), 2 * SAMPLE_RATE_HZ as usize);
}

#[test]
fn preroll_comes_back_in_order_after_wrapping_around() {
    let mut ring = RingBuffer::with_seconds(CAPACITY_SECS);
    let total = 5 * SAMPLE_RATE_HZ as usize + 123;
    // Blocks of an odd size, so the write position wraps in the middle of a block.
    for start in (0..total).step_by(1_337) {
        ring.push(&ramp(start, (start + 1_337).min(total)));
    }

    let preroll = (PREROLL_SECS * SAMPLE_RATE_HZ as f32) as usize;
    let mut out = Vec::new();
    ring.copy_latest(preroll, &mut out);

    assert_eq!(out, ramp(total - preroll, total));
}

#[test]
fn an_overrun_drops_the_oldest_samples() {
    let mut ring = RingBuffer::new(10);
    ring.push(&ramp(0, 4));
    ring.push(&ramp(4, 25));

    let mut out = Vec::new();
    ring.copy_latest(100, &mut out);

    assert_eq!(ring.len(), 10);
    assert_eq!(out, ramp(15, 25));
}

#[test]
fn asking_for_more_than_was_recorded_gives_what_there_is() {
    let mut ring = RingBuffer::new(10);
    ring.push(&ramp(0, 3));

    let mut out = Vec::new();
    ring.copy_latest(8, &mut out);

    assert_eq!(out, ramp(0, 3));
}

#[test]
fn pushing_never_grows_the_buffer() {
    let mut ring = RingBuffer::new(1_000);
    let capacity = ring.capacity();
    for _ in 0..50 {
        ring.push(&[0.5; 777]);
    }
    assert_eq!(ring.capacity(), capacity);
    assert_eq!(ring.len(), capacity);
}

#[test]
fn clear_forgets_the_audio() {
    let mut ring = RingBuffer::new(10);
    ring.push(&ramp(0, 10));
    ring.clear();

    let mut out = Vec::new();
    ring.copy_latest(10, &mut out);

    assert!(ring.is_empty());
    assert!(out.is_empty());
}
