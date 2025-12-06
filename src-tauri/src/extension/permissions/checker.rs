// src-tauri/src/extension/permissions/checker.rs
// Testable permission checking logic without Tauri State dependencies

use crate::extension::core::types::Extension;
use crate::extension::permissions::types::{Action, DbAction, ExtensionPermission, PermissionStatus, ResourceType};
use crate::extension::utils;

/// Testable permission checker that doesn't depend on Tauri State
///
/// This struct contains all the data needed for permission checks,
/// allowing us to test the permission logic without mocking AppState.
pub struct PermissionChecker {
    pub extension: Extension,
    pub permissions: Vec<ExtensionPermission>,
}

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
    table_name.starts_with("haex_") || table_name == "sqlite_master" || table_name == "sqlite_sequence"
}
