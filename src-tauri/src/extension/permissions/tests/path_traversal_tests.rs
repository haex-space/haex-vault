// src-tauri/src/extension/permissions/tests/path_traversal_tests.rs
//!
//! Tests for path traversal attack prevention
//!
//! These tests verify that the permission checking code correctly handles
//! path traversal attempts like "../", "..\", URL-encoded sequences, etc.
//!
//! The implementation normalizes paths before matching to prevent directory
//! traversal attacks.
//!

use crate::extension::permissions::manager::PermissionManager;

// ============================================================================
// Basic Functionality Tests
// ============================================================================

#[test]
fn test_matches_path_pattern_basic_functionality() {
    // Basic wildcard matching
    assert!(PermissionManager::matches_path_pattern(
        "/home/user/*",
        "/home/user/file.txt"
    ));
    assert!(PermissionManager::matches_path_pattern(
        "/home/user/*",
        "/home/user/subdir/file.txt"
    ));

    // Full wildcard
    assert!(PermissionManager::matches_path_pattern("*", "/any/path/file.txt"));

    // Exact match
    assert!(PermissionManager::matches_path_pattern(
        "/home/user/specific.txt",
        "/home/user/specific.txt"
    ));

    // Extension wildcard
    assert!(PermissionManager::matches_path_pattern("*.txt", "/any/file.txt"));
    assert!(!PermissionManager::matches_path_pattern("*.txt", "/any/file.pdf"));
}

// ============================================================================
// Path Traversal Protection Tests - All should be BLOCKED
// ============================================================================

/// Path traversal with ../ must be blocked
#[test]
fn test_path_traversal_blocked() {
    // Simple traversal - MUST be rejected
    assert!(
        !PermissionManager::matches_path_pattern(
            "/home/user/*",
            "/home/user/../etc/passwd"
        ),
        "Path traversal '../' must be blocked"
    );

    // Multiple traversals - MUST be rejected
    assert!(
        !PermissionManager::matches_path_pattern(
            "/home/user/*",
            "/home/user/../../etc/passwd"
        ),
        "Multiple '../' traversals must be blocked"
    );

    // Traversal in subdirectory - MUST be rejected
    assert!(
        !PermissionManager::matches_path_pattern(
            "/home/user/*",
            "/home/user/data/../../../etc/passwd"
        ),
        "Nested '../' traversals must be blocked"
    );
}

/// Windows-style backslash traversal must be blocked
#[test]
fn test_windows_style_traversal_blocked() {
    assert!(
        !PermissionManager::matches_path_pattern(
            "/home/user/*",
            "/home/user/..\\etc\\passwd"
        ),
        "Windows-style backslash traversal must be blocked"
    );

    // Mixed separators
    assert!(
        !PermissionManager::matches_path_pattern(
            "/home/user/*",
            "/home/user\\..\\etc\\passwd"
        ),
        "Mixed path separators with traversal must be blocked"
    );
}

/// URL-encoded traversal sequences must be decoded and blocked
#[test]
fn test_url_encoded_traversal_blocked() {
    // URL-encoded "../" (%2e%2e%2f)
    assert!(
        !PermissionManager::matches_path_pattern(
            "/home/user/*",
            "/home/user/%2e%2e%2fetc/passwd"
        ),
        "URL-encoded traversal (%2e%2e%2f) must be blocked"
    );

    // Double URL-encoded
    assert!(
        !PermissionManager::matches_path_pattern(
            "/home/user/*",
            "/home/user/%252e%252e%252fetc/passwd"
        ),
        "Double URL-encoded traversal must be blocked"
    );

    // URL-encoded backslash
    assert!(
        !PermissionManager::matches_path_pattern(
            "/home/user/*",
            "/home/user/%2e%2e%5cetc%5cpasswd"
        ),
        "URL-encoded backslash traversal must be blocked"
    );
}

/// Null byte injection must be blocked
#[test]
fn test_null_byte_injection_blocked() {
    assert!(
        !PermissionManager::matches_path_pattern(
            "/home/user/*",
            "/home/user/file.txt\0/etc/passwd"
        ),
        "Null byte injection must be blocked"
    );

    assert!(
        !PermissionManager::matches_path_pattern(
            "/home/user/*",
            "\0/etc/passwd"
        ),
        "Null byte at start must be blocked"
    );
}

/// Current directory with traversal must be blocked
#[test]
fn test_current_directory_traversal_blocked() {
    // Using ./ is fine
    assert!(PermissionManager::matches_path_pattern(
        "/home/user/*",
        "/home/user/./file.txt"
    ));

    // But /./../ traversal must be blocked
    assert!(
        !PermissionManager::matches_path_pattern(
            "/home/user/*",
            "/home/user/./../etc/passwd"
        ),
        "Path traversal via ./../ must be blocked"
    );
}

/// Double slashes with traversal must be blocked
#[test]
fn test_double_slashes_with_traversal_blocked() {
    // Double slashes should be normalized
    assert!(PermissionManager::matches_path_pattern(
        "/home/user/*",
        "/home/user//file.txt"
    ));

    // Traversal with double slashes must be blocked
    assert!(
        !PermissionManager::matches_path_pattern(
            "/home/user/*",
            "/home/user//../etc/passwd"
        ),
        "Path traversal with double slashes must be blocked"
    );
}

/// Extension wildcard must not allow traversal
#[test]
fn test_extension_wildcard_traversal_blocked() {
    // Normal extension matching works
    assert!(PermissionManager::matches_path_pattern("*.txt", "/home/file.txt"));

    // But traversal with extension must be blocked
    assert!(
        !PermissionManager::matches_path_pattern(
            "*.txt",
            "../../../etc/passwd.txt"
        ),
        "Extension wildcard must not allow relative traversal"
    );

    assert!(
        !PermissionManager::matches_path_pattern(
            "*.txt",
            "/home/user/../../../etc/secret.txt"
        ),
        "Absolute path traversal with extension wildcard must be blocked"
    );
}

// ============================================================================
// Correctly Rejected Paths Tests
// ============================================================================

#[test]
fn test_correctly_rejected_paths() {
    // Normalized paths outside the directory are correctly rejected
    assert!(!PermissionManager::matches_path_pattern(
        "/home/user/*",
        "/etc/passwd"
    ));

    assert!(!PermissionManager::matches_path_pattern(
        "/home/user/*",
        "/var/log/syslog"
    ));

    // Root path doesn't match subdirectory pattern
    assert!(!PermissionManager::matches_path_pattern("/home/user/*", "/"));
}

#[test]
fn test_case_sensitivity() {
    // Case variations - different cases don't match
    assert!(!PermissionManager::matches_path_pattern(
        "/home/user/*",
        "/HOME/USER/file.txt"
    ));

    // Mixed case shouldn't match
    assert!(!PermissionManager::matches_path_pattern(
        "/Home/User/*",
        "/home/user/file.txt"
    ));
}

#[test]
fn test_absolute_vs_relative() {
    // Relative path shouldn't match absolute pattern
    assert!(!PermissionManager::matches_path_pattern(
        "/home/user/*",
        "home/user/file.txt"
    ));

    // Absolute path shouldn't match relative pattern
    assert!(!PermissionManager::matches_path_pattern(
        "home/user/*",
        "/home/user/file.txt"
    ));
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_empty_paths() {
    // Empty path doesn't match non-empty pattern
    assert!(!PermissionManager::matches_path_pattern("/home/user/*", ""));

    // Empty pattern doesn't match non-empty path
    assert!(!PermissionManager::matches_path_pattern("", "/home/user/file.txt"));

    // Both empty - exact match
    assert!(PermissionManager::matches_path_pattern("", ""));
}

#[test]
fn test_special_directory_entries() {
    // Just .. doesn't match (normalized away or not in prefix)
    assert!(!PermissionManager::matches_path_pattern("/home/user/*", ".."));
    assert!(!PermissionManager::matches_path_pattern("/home/user/*", "."));

    // /home/user/.. normalizes to /home, which doesn't match /home/user/*
    assert!(
        !PermissionManager::matches_path_pattern("/home/user/*", "/home/user/.."),
        "/home/user/.. should normalize to /home and not match /home/user/*"
    );
}

#[test]
fn test_prefix_suffix_wildcard() {
    // Pattern with prefix and suffix wildcards
    assert!(PermissionManager::matches_path_pattern(
        "/home/user/*.txt",
        "/home/user/document.txt"
    ));

    // Subdirectories
    assert!(PermissionManager::matches_path_pattern(
        "/home/user/*.txt",
        "/home/user/subdir/document.txt"
    ));
}

// ============================================================================
// Real Attack Scenario Tests
// ============================================================================

#[test]
fn test_directory_escape_blocked() {
    // Scenario: User has permission for /data/app/*
    // Attacker tries to access /etc/passwd via traversal
    let allowed_pattern = "/data/app/*";

    // Direct access - correctly denied
    assert!(
        !PermissionManager::matches_path_pattern(allowed_pattern, "/etc/passwd"),
        "Direct access to /etc/passwd must be denied"
    );

    // Via traversal - MUST be denied (was previously vulnerable)
    assert!(
        !PermissionManager::matches_path_pattern(allowed_pattern, "/data/app/../../../etc/passwd"),
        "Traversal to /etc/passwd must be denied"
    );

    // Legitimate access within the directory
    assert!(
        PermissionManager::matches_path_pattern(allowed_pattern, "/data/app/config.json"),
        "Legitimate access within directory must be allowed"
    );
}

#[test]
fn test_sensitive_file_protection() {
    let patterns_to_test = [
        ("/home/*", "/etc/shadow"),
        ("/home/*", "/etc/passwd"),
        ("/home/*", "/root/.ssh/id_rsa"),
        ("/data/*", "/proc/self/environ"),
        ("/tmp/*", "/sys/kernel/security/lsm"),
    ];

    for (pattern, sensitive_path) in patterns_to_test {
        assert!(
            !PermissionManager::matches_path_pattern(pattern, sensitive_path),
            "Pattern '{}' must NOT match sensitive path '{}'",
            pattern,
            sensitive_path
        );
    }
}

#[test]
fn test_cross_user_directory_protection() {
    // User A has permission for /home/userA/*
    let user_a_pattern = "/home/userA/*";

    // Own files - allowed
    assert!(PermissionManager::matches_path_pattern(
        user_a_pattern,
        "/home/userA/documents/file.txt"
    ));

    // Other user's files - denied
    assert!(!PermissionManager::matches_path_pattern(
        user_a_pattern,
        "/home/userB/documents/file.txt"
    ));

    // Via traversal - MUST be denied (was previously vulnerable)
    assert!(
        !PermissionManager::matches_path_pattern(
            user_a_pattern,
            "/home/userA/../userB/documents/file.txt"
        ),
        "Cross-user access via path traversal must be blocked"
    );
}

// ============================================================================
// Path Normalization Tests
// ============================================================================

#[test]
fn test_path_normalization_removes_dots() {
    // Path with ./ should normalize correctly
    assert!(PermissionManager::matches_path_pattern(
        "/home/user/*",
        "/home/user/./subdir/./file.txt"
    ));

    // After normalization this is /home/user/subdir/file.txt
}

#[test]
fn test_path_normalization_handles_multiple_slashes() {
    // Multiple slashes should be normalized
    assert!(PermissionManager::matches_path_pattern(
        "/home/user/*",
        "/home/user///subdir//file.txt"
    ));
}

#[test]
fn test_backslash_to_forward_slash_conversion() {
    // Windows-style paths should be converted
    assert!(PermissionManager::matches_path_pattern(
        "/home/user/*",
        "/home/user\\subdir\\file.txt"
    ));
}

// ============================================================================
// Unicode Tests
// ============================================================================

#[test]
fn test_unicode_in_paths() {
    // Unicode characters in paths should work
    assert!(PermissionManager::matches_path_pattern(
        "/home/user/*",
        "/home/user/文档/файл.txt"
    ));
}

#[test]
fn test_unicode_lookalike_characters() {
    // Unicode fullwidth dots (U+FF0E) should not be treated as path separators
    // These are not valid path components and should be treated as regular characters
    assert!(PermissionManager::matches_path_pattern(
        "/home/user/*",
        "/home/user/\u{FF0E}\u{FF0E}/etc/passwd"
    ));
    // This passes because ．． is not ".." - it's unicode fullwidth dots
    // The path is literally /home/user/．．/etc/passwd which is a valid subdirectory name
}
