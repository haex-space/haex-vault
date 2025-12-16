//! Window management module
//!
//! Provides commands and utilities for managing application windows.
//! Includes platform-specific handling for Linux/GTK.

use tauri::{AppHandle, Manager, WebviewWindow};

// Linux-specific GTK imports for window.present() workaround
#[cfg(target_os = "linux")]
use gtk::prelude::GtkWindowExt;

/// Focus a window by bringing it to the foreground.
/// Uses GTK present() on Linux for proper window focusing (Tauri's set_focus()
/// doesn't work reliably on modern GNOME/GTK - known issue #5974).
///
/// This is a utility function that can be used by other modules.
pub fn focus_window(window: &WebviewWindow) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        if let Ok(gtk_window) = window.gtk_window() {
            gtk_window.present();
            println!("[window::focus_window] GTK present() called successfully");
        } else {
            println!("[window::focus_window] Failed to get GTK window, falling back to set_focus");
            window.set_focus().map_err(|e| e.to_string())?;
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let is_minimized = window.is_minimized().unwrap_or(false);
        if is_minimized {
            window.unminimize().ok();
        }
        window.set_focus().map_err(|e| e.to_string())?;
        // Bring to front using always_on_top trick
        window.set_always_on_top(true).ok();
        window.set_always_on_top(false).ok();
    }

    Ok(())
}

/// Focus the main window (bring to foreground)
/// Uses GTK present() on Linux for proper window focusing
#[tauri::command]
pub fn focus_main_window(app_handle: AppHandle) -> Result<(), String> {
    if let Some(window) = app_handle.get_webview_window("main") {
        focus_window(&window)
    } else {
        Err("Main window not found".to_string())
    }
}

/// Focus a window by its label/ID (bring to foreground)
/// Used for extension webview windows and other named windows
#[tauri::command]
pub fn focus_window_by_label(app_handle: AppHandle, label: String) -> Result<(), String> {
    if let Some(window) = app_handle.get_webview_window(&label) {
        focus_window(&window)
    } else {
        Err(format!("Window '{}' not found", label))
    }
}
