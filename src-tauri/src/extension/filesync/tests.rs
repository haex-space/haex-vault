// src-tauri/src/extension/filesync/tests.rs
//!
//! Tests for filesync module (types, helpers)
//!
//! Note: encryption tests are in encryption.rs itself

#[cfg(test)]
mod types_tests {
    use crate::extension::filesync::types::*;

    // ============================================================================
    // FileSyncState Tests
    // ============================================================================

    #[test]
    fn test_file_sync_state_serialization() {
        let states = [
            (FileSyncState::Synced, "\"synced\""),
            (FileSyncState::Syncing, "\"syncing\""),
            (FileSyncState::LocalOnly, "\"localOnly\""),
            (FileSyncState::RemoteOnly, "\"remoteOnly\""),
            (FileSyncState::Conflict, "\"conflict\""),
            (FileSyncState::Error, "\"error\""),
        ];

        for (state, expected_json) in states {
            let json = serde_json::to_string(&state).unwrap();
            assert_eq!(json, expected_json, "Failed for {:?}", state);
        }
    }

    #[test]
    fn test_file_sync_state_deserialization() {
        let cases = [
            ("\"synced\"", FileSyncState::Synced),
            ("\"syncing\"", FileSyncState::Syncing),
            ("\"localOnly\"", FileSyncState::LocalOnly),
            ("\"remoteOnly\"", FileSyncState::RemoteOnly),
            ("\"conflict\"", FileSyncState::Conflict),
            ("\"error\"", FileSyncState::Error),
        ];

        for (json, expected) in cases {
            let state: FileSyncState = serde_json::from_str(json).unwrap();
            assert_eq!(state, expected, "Failed for {}", json);
        }
    }

    // ============================================================================
    // StorageBackendType Tests
    // ============================================================================

    #[test]
    fn test_storage_backend_type_serialization() {
        let types = [
            (StorageBackendType::S3, "\"s3\""),
            (StorageBackendType::R2, "\"r2\""),
            (StorageBackendType::Minio, "\"minio\""),
            (StorageBackendType::GDrive, "\"gdrive\""),
            (StorageBackendType::Dropbox, "\"dropbox\""),
        ];

        for (backend_type, expected_json) in types {
            let json = serde_json::to_string(&backend_type).unwrap();
            assert_eq!(json, expected_json, "Failed for {:?}", backend_type);
        }
    }

    #[test]
    fn test_storage_backend_type_deserialization() {
        let cases = [
            ("\"s3\"", StorageBackendType::S3),
            ("\"r2\"", StorageBackendType::R2),
            ("\"minio\"", StorageBackendType::Minio),
            ("\"gdrive\"", StorageBackendType::GDrive),
            ("\"dropbox\"", StorageBackendType::Dropbox),
        ];

        for (json, expected) in cases {
            let backend_type: StorageBackendType = serde_json::from_str(json).unwrap();
            assert_eq!(backend_type, expected, "Failed for {}", json);
        }
    }

    #[test]
    fn test_storage_backend_type_display() {
        assert_eq!(format!("{}", StorageBackendType::S3), "s3");
        assert_eq!(format!("{}", StorageBackendType::R2), "r2");
        assert_eq!(format!("{}", StorageBackendType::Minio), "minio");
        assert_eq!(format!("{}", StorageBackendType::GDrive), "gdrive");
        assert_eq!(format!("{}", StorageBackendType::Dropbox), "dropbox");
    }

    // ============================================================================
    // SyncDirection Tests
    // ============================================================================

    #[test]
    fn test_sync_direction_serialization() {
        let directions = [
            (SyncDirection::Up, "\"up\""),
            (SyncDirection::Down, "\"down\""),
            (SyncDirection::Both, "\"both\""),
        ];

        for (direction, expected_json) in directions {
            let json = serde_json::to_string(&direction).unwrap();
            assert_eq!(json, expected_json, "Failed for {:?}", direction);
        }
    }

    #[test]
    fn test_sync_direction_deserialization() {
        let cases = [
            ("\"up\"", SyncDirection::Up),
            ("\"down\"", SyncDirection::Down),
            ("\"both\"", SyncDirection::Both),
        ];

        for (json, expected) in cases {
            let direction: SyncDirection = serde_json::from_str(json).unwrap();
            assert_eq!(direction, expected, "Failed for {}", json);
        }
    }

    // ============================================================================
    // ConflictResolution Tests
    // ============================================================================

    #[test]
    fn test_conflict_resolution_serialization() {
        let resolutions = [
            (ConflictResolution::Local, "\"local\""),
            (ConflictResolution::Remote, "\"remote\""),
            (ConflictResolution::KeepBoth, "\"keepBoth\""),
        ];

        for (resolution, expected_json) in resolutions {
            let json = serde_json::to_string(&resolution).unwrap();
            assert_eq!(json, expected_json, "Failed for {:?}", resolution);
        }
    }

    // ============================================================================
    // Request/Response Type Tests
    // ============================================================================

    #[test]
    fn test_create_space_request() {
        let json = r#"{"name": "My Space"}"#;
        let request: CreateSpaceRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.name, "My Space");
    }

    #[test]
    fn test_list_files_request_minimal() {
        let json = r#"{"spaceId": "space-123"}"#;
        let request: ListFilesRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.space_id, "space-123");
        assert!(request.path.is_none());
        assert!(request.recursive.is_none());
    }

    #[test]
    fn test_list_files_request_full() {
        let json = r#"{
            "spaceId": "space-123",
            "path": "/documents",
            "recursive": true
        }"#;
        let request: ListFilesRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.space_id, "space-123");
        assert_eq!(request.path, Some("/documents".to_string()));
        assert_eq!(request.recursive, Some(true));
    }

    #[test]
    fn test_upload_file_request_minimal() {
        let json = r#"{
            "spaceId": "space-123",
            "localPath": "/home/user/file.txt"
        }"#;
        let request: UploadFileRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.space_id, "space-123");
        assert_eq!(request.local_path, "/home/user/file.txt");
        assert!(request.remote_path.is_none());
        assert!(request.backend_ids.is_none());
    }

    #[test]
    fn test_upload_file_request_full() {
        let json = r#"{
            "spaceId": "space-123",
            "localPath": "/home/user/file.txt",
            "remotePath": "documents/file.txt",
            "backendIds": ["backend-1", "backend-2"]
        }"#;
        let request: UploadFileRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.space_id, "space-123");
        assert_eq!(request.local_path, "/home/user/file.txt");
        assert_eq!(request.remote_path, Some("documents/file.txt".to_string()));
        assert_eq!(
            request.backend_ids,
            Some(vec!["backend-1".to_string(), "backend-2".to_string()])
        );
    }

    #[test]
    fn test_download_file_request() {
        let json = r#"{
            "fileId": "file-456",
            "localPath": "/home/user/downloads/file.txt"
        }"#;
        let request: DownloadFileRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.file_id, "file-456");
        assert_eq!(request.local_path, "/home/user/downloads/file.txt");
    }

    #[test]
    fn test_add_sync_rule_request_minimal() {
        let json = r#"{
            "spaceId": "space-123",
            "localPath": "/home/user/sync",
            "backendIds": ["backend-1"]
        }"#;
        let request: AddSyncRuleRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.space_id, "space-123");
        assert_eq!(request.local_path, "/home/user/sync");
        assert_eq!(request.backend_ids, vec!["backend-1".to_string()]);
        assert!(request.direction.is_none());
    }

    #[test]
    fn test_add_sync_rule_request_with_direction() {
        let json = r#"{
            "spaceId": "space-123",
            "localPath": "/home/user/sync",
            "backendIds": ["backend-1"],
            "direction": "both"
        }"#;
        let request: AddSyncRuleRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.direction, Some(SyncDirection::Both));
    }

    #[test]
    fn test_update_sync_rule_request() {
        let json = r#"{
            "ruleId": "rule-789",
            "backendIds": ["backend-new"],
            "direction": "up",
            "enabled": false
        }"#;
        let request: UpdateSyncRuleRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.rule_id, "rule-789");
        assert_eq!(
            request.backend_ids,
            Some(vec!["backend-new".to_string()])
        );
        assert_eq!(request.direction, Some(SyncDirection::Up));
        assert_eq!(request.enabled, Some(false));
    }

    #[test]
    fn test_resolve_conflict_request() {
        let json = r#"{
            "fileId": "file-conflict",
            "resolution": "keepBoth"
        }"#;
        let request: ResolveConflictRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.file_id, "file-conflict");
        assert!(matches!(request.resolution, ConflictResolution::KeepBoth));
    }

    #[test]
    fn test_scan_local_request() {
        let json = r#"{
            "ruleId": "rule-123",
            "subpath": "documents/subfolder"
        }"#;
        let request: ScanLocalRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.rule_id, "rule-123");
        assert_eq!(request.subpath, Some("documents/subfolder".to_string()));
    }

    // ============================================================================
    // BackendConfig Tests
    // ============================================================================

    #[test]
    fn test_backend_config_s3() {
        let json = r#"{
            "type": "s3",
            "endpoint": "https://s3.amazonaws.com",
            "region": "us-east-1",
            "bucket": "my-bucket",
            "accessKeyId": "AKIAIOSFODNN7EXAMPLE",
            "secretAccessKey": "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
        }"#;
        let config: BackendConfig = serde_json::from_str(json).unwrap();

        if let BackendConfig::S3(s3_config) = config {
            assert_eq!(s3_config.endpoint, Some("https://s3.amazonaws.com".to_string()));
            assert_eq!(s3_config.region, "us-east-1");
            assert_eq!(s3_config.bucket, "my-bucket");
            assert_eq!(s3_config.access_key_id, "AKIAIOSFODNN7EXAMPLE");
        } else {
            panic!("Expected S3 config");
        }
    }

    #[test]
    fn test_backend_config_r2() {
        let json = r#"{
            "type": "r2",
            "region": "auto",
            "bucket": "r2-bucket",
            "accessKeyId": "key",
            "secretAccessKey": "secret"
        }"#;
        let config: BackendConfig = serde_json::from_str(json).unwrap();

        assert!(matches!(config, BackendConfig::R2(_)));
    }

    #[test]
    fn test_backend_config_minio() {
        let json = r#"{
            "type": "minio",
            "endpoint": "http://localhost:9000",
            "region": "us-east-1",
            "bucket": "minio-bucket",
            "accessKeyId": "minioadmin",
            "secretAccessKey": "minioadmin"
        }"#;
        let config: BackendConfig = serde_json::from_str(json).unwrap();

        if let BackendConfig::Minio(minio_config) = config {
            assert_eq!(minio_config.endpoint, Some("http://localhost:9000".to_string()));
        } else {
            panic!("Expected Minio config");
        }
    }

    #[test]
    fn test_add_backend_request() {
        let json = r#"{
            "name": "My S3 Backend",
            "config": {
                "type": "s3",
                "region": "eu-west-1",
                "bucket": "my-bucket",
                "accessKeyId": "key",
                "secretAccessKey": "secret"
            }
        }"#;
        let request: AddBackendRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.name, "My S3 Backend");
        assert!(matches!(request.config, BackendConfig::S3(_)));
    }

    // ============================================================================
    // Info Type Serialization Tests
    // ============================================================================

    #[test]
    fn test_file_info_serialization() {
        let file_info = FileInfo {
            id: "file-123".to_string(),
            space_id: "space-456".to_string(),
            name: "test.txt".to_string(),
            path: "/documents/test.txt".to_string(),
            mime_type: Some("text/plain".to_string()),
            size: 1024,
            content_hash: "abc123".to_string(),
            is_directory: false,
            sync_state: FileSyncState::Synced,
            backends: vec!["backend-1".to_string()],
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-02T00:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&file_info).unwrap();

        assert!(json.contains("\"id\":\"file-123\""));
        assert!(json.contains("\"spaceId\":\"space-456\""));
        assert!(json.contains("\"name\":\"test.txt\""));
        assert!(json.contains("\"mimeType\":\"text/plain\""));
        assert!(json.contains("\"size\":1024"));
        assert!(json.contains("\"isDirectory\":false"));
        assert!(json.contains("\"syncState\":\"synced\""));
    }

    #[test]
    fn test_file_space_serialization() {
        let space = FileSpace {
            id: "space-123".to_string(),
            name: "Personal".to_string(),
            is_personal: true,
            file_count: 42,
            total_size: 1024 * 1024,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&space).unwrap();

        assert!(json.contains("\"isPersonal\":true"));
        assert!(json.contains("\"fileCount\":42"));
        assert!(json.contains("\"totalSize\":1048576"));
    }

    #[test]
    fn test_sync_rule_serialization() {
        let rule = SyncRule {
            id: "rule-123".to_string(),
            space_id: "space-456".to_string(),
            local_path: "/home/user/sync".to_string(),
            backend_ids: vec!["backend-1".to_string(), "backend-2".to_string()],
            direction: SyncDirection::Both,
            enabled: true,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&rule).unwrap();

        assert!(json.contains("\"localPath\":\"/home/user/sync\""));
        assert!(json.contains("\"direction\":\"both\""));
        assert!(json.contains("\"enabled\":true"));
    }

    #[test]
    fn test_sync_status_serialization() {
        let status = SyncStatus {
            is_syncing: true,
            pending_uploads: 5,
            pending_downloads: 3,
            last_sync: Some("2024-01-01T12:00:00Z".to_string()),
            errors: vec![SyncError {
                file_id: "file-err".to_string(),
                file_name: "error.txt".to_string(),
                error: "Network timeout".to_string(),
                timestamp: "2024-01-01T11:00:00Z".to_string(),
            }],
        };

        let json = serde_json::to_string(&status).unwrap();

        assert!(json.contains("\"isSyncing\":true"));
        assert!(json.contains("\"pendingUploads\":5"));
        assert!(json.contains("\"pendingDownloads\":3"));
        assert!(json.contains("\"Network timeout\""));
    }

    #[test]
    fn test_sync_progress_serialization() {
        let progress = SyncProgress {
            file_id: "file-123".to_string(),
            file_name: "large.zip".to_string(),
            bytes_transferred: 512 * 1024,
            total_bytes: 1024 * 1024,
            direction: SyncProgressDirection::Upload,
        };

        let json = serde_json::to_string(&progress).unwrap();

        assert!(json.contains("\"bytesTransferred\":524288"));
        assert!(json.contains("\"totalBytes\":1048576"));
        assert!(json.contains("\"direction\":\"upload\""));
    }

    #[test]
    fn test_local_file_info_serialization() {
        let file = LocalFileInfo {
            id: "hash-123".to_string(),
            name: "document.pdf".to_string(),
            path: "/home/user/sync/document.pdf".to_string(),
            relative_path: "document.pdf".to_string(),
            mime_type: Some("application/pdf".to_string()),
            size: 2048,
            is_directory: false,
            modified_at: Some("2024-01-15T10:30:00Z".to_string()),
        };

        let json = serde_json::to_string(&file).unwrap();

        assert!(json.contains("\"relativePath\":\"document.pdf\""));
        assert!(json.contains("\"mimeType\":\"application/pdf\""));
        assert!(json.contains("\"modifiedAt\":"));
    }

    #[test]
    fn test_storage_backend_info_serialization() {
        let info = StorageBackendInfo {
            id: "backend-123".to_string(),
            backend_type: StorageBackendType::S3,
            name: "AWS S3".to_string(),
            enabled: true,
            created_at: "2024-01-01T00:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&info).unwrap();

        assert!(json.contains("\"type\":\"s3\""));
        assert!(json.contains("\"enabled\":true"));
    }
}

#[cfg(test)]
mod helpers_tests {
    use crate::extension::filesync::helpers::*;
    use crate::extension::filesync::types::*;
    use serde_json::Value as JsonValue;

    // ============================================================================
    // Row Parsing Helper Tests
    // ============================================================================

    #[test]
    fn test_get_string() {
        let row = vec![
            JsonValue::String("hello".to_string()),
            JsonValue::String("world".to_string()),
            JsonValue::Null,
        ];

        assert_eq!(get_string(&row, 0), "hello");
        assert_eq!(get_string(&row, 1), "world");
        assert_eq!(get_string(&row, 2), ""); // Null -> empty string
        assert_eq!(get_string(&row, 99), ""); // Out of bounds -> empty string
    }

    #[test]
    fn test_get_string_non_string_values() {
        let row = vec![
            JsonValue::Number(42.into()),
            JsonValue::Bool(true),
            JsonValue::Array(vec![]),
        ];

        // Non-string values should return empty string
        assert_eq!(get_string(&row, 0), "");
        assert_eq!(get_string(&row, 1), "");
        assert_eq!(get_string(&row, 2), "");
    }

    #[test]
    fn test_get_bool() {
        let row = vec![
            JsonValue::Number(1.into()),
            JsonValue::Number(0.into()),
            JsonValue::Number(42.into()),
            JsonValue::Null,
        ];

        assert!(get_bool(&row, 0)); // 1 -> true
        assert!(!get_bool(&row, 1)); // 0 -> false
        assert!(get_bool(&row, 2)); // Non-zero -> true
        assert!(!get_bool(&row, 3)); // Null -> false
        assert!(!get_bool(&row, 99)); // Out of bounds -> false
    }

    #[test]
    fn test_get_u64() {
        let row = vec![
            JsonValue::Number(123.into()),
            JsonValue::Number(0.into()),
            JsonValue::Number(999999999.into()),
            JsonValue::Null,
        ];

        assert_eq!(get_u64(&row, 0), 123);
        assert_eq!(get_u64(&row, 1), 0);
        assert_eq!(get_u64(&row, 2), 999999999);
        assert_eq!(get_u64(&row, 3), 0); // Null -> 0
        assert_eq!(get_u64(&row, 99), 0); // Out of bounds -> 0
    }

    // ============================================================================
    // Type Conversion Helper Tests
    // ============================================================================

    #[test]
    fn test_sync_state_to_string() {
        assert_eq!(sync_state_to_string(&FileSyncState::Synced), "synced");
        assert_eq!(sync_state_to_string(&FileSyncState::Syncing), "syncing");
        assert_eq!(sync_state_to_string(&FileSyncState::LocalOnly), "local_only");
        assert_eq!(sync_state_to_string(&FileSyncState::RemoteOnly), "remote_only");
        assert_eq!(sync_state_to_string(&FileSyncState::Conflict), "conflict");
        assert_eq!(sync_state_to_string(&FileSyncState::Error), "error");
    }

    #[test]
    fn test_parse_sync_state() {
        assert_eq!(parse_sync_state("synced"), FileSyncState::Synced);
        assert_eq!(parse_sync_state("syncing"), FileSyncState::Syncing);
        assert_eq!(parse_sync_state("local_only"), FileSyncState::LocalOnly);
        assert_eq!(parse_sync_state("remote_only"), FileSyncState::RemoteOnly);
        assert_eq!(parse_sync_state("conflict"), FileSyncState::Conflict);
        assert_eq!(parse_sync_state("error"), FileSyncState::Error);
        // Unknown values default to Error
        assert_eq!(parse_sync_state("unknown"), FileSyncState::Error);
        assert_eq!(parse_sync_state(""), FileSyncState::Error);
    }

    #[test]
    fn test_parse_sync_state_roundtrip() {
        let states = [
            FileSyncState::Synced,
            FileSyncState::Syncing,
            FileSyncState::LocalOnly,
            FileSyncState::RemoteOnly,
            FileSyncState::Conflict,
            FileSyncState::Error,
        ];

        for state in states {
            let string = sync_state_to_string(&state);
            let parsed = parse_sync_state(string);
            assert_eq!(parsed, state);
        }
    }

    #[test]
    fn test_parse_backend_type() {
        assert_eq!(parse_backend_type("s3"), Some(StorageBackendType::S3));
        assert_eq!(parse_backend_type("r2"), Some(StorageBackendType::R2));
        assert_eq!(parse_backend_type("minio"), Some(StorageBackendType::Minio));
        assert_eq!(parse_backend_type("gdrive"), Some(StorageBackendType::GDrive));
        assert_eq!(parse_backend_type("dropbox"), Some(StorageBackendType::Dropbox));
        assert_eq!(parse_backend_type("unknown"), None);
        assert_eq!(parse_backend_type(""), None);
        assert_eq!(parse_backend_type("S3"), None); // Case sensitive
    }

    #[test]
    fn test_parse_sync_direction() {
        assert_eq!(parse_sync_direction("up"), SyncDirection::Up);
        assert_eq!(parse_sync_direction("down"), SyncDirection::Down);
        assert_eq!(parse_sync_direction("both"), SyncDirection::Both);
        // Unknown values default to Both
        assert_eq!(parse_sync_direction("unknown"), SyncDirection::Both);
        assert_eq!(parse_sync_direction(""), SyncDirection::Both);
    }

    #[test]
    fn test_sync_direction_to_string() {
        assert_eq!(sync_direction_to_string(&SyncDirection::Up), "up");
        assert_eq!(sync_direction_to_string(&SyncDirection::Down), "down");
        assert_eq!(sync_direction_to_string(&SyncDirection::Both), "both");
    }

    #[test]
    fn test_sync_direction_roundtrip() {
        let directions = [SyncDirection::Up, SyncDirection::Down, SyncDirection::Both];

        for direction in directions {
            let string = sync_direction_to_string(&direction);
            let parsed = parse_sync_direction(&string);
            assert_eq!(parsed, direction);
        }
    }

    // ============================================================================
    // Edge Cases
    // ============================================================================

    #[test]
    fn test_get_string_empty_row() {
        let row: Vec<JsonValue> = vec![];
        assert_eq!(get_string(&row, 0), "");
    }

    #[test]
    fn test_get_bool_empty_row() {
        let row: Vec<JsonValue> = vec![];
        assert!(!get_bool(&row, 0));
    }

    #[test]
    fn test_get_u64_empty_row() {
        let row: Vec<JsonValue> = vec![];
        assert_eq!(get_u64(&row, 0), 0);
    }

    #[test]
    fn test_get_string_with_special_chars() {
        let row = vec![
            JsonValue::String("hello\nworld".to_string()),
            JsonValue::String("path/to/file".to_string()),
            JsonValue::String("unicode: 日本語".to_string()),
        ];

        assert_eq!(get_string(&row, 0), "hello\nworld");
        assert_eq!(get_string(&row, 1), "path/to/file");
        assert_eq!(get_string(&row, 2), "unicode: 日本語");
    }

    #[test]
    fn test_get_u64_large_numbers() {
        let row = vec![JsonValue::Number(serde_json::Number::from(u64::MAX as i64))];
        // Note: This tests i64 max because JsonValue uses i64
        let value = get_u64(&row, 0);
        assert!(value > 0);
    }
}
