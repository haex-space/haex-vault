//! Extension display-mode + webview-window commands.

use crate::{extension::error::ExtensionError, AppState};
use tauri::{AppHandle, State};

#[tauri::command]
pub fn update_extension_display_mode(
    extension_id: String,
    display_mode: crate::extension::core::manifest::DisplayMode,
    state: State<'_, AppState>,
) -> Result<(), ExtensionError> {
    state
        .extension_manager
        .update_display_mode(&extension_id, display_mode, &state)
}

// ============================================================================
// WebviewWindow Commands (Desktop only)
// ============================================================================

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub fn open_extension_webview_window(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    extension_id: String,
    title: String,
    width: f64,
    height: f64,
    x: Option<f64>,
    y: Option<f64>,
    minimized: Option<bool>,
) -> Result<String, ExtensionError> {
    eprintln!(
        "[open_extension_webview_window] Received extension_id: {}, minimized: {:?}",
        extension_id, minimized
    );
    // Returns the window_id (generated UUID without dashes)
    state.extension_webview_manager.open_extension_window(
        &app_handle,
        &state.extension_manager,
        extension_id,
        title,
        width,
        height,
        x,
        y,
        minimized,
    )
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub fn close_extension_webview_window(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    window_id: String,
) -> Result<(), ExtensionError> {
    state
        .extension_webview_manager
        .close_extension_window(&app_handle, &window_id)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub fn focus_extension_webview_window(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    window_id: String,
) -> Result<(), ExtensionError> {
    state
        .extension_webview_manager
        .focus_extension_window(&app_handle, &window_id)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub fn update_extension_webview_window_position(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    window_id: String,
    x: f64,
    y: f64,
) -> Result<(), ExtensionError> {
    state
        .extension_webview_manager
        .update_extension_window_position(&app_handle, &window_id, x, y)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub fn update_extension_webview_window_size(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    window_id: String,
    width: f64,
    height: f64,
) -> Result<(), ExtensionError> {
    state
        .extension_webview_manager
        .update_extension_window_size(&app_handle, &window_id, width, height)
}

/// Close all extension webview windows.
/// Called when the vault is closed or becomes unavailable (e.g., webview reload).
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub fn close_all_extension_webview_windows(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), ExtensionError> {
    state
        .extension_webview_manager
        .close_all_extension_windows(&app_handle)
}
