// src-tauri/src/extension/utils.rs
// Utility functions for extension management

use crate::extension::core::manifest::ExtensionManifest;
use crate::extension::core::types::Extension;
use crate::extension::error::ExtensionError;

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

/// Ed25519 public key length in bytes
const ED25519_PUBLIC_KEY_LENGTH: usize = 32;

/// Validates that a public key is a valid Ed25519 public key in hex format.
///
/// A valid public key must:
/// - Be exactly 64 hex characters (32 bytes)
/// - Contain only valid hex characters (0-9, a-f, A-F)
///
/// # Examples
/// ```
/// // Valid public key (64 hex chars)
/// assert!(validate_public_key("b4401f13f65e576b8a30ff9fd83df82a8bb707e1994d40c99996fe88603cefca").is_ok());
///
/// // Invalid: too short
/// assert!(validate_public_key("abc123").is_err());
///
/// // Invalid: contains non-hex characters
/// assert!(validate_public_key("demo_test_key_12345").is_err());
/// ```
pub fn validate_public_key(public_key: &str) -> Result<(), ExtensionError> {
    // Check length: Ed25519 public key is 32 bytes = 64 hex characters
    if public_key.len() != ED25519_PUBLIC_KEY_LENGTH * 2 {
        return Err(ExtensionError::ValidationError {
            reason: format!(
                "Invalid public key length: expected {} hex characters, got {}",
                ED25519_PUBLIC_KEY_LENGTH * 2,
                public_key.len()
            ),
        });
    }

    // Check that it's valid hex
    if !public_key.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ExtensionError::ValidationError {
            reason: "Invalid public key: must contain only hex characters (0-9, a-f, A-F)".to_string(),
        });
    }

    Ok(())
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

    #[test]
    fn test_validate_public_key_valid() {
        // Valid Ed25519 public key (64 hex chars)
        assert!(validate_public_key(
            "b4401f13f65e576b8a30ff9fd83df82a8bb707e1994d40c99996fe88603cefca"
        )
        .is_ok());
    }

    #[test]
    fn test_validate_public_key_valid_uppercase() {
        // Valid with uppercase hex chars
        assert!(validate_public_key(
            "B4401F13F65E576B8A30FF9FD83DF82A8BB707E1994D40C99996FE88603CEFCA"
        )
        .is_ok());
    }

    #[test]
    fn test_validate_public_key_too_short() {
        assert!(validate_public_key("abc123").is_err());
    }

    #[test]
    fn test_validate_public_key_too_long() {
        assert!(validate_public_key(
            "b4401f13f65e576b8a30ff9fd83df82a8bb707e1994d40c99996fe88603cefca00"
        )
        .is_err());
    }

    #[test]
    fn test_validate_public_key_invalid_chars() {
        // Contains underscores and other non-hex chars
        assert!(validate_public_key("demo_test_key_12345").is_err());
    }

    #[test]
    fn test_validate_public_key_empty() {
        assert!(validate_public_key("").is_err());
    }
}
