//! Extension Shell Commands
//!
//! Permission-checked commands for PTY/shell access.
//! Extensions must have `shell` permission with `execute` action.

use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::utils::{emit_permission_prompt_if_needed, resolve_extension_id};
use crate::AppState;
use tauri::{AppHandle, State, WebviewWindow};

use super::types::{ShellCreateOptions, ShellCreateResponse};

/// Check shell permissions for an extension
async fn check_shell_execute_permission(
    app_handle: &AppHandle,
    state: &State<'_, AppState>,
    extension_id: &str,
) -> Result<(), ExtensionError> {
    let permission_result =
        PermissionManager::check_shell_permission(state, extension_id, "*", &[]).await;

    if let Err(ref e) = permission_result {
        emit_permission_prompt_if_needed(app_handle, e);
    }
    permission_result
}

/// Create a new PTY shell session
#[tauri::command(rename_all = "camelCase")]
pub async fn extension_shell_create(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    options: ShellCreateOptions,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<ShellCreateResponse, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    check_shell_execute_permission(&app_handle, &state, &extension_id).await?;

    let (session_id, shell_name) = state
        .pty_manager
        .create_session(&app_handle, &extension_id, options)
        .await
        .map_err(|reason| ExtensionError::Shell {
            reason,
            exit_code: None,
        })?;

    Ok(ShellCreateResponse { session_id, shell_name })
}

/// Write data to a shell session's stdin
#[tauri::command(rename_all = "camelCase")]
pub async fn extension_shell_write(
    _app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    session_id: String,
    data: String,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<(), ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    if !state
        .pty_manager
        .session_belongs_to(&session_id, &extension_id)
        .await
    {
        return Err(ExtensionError::PermissionDenied {
            extension_id: extension_id.clone(),
            operation: "shell:write".to_string(),
            resource: session_id,
        });
    }

    state
        .pty_manager
        .write_to_session(&session_id, &data)
        .await
        .map_err(|reason| ExtensionError::Shell {
            reason,
            exit_code: None,
        })
}

/// Resize a shell session's terminal
#[tauri::command(rename_all = "camelCase")]
pub async fn extension_shell_resize(
    _app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    session_id: String,
    cols: u16,
    rows: u16,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<(), ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    if !state
        .pty_manager
        .session_belongs_to(&session_id, &extension_id)
        .await
    {
        return Err(ExtensionError::PermissionDenied {
            extension_id: extension_id.clone(),
            operation: "shell:resize".to_string(),
            resource: session_id,
        });
    }

    state
        .pty_manager
        .resize_session(&session_id, cols, rows)
        .await
        .map_err(|reason| ExtensionError::Shell {
            reason,
            exit_code: None,
        })
}

/// Close a shell session
#[tauri::command(rename_all = "camelCase")]
pub async fn extension_shell_close(
    _app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    session_id: String,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<(), ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    if !state
        .pty_manager
        .session_belongs_to(&session_id, &extension_id)
        .await
    {
        return Err(ExtensionError::PermissionDenied {
            extension_id: extension_id.clone(),
            operation: "shell:close".to_string(),
            resource: session_id,
        });
    }

    state
        .pty_manager
        .close_session(&session_id)
        .await
        .map_err(|reason| ExtensionError::Shell {
            reason,
            exit_code: None,
        })
}
