//! Application Context Module
//!
//! Manages the application context (theme, locale, platform, device_id)
//! that is shared with extensions. Extensions can query this context
//! and receive updates when it changes.

use crate::extension::error::ExtensionError;
use crate::AppState;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

// ============================================================================
// Types
// ============================================================================

/// Application context shared with extensions.
/// Contains theme, locale, platform, and device information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationContext {
    pub theme: String,
    pub locale: String,
    #[serde(default)]
    pub platform: String,
    pub device_id: String,
}

impl Default for ApplicationContext {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            locale: "en".to_string(),
            platform: String::new(),
            device_id: String::new(),
        }
    }
}

// ============================================================================
// Tauri Commands
// ============================================================================

/// Get application context (theme, locale, platform, device_id).
/// Used by extensions to get current application state.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub fn extension_context_get(
    state: State<'_, AppState>,
) -> Result<ApplicationContext, ExtensionError> {
    eprintln!("[Extension] extension_context_get called");
    let context = state
        .context
        .lock()
        .map_err(|e| ExtensionError::MutexPoisoned {
            reason: e.to_string(),
        })?;
    eprintln!(
        "[Extension] Returning context: theme={}, locale={}, platform={}, device_id={}",
        context.theme, context.locale, context.platform, context.device_id
    );
    Ok(context.clone())
}

/// Stores the current application context in state for extension access.
/// This is called when the theme/locale changes so webview extensions can query it.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub fn extension_context_set(
    state: State<'_, AppState>,
    context: ApplicationContext,
) -> Result<(), ExtensionError> {
    eprintln!(
        "[Extension] extension_context_set called: theme={}, locale={}, platform={}, device_id={}",
        context.theme, context.locale, context.platform, context.device_id
    );
    let mut ctx = state
        .context
        .lock()
        .map_err(|e| ExtensionError::MutexPoisoned {
            reason: e.to_string(),
        })?;
    *ctx = context;
    eprintln!("[Extension] Context updated in state");
    Ok(())
}

/// Emits an event to all extension webview windows.
/// Used to broadcast context changes, sync events, etc. to native webview extensions.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub fn extension_emit_to_all(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    event: String,
    payload: serde_json::Value,
) -> Result<(), ExtensionError> {
    use tauri::Emitter;

    eprintln!(
        "[Extension] Emitting event '{}' to all webview windows",
        event
    );

    // First, emit to all registered extension windows (native WebView windows)
    state
        .extension_webview_manager
        .emit_to_all_extensions(&app_handle, &event, payload.clone())?;

    // Also emit globally to catch any extension windows not in the manager
    // (e.g., embedded webviews within the main window)
    if let Err(e) = app_handle.emit(&event, payload) {
        eprintln!(
            "[Extension] Failed to emit event '{}' globally: {}",
            event, e
        );
    } else {
        eprintln!("[Extension] Event '{}' emitted globally", event);
    }

    Ok(())
}
