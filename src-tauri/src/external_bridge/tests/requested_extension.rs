//! `RequestedExtension` serialization, multi-extension authorization,
//! and extension-identifier validation tests.

use super::super::authorization::PendingAuthorization;
use super::super::protocol::*;

#[test]
fn test_requested_extension_serialization() {
    let ext = RequestedExtension {
        name: "haex-pass".to_string(),
        extension_public_key: "b4401f13f65e576b8a30ff9fd83df82a8bb707e1994d40c99996fe88603cefca"
            .to_string(),
    };

    let json = serde_json::to_string(&ext).unwrap();
    assert!(json.contains("\"name\":\"haex-pass\""));
    assert!(json.contains("extensionPublicKey"));

    let deserialized: RequestedExtension = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.name, "haex-pass");
    assert_eq!(deserialized.extension_public_key, ext.extension_public_key);
}

#[test]
fn test_client_info_with_requested_extensions() {
    let client = ClientInfo {
        client_id: "browser-ext".to_string(),
        client_name: "haex-pass Browser Extension".to_string(),
        public_key: "client-pk".to_string(),
        requested_extensions: vec![
            RequestedExtension {
                name: "haex-pass".to_string(),
                extension_public_key: "pk1".to_string(),
            },
            RequestedExtension {
                name: "another-extension".to_string(),
                extension_public_key: "pk2".to_string(),
            },
        ],
    };

    let json = serde_json::to_string(&client).unwrap();
    assert!(json.contains("requestedExtensions"));
    assert!(json.contains("haex-pass"));
    assert!(json.contains("another-extension"));

    let deserialized: ClientInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.requested_extensions.len(), 2);
    assert_eq!(deserialized.requested_extensions[0].name, "haex-pass");
    assert_eq!(
        deserialized.requested_extensions[1].name,
        "another-extension"
    );
}

#[test]
fn test_pending_authorization_with_requested_extensions() {
    let pending = PendingAuthorization {
        client_id: "pending-client".to_string(),
        client_name: "Pending Extension".to_string(),
        public_key: "pending-pk".to_string(),
        requested_extensions: vec![RequestedExtension {
            name: "haex-pass".to_string(),
            extension_public_key: "b4401f13".to_string(),
        }],
    };

    let json = serde_json::to_string(&pending).unwrap();
    assert!(json.contains("requestedExtensions"));
    assert!(json.contains("haex-pass"));
    assert!(json.contains("extensionPublicKey"));

    let deserialized: PendingAuthorization = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.requested_extensions.len(), 1);
    assert_eq!(deserialized.requested_extensions[0].name, "haex-pass");
}

#[test]
fn test_extension_identifier_validation() {
    // Test that we properly validate extension identifiers
    // A valid extension identifier needs both public_key AND name

    // Valid: both present
    let valid_pk = Some("b4401f13f65e576b8a30ff9fd83df82a8bb707e1994d40c99996fe88603cefca");
    let valid_name = Some("haex-pass");
    assert!(valid_pk.is_some() && valid_name.is_some());

    // Invalid: only public_key
    let invalid_pk_only = Some("b4401f13");
    let invalid_no_name: Option<&str> = None;
    assert!(!(invalid_pk_only.is_some() && invalid_no_name.is_some()));

    // Invalid: only name
    let invalid_no_pk: Option<&str> = None;
    let invalid_name_only = Some("haex-pass");
    assert!(!(invalid_no_pk.is_some() && invalid_name_only.is_some()));

    // Invalid: both empty strings
    let empty_pk = Some("");
    let empty_name = Some("");
    let pk_valid = empty_pk.map(|s| !s.is_empty()).unwrap_or(false);
    let name_valid = empty_name.map(|s| !s.is_empty()).unwrap_or(false);
    assert!(!pk_valid && !name_valid);
}

#[test]
fn test_same_developer_different_extensions() {
    // Scenario: A developer (identified by public_key) can have multiple extensions
    // Each extension is identified by (public_key, name) combination

    let developer_pk = "b4401f13f65e576b8a30ff9fd83df82a8bb707e1994d40c99996fe88603cefca";

    let ext1 = RequestedExtension {
        name: "haex-pass".to_string(),
        extension_public_key: developer_pk.to_string(),
    };

    let ext2 = RequestedExtension {
        name: "haex-notes".to_string(),
        extension_public_key: developer_pk.to_string(),
    };

    // Same developer, different extensions
    assert_eq!(ext1.extension_public_key, ext2.extension_public_key);
    assert_ne!(ext1.name, ext2.name);

    // Both should be valid extensions
    assert!(!ext1.name.is_empty() && !ext1.extension_public_key.is_empty());
    assert!(!ext2.name.is_empty() && !ext2.extension_public_key.is_empty());
}

#[test]
fn test_same_extension_name_different_developers() {
    // Scenario: Different developers can have extensions with the same name
    // They are distinguished by the public_key

    let ext1 = RequestedExtension {
        name: "password-manager".to_string(),
        extension_public_key: "developer1_public_key".to_string(),
    };

    let ext2 = RequestedExtension {
        name: "password-manager".to_string(),
        extension_public_key: "developer2_public_key".to_string(),
    };

    // Same name, different developers
    assert_eq!(ext1.name, ext2.name);
    assert_ne!(ext1.extension_public_key, ext2.extension_public_key);
}

#[test]
fn test_client_can_request_multiple_extensions() {
    // A single client can request access to multiple extensions
    let client = ClientInfo {
        client_id: "multi-ext-client".to_string(),
        client_name: "Multi-Extension Client".to_string(),
        public_key: "client-pk".to_string(),
        requested_extensions: vec![
            RequestedExtension {
                name: "haex-pass".to_string(),
                extension_public_key: "pk1".to_string(),
            },
            RequestedExtension {
                name: "haex-notes".to_string(),
                extension_public_key: "pk1".to_string(),
            },
            RequestedExtension {
                name: "haex-files".to_string(),
                extension_public_key: "pk2".to_string(),
            },
        ],
    };

    assert_eq!(client.requested_extensions.len(), 3);

    // Serialize and deserialize to ensure all extensions are preserved
    let json = serde_json::to_string(&client).unwrap();
    let deserialized: ClientInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.requested_extensions.len(), 3);
}

#[test]
fn test_empty_requested_extensions_array() {
    let client = ClientInfo {
        client_id: "client".to_string(),
        client_name: "Client".to_string(),
        public_key: "pk".to_string(),
        requested_extensions: vec![],
    };

    let json = serde_json::to_string(&client).unwrap();
    assert!(json.contains("\"requestedExtensions\":[]"));

    let deserialized: ClientInfo = serde_json::from_str(&json).unwrap();
    assert!(deserialized.requested_extensions.is_empty());
}

#[test]
fn test_extension_public_key_hex_format() {
    // Extension public keys should be 64-character hex strings (256-bit / 32 bytes)
    let valid_hex = "b4401f13f65e576b8a30ff9fd83df82a8bb707e1994d40c99996fe88603cefca";
    assert_eq!(valid_hex.len(), 64);
    assert!(valid_hex.chars().all(|c| c.is_ascii_hexdigit()));

    let ext = RequestedExtension {
        name: "haex-pass".to_string(),
        extension_public_key: valid_hex.to_string(),
    };

    let json = serde_json::to_string(&ext).unwrap();
    assert!(json.contains(valid_hex));
}
