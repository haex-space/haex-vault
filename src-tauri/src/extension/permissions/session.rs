// src-tauri/src/extension/permissions/session.rs
//!
//! Session-based permission storage (in-memory, not persisted)
//!
//! These permissions are granted for the current session only and are cleared
//! when the application restarts.

use super::types::{Action, ExtensionPermission, PermissionStatus, ResourceType};
use std::collections::HashMap;
use std::sync::Mutex;

/// Key for session permission lookup.
///
/// `action` is part of the key so that action-level resources never bleed across
/// actions: an "allow once" Read grant must not satisfy a later Write check on
/// the same target (which would break the distinct Identity Read/Write model and
/// the `RwAction` resources). Matching is exact — a `ReadWrite` session grant
/// does not implicitly cover a separate `Read` check, which fails safe (re-prompt
/// rather than silently widen access).
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct SessionPermissionKey {
    extension_id: String,
    action: Action,
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
            extension_id: permission.principal_id.clone(),
            action: permission.action.clone(),
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
        action: &Action,
        resource_type: ResourceType,
        target: &str,
    ) -> Option<PermissionStatus> {
        let key = SessionPermissionKey {
            extension_id: extension_id.to_string(),
            action: action.clone(),
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
        action: &Action,
        resource_type: ResourceType,
        target: &str,
    ) -> bool {
        self.get_permission(extension_id, action, resource_type, target)
            == Some(PermissionStatus::Granted)
    }

    /// Check if a session permission denies access (returns true if denied)
    pub fn is_denied(
        &self,
        extension_id: &str,
        action: &Action,
        resource_type: ResourceType,
        target: &str,
    ) -> bool {
        self.get_permission(extension_id, action, resource_type, target)
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

    /// Session-only counterpart of `PermissionManager::save_passwords_grant`:
    /// grants/denies a passwords tag decision for the current session,
    /// covering possibly multiple tags plus the "exactly one default label"
    /// invariant. Unlike the DB path there is no existing row to look up —
    /// `set_permission` simply overwrites whatever was keyed for
    /// `(extension_id, action, Passwords, tag)`, which also naturally
    /// implements the "revoke deselected tags" behaviour (an overwrite with
    /// `status = Denied`).
    ///
    /// `previously_offered_tags` are the tags the user was originally shown
    /// for THIS prompt; any excluded from `tags` are explicitly denied,
    /// unless the new grant is the wildcard (`"*"`) — see
    /// `save_passwords_grant` for why wildcard grants skip revocation.
    pub fn set_passwords_grant(
        &self,
        extension_id: &str,
        action: super::types::PasswordsAction,
        tags: &[String],
        default_tag: Option<&str>,
        status: PermissionStatus,
        previously_offered_tags: &[String],
    ) -> Result<(), crate::extension::error::ExtensionError> {
        use crate::extension::permissions::manager::normalize_passwords_grant_tags;

        let (effective_tags, is_wildcard) = normalize_passwords_grant_tags(
            tags,
            default_tag,
            action.clone(),
            status,
            extension_id,
        )?;

        for tag in &effective_tags {
            let raw_constraints = if !is_wildcard && default_tag == Some(tag.as_str()) {
                Some(serde_json::json!({ "default": true }))
            } else {
                None
            };
            self.set_permission(ExtensionPermission {
                id: format!("session-{}", uuid::Uuid::new_v4()),
                principal_id: extension_id.to_string(),
                resource_type: ResourceType::Passwords,
                action: Action::Passwords(action.clone()),
                target: tag.clone(),
                constraints: None,
                status,
                raw_constraints,
            });
        }

        if !is_wildcard {
            for tag in previously_offered_tags {
                if tag == "*" || effective_tags.iter().any(|t| t == tag) {
                    continue;
                }
                self.set_permission(ExtensionPermission {
                    id: format!("session-{}", uuid::Uuid::new_v4()),
                    principal_id: extension_id.to_string(),
                    resource_type: ResourceType::Passwords,
                    action: Action::Passwords(action.clone()),
                    target: tag.clone(),
                    constraints: None,
                    status: PermissionStatus::Denied,
                    raw_constraints: None,
                });
            }
        }

        Ok(())
    }

    /// Remove the session permission(s) for a given resource + target.
    ///
    /// Because the key now also includes `action`, a single resource/target can
    /// hold multiple entries (e.g. a Read and a Write grant). Revoking from the
    /// settings UI is target-scoped — it conservatively clears every action for
    /// that resource/target rather than leaving a partial grant behind.
    pub fn remove_permissions_for_target(
        &self,
        extension_id: &str,
        resource_type: ResourceType,
        target: &str,
    ) {
        if let Ok(mut perms) = self.permissions.lock() {
            perms.retain(|k, _| {
                !(k.extension_id == extension_id
                    && k.resource_type == resource_type
                    && k.target == target)
            });
        }
    }
}
