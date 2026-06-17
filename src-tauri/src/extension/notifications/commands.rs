//! Tauri commands for the generic notifications API.
//!
//! `show` fires an OS notification and records it (pinned to the calling
//! extension) so a later click can deep-link back to the same extension.
//! `dismiss` drops the registry entry for an own notification.

use tauri::{AppHandle, State, WebviewWindow};
use tauri_plugin_notification::NotificationExt;

use crate::extension::error::ExtensionError;
use crate::extension::notifications::types::{NotificationOptions, ShowResult};
use crate::extension::notifications::NotificationRecord;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::NotificationsAction;
use crate::extension::utils::{emit_permission_prompt_if_needed, resolve_extension_id};
use crate::AppState;

/// Show an OS notification on behalf of an extension. Requires the
/// `notifications` permission with action `show`.
#[tauri::command]
pub async fn extension_notifications_show(
    app_handle: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    options: NotificationOptions,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<ShowResult, ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    let check = PermissionManager::check_notifications_permission(
        &state,
        &extension_id,
        NotificationsAction::Show,
    )
    .await;
    if let Err(ref e) = check {
        emit_permission_prompt_if_needed(&app_handle, e);
    }
    check?;

    let id = uuid::Uuid::new_v4().to_string();

    state.notifications.insert(
        id.clone(),
        NotificationRecord {
            extension_id,
            primary: options.primary.clone(),
            actions: options.actions.clone(),
            tag: options.tag.clone(),
        },
    );

    // Build the OS notification. On desktop only title/body/icon are honoured;
    // `extra`/actions are used on mobile. We still attach the id as `extra` so a
    // mobile click can correlate back to the registry.
    let mut builder = app_handle
        .notification()
        .builder()
        .title(options.title.as_str());
    if let Some(body) = options.body.as_deref() {
        builder = builder.body(body);
    }
    if let Some(icon) = options.icon.as_deref() {
        builder = builder.icon(icon);
    }
    builder = builder.extra("notificationId", id.as_str());

    builder
        .show()
        .map_err(|e| ExtensionError::ValidationError {
            reason: format!("failed to show notification: {e}"),
        })?;

    Ok(ShowResult { id })
}

/// Dismiss a previously shown notification. Scope: only the calling extension's
/// own notifications. On desktop this only drops the registry entry (so a later
/// click no longer routes) — the plugin can't recall an already-shown OS
/// notification.
#[tauri::command]
pub async fn extension_notifications_dismiss(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
    public_key: Option<String>,
    name: Option<String>,
) -> Result<(), ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;
    state.notifications.remove_if_owned(&id, &extension_id);
    Ok(())
}
