use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::*;

/// A camera that hands out `frames` pictures and then sets `stop` (or fails with `then`).
struct FakeCamera {
    left: usize,
    stop: Arc<AtomicBool>,
    then: Option<CameraProblem>,
    pulled: usize,
}

impl FakeCamera {
    fn new(frames: usize, stop: &Arc<AtomicBool>) -> Self {
        Self {
            left: frames,
            stop: Arc::clone(stop),
            then: None,
            pulled: 0,
        }
    }
}

impl FrameSource for FakeCamera {
    fn next_frame(&mut self) -> Result<Vec<u8>, CameraProblem> {
        if self.left == 0 {
            return match self.then {
                Some(problem) => Err(problem),
                None => {
                    self.stop.store(true, Ordering::SeqCst);
                    Ok(vec![1])
                }
            };
        }
        self.left -= 1;
        self.pulled += 1;
        Ok(vec![self.pulled as u8])
    }
}

/// Runs `camera` with a clock that moves 33 ms per picture; returns what the preview and the form got.
fn drive(
    camera: &mut FakeCamera,
    stop: &AtomicBool,
    read: impl Fn(&[u8]) -> Option<String>,
) -> (Result<(), CameraProblem>, usize, Vec<String>) {
    let ticks = Cell::new(0u64);
    let frames = RefCell::new(0usize);
    let texts = RefCell::new(Vec::new());
    let result = run(
        camera,
        stop,
        &mut || {
            ticks.set(ticks.get() + 33);
            ticks.get()
        },
        &read,
        &mut |_| *frames.borrow_mut() += 1,
        &mut |text| texts.borrow_mut().push(text.to_string()),
    );
    (result, frames.into_inner(), texts.into_inner())
}

#[test]
fn the_preview_gets_a_picture_about_every_100_ms_not_every_frame() {
    let stop = Arc::new(AtomicBool::new(false));
    let mut camera = FakeCamera::new(30, &stop);

    let (result, previews, _) = drive(&mut camera, &stop, |_| None);

    assert_eq!(result, Ok(()));
    // 30 pictures of 33 ms are one second: about ten for the preview.
    assert!((8..=11).contains(&previews), "{previews}");
}

#[test]
fn a_code_is_looked_for_only_every_250_ms() {
    let stop = Arc::new(AtomicBool::new(false));
    let mut camera = FakeCamera::new(30, &stop);
    let looks = Cell::new(0);

    let (result, _, _) = drive(&mut camera, &stop, |_| {
        looks.set(looks.get() + 1);
        None
    });

    assert_eq!(result, Ok(()));

    assert!((3..=5).contains(&looks.get()), "{}", looks.get());
}

#[test]
fn a_code_held_still_is_reported_once_and_again_after_three_seconds() {
    let stop = Arc::new(AtomicBool::new(false));
    // 200 pictures are 6.6 seconds.
    let mut camera = FakeCamera::new(200, &stop);

    let (_, _, texts) = drive(&mut camera, &stop, |_| Some("same".to_string()));

    assert_eq!(texts, ["same", "same", "same"]);
}

#[test]
fn another_text_is_reported_at_once() {
    let stop = Arc::new(AtomicBool::new(false));
    let mut camera = FakeCamera::new(30, &stop);
    let turn = Cell::new(0);

    let (_, _, texts) = drive(&mut camera, &stop, |_| {
        turn.set(turn.get() + 1);
        Some(if turn.get() % 2 == 0 { "b" } else { "a" }.to_string())
    });

    assert!(texts.len() >= 3);
    assert!(texts.windows(2).all(|pair| pair[0] != pair[1]), "{texts:?}");
}

#[test]
fn a_failing_camera_ends_the_loop_with_its_problem() {
    let stop = Arc::new(AtomicBool::new(false));
    let mut camera = FakeCamera::new(5, &stop);
    camera.then = Some(CameraProblem::Busy);

    let (result, _, _) = drive(&mut camera, &stop, |_| None);

    assert_eq!(result, Err(CameraProblem::Busy));
}

#[test]
fn a_stop_ends_the_loop_without_pulling_another_picture() {
    let stop = Arc::new(AtomicBool::new(true));
    let mut camera = FakeCamera::new(10, &stop);

    let (result, previews, _) = drive(&mut camera, &stop, |_| None);

    assert_eq!(result, Ok(()));
    assert_eq!((previews, camera.pulled), (0, 0));
}

#[test]
fn a_torn_frame_is_skipped() {
    struct Torn(usize, Arc<AtomicBool>);
    impl FrameSource for Torn {
        fn next_frame(&mut self) -> Result<Vec<u8>, CameraProblem> {
            self.0 += 1;
            if self.0 > 3 {
                self.1.store(true, Ordering::SeqCst);
            }
            Ok(if self.0 == 1 { Vec::new() } else { vec![7] })
        }
    }
    let stop = Arc::new(AtomicBool::new(false));
    let seen = Cell::new(0);
    let mut camera = Torn(0, Arc::clone(&stop));
    let mut now = 0u64;

    let result = run(
        &mut camera,
        &stop,
        &mut || {
            now += 200;
            now
        },
        &|jpeg| {
            seen.set(seen.get() + 1);
            assert!(!jpeg.is_empty());
            None
        },
        &mut |_| {},
        &mut |_| {},
    );

    assert_eq!(result, Ok(()));
    assert!(seen.get() >= 2);
}
