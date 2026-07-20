//! Permission Management Commands

use crate::{
    extension::{
        core::{EditablePermissions, PermissionEntry},
        error::ExtensionError,
        permissions::{
            manager::PermissionManager,
            types::{ExtensionPermission, Principal, ResourceType},
        },
    },
    AppState,
};
use tauri::State;

/// Converts internal ExtensionPermission list to UI-friendly EditablePermissions format
pub(crate) fn convert_to_editable_permissions(
    permissions: Vec<ExtensionPermission>,
) -> EditablePermissions {
    let mut database = Vec::new();
    let mut filesystem = Vec::new();
    let mut http = Vec::new();
    let mut shell = Vec::new();
    let mut sync_servers = Vec::new();
    let mut cloud_storage = Vec::new();
    let mut sync_rules = Vec::new();
    let mut spaces = Vec::new();
    let mut identities = Vec::new();
    let mut passwords = Vec::new();
    let mut bookmarks = Vec::new();
    let mut mail = Vec::new();
    let mut notifications = Vec::new();

    for perm in permissions {
        let entry = PermissionEntry {
            target: perm.target,
            operation: Some(perm.action.as_str()),
            // Prefer the raw constraints (passwords `{"default":true}`) so the
            // marker survives a get -> edit -> update round-trip; fall back to
            // the typed constraints for all other resource types.
            constraints: perm.raw_constraints.or_else(|| {
                perm.constraints
                    .map(|c| serde_json::to_value(c).unwrap_or_default())
            }),
            status: Some(perm.status),
        };

        match perm.resource_type {
            ResourceType::Db => database.push(entry),
            ResourceType::Fs => filesystem.push(entry),
            ResourceType::Web => http.push(entry),
            ResourceType::Shell => shell.push(entry),
            ResourceType::SyncServers => sync_servers.push(entry),
            ResourceType::CloudStorage => cloud_storage.push(entry),
            ResourceType::SyncRules => sync_rules.push(entry),
            ResourceType::Spaces => spaces.push(entry),
            ResourceType::Identities => identities.push(entry),
            ResourceType::Passwords => passwords.push(entry),
            ResourceType::Bookmarks => bookmarks.push(entry),
            ResourceType::Mail => mail.push(entry),
            ResourceType::Notifications => notifications.push(entry),
            // `ExtensionApi` rows (external bridge clients calling other
            // extensions) have no slot in this installed-extension editable
            // model — the bridge client settings UI reads them separately.
            ResourceType::ExtensionApi => {}
        }
    }

    EditablePermissions {
        database: if database.is_empty() {
            None
        } else {
            Some(database)
        },
        filesystem: if filesystem.is_empty() {
            None
        } else {
            Some(filesystem)
        },
        http: if http.is_empty() { None } else { Some(http) },
        shell: if shell.is_empty() { None } else { Some(shell) },
        sync_servers: if sync_servers.is_empty() {
            None
        } else {
            Some(sync_servers)
        },
        cloud_storage: if cloud_storage.is_empty() {
            None
        } else {
            Some(cloud_storage)
        },
        sync_rules: if sync_rules.is_empty() {
            None
        } else {
            Some(sync_rules)
        },
        spaces: if spaces.is_empty() {
            None
        } else {
            Some(spaces)
        },
        identities: if identities.is_empty() {
            None
        } else {
            Some(identities)
        },
        passwords: if passwords.is_empty() {
            None
        } else {
            Some(passwords)
        },
        bookmarks: if bookmarks.is_empty() {
            None
        } else {
            Some(bookmarks)
        },
        mail: if mail.is_empty() { None } else { Some(mail) },
        notifications: if notifications.is_empty() {
            None
        } else {
            Some(notifications)
        },
    }
}

#[tauri::command]
pub async fn get_extension_permissions(
    extension_id: String,
    state: State<'_, AppState>,
) -> Result<EditablePermissions, ExtensionError> {
    // Load permissions from database (same for dev and production extensions)
    let permissions =
        PermissionManager::get_permissions(&state, &Principal::Extension(extension_id.clone()))
            .await?;
    Ok(convert_to_editable_permissions(permissions))
}

#[tauri::command]
pub async fn update_extension_permissions(
    extension_id: String,
    permissions: EditablePermissions,
    state: State<'_, AppState>,
) -> Result<(), ExtensionError> {
    // Atomically replace permissions in a single transaction so a partial-save
    // failure cannot leave the extension with zero permissions.
    let internal_permissions = permissions.to_internal_permissions(&extension_id);
    PermissionManager::replace_permissions(&state, &extension_id, &internal_permissions).await?;

    Ok(())
}
