//! The camera gate (spec 085, research R5): the webview may open the camera only while the link
//! sheet's scanner is open.
//!
//! The permission handler of the app answers every request of the window, and extension frames run
//! in the same window (spec 017). It therefore allows the camera for exactly the time between
//! `camera_scan_begin` and `camera_scan_end`, and leaves every other request, and the camera
//! outside that time, to the system's own behavior. The process ends with the window (spec 013), so
//! a gate left open never outlives it.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::webview::{PermissionKind, PermissionResponse};
use tauri::{State, Window};

use crate::error::{HolziError, Result};

/// Whether a scan is open. Managed as state; the permission handler reads it.
#[derive(Debug, Default)]
pub struct CameraGate(AtomicBool);

impl CameraGate {
    /// A scan opens: the next camera request is allowed.
    pub fn begin(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// The scan ended; idempotent.
    pub fn end(&self) {
        self.0.store(false, Ordering::SeqCst);
    }

    /// The answer to a request of the webview: `Allow` for the camera while a scan is open,
    /// `Deny` for the camera outside a scan, and `Default` for everything else.
    pub fn answer(&self, kind: PermissionKind) -> PermissionResponse {
        if kind == PermissionKind::Camera {
            if self.0.load(Ordering::SeqCst) {
                PermissionResponse::Allow
            } else {
                PermissionResponse::Deny
            }
        } else {
            PermissionResponse::Default
        }
    }
}

/// Only the app's own window opens the gate.
fn require_main(window: &Window) -> Result<()> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err(HolziError::InvalidInput {
            reason: "the camera is only for the main window".to_string(),
        })
    }
}

/// The link sheet's scanner opens: the camera may be used until `camera_scan_end`.
#[tauri::command]
pub fn camera_scan_begin(window: Window, gate: State<'_, CameraGate>) -> Result<()> {
    require_main(&window)?;
    gate.begin();
    Ok(())
}

/// The scanner closed, or never started; safe to call at any time.
#[tauri::command]
pub fn camera_scan_end(window: Window, gate: State<'_, CameraGate>) -> Result<()> {
    require_main(&window)?;
    gate.end();
    Ok(())
}

/// WebKitGTK offers `navigator.mediaDevices` only when its `enable-media-stream` setting is on, and
/// wry does not set it. Without it the scanner finds no camera on Linux.
#[cfg(target_os = "linux")]
pub fn enable_media_stream(app: &tauri::AppHandle) {
    use tauri::Manager;
    use webkit2gtk::{SettingsExt, WebViewExt};

    let Some(window) = app.get_webview_window("main") else {
        log::warn!("camera: no main window to enable the media stream on");
        return;
    };
    let result = window.with_webview(|webview| {
        if let Some(settings) = webview.inner().settings() {
            settings.set_enable_media_stream(true);
        }
    });
    if let Err(error) = result {
        log::warn!("camera: the media stream setting could not be applied: {error}");
    }
}

#[cfg(test)]
#[path = "camera_gate_tests.rs"]
mod tests;
