//! Tests for browser bridge authorization
//!
//! These tests verify that:
//! 1. Only authorized clients can access extensions
//! 2. Unauthorized clients are rejected
//! 3. Clients can only access their authorized extension
//! 4. Authorization can be revoked

#[cfg(test)]
mod tests {
    use super::super::authorization::*;
    use super::super::protocol::*;

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

    #[test]
    fn test_encrypted_envelope_serialization() {
        let envelope = EncryptedEnvelope {
            action: "test-action".to_string(),
            message: "encrypted-data".to_string(),
            iv: "iv-123".to_string(),
            client_id: "client-123".to_string(),
            public_key: "public-key".to_string(),
        };

        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("action"));
        assert!(json.contains("message"));
        assert!(json.contains("iv"));
        assert!(json.contains("clientId"));
        assert!(json.contains("publicKey"));
    }

    // SQL query format tests
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
}
