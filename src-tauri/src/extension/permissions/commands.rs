// src-tauri/src/extension/permissions/commands.rs

use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::{
    Action, DbAction, ExtensionPermission, FsAction, PermissionStatus, ResourceType, WebAction,
};
use crate::AppState;
use std::path::Path;
use tauri::State;

#[tauri::command]
pub async fn check_web_permission(
    extension_id: String,
    url: String,
    state: State<'_, AppState>,
) -> Result<(), ExtensionError> {
    PermissionManager::check_web_permission(&state, &extension_id, &url).await
}

#[tauri::command]
pub async fn check_database_permission(
    extension_id: String,
    resource: String,
    operation: String,
    state: State<'_, AppState>,
) -> Result<(), ExtensionError> {
    let action = match operation.as_str() {
        "read" => crate::extension::permissions::types::Action::Database(
            crate::extension::permissions::types::DbAction::Read,
        ),
        "write" => crate::extension::permissions::types::Action::Database(
            crate::extension::permissions::types::DbAction::ReadWrite,
        ),
        _ => {
            return Err(ExtensionError::ValidationError {
                reason: format!("Invalid database operation: {}", operation),
            })
        }
    };

    PermissionManager::check_database_permission(&state, &extension_id, action, &resource).await
}

#[tauri::command]
pub async fn check_filesystem_permission(
    extension_id: String,
    path: String,
    operation: String,
    state: State<'_, AppState>,
) -> Result<(), ExtensionError> {
    let action = match operation.as_str() {
        "read" => crate::extension::permissions::types::Action::Filesystem(
            crate::extension::permissions::types::FsAction::Read,
        ),
        "write" => crate::extension::permissions::types::Action::Filesystem(
            crate::extension::permissions::types::FsAction::ReadWrite,
        ),
        _ => {
            return Err(ExtensionError::ValidationError {
                reason: format!("Invalid filesystem operation: {}", operation),
            })
        }
    };

    let file_path = Path::new(&path);
    PermissionManager::check_filesystem_permission(&state, &extension_id, action, file_path).await
}

/// Grants or denies a permission for the current session only (not persisted to database)
///
/// Called by the frontend when user makes a decision without checking "remember".
/// These permissions are cleared when the application restarts.
#[tauri::command]
pub fn grant_session_permission(
    extension_id: String,
    resource_type: String,
    target: String,
    decision: String,
    state: State<'_, AppState>,
) -> Result<(), ExtensionError> {
    let resource_type_enum = ResourceType::from_str(&resource_type)?;
    let status = PermissionStatus::from_str(&decision)?;

    state
        .session_permissions
        .set_permission(&extension_id, resource_type_enum, &target, status);

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
    extension_id: String,
    resource_type: String,
    action: String,
    target: String,
    decision: String,
    state: State<'_, AppState>,
) -> Result<(), ExtensionError> {
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
                reason: format!("Invalid decision: {}. Expected 'granted', 'denied', or 'ask'", decision),
            })
        }
    };

    // Parse resource type
    let resource_type_enum = match resource_type.as_str() {
        "db" => ResourceType::Db,
        "web" => ResourceType::Web,
        "fs" => ResourceType::Fs,
        "shell" => ResourceType::Shell,
        "filesync" => ResourceType::Filesync,
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
        ResourceType::Shell => Action::Shell(crate::extension::permissions::types::ShellAction::Execute),
        ResourceType::Filesync => {
            let filesync_action = match action.to_lowercase().as_str() {
                "read" => crate::extension::permissions::types::FileSyncAction::Read,
                "readwrite" | "read_write" => crate::extension::permissions::types::FileSyncAction::ReadWrite,
                _ => crate::extension::permissions::types::FileSyncAction::Read,
            };
            Action::FileSync(filesync_action)
        }
    };

    // Check if permission already exists
    let existing_permissions = PermissionManager::get_permissions(&state, &extension_id).await?;

    let existing_permission = existing_permissions.iter().find(|p| {
        p.resource_type == resource_type_enum && p.target == target
    });

    if let Some(existing) = existing_permission {
        // Update existing permission
        PermissionManager::update_permission_status(&state, &existing.id, status).await?;
    } else {
        // Create new permission
        let new_permission = ExtensionPermission {
            id: uuid::Uuid::new_v4().to_string(),
            extension_id: extension_id.clone(),
            resource_type: resource_type_enum,
            action: action_enum,
            target,
            constraints: None,
            status,
            haex_timestamp: None,
        };

        PermissionManager::save_permissions(&state, &[new_permission]).await?;
    }

    Ok(())
}
