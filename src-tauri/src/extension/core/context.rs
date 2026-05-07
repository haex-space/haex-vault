//! Application Context Module
//!
//! Manages the application context (theme, locale, platform, device_id)
//! that is shared with extensions. Extensions can query this context
//! and receive updates when it changes.

#[cfg(desktop)]
use crate::extension::error::ExtensionError;
#[cfg(desktop)]
use crate::AppState;
use serde::{Deserialize, Serialize};
#[cfg(desktop)]
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

/// Broadcasts an event to ALL extension webview windows.
/// Only use for events that are explicitly part of the public extension API
/// (e.g., CONTEXT_CHANGED — non-sensitive metadata that every extension
/// observes by design).
///
/// Iframe extensions are notified via postMessage from the frontend, NOT via
/// this command — see `broadcastContext` in stores/extensions/broadcast.ts.
/// For permission-filtered or extension-targeted events, use
/// extension_webview_emit instead.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub fn extension_webview_broadcast(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    event: String,
    payload: serde_json::Value,
) -> Result<(), ExtensionError> {
    eprintln!(
        "[Extension] Broadcasting event '{}' to all extension webview windows",
        event
    );

    // emit_to_all_extensions iterates over registered extension webview labels
    // and uses emit_to(label, …) — never broadcasts to the main window or to
    // unregistered webviews.
    state
        .extension_webview_manager
        .emit_to_all_extensions(&app_handle, &event, payload)?;

    Ok(())
}

/// Emits an event to ALL webview windows of a specific extension.
/// Used for permission-filtered events that should only go to authorized extensions.
/// Returns true if event was sent to at least one webview, false if extension has no webview.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub fn extension_webview_emit(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    extension_id: String,
    event: String,
    payload: serde_json::Value,
) -> Result<bool, ExtensionError> {
    eprintln!(
        "[Extension] Emitting event '{}' to all webviews of extension '{}'",
        event, extension_id
    );

    state
        .extension_webview_manager
        .emit_to_all_extension_windows(&app_handle, &extension_id, &event, payload)
}
