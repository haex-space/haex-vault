//! The loop that reads the camera: it hands every picture to the preview, offers some of them to
//! the search for a QR code, and ends when it is told to or the camera fails. Written against a
//! `FrameSource` so a fake camera and a fake clock can drive it in tests. The search itself is
//! slow (a JPEG is decoded and scanned), so it never runs here: a stutter in the preview every
//! time it ran was what this was split for.

use std::sync::atomic::{AtomicBool, Ordering};

use super::CameraProblem;

/// How often a picture is offered to the search for a QR code.
pub const LOOK_EVERY_MS: u64 = 250;
/// The same text is reported again only after this long, so a code held still is not a flood.
pub const SAME_TEXT_AGAIN_MS: u64 = 3_000;

/// Where the pictures come from: one JPEG per call, blocking until the next one is there.
pub trait FrameSource {
    fn next_frame(&mut self) -> Result<Vec<u8>, CameraProblem>;
}

fn due(last: Option<u64>, now: u64, every: u64) -> bool {
    last.is_none_or(|at| now.saturating_sub(at) >= every)
}

/// Reads pictures from `source` until `stop` is set (then `Ok`) or the camera fails (then the
/// problem). `clock` is milliseconds from any start. `frame` gets every picture and `look` gets one
/// every [`LOOK_EVERY_MS`]; both must return quickly.
pub fn run(
    source: &mut dyn FrameSource,
    stop: &AtomicBool,
    clock: &mut dyn FnMut() -> u64,
    frame: &mut dyn FnMut(&[u8]),
    look: &mut dyn FnMut(&[u8]),
) -> Result<(), CameraProblem> {
    let mut last_look = None;
    while !stop.load(Ordering::SeqCst) {
        let jpeg = source.next_frame()?;
        // A torn frame has no bytes; the next one will do.
        if jpeg.is_empty() {
            continue;
        }
        frame(&jpeg);
        let now = clock();
        if due(last_look, now, LOOK_EVERY_MS) {
            last_look = Some(now);
            look(&jpeg);
        }
    }
    Ok(())
}

/// Which texts of the search are told on: a code held still is found again and again, so the same
/// text is told again only after a while.
#[derive(Debug, Default)]
pub struct Reported(Option<(String, u64)>);

impl Reported {
    /// Whether `text`, found at `now` (milliseconds), is to be told.
    pub fn should_tell(&mut self, text: &str, now: u64) -> bool {
        let repeat = self
            .0
            .as_ref()
            .is_some_and(|(seen, at)| seen == text && now.saturating_sub(*at) < SAME_TEXT_AGAIN_MS);
        if !repeat {
            self.0 = Some((text.to_string(), now));
        }
        !repeat
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
