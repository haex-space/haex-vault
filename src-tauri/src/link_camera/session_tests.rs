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

/// Runs `camera` with a clock that moves 33 ms per picture; returns what the preview and the
/// search were given.
fn drive(camera: &mut FakeCamera, stop: &AtomicBool) -> (Result<(), CameraProblem>, usize, usize) {
    let ticks = Cell::new(0u64);
    let frames = RefCell::new(0usize);
    let looks = RefCell::new(0usize);
    let result = run(
        camera,
        stop,
        &mut || {
            ticks.set(ticks.get() + 33);
            ticks.get()
        },
        &mut |_| *frames.borrow_mut() += 1,
        &mut |_| *looks.borrow_mut() += 1,
    );
    (result, frames.into_inner(), looks.into_inner())
}

#[test]
fn the_preview_gets_every_picture() {
    let stop = Arc::new(AtomicBool::new(false));
    let mut camera = FakeCamera::new(30, &stop);

    let (result, frames, _) = drive(&mut camera, &stop);

    assert_eq!(result, Ok(()));
    // The 30 pictures and the one the fake hands out when it stops.
    assert_eq!(frames, 31);
}

#[test]
fn a_picture_is_offered_to_the_search_only_every_250_ms() {
    let stop = Arc::new(AtomicBool::new(false));
    let mut camera = FakeCamera::new(30, &stop);

    let (_, _, looks) = drive(&mut camera, &stop);

    // One second of 33 ms pictures.
    assert!((3..=5).contains(&looks), "{looks}");
}

#[test]
fn a_failing_camera_ends_the_loop_with_its_problem() {
    let stop = Arc::new(AtomicBool::new(false));
    let mut camera = FakeCamera::new(5, &stop);
    camera.then = Some(CameraProblem::Busy);

    let (result, _, _) = drive(&mut camera, &stop);

    assert_eq!(result, Err(CameraProblem::Busy));
}

#[test]
fn a_stop_ends_the_loop_without_pulling_another_picture() {
    let stop = Arc::new(AtomicBool::new(true));
    let mut camera = FakeCamera::new(10, &stop);

    let (result, frames, looks) = drive(&mut camera, &stop);

    assert_eq!(result, Ok(()));
    assert_eq!((frames, looks, camera.pulled), (0, 0, 0));
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
            now += 300;
            now
        },
        &mut |jpeg| {
            seen.set(seen.get() + 1);
            assert!(!jpeg.is_empty());
        },
        &mut |jpeg| assert!(!jpeg.is_empty()),
    );

    assert_eq!(result, Ok(()));
    assert!(seen.get() >= 2);
}

#[test]
fn a_code_held_still_is_told_once_and_again_after_three_seconds() {
    let mut reported = Reported::default();

    assert!(reported.should_tell("same", 0));
    assert!(!reported.should_tell("same", 250));
    assert!(!reported.should_tell("same", 2_999));
    assert!(reported.should_tell("same", 3_000));
    assert!(!reported.should_tell("same", 3_250));
}

#[test]
fn another_text_is_told_at_once() {
    let mut reported = Reported::default();

    assert!(reported.should_tell("a", 0));
    assert!(reported.should_tell("b", 250));
    assert!(reported.should_tell("a", 500));
}
