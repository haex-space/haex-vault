// src-tauri/src/extension/permissions/commands.rs
//!
//! Tauri commands for extension permission operations
//!
//! These commands work for both WebView and iframe extensions:
//! - WebView: extension_id is resolved from the window context
//! - iframe: extension_id is resolved from public_key/name parameters
//!           (verified by frontend via origin check)

use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::{
    Action, DbAction, ExtensionApiAction, ExtensionPermission, FsAction, PasswordsAction,
    PermissionStatus, Principal, ResourceType, RwAction, WebAction,
};
use crate::extension::utils::{
    require_main_window, resolve_extension_id, PermissionResolvedPayload, EVENT_PERMISSION_RESOLVED,
};
use crate::AppState;
use std::path::Path;
use tauri::{AppHandle, State, WebviewWindow};
// Mobile builds have no `extension_webview_manager`; the resolution event is
// emitted to the main window directly, which needs the Emitter trait in scope.
#[cfg(any(target_os = "android", target_os = "ios"))]
use tauri::Emitter;

/// Parses a read/readWrite action string for the shared sync resources
/// (`syncServers`, `cloudStorage`, `syncRules`). Defaults to `Read` for
/// unknown values, mirroring the lenient parsing used for other resources.
///
/// Note: this lenient default is deliberate and matches the sibling Db/Spaces
/// command arms — do NOT "fix" it by swapping in `RwAction::from_str`, which
/// errors on unknown input instead.
fn parse_rw_action(action: &str) -> RwAction {
    match action.to_lowercase().as_str() {
        "readwrite" | "read_write" => RwAction::ReadWrite,
        _ => RwAction::Read,
    }
}

// =============================================================================
// Permission Check Commands (unified for WebView and iframe)
// =============================================================================

/// Check web/fetch permission
#[tauri::command]
pub async fn extension_permissions_check_web(
    window: WebviewWindow,
    state: State<'_, AppState>,
    url: String,
    // Optional parameters for iframe mode (verified by frontend via origin)
    public_key: Option<String>,
    name: Option<String>,
) -> Result<(), ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;
    PermissionManager::check_web_permission(&state, &Principal::Extension(extension_id), &url).await
}

/// Check database permission
#[tauri::command]
pub async fn extension_permissions_check_database(
    window: WebviewWindow,
    state: State<'_, AppState>,
    resource: String,
    operation: String,
    // Optional parameters for iframe mode (verified by frontend via origin)
    public_key: Option<String>,
    name: Option<String>,
) -> Result<(), ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    let action = match operation.as_str() {
        "read" => Action::Database(DbAction::Read),
        "write" => Action::Database(DbAction::ReadWrite),
        _ => {
            return Err(ExtensionError::ValidationError {
                reason: format!("Invalid database operation: {}", operation),
            })
        }
    };

    PermissionManager::check_database_permission(
        &state,
        &Principal::Extension(extension_id),
        action,
        &resource,
    )
    .await
}

/// Check filesystem permission
#[tauri::command]
pub async fn extension_permissions_check_filesystem(
    window: WebviewWindow,
    state: State<'_, AppState>,
    path: String,
    operation: String,
    // Optional parameters for iframe mode (verified by frontend via origin)
    public_key: Option<String>,
    name: Option<String>,
) -> Result<(), ExtensionError> {
    let extension_id = resolve_extension_id(&window, &state, public_key, name)?;

    let action = match operation.as_str() {
        "read" => Action::Filesystem(FsAction::Read),
        "write" => Action::Filesystem(FsAction::ReadWrite),
        _ => {
            return Err(ExtensionError::ValidationError {
                reason: format!("Invalid filesystem operation: {}", operation),
            })
        }
    };

    let file_path = Path::new(&path);
    PermissionManager::check_filesystem_permission(
        &state,
        &Principal::Extension(extension_id),
        action,
        file_path,
    )
    .await
}

// =============================================================================
// Legacy Commands (for internal use by frontend)
// =============================================================================

/// Notify the owning extension's webview that a permission prompt was resolved.
///
/// Called by the frontend permission-prompt flow after every decision
/// (granted / denied / one-time allow / cancel). The extension's SDK listens
/// for this event to auto-retry the original request (on grant) or fail it
/// cleanly (on deny). `target` MUST be the original prompt target so it
/// matches the PermissionPromptRequired error the SDK is waiting on.
#[tauri::command]
pub async fn notify_extension_permission_decision(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    extension_id: String,
    resource_type: String,
    action: String,
    target: String,
    decision: String,
) -> Result<(), ExtensionError> {
    // The SDK only resolves its pending request on "granted" | "denied"; reject
    // anything else at the boundary so a malformed decision can't silently
    // drift the protocol.
    if decision != "granted" && decision != "denied" {
        return Err(ExtensionError::ValidationError {
            reason: format!("Invalid decision: {decision}. Expected 'granted' or 'denied'"),
        });
    }

    // Wake any external-bridge waiter blocked on this exact (principal,
    // resource_type, action, target) key — grant OR deny, so a denial
    // resolves the blocked request immediately instead of idling until its
    // timeout. `target` is the original prompt target (see doc above), so it
    // matches the key the bridge registered from the PermissionPromptRequired
    // error. No-op if nothing is waiting (the common case: native/iframe
    // extension calls never register a waiter, only the external bridge does).
    state
        .permission_prompt_waiters
        .wake(&(
            extension_id.clone(),
            resource_type.clone(),
            action.clone(),
            target.clone(),
        ))
        .await;

    let payload = PermissionResolvedPayload {
        extension_id: extension_id.clone(),
        resource_type,
        action,
        target,
        decision,
    };

    // Deliver the resolution event so the extension SDK can auto-retry (grant)
    // or fail cleanly (deny). Propagate delivery failures instead of swallowing
    // them — a dropped event leaves the SDK waiting until its own timeout.
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    state
        .extension_webview_manager
        .emit_to_extension_or_main(
            &app_handle,
            &extension_id,
            EVENT_PERMISSION_RESOLVED,
            payload,
        )
        .map_err(|e| ExtensionError::WebError {
            reason: format!("Failed to notify extension of permission decision: {e}"),
        })?;

    // Mobile builds have no extension_webview_manager — emit to the main window.
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = &state;
        app_handle
            .emit_to("main", EVENT_PERMISSION_RESOLVED, payload)
            .map_err(|e| ExtensionError::WebError {
                reason: format!("Failed to notify extension of permission decision: {e}"),
            })?;
    }

    Ok(())
}

/// Grants or denies a permission for the current session only (not persisted to database)
///
/// Called by the frontend when user makes a decision without checking "remember".
/// These permissions are cleared when the application restarts.
#[tauri::command]
pub fn grant_session_permission(
    window: WebviewWindow,
    extension_id: String,
    resource_type: String,
    action: String,
    target: String,
    decision: String,
    state: State<'_, AppState>,
    tags: Option<Vec<String>>,
    default_tag: Option<String>,
) -> Result<(), ExtensionError> {
    // Owner-only: only the main window (which renders the consent dialog) may
    // grant permissions. An extension must never grant itself rights.
    require_main_window(&window)?;

    let resource_type_enum = ResourceType::from_str(&resource_type)?;
    let status = PermissionStatus::from_str(&decision)?;
    let action_enum = match resource_type_enum {
        // Web permissions are domain-scoped and method-agnostic (see
        // check_web_permission); the prompt's action string ("request") is not
        // a WebAction, so every web grant maps to WebAction::All — matching
        // both the check's lookup key and resolve_permission_prompt's action.
        ResourceType::Web => Action::Web(WebAction::All),
        _ => Action::from_str(&resource_type_enum, &action)?,
    };

    // Passwords grants can cover multiple tags plus a default-label choice —
    // handled separately from the generic single-target session grant below.
    if let Action::Passwords(passwords_action) = action_enum {
        let requested_tags = tags.unwrap_or_default();
        state.session_permissions.set_passwords_grant(
            &extension_id,
            passwords_action,
            &requested_tags,
            default_tag.as_deref(),
            status,
        )?;

        eprintln!(
            "[SessionPermission] Set passwords permission for extension {}: tags={:?} status={:?}",
            extension_id, requested_tags, status
        );

        return Ok(());
    }

    let permission = ExtensionPermission {
        id: format!("session-{}", uuid::Uuid::new_v4()),
        principal_id: extension_id.clone(),
        resource_type: resource_type_enum,
        action: action_enum,
        target: target.clone(),
        constraints: None,
        status,
        // Passwords default-labels (`{"default":true}`) originate only from the
        // manifest, never from interactive prompts — `None` is correct here.
        raw_constraints: None,
    };

    state.session_permissions.set_permission(permission);

    eprintln!(
        "[SessionPermission] Set {} permission for extension {} on {}: {:?}",
        resource_type, extension_id, target, status
    );

    Ok(())
}

/// Resolves a permission prompt by updating or creating a permission entry in the database
///
/// Called by the frontend after the user makes a decision in the permission dialog
/// with "remember" checked.
#[tauri::command]
pub async fn resolve_permission_prompt(
    window: WebviewWindow,
    extension_id: String,
    resource_type: String,
    action: String,
    target: String,
    decision: String,
    state: State<'_, AppState>,
    tags: Option<Vec<String>>,
    default_tag: Option<String>,
) -> Result<(), ExtensionError> {
    // Owner-only: only the main window (which renders the consent dialog) may
    // persist a permission decision. An extension must never grant itself rights.
    require_main_window(&window)?;

    // For "ask" (one-time allow), we don't store anything - just return Ok
    if decision == "ask" {
        return Ok(());
    }

    // Parse the decision into a PermissionStatus
    let status = match decision.as_str() {
        "granted" => PermissionStatus::Granted,
        "denied" => PermissionStatus::Denied,
        _ => {
            return Err(ExtensionError::ValidationError {
                reason: format!(
                    "Invalid decision: {}. Expected 'granted', 'denied', or 'ask'",
                    decision
                ),
            })
        }
    };

    // Parse resource type
    let resource_type_enum = match resource_type.as_str() {
        "db" => ResourceType::Db,
        "web" => ResourceType::Web,
        "fs" => ResourceType::Fs,
        "shell" => ResourceType::Shell,
        "syncServers" => ResourceType::SyncServers,
        "cloudStorage" => ResourceType::CloudStorage,
        "syncRules" => ResourceType::SyncRules,
        "spaces" => ResourceType::Spaces,
        "identities" => ResourceType::Identities,
        "passwords" => ResourceType::Passwords,
        "bookmarks" => ResourceType::Bookmarks,
        "mail" => ResourceType::Mail,
        "notifications" => ResourceType::Notifications,
        "extensionApi" => ResourceType::ExtensionApi,
        _ => {
            return Err(ExtensionError::ValidationError {
                reason: format!("Invalid resource type: {}", resource_type),
            })
        }
    };

    // Parse action based on resource type
    let action_enum = match resource_type_enum {
        ResourceType::Db => {
            let db_action = match action.to_lowercase().as_str() {
                "read" => DbAction::Read,
                "readwrite" | "read_write" => DbAction::ReadWrite,
                "create" => DbAction::Create,
                "delete" => DbAction::Delete,
                "alterdrop" | "alter_drop" => DbAction::AlterDrop,
                _ => DbAction::Read, // Default to read for unknown
            };
            Action::Database(db_action)
        }
        ResourceType::Web => Action::Web(WebAction::All),
        ResourceType::Fs => {
            let fs_action = match action.to_lowercase().as_str() {
                "read" => FsAction::Read,
                "readwrite" | "read_write" => FsAction::ReadWrite,
                _ => FsAction::Read,
            };
            Action::Filesystem(fs_action)
        }
        ResourceType::Shell => {
            Action::Shell(crate::extension::permissions::types::ShellAction::Execute)
        }
        ResourceType::SyncServers => Action::SyncServers(parse_rw_action(&action)),
        ResourceType::CloudStorage => Action::CloudStorage(parse_rw_action(&action)),
        ResourceType::SyncRules => Action::SyncRules(parse_rw_action(&action)),
        ResourceType::Spaces => {
            let space_action = match action.to_lowercase().as_str() {
                "read" => crate::extension::permissions::types::SpaceAction::Read,
                "readwrite" | "read_write" => {
                    crate::extension::permissions::types::SpaceAction::ReadWrite
                }
                _ => crate::extension::permissions::types::SpaceAction::Read,
            };
            Action::Spaces(space_action)
        }
        ResourceType::Identities => {
            let identity_action = match action.to_lowercase().as_str() {
                "read" => crate::extension::permissions::types::IdentityAction::Read,
                "write" => crate::extension::permissions::types::IdentityAction::Write,
                _ => {
                    return Err(ExtensionError::ValidationError {
                        reason: format!(
                            "Invalid identities action: {action} (expected 'read' or 'write')"
                        ),
                    })
                }
            };
            Action::Identities(identity_action)
        }
        ResourceType::Passwords => {
            let passwords_action = match action.to_lowercase().as_str() {
                "read" => PasswordsAction::Read,
                "readwrite" | "read_write" => PasswordsAction::ReadWrite,
                _ => {
                    return Err(ExtensionError::ValidationError {
                        reason: format!("Invalid passwords action: {action}"),
                    })
                }
            };
            // Passwords grants can cover multiple tags plus a default-label
            // choice — handled entirely separately (below) from the generic
            // single-target upsert, so return here directly.
            let requested_tags = tags.unwrap_or_default();
            PermissionManager::save_passwords_grant(
                &state,
                &extension_id,
                passwords_action,
                &requested_tags,
                default_tag.as_deref(),
                status,
            )
            .await?;
            return Ok(());
        }
        ResourceType::Bookmarks => {
            let bookmarks_action = match action.to_lowercase().as_str() {
                "read" => RwAction::Read,
                "readwrite" | "read_write" => RwAction::ReadWrite,
                _ => {
                    return Err(ExtensionError::ValidationError {
                        reason: format!("Invalid bookmarks action: {action}"),
                    })
                }
            };
            Action::Bookmarks(bookmarks_action)
        }
        ResourceType::Mail => {
            let mail_action = match action.to_lowercase().as_str() {
                "fetch" => crate::extension::permissions::types::MailAction::Fetch,
                "send" => crate::extension::permissions::types::MailAction::Send,
                _ => {
                    return Err(ExtensionError::ValidationError {
                        reason: format!(
                            "Invalid mail action: {action} (expected 'fetch' or 'send')"
                        ),
                    })
                }
            };
            Action::Mail(mail_action)
        }
        ResourceType::Notifications => {
            let notifications_action = match action.to_lowercase().as_str() {
                "show" => crate::extension::permissions::types::NotificationsAction::Show,
                _ => {
                    return Err(ExtensionError::ValidationError {
                        reason: format!("Invalid notifications action: {action} (expected 'show')"),
                    })
                }
            };
            Action::Notifications(notifications_action)
        }
        ResourceType::ExtensionApi => {
            let extension_api_action = match action.to_lowercase().as_str() {
                "call" => ExtensionApiAction::Call,
                _ => {
                    return Err(ExtensionError::ValidationError {
                        reason: format!("Invalid extensionApi action: {action} (expected 'call')"),
                    })
                }
            };
            Action::ExtensionApi(extension_api_action)
        }
    };

    // Check if permission already exists.
    //
    // Mail allows multiple permissions per host (one each for `fetch`
    // and `send`), so for `Mail` we also match on the action to avoid
    // a `send` decision overwriting a stored `fetch` decision.
    let existing_permissions =
        PermissionManager::get_permissions(&state, &Principal::Extension(extension_id.clone()))
            .await?;

    let existing_permission = existing_permissions.iter().find(|p| {
        if p.resource_type != resource_type_enum || p.target != target {
            return false;
        }
        if matches!(resource_type_enum, ResourceType::Mail) {
            return p.action == action_enum;
        }
        true
    });

    if let Some(existing) = existing_permission {
        if existing.action == action_enum {
            // Update existing permission
            PermissionManager::update_permission_status(&state, &existing.id, status).await?;
        } else {
            // The stored action no longer matches what was just requested (e.g. a
            // `read` grant followed by a `readWrite` request) — persist the new
            // action too, not just the status, so a later permission check for the
            // upgraded action succeeds instead of re-prompting forever.
            let updated_permission = ExtensionPermission {
                id: existing.id.clone(),
                principal_id: existing.principal_id.clone(),
                resource_type: resource_type_enum,
                action: action_enum,
                target,
                constraints: existing.constraints.clone(),
                status,
                raw_constraints: existing.raw_constraints.clone(),
            };
            PermissionManager::update_permission(&state, &updated_permission).await?;
        }
    } else {
        // Create new permission
        let new_permission = ExtensionPermission {
            id: uuid::Uuid::new_v4().to_string(),
            principal_id: extension_id.clone(),
            resource_type: resource_type_enum,
            action: action_enum,
            target,
            constraints: None,
            status,
            // Passwords default-labels (`{"default":true}`) originate only from
            // the manifest, never from interactive prompts — `None` is correct here.
            raw_constraints: None,
        };

        PermissionManager::save_permissions(&state, &[new_permission]).await?;
    }

    Ok(())
}

// =============================================================================
// Session Permission Commands (for frontend settings view)
// =============================================================================

/// Get all session permissions for an extension
///
/// Returns all in-memory permissions that are only valid for the current session.
/// Used by the extension settings UI to display temporary permissions.
#[tauri::command]
pub fn get_extension_session_permissions(
    extension_id: String,
    state: State<'_, AppState>,
) -> Vec<ExtensionPermission> {
    state
        .session_permissions
        .get_permissions_for_extension(&extension_id)
}

/// Remove a session permission for an extension
///
/// Removes a specific in-memory permission. Used when user wants to revoke
/// a temporary permission from the settings UI.
#[tauri::command]
pub fn remove_extension_session_permission(
    extension_id: String,
    resource_type: String,
    target: String,
    state: State<'_, AppState>,
) -> Result<(), ExtensionError> {
    let resource_type_enum = ResourceType::from_str(&resource_type)?;

    state.session_permissions.remove_permissions_for_target(
        &extension_id,
        resource_type_enum,
        &target,
    );

    eprintln!(
        "[SessionPermission] Removed {} permission for extension {} on {}",
        resource_type, extension_id, target
    );

    Ok(())
}
