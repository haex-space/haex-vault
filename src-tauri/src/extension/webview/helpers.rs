use crate::extension::core::protocol::ExtensionInfo;
use crate::extension::error::ExtensionError;
use crate::AppState;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State, WebviewWindow};

/// Event name for permission prompt required
pub const EVENT_PERMISSION_PROMPT_REQUIRED: &str = "extension:permission-prompt-required";

/// Payload for permission prompt event
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionPromptPayload {
    pub extension_id: String,
    pub extension_name: String,
    pub resource_type: String,
    pub action: String,
    pub target: String,
}

/// Emits a permission prompt event if the error is PermissionPromptRequired
pub fn emit_permission_prompt_if_needed(app_handle: &AppHandle, error: &ExtensionError) {
    if let ExtensionError::PermissionPromptRequired {
        extension_id,
        extension_name,
        resource_type,
        action,
        target,
    } = error
    {
        let payload = PermissionPromptPayload {
            extension_id: extension_id.clone(),
            extension_name: extension_name.clone(),
            resource_type: resource_type.clone(),
            action: action.clone(),
            target: target.clone(),
        };
        let _ = app_handle.emit(EVENT_PERMISSION_PROMPT_REQUIRED, &payload);
    }
}

/// Wrapper that emits permission prompt event on error
pub async fn with_permission_prompt<T, F, Fut>(
    app_handle: &AppHandle,
    f: F,
) -> Result<T, ExtensionError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T, ExtensionError>>,
{
    let result = f().await;
    if let Err(ref e) = result {
        emit_permission_prompt_if_needed(app_handle, e);
    }
    result
}

/// Get extension_id from window (SECURITY: window_id from Tauri, cannot be spoofed)
pub fn get_extension_id(
    window: &WebviewWindow,
    state: &State<AppState>,
) -> Result<String, ExtensionError> {
    let window_id = window.label();
    eprintln!(
        "[webview_api] Looking up extension_id for window: {}",
        window_id
    );

    let windows = state
        .extension_webview_manager
        .windows
        .lock()
        .map_err(|e| ExtensionError::MutexPoisoned {
            reason: e.to_string(),
        })?;

    eprintln!("[webview_api] HashMap contents: {:?}", *windows);

    let extension_id =
        windows
            .get(window_id)
            .cloned()
            .ok_or_else(|| ExtensionError::ValidationError {
                reason: format!(
                    "Window {} is not registered as an extension window",
                    window_id
                ),
            })?;

    eprintln!("[webview_api] Found extension_id: {}", extension_id);
    Ok(extension_id)
}

/// Get full extension info (public_key, name, version) from window
pub fn get_extension_info_from_window(
    window: &WebviewWindow,
    state: &State<AppState>,
) -> Result<ExtensionInfo, ExtensionError> {
    let extension_id = get_extension_id(window, state)?;

    // Get extension from ExtensionManager using the database UUID
    let extension = state
        .extension_manager
        .get_extension(&extension_id)
        .ok_or_else(|| ExtensionError::ValidationError {
            reason: format!("Extension with ID {} not found", extension_id),
        })?;

    let version = match &extension.source {
        crate::extension::core::types::ExtensionSource::Production { version, .. } => {
            version.clone()
        }
        crate::extension::core::types::ExtensionSource::Development { .. } => "dev".to_string(),
    };

    Ok(ExtensionInfo {
        public_key: extension.manifest.public_key,
        name: extension.manifest.name,
        version,
    })
}
