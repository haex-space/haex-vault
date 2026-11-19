//! The space the system takes at the window's edges on a phone (spec 043 FR-011, FR-012): status
//! and navigation bar, display cutout, on-screen keyboard. The plugin crate reports it, this module
//! keeps the last value and tells the window, which lays itself out with it. Kept in the core
//! rather than written into the page by the plugin, so a reloaded page asks again and loses nothing.

use std::sync::{Mutex, PoisonError};

use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tauri_plugin_haex_android::{HaexAndroidExt, Insets};

/// The event that carries new insets to the window.
pub const EVENT: &str = "device-insets";

/// The last insets the system reported; none before the first report and on a desktop.
#[derive(Default)]
pub struct DeviceInsets(Mutex<Option<Insets>>);

/// Starts listening to the plugin; on a desktop nothing ever arrives.
pub fn watch<R: Runtime>(app: &AppHandle<R>) {
    app.manage(DeviceInsets::default());
    let handle = app.clone();
    let watched = app.haex_android().watch_insets(move |insets| {
        if let Some(state) = handle.try_state::<DeviceInsets>() {
            *state.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(insets);
        }
        if let Err(error) = handle.emit(EVENT, insets) {
            log::warn!("insets: {error}");
        }
    });
    if let Err(error) = watched {
        log::warn!("insets: not watched: {error}");
    }
}

/// The insets now, for a page that has just loaded.
#[tauri::command]
pub fn device_insets(state: State<'_, DeviceInsets>) -> Option<Insets> {
    *state.0.lock().unwrap_or_else(PoisonError::into_inner)
}
