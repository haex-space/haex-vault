// src-tauri/src/extension/permissions/checker.rs
// Testable permission checking logic without Tauri State dependencies

use crate::extension::core::types::Extension;
use crate::extension::permissions::types::{
    Action, DbAction, ExtensionPermission, FsAction, PermissionConstraints, PermissionStatus,
    ResourceType,
};
use crate::extension::utils;
use std::path::Path;

/// Testable permission checker that doesn't depend on Tauri State
///
/// This struct contains all the data needed for permission checks,
/// allowing us to test the permission logic without mocking AppState.
#[allow(dead_code)]
pub struct PermissionChecker {
    pub extension: Extension,
    pub permissions: Vec<ExtensionPermission>,
}

#[allow(dead_code)]
impl PermissionChecker {
    /// Creates a new PermissionChecker
    pub fn new(extension: Extension, permissions: Vec<ExtensionPermission>) -> Self {
        Self {
            extension,
            permissions,
        }
    }

    /// Checks if the extension can access a specific table with the given action
    ///
    /// # Table Isolation Rules
    /// 1. Extensions have automatic access to their own tables (prefix: {public_key}__{name}__)
    /// 2. System tables (haex_*) cannot be accessed unless explicitly granted
    /// 3. Other extensions' tables cannot be accessed without explicit permission
    /// 4. Explicit permissions support wildcards:
    ///    - "*" grants access to all non-system tables
    ///    - "prefix__*" grants access to all tables starting with prefix__
    ///    - "exact_table" grants access to specific table
    pub fn can_access_table(&self, table_name: &str, action: DbAction) -> bool {
        // Remove quotes from table name if present
        let clean_table_name = table_name.trim_matches('"').trim_matches('`');

        // Rule 1: Auto-allow access to own tables
        if utils::is_extension_table(
            clean_table_name,
            &self.extension.manifest.public_key,
            &self.extension.manifest.name,
        ) {
            return true;
        }

        // Rule 2: Check explicit permissions
        self.has_explicit_permission(clean_table_name, action)
    }

    /// Checks if there's an explicit permission for the table and action
    fn has_explicit_permission(&self, table_name: &str, action: DbAction) -> bool {
        self.permissions
            .iter()
            .filter(|perm| perm.status == PermissionStatus::Granted)
            .filter(|perm| perm.resource_type == ResourceType::Db)
            .filter(|perm| matches_action(&perm.action, action))
            .any(|perm| matches_target(&perm.target, table_name))
    }

    /// Checks if a table is auto-allowed (extension's own table)
    pub fn is_auto_allowed_table(&self, table_name: &str) -> bool {
        let clean_table_name = table_name.trim_matches('"').trim_matches('`');
        utils::is_extension_table(
            clean_table_name,
            &self.extension.manifest.public_key,
            &self.extension.manifest.name,
        )
    }

    /// Checks if a target pattern matches a table name (public wrapper)
    pub fn matches_table_pattern(&self, target: &str, table_name: &str) -> bool {
        let clean_table_name = table_name.trim_matches('"').trim_matches('`');
        matches_target(target, clean_table_name)
    }

    /// Checks if an action allows a specific DbAction (public wrapper)
    pub fn action_allows_db_action(&self, permission_action: &Action, required: DbAction) -> bool {
        matches_action(permission_action, required)
    }

    /// Silent filesystem-read predicate for a specific path.
    ///
    /// Returns `true` iff the extension is *currently* allowed to read
    /// `file_path` without prompting. Used by the file-change broadcast layer
    /// to scope event fan-out to authorised extensions only.
    ///
    /// Rules, in order:
    /// - An explicit session-`Denied` entry always wins → `false`.
    /// - Otherwise: a DB permission with matching path + `Granted` status → `true`.
    /// - A DB permission in `Ask` state, combined with a session grant → `true`
    ///   (the user approved it for this session).
    /// - A DB permission in `Ask` without session grant → `false`
    ///   (we must not prompt from this context).
    /// - A DB permission in `Denied` status → `false`.
    /// - No matching DB permission, but a session grant exists → `true`.
    /// - Constraint violations (e.g. file extension not in allow-list) → `false`.
    /// - Anything else → `false`.
    ///
    /// `session_granted` / `session_denied` encode the state of
    /// `AppState::session_permissions` for this `(extension, path)` pair. The
    /// caller resolves them against the live store; the predicate stays pure.
    pub fn can_read_path_silently(
        &self,
        file_path: &Path,
        session_granted: bool,
        session_denied: bool,
    ) -> bool {
        if session_denied {
            return false;
        }

        let file_path_str = file_path.to_string_lossy();

        let matching = self.permissions.iter().find(|perm| {
            perm.resource_type == ResourceType::Fs
                && matches_fs_action_for_read(&perm.action)
                && super::manager::PermissionManager::matches_path_pattern(
                    &perm.target,
                    &file_path_str,
                )
        });

        let passes_constraints = |perm: &ExtensionPermission| -> bool {
            let Some(PermissionConstraints::Filesystem(constraints)) = &perm.constraints else {
                return true;
            };
            let Some(allowed_ext) = &constraints.allowed_extensions else {
                return true;
            };
            match file_path.extension() {
                Some(ext) => {
                    let ext_str = format!(".{}", ext.to_string_lossy());
                    allowed_ext.contains(&ext_str)
                }
                None => false,
            }
        };

        match matching {
            Some(perm) if !passes_constraints(perm) => false,
            Some(perm) => match perm.status {
                PermissionStatus::Granted => true,
                PermissionStatus::Denied => false,
                PermissionStatus::Ask => session_granted,
            },
            None => session_granted,
        }
    }
}

/// Does `permission_action` authorise filesystem reads? Both `Read` and
/// `ReadWrite` qualify.
fn matches_fs_action_for_read(permission_action: &Action) -> bool {
    matches!(
        permission_action,
        Action::Filesystem(FsAction::Read) | Action::Filesystem(FsAction::ReadWrite)
    )
}

/// Checks if an action matches the required DbAction
pub(crate) fn matches_action(permission_action: &Action, required: DbAction) -> bool {
    match permission_action {
        Action::Database(action) => match (action, required) {
            // Exact match
            (a, b) if a == &b => true,
            // ReadWrite includes Read
            (DbAction::ReadWrite, DbAction::Read) => true,
            _ => false,
        },
        _ => false,
    }
}

/// Checks if a target pattern matches a table name
///
/// Supports:
/// - "*" - matches all non-system tables
/// - "prefix__*" - matches all tables starting with "prefix__"
/// - "exact_table" - matches exact table name
pub(crate) fn matches_target(target: &str, table_name: &str) -> bool {
    // System tables are never matched by any pattern
    if is_system_table(table_name) {
        return false;
    }

    // Full wildcard matches all non-system tables
    if target == "*" {
        return true;
    }

    // Prefix wildcard: "some_prefix__*"
    if let Some(prefix) = target.strip_suffix('*') {
        return table_name.starts_with(prefix);
    }

    // Exact match
    target == table_name
}

/// Checks if a table is a system table
pub(crate) fn is_system_table(table_name: &str) -> bool {
    table_name.starts_with("haex_")
        || table_name.starts_with("sqlite_") // Covers sqlite_master, sqlite_sequence, sqlite_stat1, etc.
}
