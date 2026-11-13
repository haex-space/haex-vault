//! The camera of the link sheet where the webview has none that works (spec 085, research R4).
//!
//! WebKitGTK on Linux plays a camera stream through a GStreamer pipeline that stops after the
//! first picture, so the preview of the webview freezes (measured outside the app in a plain
//! WebKitGTK window: one distinct picture in six seconds, while GStreamer pulls about 26 pictures
//! per second from the same device). On Linux the camera is therefore read here, in a thread of
//! its own: the window gets every picture as the raw bytes of a JPEG through a channel, for the
//! preview, and the text of a QR code in one as an event. Nothing of the picture is kept.

pub mod qr;
pub mod session;

#[cfg(target_os = "linux")]
mod capture;

use serde::Serialize;

/// The event with the text of a QR code a picture holds.
pub const TEXT_EVENT: &str = "link-camera-text";
/// The event with the codes (any format) a picture holds, for the camera of a pass (spec 092): sent
/// to the main window only, because a code is a secret.
pub const CODES_EVENT: &str = "link-camera-codes";
/// The event with the reason the camera stopped by itself.
pub const ERROR_EVENT: &str = "link-camera-error";

/// Why the camera cannot be used, as far as the person can act on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(rename_all = "camelCase")]
pub enum CameraProblem {
    #[error("no camera found")]
    NoCamera,
    #[error("the camera is used by another program")]
    Busy,
    #[error("the camera may not be used")]
    Denied,
    #[error("the camera gives no picture format this reads")]
    Unsupported,
    #[error("the camera failed")]
    Failed,
}

impl CameraProblem {
    /// The word the window knows the problem by.
    pub fn key(self) -> &'static str {
        match self {
            CameraProblem::NoCamera => "noCamera",
            CameraProblem::Busy => "busy",
            CameraProblem::Denied => "denied",
            CameraProblem::Unsupported => "unsupported",
            CameraProblem::Failed => "failed",
        }
    }
}

pub mod commands;
pub use commands::LinkCamera;
