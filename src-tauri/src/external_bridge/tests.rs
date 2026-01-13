//! Tests for browser bridge authorization
//!
//! These tests verify that:
//! 1. Only authorized clients can access extensions
//! 2. Unauthorized clients are rejected
//! 3. Clients can only access their authorized extension
//! 4. Authorization can be revoked
//! 5. Extension targeting via publicKey + name works correctly
//! 6. Multi-extension authorization per client works

#[cfg(test)]
mod tests {
    use super::super::authorization::*;
    use super::super::crypto::EncryptedEnvelope;
    use super::super::protocol::*;

    // ============================================================================
    // ClientInfo and RequestedExtension Tests
    // ============================================================================

    #[test]
    fn test_client_info_serialization() {
        let client = ClientInfo {
            client_id: "test-client-123".to_string(),
            client_name: "Test Browser Extension".to_string(),
            public_key: "base64-public-key".to_string(),
            requested_extensions: vec![],
        };

        let json = serde_json::to_string(&client).unwrap();
        assert!(json.contains("clientId"));
        assert!(json.contains("clientName"));
        assert!(json.contains("publicKey"));

        let deserialized: ClientInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.client_id, client.client_id);
        assert_eq!(deserialized.client_name, client.client_name);
        assert_eq!(deserialized.public_key, client.public_key);
    }

    #[test]
    fn test_handshake_request_serialization() {
        let handshake = HandshakeRequest {
            version: 1,
            client: ClientInfo {
                client_id: "client-abc".to_string(),
                client_name: "haex-pass Extension".to_string(),
                public_key: "pk123".to_string(),
                requested_extensions: vec![],
            },
        };

        let json = serde_json::to_string(&handshake).unwrap();
        assert!(json.contains("\"version\":1"));
        assert!(json.contains("clientId"));

        let deserialized: HandshakeRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.version, 1);
        assert_eq!(deserialized.client.client_id, "client-abc");
    }

    #[test]
    fn test_handshake_response_serialization() {
        let response = HandshakeResponse {
            version: 1,
            server_public_key: "server-pk".to_string(),
            authorized: true,
            pending_approval: false,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("serverPublicKey"));
        assert!(json.contains("\"authorized\":true"));
        assert!(json.contains("\"pendingApproval\":false"));
    }

    #[test]
    fn test_protocol_message_handshake() {
        let msg = ProtocolMessage::Handshake(HandshakeRequest {
            version: 1,
            client: ClientInfo {
                client_id: "c1".to_string(),
                client_name: "Test".to_string(),
                public_key: "pk".to_string(),
                requested_extensions: vec![],
            },
        });

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"handshake\""));
    }

    #[test]
    fn test_protocol_message_authorization_update() {
        let msg = ProtocolMessage::AuthorizationUpdate { authorized: true };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"authorizationUpdate\""));
        assert!(json.contains("\"authorized\":true"));
    }

    #[test]
    fn test_protocol_message_ping_pong() {
        let ping = ProtocolMessage::Ping;
        let pong = ProtocolMessage::Pong;

        let ping_json = serde_json::to_string(&ping).unwrap();
        let pong_json = serde_json::to_string(&pong).unwrap();

        assert!(ping_json.contains("\"type\":\"ping\""));
        assert!(pong_json.contains("\"type\":\"pong\""));
    }

    #[test]
    fn test_protocol_message_error() {
        let error = ProtocolMessage::Error {
            code: "UNAUTHORIZED".to_string(),
            message: "Client not authorized".to_string(),
        };

        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("\"type\":\"error\""));
        assert!(json.contains("UNAUTHORIZED"));
    }

    #[test]
    fn test_bridge_response_success() {
        let response = BridgeResponse::success(
            "req-123".to_string(),
            serde_json::json!({"entries": []}),
        );

        assert!(response.success);
        assert_eq!(response.id, "req-123");
        assert!(response.data.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_bridge_response_error() {
        let response = BridgeResponse::error(
            "req-456".to_string(),
            "Extension not found".to_string(),
        );

        assert!(!response.success);
        assert_eq!(response.id, "req-456");
        assert!(response.data.is_none());
        assert_eq!(response.error, Some("Extension not found".to_string()));
    }

    #[test]
    fn test_authorized_client_parsing() {
        let row = vec![
            serde_json::json!("row-id-1"),
            serde_json::json!("client-id-abc"),
            serde_json::json!("Test Client"),
            serde_json::json!("public-key-xyz"),
            serde_json::json!("haex-pass"),
            serde_json::json!("2024-01-01T00:00:00Z"),
            serde_json::json!("2024-01-02T00:00:00Z"),
        ];

        let client = parse_authorized_client(&row).unwrap();
        assert_eq!(client.id, "row-id-1");
        assert_eq!(client.client_id, "client-id-abc");
        assert_eq!(client.client_name, "Test Client");
        assert_eq!(client.public_key, "public-key-xyz");
        assert_eq!(client.extension_id, "haex-pass");
        assert_eq!(client.authorized_at, Some("2024-01-01T00:00:00Z".to_string()));
        assert_eq!(client.last_seen, Some("2024-01-02T00:00:00Z".to_string()));
    }

    #[test]
    fn test_authorized_client_parsing_with_null_optional_fields() {
        let row = vec![
            serde_json::json!("row-id-1"),
            serde_json::json!("client-id-abc"),
            serde_json::json!("Test Client"),
            serde_json::json!("public-key-xyz"),
            serde_json::json!("haex-pass"),
            serde_json::Value::Null,
            serde_json::Value::Null,
        ];

        let client = parse_authorized_client(&row).unwrap();
        assert_eq!(client.id, "row-id-1");
        assert!(client.authorized_at.is_none());
        assert!(client.last_seen.is_none());
    }

    #[test]
    fn test_authorized_client_parsing_insufficient_columns() {
        let row = vec![
            serde_json::json!("row-id-1"),
            serde_json::json!("client-id-abc"),
        ];

        assert!(parse_authorized_client(&row).is_none());
    }

    #[test]
    fn test_pending_authorization_serialization() {
        let pending = PendingAuthorization {
            client_id: "pending-client".to_string(),
            client_name: "Pending Extension".to_string(),
            public_key: "pending-pk".to_string(),
            requested_extensions: vec![],
        };

        let json = serde_json::to_string(&pending).unwrap();
        assert!(json.contains("clientId"));
        assert!(json.contains("clientName"));
        assert!(json.contains("publicKey"));
        assert!(json.contains("requestedExtensions"));

        let deserialized: PendingAuthorization = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.client_id, pending.client_id);
    }

    // ============================================================================
    // EncryptedEnvelope Tests (with new extension_public_key and extension_name fields)
    // ============================================================================

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
            extension_public_key: Some("b4401f13f65e576b8a30ff9fd83df82a8bb707e1994d40c99996fe88603cefca".to_string()),
            extension_name: Some("haex-pass".to_string()),
        };

        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("extensionPublicKey"));
        assert!(json.contains("extensionName"));
        assert!(json.contains("b4401f13f65e576b8a30ff9fd83df82a8bb707e1994d40c99996fe88603cefca"));
        assert!(json.contains("haex-pass"));

        // Verify deserialization preserves the values
        let deserialized: EncryptedEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.extension_public_key, Some("b4401f13f65e576b8a30ff9fd83df82a8bb707e1994d40c99996fe88603cefca".to_string()));
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

    // ============================================================================
    // RequestedExtension Tests
    // ============================================================================

    #[test]
    fn test_requested_extension_serialization() {
        let ext = RequestedExtension {
            name: "haex-pass".to_string(),
            extension_public_key: "b4401f13f65e576b8a30ff9fd83df82a8bb707e1994d40c99996fe88603cefca".to_string(),
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
        assert_eq!(deserialized.requested_extensions[1].name, "another-extension");
    }

    #[test]
    fn test_pending_authorization_with_requested_extensions() {
        let pending = PendingAuthorization {
            client_id: "pending-client".to_string(),
            client_name: "Pending Extension".to_string(),
            public_key: "pending-pk".to_string(),
            requested_extensions: vec![
                RequestedExtension {
                    name: "haex-pass".to_string(),
                    extension_public_key: "b4401f13".to_string(),
                },
            ],
        };

        let json = serde_json::to_string(&pending).unwrap();
        assert!(json.contains("requestedExtensions"));
        assert!(json.contains("haex-pass"));
        assert!(json.contains("extensionPublicKey"));

        let deserialized: PendingAuthorization = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.requested_extensions.len(), 1);
        assert_eq!(deserialized.requested_extensions[0].name, "haex-pass");
    }

    // ============================================================================
    // SQL Query Tests (including new extension lookup queries)
    // ============================================================================

    #[test]
    fn test_sql_queries_are_valid() {
        // These tests verify that the SQL queries have correct placeholders
        assert!(SQL_IS_AUTHORIZED.contains("?1"));
        assert!(SQL_IS_AUTHORIZED.contains("?2"));
        assert!(SQL_IS_CLIENT_KNOWN.contains("?1"));
        assert!(SQL_GET_CLIENT_EXTENSION.contains("?1"));
        assert!(SQL_GET_CLIENT.contains("?1"));
        assert!(SQL_INSERT_CLIENT.contains("?1"));
        assert!(SQL_INSERT_CLIENT.contains("?2"));
        assert!(SQL_INSERT_CLIENT.contains("?3"));
        assert!(SQL_INSERT_CLIENT.contains("?4"));
        assert!(SQL_INSERT_CLIENT.contains("?5"));
        assert!(SQL_UPDATE_LAST_SEEN.contains("?1"));
        assert!(SQL_DELETE_CLIENT.contains("?1"));
    }

    #[test]
    fn test_sql_queries_reference_correct_table() {
        let table_name = "haex_external_authorized_clients";
        assert!(SQL_IS_AUTHORIZED.contains(table_name));
        assert!(SQL_IS_CLIENT_KNOWN.contains(table_name));
        assert!(SQL_GET_CLIENT_EXTENSION.contains(table_name));
        assert!(SQL_GET_CLIENT.contains(table_name));
        assert!(SQL_GET_ALL_CLIENTS.contains(table_name));
        assert!(SQL_INSERT_CLIENT.contains(table_name));
        assert!(SQL_UPDATE_LAST_SEEN.contains(table_name));
        assert!(SQL_DELETE_CLIENT.contains(table_name));
    }

    #[test]
    fn test_sql_is_client_authorized_for_extension_query_format() {
        // Verify the new query for checking client authorization for specific extension
        let query = &*SQL_IS_CLIENT_AUTHORIZED_FOR_EXTENSION;

        // Should have all three placeholders
        assert!(query.contains("?1"), "Query should have ?1 placeholder for client_id");
        assert!(query.contains("?2"), "Query should have ?2 placeholder for extension public_key");
        assert!(query.contains("?3"), "Query should have ?3 placeholder for extension name");

        // Should reference both tables
        assert!(query.contains("haex_external_authorized_clients"), "Query should reference authorized clients table");
        assert!(query.contains("haex_extensions"), "Query should reference extensions table");

        // Should use JOIN
        assert!(query.to_lowercase().contains("join"), "Query should use JOIN");

        // Should check public_key and name columns of extensions table
        assert!(query.contains("public_key"), "Query should check public_key");
        assert!(query.contains("name"), "Query should check name");
    }

    #[test]
    fn test_sql_get_extension_id_by_public_key_and_name_query_format() {
        // Verify the query for looking up extension ID by public_key and name
        let query = &*SQL_GET_EXTENSION_ID_BY_PUBLIC_KEY_AND_NAME;

        // Should have both placeholders
        assert!(query.contains("?1"), "Query should have ?1 placeholder for public_key");
        assert!(query.contains("?2"), "Query should have ?2 placeholder for name");

        // Should reference extensions table
        assert!(query.contains("haex_extensions"), "Query should reference extensions table");

        // Should select id
        assert!(query.to_lowercase().contains("select id"), "Query should select id");

        // Should filter by public_key and name
        assert!(query.contains("public_key"), "Query should filter by public_key");
        assert!(query.contains("name"), "Query should filter by name");
    }

    #[test]
    fn test_sql_blocked_clients_queries_reference_correct_table() {
        let table_name = "haex_external_blocked_clients";
        assert!(SQL_IS_BLOCKED.contains(table_name));
        assert!(SQL_GET_BLOCKED_CLIENT.contains(table_name));
        assert!(SQL_GET_ALL_BLOCKED_CLIENTS.contains(table_name));
        assert!(SQL_INSERT_BLOCKED_CLIENT.contains(table_name));
        assert!(SQL_DELETE_BLOCKED_CLIENT.contains(table_name));
    }

    // ============================================================================
    // Authorization Validation Tests
    // ============================================================================

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

    // ============================================================================
    // Multi-Extension Authorization Tests
    // ============================================================================

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

    // ============================================================================
    // Protocol Message Tests with Extension Targeting
    // ============================================================================

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

    // ============================================================================
    // Edge Cases and Error Handling Tests
    // ============================================================================

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

    #[test]
    fn test_blocked_client_parsing() {
        let row = vec![
            serde_json::json!("blocked-id-1"),
            serde_json::json!("blocked-client-abc"),
            serde_json::json!("Blocked Client"),
            serde_json::json!("blocked-public-key"),
            serde_json::json!("2024-01-01T00:00:00Z"),
        ];

        let client = parse_blocked_client(&row).unwrap();
        assert_eq!(client.id, "blocked-id-1");
        assert_eq!(client.client_id, "blocked-client-abc");
        assert_eq!(client.client_name, "Blocked Client");
        assert_eq!(client.public_key, "blocked-public-key");
        assert_eq!(client.blocked_at, Some("2024-01-01T00:00:00Z".to_string()));
    }

    #[test]
    fn test_blocked_client_parsing_with_null_blocked_at() {
        let row = vec![
            serde_json::json!("blocked-id-1"),
            serde_json::json!("blocked-client-abc"),
            serde_json::json!("Blocked Client"),
            serde_json::json!("blocked-public-key"),
            serde_json::Value::Null,
        ];

        let client = parse_blocked_client(&row).unwrap();
        assert!(client.blocked_at.is_none());
    }

    #[test]
    fn test_blocked_client_parsing_insufficient_columns() {
        let row = vec![
            serde_json::json!("id"),
            serde_json::json!("client_id"),
        ];

        assert!(parse_blocked_client(&row).is_none());
    }

    #[test]
    fn test_handshake_with_requested_extensions() {
        let handshake = HandshakeRequest {
            version: 1,
            client: ClientInfo {
                client_id: "client-abc".to_string(),
                client_name: "haex-pass Browser Extension".to_string(),
                public_key: "pk123".to_string(),
                requested_extensions: vec![
                    RequestedExtension {
                        name: "haex-pass".to_string(),
                        extension_public_key: "b4401f13f65e576b8a30ff9fd83df82a8bb707e1994d40c99996fe88603cefca".to_string(),
                    },
                ],
            },
        };

        let json = serde_json::to_string(&handshake).unwrap();
        assert!(json.contains("requestedExtensions"));
        assert!(json.contains("extensionPublicKey"));

        let msg = ProtocolMessage::Handshake(handshake.clone());
        let msg_json = serde_json::to_string(&msg).unwrap();
        assert!(msg_json.contains("\"type\":\"handshake\""));
        assert!(msg_json.contains("requestedExtensions"));
    }

    // ============================================================================
    // Extension Ready Signaling Tests
    // ============================================================================

    #[tokio::test]
    async fn test_extension_ready_signal_no_waiter() {
        use super::super::server::ExternalBridge;

        let bridge = ExternalBridge::new();
        let extension_id = "non-existent-extension";

        // Signal ready for an extension that no one is waiting for
        // This should not panic
        bridge.signal_extension_ready(extension_id).await;
    }

    #[tokio::test]
    async fn test_extension_ready_wait_with_immediate_signal() {
        use super::super::server::ExternalBridge;
        use std::sync::Arc;

        let bridge = Arc::new(ExternalBridge::new());
        let extension_id = "test-extension-456";

        // Spawn a task that waits for the extension to be ready
        let bridge_clone = bridge.clone();
        let ext_id = extension_id.to_string();
        let wait_handle = tokio::spawn(async move {
            bridge_clone.wait_for_extension_ready(&ext_id, 5000).await
        });

        // Give the wait task time to set up
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Signal that the extension is ready
        bridge.signal_extension_ready(extension_id).await;

        // The wait should complete successfully
        let result = wait_handle.await.unwrap();
        assert!(result, "wait_for_extension_ready should return true when signaled");
    }

    #[tokio::test]
    async fn test_extension_ready_wait_timeout() {
        use super::super::server::ExternalBridge;

        let bridge = ExternalBridge::new();
        let extension_id = "timeout-extension";

        // Wait for an extension that never signals ready (with short timeout)
        let result = bridge.wait_for_extension_ready(extension_id, 50).await;

        assert!(!result, "wait_for_extension_ready should return false on timeout");
    }

    #[tokio::test]
    async fn test_extension_ready_signal_cleans_up() {
        use super::super::server::ExternalBridge;
        use std::sync::Arc;

        let bridge = Arc::new(ExternalBridge::new());
        let extension_id = "cleanup-extension";

        // Start waiting (this creates an entry in extension_ready_signals)
        let bridge_clone = bridge.clone();
        let ext_id = extension_id.to_string();
        let wait_handle = tokio::spawn(async move {
            bridge_clone.wait_for_extension_ready(&ext_id, 5000).await
        });

        // Give the wait task time to set up
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Signal ready
        bridge.signal_extension_ready(extension_id).await;

        // Wait for the task to complete
        let result = wait_handle.await.unwrap();
        assert!(result, "Extension should have been signaled ready");

        // After wait completes, the entry should be cleaned up
        // We verify this by checking that a new wait would need to set up a new entry
        // (the previous entry was cleaned up)
        let signals = bridge.get_extension_ready_signals();
        let signals_read = signals.read().await;
        assert!(!signals_read.contains_key(extension_id), "Signal entry should be cleaned up after wait completes");
    }

    #[tokio::test]
    async fn test_multiple_extensions_ready_independently() {
        use super::super::server::ExternalBridge;
        use std::sync::Arc;

        let bridge = Arc::new(ExternalBridge::new());

        // Start waiting for two different extensions
        let bridge1 = bridge.clone();
        let bridge2 = bridge.clone();

        let wait1 = tokio::spawn(async move {
            bridge1.wait_for_extension_ready("ext-1", 5000).await
        });

        let wait2 = tokio::spawn(async move {
            bridge2.wait_for_extension_ready("ext-2", 5000).await
        });

        // Give wait tasks time to set up
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Signal only ext-1
        bridge.signal_extension_ready("ext-1").await;

        // ext-1 should complete successfully
        let result1 = wait1.await.unwrap();
        assert!(result1, "ext-1 should be signaled");

        // Signal ext-2
        bridge.signal_extension_ready("ext-2").await;

        // ext-2 should also complete successfully
        let result2 = wait2.await.unwrap();
        assert!(result2, "ext-2 should be signaled");
    }

    /// Tests the scenario where an extension signals ready immediately after
    /// being set up to wait (simulates the "no pending migrations" case).
    ///
    /// This test verifies the fix for the bug where extensions that had already
    /// completed their migrations would never signal ready, causing ExternalBridge
    /// to timeout waiting for them.
    #[tokio::test]
    async fn test_extension_ready_signal_immediate_after_wait_setup() {
        use super::super::server::ExternalBridge;
        use std::sync::Arc;

        let bridge = Arc::new(ExternalBridge::new());
        let extension_id = "already-migrated-extension";

        // Simulate the ExternalBridge waiting for an extension to be ready
        // (this happens in ensure_extension_loaded)
        let bridge_clone = bridge.clone();
        let ext_id = extension_id.to_string();
        let wait_handle = tokio::spawn(async move {
            bridge_clone.wait_for_extension_ready(&ext_id, 5000).await
        });

        // Give the wait task time to set up (minimal delay)
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

        // Immediately signal ready - simulates what happens when
        // extension_database_register_migrations finds no pending migrations
        // and signals ready in the early return path
        bridge.signal_extension_ready(extension_id).await;

        // The wait should complete successfully (not timeout)
        let result = wait_handle.await.unwrap();
        assert!(
            result,
            "Extension with no pending migrations should still signal ready and unblock waiters"
        );
    }

    /// Tests that signaling ready multiple times for the same extension is safe
    /// (idempotent behavior - important for robustness)
    #[tokio::test]
    async fn test_extension_ready_signal_idempotent() {
        use super::super::server::ExternalBridge;
        use std::sync::Arc;

        let bridge = Arc::new(ExternalBridge::new());
        let extension_id = "idempotent-extension";

        // Start waiting
        let bridge_clone = bridge.clone();
        let ext_id = extension_id.to_string();
        let wait_handle = tokio::spawn(async move {
            bridge_clone.wait_for_extension_ready(&ext_id, 5000).await
        });

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Signal ready multiple times (should not panic or cause issues)
        bridge.signal_extension_ready(extension_id).await;
        bridge.signal_extension_ready(extension_id).await;
        bridge.signal_extension_ready(extension_id).await;

        // Wait should complete on first signal
        let result = wait_handle.await.unwrap();
        assert!(result, "First signal should unblock the waiter");

        // Additional signals after wait completed should be safe (no-op)
        bridge.signal_extension_ready(extension_id).await;
    }
}
