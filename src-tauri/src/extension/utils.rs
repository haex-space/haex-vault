// src-tauri/src/extension/utils.rs
// Utility functions for extension management

use crate::extension::core::manifest::ExtensionManifest;
use crate::extension::core::types::Extension;

/// Generates the table prefix for an extension
/// Format: {public_key}__{extension_name}__
///
/// This prefix ensures table isolation between extensions.
/// Each extension can only access tables that start with this prefix.
///
/// # Examples
/// ```
/// let prefix = get_extension_table_prefix("abc123", "my_extension");
/// assert_eq!(prefix, "abc123__my_extension__");
/// ```
pub fn get_extension_table_prefix(public_key: &str, extension_name: &str) -> String {
    format!("{}__{}__", public_key, extension_name)
}

/// Generates the table prefix from an Extension instance
pub fn get_extension_table_prefix_from_extension(extension: &Extension) -> String {
    get_extension_table_prefix(&extension.manifest.public_key, &extension.manifest.name)
}

/// Generates the table prefix from an ExtensionManifest
pub fn get_extension_table_prefix_from_manifest(manifest: &ExtensionManifest) -> String {
    get_extension_table_prefix(&manifest.public_key, &manifest.name)
}

/// Checks if a table name belongs to a specific extension
///
/// # Examples
/// ```
/// assert!(is_extension_table("abc123__my_ext__users", "abc123", "my_ext"));
/// assert!(!is_extension_table("other__ext__users", "abc123", "my_ext"));
/// assert!(!is_extension_table("haex_system_table", "abc123", "my_ext"));
/// ```
pub fn is_extension_table(table_name: &str, public_key: &str, extension_name: &str) -> bool {
    let prefix = get_extension_table_prefix(public_key, extension_name);
    table_name.starts_with(&prefix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_extension_table_prefix() {
        let prefix = get_extension_table_prefix("test_key", "test_extension");
        assert_eq!(prefix, "test_key__test_extension__");
    }

    #[test]
    fn test_get_extension_table_prefix_with_special_chars() {
        let prefix = get_extension_table_prefix("key-123", "my_ext.v1");
        assert_eq!(prefix, "key-123__my_ext.v1__");
    }

    #[test]
    fn test_is_extension_table_own_table() {
        assert!(is_extension_table(
            "test_key__test_ext__users",
            "test_key",
            "test_ext"
        ));
    }

    #[test]
    fn test_is_extension_table_other_extension() {
        assert!(!is_extension_table(
            "other_key__other_ext__users",
            "test_key",
            "test_ext"
        ));
    }

    #[test]
    fn test_is_extension_table_system_table() {
        assert!(!is_extension_table(
            "haex_extensions",
            "test_key",
            "test_ext"
        ));
    }

    #[test]
    fn test_is_extension_table_similar_prefix() {
        // Should not match if prefix is similar but not exact
        assert!(!is_extension_table(
            "test_key__test_ext_fake__users",
            "test_key",
            "test_ext"
        ));
    }
}
