//! The screen while a code is shown large (spec 092, FR-008): full brightness and the screen kept
//! on, both given back when the view closes. Only Android has a window setting for each; the other
//! platforms do nothing here (the webview's Screen Wake Lock covers the second), and the
//! window shows the code large either way. A failure is logged, never an error: showing the code
//! comes first.

use tauri::{AppHandle, Runtime};
use tauri_plugin_holzi_android::HolziAndroidExt;

/// Hands the brightness to the window or back to the system.
fn bright<R: Runtime>(app: &AppHandle<R>, on: bool) {
    if let Err(error) = app.holzi_android().set_screen_bright(on) {
        log::warn!("screen brightness: {error}");
    }
}

/// Keeps the screen on, or lets the system turn it off again.
fn keep_on<R: Runtime>(app: &AppHandle<R>, on: bool) {
    if let Err(error) = app.holzi_android().set_keep_screen_on(on) {
        log::warn!("keep screen on: {error}");
    }
}

/// Full window brightness (`on`) or the system's value.
#[tauri::command]
pub async fn screen_set_bright(app: AppHandle, on: bool) {
    bright(&app, on);
}

/// Keeps the screen on (`on`) or releases it.
#[tauri::command]
pub async fn screen_set_keep_on(app: AppHandle, on: bool) {
    keep_on(&app, on);
}
