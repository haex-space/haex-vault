//! The camera through Video4Linux: the first capture device that gives motion JPEG, read with
//! memory-mapped buffers. Every laptop camera and nearly every USB camera does motion JPEG.

use std::io;

use v4l::buffer::Type;
use v4l::capability::Flags;
use v4l::io::mmap::Stream;
use v4l::io::traits::CaptureStream;
use v4l::video::Capture;
use v4l::{Device, FourCC};

use super::session::FrameSource;
use super::CameraProblem;

/// The picture size asked for: big enough that a QR code on a phone screen at arm's length has
/// modules the decoder can read, small enough to send to the preview. The driver may give another.
const WANT_WIDTH: u32 = 960;
const WANT_HEIGHT: u32 = 540;
/// Buffers the driver fills in turn.
const BUFFERS: u32 = 4;

/// How long one wait for a picture lasts before the stop flag is looked at.
const WAIT_FOR_PICTURE: std::time::Duration = std::time::Duration::from_secs(1);

/// The wait for a picture ran out (v4l reports it as `TimedOut`).
fn is_timeout(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::TimedOut
}

/// The error of the system as the case a person can act on.
fn problem_of(error: &io::Error) -> CameraProblem {
    match error.raw_os_error() {
        // EBUSY: another program has the camera.
        Some(16) => CameraProblem::Busy,
        // EPERM and EACCES: the user may not open the device.
        Some(1 | 13) => CameraProblem::Denied,
        // ENODEV, ENOENT, ENXIO: the camera was unplugged.
        Some(19 | 2 | 6) => CameraProblem::NoCamera,
        _ => CameraProblem::Failed,
    }
}

fn motion_jpeg() -> FourCC {
    FourCC::new(b"MJPG")
}

/// Whether `device` is a capture device that can give motion JPEG.
fn gives_motion_jpeg(device: &Device) -> bool {
    let capture = device
        .query_caps()
        .is_ok_and(|caps| caps.capabilities.contains(Flags::VIDEO_CAPTURE));
    capture
        && device
            .enum_formats()
            .is_ok_and(|formats| formats.iter().any(|format| format.fourcc == motion_jpeg()))
}

/// The first device that can be used, or the reason none can.
pub fn find_camera() -> Result<Device, CameraProblem> {
    let mut denied = false;
    for node in v4l::context::enum_devices() {
        match Device::with_path(node.path()) {
            Ok(device) if gives_motion_jpeg(&device) => return Ok(device),
            Ok(_) => {}
            Err(error) => denied |= problem_of(&error) == CameraProblem::Denied,
        }
    }
    Err(if denied {
        CameraProblem::Denied
    } else {
        CameraProblem::NoCamera
    })
}

/// A started capture of `device`: asks for motion JPEG and starts the stream.
pub fn start(device: &Device) -> Result<CaptureSource<'_>, CameraProblem> {
    let mut format = device.format().map_err(|e| problem_of(&e))?;
    format.width = WANT_WIDTH;
    format.height = WANT_HEIGHT;
    format.fourcc = motion_jpeg();
    let granted = device.set_format(&format).map_err(|e| problem_of(&e))?;
    if granted.fourcc != motion_jpeg() {
        return Err(CameraProblem::Unsupported);
    }
    let mut stream =
        Stream::with_buffers(device, Type::VideoCapture, BUFFERS).map_err(|e| problem_of(&e))?;
    // Without a timeout the wait for a picture never ends when the driver stops delivering, and the
    // stop flag is never looked at: the camera could not be given back.
    stream.set_timeout(WAIT_FOR_PICTURE);
    Ok(CaptureSource { stream })
}

/// The pictures of a running capture.
pub struct CaptureSource<'a> {
    stream: Stream<'a>,
}

impl FrameSource for CaptureSource<'_> {
    fn next_frame(&mut self) -> Result<Vec<u8>, CameraProblem> {
        let (buffer, meta) = match self.stream.next() {
            Ok(picture) => picture,
            // No picture in time: an empty one, so the loop looks at its stop flag and asks again.
            Err(error) if is_timeout(&error) => return Ok(Vec::new()),
            Err(error) => return Err(problem_of(&error)),
        };
        let used = (meta.bytesused as usize).min(buffer.len());
        Ok(buffer[..used].to_vec())
    }
}

#[cfg(test)]
#[path = "capture_tests.rs"]
mod tests;
