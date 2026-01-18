// src-tauri/src/extension/permissions/session.rs
//!
//! Session-based permission storage (in-memory, not persisted)
//!
//! These permissions are granted for the current session only and are cleared
//! when the application restarts.

use super::types::{ExtensionPermission, PermissionStatus, ResourceType};
use std::collections::HashMap;
use std::sync::Mutex;

/// Key for session permission lookup
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct SessionPermissionKey {
    extension_id: String,
    resource_type: ResourceType,
    target: String,
}

/// Session permission store - holds permissions that are only valid for the current session
#[derive(Debug, Default)]
pub struct SessionPermissionStore {
    /// Map of permission key to full permission entry
    permissions: Mutex<HashMap<SessionPermissionKey, ExtensionPermission>>,
}

impl SessionPermissionStore {
    pub fn new() -> Self {
        Self {
            permissions: Mutex::new(HashMap::new()),
        }
    }

    /// Store a permission for the current session
    pub fn set_permission(&self, permission: ExtensionPermission) {
        let key = SessionPermissionKey {
            extension_id: permission.extension_id.clone(),
            resource_type: permission.resource_type,
            target: permission.target.clone(),
        };

        if let Ok(mut perms) = self.permissions.lock() {
            perms.insert(key, permission);
        }
    }

    /// Check if a session permission exists for the given parameters
    /// Returns Some(status) if found, None if not found
    pub fn get_permission(
        &self,
        extension_id: &str,
        resource_type: ResourceType,
        target: &str,
    ) -> Option<PermissionStatus> {
        let key = SessionPermissionKey {
            extension_id: extension_id.to_string(),
            resource_type,
            target: target.to_string(),
        };

        self.permissions
            .lock()
            .ok()
            .and_then(|perms| perms.get(&key).map(|p| p.status))
    }

    /// Check if a session permission grants access (returns true if granted)
    pub fn is_granted(
        &self,
        extension_id: &str,
        resource_type: ResourceType,
        target: &str,
    ) -> bool {
        self.get_permission(extension_id, resource_type, target)
            == Some(PermissionStatus::Granted)
    }

    /// Check if a session permission denies access (returns true if denied)
    pub fn is_denied(
        &self,
        extension_id: &str,
        resource_type: ResourceType,
        target: &str,
    ) -> bool {
        self.get_permission(extension_id, resource_type, target)
            == Some(PermissionStatus::Denied)
    }

    /// Clear all session permissions for an extension
    pub fn clear_extension(&self, extension_id: &str) {
        if let Ok(mut perms) = self.permissions.lock() {
            perms.retain(|k, _| k.extension_id != extension_id);
        }
    }

    /// Clear all session permissions
    pub fn clear_all(&self) {
        if let Ok(mut perms) = self.permissions.lock() {
            perms.clear();
        }
    }

    /// Get all session permissions for a specific extension
    pub fn get_permissions_for_extension(&self, extension_id: &str) -> Vec<ExtensionPermission> {
        self.permissions
            .lock()
            .ok()
            .map(|perms| {
                perms
                    .iter()
                    .filter(|(k, _)| k.extension_id == extension_id)
                    .map(|(_, v)| v.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Remove a specific session permission
    pub fn remove_permission(
        &self,
        extension_id: &str,
        resource_type: ResourceType,
        target: &str,
    ) {
        let key = SessionPermissionKey {
            extension_id: extension_id.to_string(),
            resource_type,
            target: target.to_string(),
        };

        if let Ok(mut perms) = self.permissions.lock() {
            perms.remove(&key);
        }
    }
}
