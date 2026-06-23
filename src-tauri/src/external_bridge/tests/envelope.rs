//! `EncryptedEnvelope` and `ProtocolMessage::Request/Response` tests
//! covering the `extension_public_key` / `extension_name` targeting fields.

use super::super::crypto::EncryptedEnvelope;
use super::super::protocol::*;

#[test]
fn test_encrypted_envelope_serialization_basic() {
    let envelope = EncryptedEnvelope {
        action: "test-action".to_string(),
        message: "encrypted-data".to_string(),
        iv: "iv-123".to_string(),
        client_id: "client-123".to_string(),
        public_key: "public-key".to_string(),
        extension_public_key: None,
        extension_name: None,
    };

    let json = serde_json::to_string(&envelope).unwrap();
    assert!(json.contains("action"));
    assert!(json.contains("message"));
    assert!(json.contains("iv"));
    assert!(json.contains("clientId"));
    assert!(json.contains("publicKey"));
}

#[test]
fn test_encrypted_envelope_with_extension_identifiers() {
    let envelope = EncryptedEnvelope {
        action: "get-logins".to_string(),
        message: "encrypted-payload".to_string(),
        iv: "random-iv-12".to_string(),
        client_id: "browser-ext-123".to_string(),
        public_key: "client-ephemeral-key".to_string(),
        extension_public_key: Some(
            "b4401f13f65e576b8a30ff9fd83df82a8bb707e1994d40c99996fe88603cefca".to_string(),
        ),
        extension_name: Some("haex-pass".to_string()),
    };

    let json = serde_json::to_string(&envelope).unwrap();
    assert!(json.contains("extensionPublicKey"));
    assert!(json.contains("extensionName"));
    assert!(json.contains("b4401f13f65e576b8a30ff9fd83df82a8bb707e1994d40c99996fe88603cefca"));
    assert!(json.contains("haex-pass"));

    // Verify deserialization preserves the values
    let deserialized: EncryptedEnvelope = serde_json::from_str(&json).unwrap();
    assert_eq!(
        deserialized.extension_public_key,
        Some("b4401f13f65e576b8a30ff9fd83df82a8bb707e1994d40c99996fe88603cefca".to_string())
    );
    assert_eq!(deserialized.extension_name, Some("haex-pass".to_string()));
}

#[test]
fn test_encrypted_envelope_deserialization_without_extension_fields() {
    // Test backward compatibility: old messages without extension fields should still deserialize
    let json = r#"{
        "action": "get-logins",
        "message": "encrypted",
        "iv": "iv123",
        "clientId": "client1",
        "publicKey": "pk123"
    }"#;

    let envelope: EncryptedEnvelope = serde_json::from_str(json).unwrap();
    assert_eq!(envelope.action, "get-logins");
    assert!(envelope.extension_public_key.is_none());
    assert!(envelope.extension_name.is_none());
}

#[test]
fn test_encrypted_envelope_extension_fields_default_to_none() {
    // Verify #[serde(default)] works correctly
    let json = r#"{
        "action": "test",
        "message": "msg",
        "iv": "iv",
        "clientId": "cid",
        "publicKey": "pk"
    }"#;

    let envelope: EncryptedEnvelope = serde_json::from_str(json).unwrap();
    assert_eq!(envelope.extension_public_key, None);
    assert_eq!(envelope.extension_name, None);
}

#[test]
fn test_protocol_message_request_with_extension_target() {
    let envelope = EncryptedEnvelope {
        action: "get-logins".to_string(),
        message: "base64-encrypted-data".to_string(),
        iv: "base64-iv".to_string(),
        client_id: "client-123".to_string(),
        public_key: "ephemeral-pk".to_string(),
        extension_public_key: Some("target-ext-pk".to_string()),
        extension_name: Some("haex-pass".to_string()),
    };

    let msg = ProtocolMessage::Request(envelope);
    let json = serde_json::to_string(&msg).unwrap();

    assert!(json.contains("\"type\":\"request\""));
    assert!(json.contains("extensionPublicKey"));
    assert!(json.contains("extensionName"));
    assert!(json.contains("target-ext-pk"));
    assert!(json.contains("haex-pass"));
}

#[test]
fn test_protocol_message_response_no_extension_fields_needed() {
    // Responses don't need extension fields - they're routed back via the request channel
    let envelope = EncryptedEnvelope {
        action: "get-logins".to_string(),
        message: "base64-encrypted-response".to_string(),
        iv: "base64-iv".to_string(),
        client_id: "".to_string(), // Server doesn't have client_id
        public_key: "server-ephemeral-pk".to_string(),
        extension_public_key: None,
        extension_name: None,
    };

    let msg = ProtocolMessage::Response(envelope);
    let json = serde_json::to_string(&msg).unwrap();

    assert!(json.contains("\"type\":\"response\""));
    // Extension fields should be null/absent
}
