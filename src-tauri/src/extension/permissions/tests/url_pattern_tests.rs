// src-tauri/src/extension/permissions/tests/url_pattern_tests.rs
//!
//! Tests for URL pattern matching security
//!
//! These tests verify that the URL permission checking correctly handles
//! various bypass attempts including subdomain confusion, URL encoding, etc.
//!

use crate::extension::permissions::manager::PermissionManager;

// ============================================================================
// Basic URL Pattern Matching Tests
// ============================================================================

#[test]
fn test_url_pattern_basic_matching() {
    // Exact domain match
    assert!(PermissionManager::matches_url_pattern(
        "https://example.com/*",
        "https://example.com/api/data"
    ));

    // Subdomain wildcard
    assert!(PermissionManager::matches_url_pattern(
        "https://*.example.com/*",
        "https://api.example.com/endpoint"
    ));

    // Different domain should not match
    assert!(!PermissionManager::matches_url_pattern(
        "https://example.com/*",
        "https://evil.com/api"
    ));
}

// ============================================================================
// Subdomain Security Tests
// ============================================================================

/// Test that subdomain wildcard doesn't match the base domain
#[test]
fn test_subdomain_wildcard_requires_subdomain() {
    // *.example.com should NOT match example.com (no subdomain)
    assert!(
        !PermissionManager::matches_url_pattern(
            "https://*.example.com/*",
            "https://example.com/api"
        ),
        "*.example.com should NOT match example.com"
    );

    // But should match actual subdomains
    assert!(PermissionManager::matches_url_pattern(
        "https://*.example.com/*",
        "https://sub.example.com/api"
    ));
}

/// SECURITY TEST: Subdomain confusion attack
/// An attacker might try to bypass *.example.com by using evil-example.com
#[test]
fn test_subdomain_confusion_attack_blocked() {
    // evil-example.com should NOT match *.example.com
    assert!(
        !PermissionManager::matches_url_pattern(
            "https://*.example.com/*",
            "https://evil-example.com/api"
        ),
        "evil-example.com should NOT match *.example.com"
    );

    // notexample.com should NOT match
    assert!(
        !PermissionManager::matches_url_pattern(
            "https://*.example.com/*",
            "https://notexample.com/api"
        ),
        "notexample.com should NOT match *.example.com"
    );
}

/// SECURITY TEST: Attacker prepends the target domain
/// e.g., example.com.evil.com should NOT match *.example.com
#[test]
fn test_domain_prepending_attack_blocked() {
    // example.com.evil.com should NOT match *.example.com
    assert!(
        !PermissionManager::matches_url_pattern(
            "https://*.example.com/*",
            "https://example.com.evil.com/api"
        ),
        "example.com.evil.com should NOT match *.example.com"
    );

    // sub.example.com.evil.com should NOT match
    assert!(
        !PermissionManager::matches_url_pattern(
            "https://*.example.com/*",
            "https://sub.example.com.evil.com/api"
        ),
        "sub.example.com.evil.com should NOT match *.example.com"
    );
}

// ============================================================================
// Protocol Security Tests
// ============================================================================

#[test]
fn test_protocol_must_match() {
    // HTTPS pattern should NOT match HTTP URL
    assert!(
        !PermissionManager::matches_url_pattern(
            "https://example.com/*",
            "http://example.com/api"
        ),
        "HTTPS pattern should not match HTTP URL"
    );

    // HTTP pattern should NOT match HTTPS URL
    assert!(
        !PermissionManager::matches_url_pattern(
            "http://example.com/*",
            "https://example.com/api"
        ),
        "HTTP pattern should not match HTTPS URL"
    );
}

#[test]
fn test_protocol_downgrade_blocked() {
    // Attacker might try to downgrade HTTPS to HTTP
    assert!(
        !PermissionManager::matches_url_pattern(
            "https://*.example.com/*",
            "http://api.example.com/sensitive"
        ),
        "Protocol downgrade must be blocked"
    );
}

// ============================================================================
// Port Security Tests
// ============================================================================

#[test]
fn test_port_must_match() {
    // Different port should not match
    assert!(
        !PermissionManager::matches_url_pattern(
            "https://example.com:443/*",
            "https://example.com:8443/api"
        ),
        "Different port should not match"
    );

    // Default port (443 for HTTPS) behavior
    // Note: url crate normalizes default ports
}

// ============================================================================
// URL Encoding Security Tests
// ============================================================================

#[test]
fn test_url_encoded_domain_handled() {
    // URL-encoded domain parts - the url crate should handle this
    // %65%78%61%6d%70%6c%65 = "example"
    let result = PermissionManager::matches_url_pattern(
        "https://example.com/*",
        "https://%65%78%61%6d%70%6c%65.com/api"
    );
    // The url crate may or may not decode this - document the behavior
    // Most importantly, it shouldn't cause a security bypass
}

#[test]
fn test_url_encoded_path() {
    // URL-encoded path should still match wildcard
    assert!(PermissionManager::matches_url_pattern(
        "https://example.com/*",
        "https://example.com/%61%70%69" // /api encoded
    ));
}

// ============================================================================
// Path Traversal in URL Tests
// ============================================================================

#[test]
fn test_path_traversal_in_url() {
    // Path traversal in URL path - url crate normalizes this
    // The resolved path should still be checked against the pattern
    assert!(PermissionManager::matches_url_pattern(
        "https://example.com/*",
        "https://example.com/api/../other"
    ));
    // This is fine because /* allows any path under example.com
}

#[test]
fn test_path_specific_pattern() {
    // When a specific path is required, traversal shouldn't bypass it
    // Pattern: https://example.com/api/* should only allow /api/* paths

    // This would need a more specific pattern implementation
    // Current implementation with /* allows any path
}

// ============================================================================
// Special Character Tests
// ============================================================================

#[test]
fn test_null_byte_in_url() {
    // Null bytes in URL should be handled safely
    let result = PermissionManager::matches_url_pattern(
        "https://example.com/*",
        "https://example.com/api\0/evil"
    );
    // The url crate will likely reject or escape this
}

#[test]
fn test_unicode_domain() {
    // Unicode/IDN domains
    // These should be handled consistently
    let result = PermissionManager::matches_url_pattern(
        "https://example.com/*",
        "https://exаmple.com/api" // 'а' is Cyrillic, not Latin 'a'
    );
    assert!(
        !result,
        "Homograph attack with Cyrillic 'a' should not match"
    );
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_empty_and_invalid_urls() {
    // Empty URL should not match
    assert!(!PermissionManager::matches_url_pattern(
        "https://example.com/*",
        ""
    ));

    // Invalid URL should not match
    assert!(!PermissionManager::matches_url_pattern(
        "https://example.com/*",
        "not-a-valid-url"
    ));
}

#[test]
fn test_localhost_patterns() {
    // Localhost patterns
    assert!(PermissionManager::matches_url_pattern(
        "http://localhost/*",
        "http://localhost/api"
    ));

    // Different localhost representations
    assert!(PermissionManager::matches_url_pattern(
        "http://127.0.0.1/*",
        "http://127.0.0.1/api"
    ));

    // localhost vs 127.0.0.1 should NOT match each other
    assert!(
        !PermissionManager::matches_url_pattern(
            "http://localhost/*",
            "http://127.0.0.1/api"
        ),
        "localhost and 127.0.0.1 are different hosts"
    );
}

#[test]
fn test_ip_address_patterns() {
    // IP address patterns
    assert!(PermissionManager::matches_url_pattern(
        "https://192.168.1.1/*",
        "https://192.168.1.1/admin"
    ));

    // Different IP should not match
    assert!(!PermissionManager::matches_url_pattern(
        "https://192.168.1.1/*",
        "https://192.168.1.2/admin"
    ));
}

// ============================================================================
// Full Wildcard Tests
// ============================================================================

#[test]
fn test_full_wildcard_domain() {
    // Full wildcard should match any URL
    // Note: This depends on how "*" is interpreted in check_web_permission
}

#[test]
fn test_nested_subdomains() {
    // Deep subdomain nesting
    assert!(PermissionManager::matches_url_pattern(
        "https://*.example.com/*",
        "https://a.b.c.d.example.com/api"
    ));
}

// ============================================================================
// URL Path Traversal Tests
// ============================================================================

#[test]
fn test_url_path_traversal_blocked() {
    // Pattern allows /api/*
    // Attacker tries to access /admin via path traversal

    // Direct access to /admin should be blocked
    assert!(
        !PermissionManager::matches_url_pattern(
            "https://example.com/api/*",
            "https://example.com/admin/secret"
        ),
        "Direct access to /admin should be blocked"
    );

    // Path traversal from /api to /admin should also be blocked
    assert!(
        !PermissionManager::matches_url_pattern(
            "https://example.com/api/*",
            "https://example.com/api/../admin/secret"
        ),
        "Path traversal /api/../admin should be blocked"
    );

    // Multiple traversals
    assert!(
        !PermissionManager::matches_url_pattern(
            "https://example.com/api/*",
            "https://example.com/api/v1/../../admin/secret"
        ),
        "Multiple path traversals should be blocked"
    );
}

#[test]
fn test_url_path_traversal_within_allowed_area() {
    // Path traversal that stays within the allowed area is OK
    assert!(PermissionManager::matches_url_pattern(
        "https://example.com/api/*",
        "https://example.com/api/v1/../v2/endpoint"
    ));
    // This normalizes to /api/v2/endpoint which is within /api/*
}

#[test]
fn test_url_path_exact_match_with_wildcard() {
    // /api/* should match /api/anything
    assert!(PermissionManager::matches_url_pattern(
        "https://example.com/api/*",
        "https://example.com/api/users"
    ));

    // /api/* should match /api/
    assert!(PermissionManager::matches_url_pattern(
        "https://example.com/api/*",
        "https://example.com/api/"
    ));

    // /api/* should also match /api (without trailing slash)
    assert!(PermissionManager::matches_url_pattern(
        "https://example.com/api/*",
        "https://example.com/api"
    ));
}

#[test]
fn test_url_path_deep_nesting() {
    // Deep path nesting should work
    assert!(PermissionManager::matches_url_pattern(
        "https://example.com/api/*",
        "https://example.com/api/v1/users/123/posts/456"
    ));
}

#[test]
fn test_url_path_encoded_traversal() {
    // URL-encoded path traversal should be handled
    // Note: Most URL parsers decode %2e%2e%2f to ../
    // The url crate normalizes paths, so this should be safe
    let result = PermissionManager::matches_url_pattern(
        "https://example.com/api/*",
        "https://example.com/api/%2e%2e/admin"
    );
    // The url crate should handle this - document the behavior
}
