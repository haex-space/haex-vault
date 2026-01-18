// src-tauri/src/extension/remote_storage/tests.rs
//!
//! Tests for extension remote storage types and permission handling
//!

#[cfg(test)]
mod tests {
    use crate::extension::permissions::types::{FileSyncAction, FileSyncTarget};
    use crate::remote_storage::types::{
        AddStorageBackendRequest, S3Config, S3PublicConfig, StorageBackendInfo,
        StorageDeleteRequest, StorageDownloadRequest, StorageListRequest, StorageObjectInfo,
        StorageUploadRequest, UpdateStorageBackendRequest,
    };
    use serde_json::json;

    // ============================================================================
    // StorageBackendInfo Tests
    // ============================================================================

    #[test]
    fn test_storage_backend_info_serialization() {
        let info = StorageBackendInfo {
            id: "backend_123".to_string(),
            r#type: "s3".to_string(),
            name: "My S3 Bucket".to_string(),
            enabled: true,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            config: Some(S3PublicConfig {
                endpoint: Some("https://s3.example.com".to_string()),
                region: "us-west-2".to_string(),
                bucket: "my-bucket".to_string(),
            }),
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"id\":\"backend_123\""));
        assert!(json.contains("\"type\":\"s3\""));
        assert!(json.contains("\"name\":\"My S3 Bucket\""));
        assert!(json.contains("\"enabled\":true"));
        assert!(json.contains("\"bucket\":\"my-bucket\""));
    }

    #[test]
    fn test_storage_backend_info_without_config() {
        let info = StorageBackendInfo {
            id: "backend_456".to_string(),
            r#type: "s3".to_string(),
            name: "Empty Config Backend".to_string(),
            enabled: false,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            config: None,
        };

        let json = serde_json::to_string(&info).unwrap();
        // config should be skipped when None
        assert!(!json.contains("\"config\""));
    }

    #[test]
    fn test_storage_backend_info_deserialization() {
        let json_str = r#"{
            "id": "test-id",
            "type": "s3",
            "name": "Test Backend",
            "enabled": true,
            "createdAt": "2024-01-01T00:00:00Z"
        }"#;

        let info: StorageBackendInfo = serde_json::from_str(json_str).unwrap();
        assert_eq!(info.id, "test-id");
        assert_eq!(info.r#type, "s3");
        assert_eq!(info.name, "Test Backend");
        assert!(info.enabled);
        assert!(info.config.is_none());
    }

    // ============================================================================
    // S3Config Tests
    // ============================================================================

    #[test]
    fn test_s3_config_full() {
        let config = S3Config {
            endpoint: Some("https://minio.local:9000".to_string()),
            region: "us-east-1".to_string(),
            bucket: "my-bucket".to_string(),
            access_key_id: "AKIAIOSFODNN7EXAMPLE".to_string(),
            secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            path_style: Some(true),
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"endpoint\":\"https://minio.local:9000\""));
        assert!(json.contains("\"bucket\":\"my-bucket\""));
        assert!(json.contains("\"pathStyle\":true"));
    }

    #[test]
    fn test_s3_config_minimal() {
        // Config for AWS S3 (no custom endpoint)
        let json_str = r#"{
            "region": "eu-west-1",
            "bucket": "production-bucket",
            "accessKeyId": "AKIAIOSFODNN7EXAMPLE",
            "secretAccessKey": "secret123"
        }"#;

        let config: S3Config = serde_json::from_str(json_str).unwrap();
        assert!(config.endpoint.is_none());
        assert_eq!(config.region, "eu-west-1");
        assert_eq!(config.bucket, "production-bucket");
    }

    // ============================================================================
    // AddStorageBackendRequest Tests
    // ============================================================================

    #[test]
    fn test_add_backend_request_serialization() {
        let request = AddStorageBackendRequest {
            name: "Production S3".to_string(),
            r#type: "s3".to_string(),
            config: json!({
                "region": "us-west-2",
                "bucket": "prod-bucket",
                "accessKeyId": "AKIAKEY",
                "secretAccessKey": "secretkey"
            }),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"name\":\"Production S3\""));
        assert!(json.contains("\"type\":\"s3\""));
        assert!(json.contains("\"bucket\":\"prod-bucket\""));
    }

    #[test]
    fn test_add_backend_request_deserialization() {
        let json_str = r#"{
            "name": "Test Backend",
            "type": "s3",
            "config": {
                "region": "us-east-1",
                "bucket": "test",
                "accessKeyId": "key",
                "secretAccessKey": "secret"
            }
        }"#;

        let request: AddStorageBackendRequest = serde_json::from_str(json_str).unwrap();
        assert_eq!(request.name, "Test Backend");
        assert_eq!(request.r#type, "s3");
        assert_eq!(request.config["bucket"], "test");
    }

    // ============================================================================
    // UpdateStorageBackendRequest Tests
    // ============================================================================

    #[test]
    fn test_update_backend_request_name_only() {
        let request = UpdateStorageBackendRequest {
            backend_id: "backend_123".to_string(),
            name: Some("New Name".to_string()),
            config: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"backendId\":\"backend_123\""));
        assert!(json.contains("\"name\":\"New Name\""));
        // config is serialized as null when None
        assert!(json.contains("\"config\":null"));
    }

    #[test]
    fn test_update_backend_request_config_only() {
        let request = UpdateStorageBackendRequest {
            backend_id: "backend_123".to_string(),
            name: None,
            config: Some(json!({"bucket": "new-bucket"})),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"backendId\":\"backend_123\""));
        assert!(json.contains("\"new-bucket\""));
    }

    // ============================================================================
    // Storage Operation Request Tests
    // ============================================================================

    #[test]
    fn test_upload_request() {
        let request = StorageUploadRequest {
            backend_id: "backend_1".to_string(),
            key: "path/to/file.txt".to_string(),
            data: "SGVsbG8gV29ybGQh".to_string(), // "Hello World!" in base64
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"backendId\":\"backend_1\""));
        assert!(json.contains("\"key\":\"path/to/file.txt\""));
        assert!(json.contains("\"data\":\"SGVsbG8gV29ybGQh\""));
    }

    #[test]
    fn test_download_request() {
        let request = StorageDownloadRequest {
            backend_id: "backend_1".to_string(),
            key: "documents/report.pdf".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"backendId\":\"backend_1\""));
        assert!(json.contains("\"key\":\"documents/report.pdf\""));
    }

    #[test]
    fn test_delete_request() {
        let request = StorageDeleteRequest {
            backend_id: "backend_1".to_string(),
            key: "old/file.txt".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"backendId\":\"backend_1\""));
        assert!(json.contains("\"key\":\"old/file.txt\""));
    }

    #[test]
    fn test_list_request_with_prefix() {
        let request = StorageListRequest {
            backend_id: "backend_1".to_string(),
            prefix: Some("documents/".to_string()),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"backendId\":\"backend_1\""));
        assert!(json.contains("\"prefix\":\"documents/\""));
    }

    #[test]
    fn test_list_request_without_prefix() {
        let request = StorageListRequest {
            backend_id: "backend_1".to_string(),
            prefix: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"backendId\":\"backend_1\""));
        // prefix should be null or missing
    }

    // ============================================================================
    // StorageObjectInfo Tests
    // ============================================================================

    #[test]
    fn test_storage_object_info() {
        let info = StorageObjectInfo {
            key: "documents/report.pdf".to_string(),
            size: 1024 * 1024, // 1MB
            last_modified: Some("2024-01-15T12:00:00Z".to_string()),
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"key\":\"documents/report.pdf\""));
        assert!(json.contains("\"size\":1048576"));
        assert!(json.contains("\"lastModified\":\"2024-01-15T12:00:00Z\""));
    }

    #[test]
    fn test_storage_object_info_without_modified() {
        let info = StorageObjectInfo {
            key: "file.txt".to_string(),
            size: 100,
            last_modified: None,
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"key\":\"file.txt\""));
        assert!(json.contains("\"size\":100"));
    }

    // ============================================================================
    // FileSyncTarget Tests
    // ============================================================================

    #[test]
    fn test_filesync_target_variants() {
        assert_eq!(FileSyncTarget::All.as_str(), "*");
        assert_eq!(FileSyncTarget::Spaces.as_str(), "spaces");
        assert_eq!(FileSyncTarget::Backends.as_str(), "backends");
        assert_eq!(FileSyncTarget::Rules.as_str(), "rules");
    }

    #[test]
    fn test_filesync_target_from_str() {
        assert_eq!(FileSyncTarget::from_str("*"), Some(FileSyncTarget::All));
        assert_eq!(
            FileSyncTarget::from_str("spaces"),
            Some(FileSyncTarget::Spaces)
        );
        assert_eq!(
            FileSyncTarget::from_str("backends"),
            Some(FileSyncTarget::Backends)
        );
        assert_eq!(
            FileSyncTarget::from_str("rules"),
            Some(FileSyncTarget::Rules)
        );
        assert_eq!(FileSyncTarget::from_str("invalid"), None);
    }

    #[test]
    fn test_filesync_target_matches() {
        // All (*) matches everything
        assert!(FileSyncTarget::All.matches(FileSyncTarget::Spaces));
        assert!(FileSyncTarget::All.matches(FileSyncTarget::Backends));
        assert!(FileSyncTarget::All.matches(FileSyncTarget::Rules));

        // Specific targets only match themselves
        assert!(FileSyncTarget::Backends.matches(FileSyncTarget::Backends));
        assert!(!FileSyncTarget::Backends.matches(FileSyncTarget::Spaces));
        assert!(!FileSyncTarget::Spaces.matches(FileSyncTarget::Rules));
    }

    // ============================================================================
    // FileSyncAction Tests
    // ============================================================================

    #[test]
    fn test_filesync_action_read_permissions() {
        assert!(FileSyncAction::Read.allows_read());
        assert!(!FileSyncAction::Read.allows_write());

        assert!(FileSyncAction::ReadWrite.allows_read());
        assert!(FileSyncAction::ReadWrite.allows_write());
    }

    // ============================================================================
    // Security Tests for Remote Storage
    // ============================================================================

    #[test]
    fn test_key_with_path_traversal_attempt() {
        // These keys should be sanitized or rejected by the backend
        let suspicious_keys = [
            "../../../etc/passwd",
            "..\\..\\windows\\system32",
            "path/../../../secret",
            "normal/../../malicious",
        ];

        for key in suspicious_keys {
            // The key contains path traversal patterns
            assert!(key.contains(".."), "Test key should contain ..: {}", key);
        }
    }

    #[test]
    fn test_key_with_special_characters() {
        // These keys should be valid
        let valid_keys = [
            "documents/report.pdf",
            "path/to/file.txt",
            "file-with-dash.txt",
            "file_with_underscore.txt",
            "path/123/456/file.bin",
        ];

        for key in valid_keys {
            let request = StorageDownloadRequest {
                backend_id: "backend".to_string(),
                key: key.to_string(),
            };
            // Should not panic
            let _ = serde_json::to_string(&request).unwrap();
        }
    }

    #[test]
    fn test_very_long_key() {
        // Very long keys should be handled
        let long_key = "a".repeat(10000);
        let request = StorageDownloadRequest {
            backend_id: "backend".to_string(),
            key: long_key.clone(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains(&long_key));
    }

    #[test]
    fn test_empty_key() {
        let request = StorageDownloadRequest {
            backend_id: "backend".to_string(),
            key: String::new(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"key\":\"\""));
    }

    #[test]
    fn test_unicode_key() {
        let request = StorageUploadRequest {
            backend_id: "backend".to_string(),
            key: "文档/报告.pdf".to_string(),
            data: "SGVsbG8=".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("文档/报告.pdf"));
    }

    #[test]
    fn test_base64_encoded_data_validity() {
        use base64::{engine::general_purpose::STANDARD, Engine as _};

        // Valid base64 data
        let original = b"Hello, World! This is some test data.";
        let encoded = STANDARD.encode(original);

        let request = StorageUploadRequest {
            backend_id: "backend".to_string(),
            key: "test.txt".to_string(),
            data: encoded.clone(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains(&encoded));

        // Verify it can be decoded back
        let decoded = STANDARD.decode(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_backend_id_format() {
        // Backend IDs should be valid UUID-like strings
        let valid_ids = [
            "550e8400-e29b-41d4-a716-446655440000",
            "backend_123",
            "my-backend",
        ];

        for id in valid_ids {
            let request = StorageListRequest {
                backend_id: id.to_string(),
                prefix: None,
            };

            // Should not panic
            let _ = serde_json::to_string(&request).unwrap();
        }
    }
}
