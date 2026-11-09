//! The loop that reads the camera: it hands pictures to the preview at a calm rate, looks for a QR
//! code in some of them, and ends when it is told to or the camera fails. Written against a
//! `FrameSource` so a fake camera and a fake clock can drive it in tests.

use std::sync::atomic::{AtomicBool, Ordering};

use super::CameraProblem;

/// How often a picture goes to the preview.
pub const PREVIEW_EVERY_MS: u64 = 100;
/// How often a picture is searched for a QR code: decoding is the expensive part.
pub const DECODE_EVERY_MS: u64 = 250;
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
/// problem). `clock` is milliseconds from any start, `read` finds the QR code in a picture, and
/// `frame` and `text` are told what the preview and the form need.
pub fn run(
    source: &mut dyn FrameSource,
    stop: &AtomicBool,
    clock: &mut dyn FnMut() -> u64,
    read: &dyn Fn(&[u8]) -> Option<String>,
    frame: &mut dyn FnMut(&[u8]),
    text: &mut dyn FnMut(&str),
) -> Result<(), CameraProblem> {
    let mut last_preview = None;
    let mut last_decode = None;
    let mut last_text: Option<(String, u64)> = None;
    while !stop.load(Ordering::SeqCst) {
        let jpeg = source.next_frame()?;
        // A torn frame has no bytes; the next one will do.
        if jpeg.is_empty() {
            continue;
        }
        let now = clock();
        if due(last_preview, now, PREVIEW_EVERY_MS) {
            last_preview = Some(now);
            frame(&jpeg);
        }
        if due(last_decode, now, DECODE_EVERY_MS) {
            last_decode = Some(now);
            if let Some(found) = read(&jpeg) {
                let repeat = last_text.as_ref().is_some_and(|(seen, at)| {
                    *seen == found && now.saturating_sub(*at) < SAME_TEXT_AGAIN_MS
                });
                if !repeat {
                    text(&found);
                    last_text = Some((found, now));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
