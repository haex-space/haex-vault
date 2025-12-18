// src-tauri/src/extension/filesystem/tests.rs
//!
//! Tests for filesystem module (file_io and watcher types)
//!

#[cfg(test)]
mod file_io_tests {
    use crate::extension::filesystem::file_io::*;

    // ============================================================================
    // extract_filename Tests
    // ============================================================================

    #[test]
    fn test_extract_filename_unix_path() {
        assert_eq!(extract_filename("/home/user/documents/file.txt"), "file.txt");
        assert_eq!(extract_filename("/root/test.pdf"), "test.pdf");
        assert_eq!(extract_filename("/var/log/app.log"), "app.log");
    }

    #[test]
    fn test_extract_filename_nested_path() {
        assert_eq!(
            extract_filename("/home/user/projects/app/src/main.rs"),
            "main.rs"
        );
    }

    #[test]
    fn test_extract_filename_root_file() {
        assert_eq!(extract_filename("/file.txt"), "file.txt");
    }

    #[test]
    fn test_extract_filename_no_extension() {
        assert_eq!(extract_filename("/home/user/README"), "README");
        assert_eq!(extract_filename("/home/user/Makefile"), "Makefile");
    }

    #[test]
    fn test_extract_filename_hidden_file() {
        assert_eq!(extract_filename("/home/user/.gitignore"), ".gitignore");
        assert_eq!(extract_filename("/home/user/.env"), ".env");
    }

    #[test]
    fn test_extract_filename_multiple_dots() {
        assert_eq!(extract_filename("/home/user/file.tar.gz"), "file.tar.gz");
        assert_eq!(
            extract_filename("/home/user/backup.2024.01.01.zip"),
            "backup.2024.01.01.zip"
        );
    }

    #[test]
    fn test_extract_filename_content_uri() {
        assert_eq!(
            extract_filename("content://com.android.providers.media.documents/document/image%3A123"),
            "image%3A123"
        );
        assert_eq!(
            extract_filename("content://com.android.externalstorage.documents/document/primary%3ADCIM%2Fphoto.jpg"),
            "primary%3ADCIM%2Fphoto.jpg"
        );
    }

    #[test]
    fn test_extract_filename_content_uri_simple() {
        assert_eq!(
            extract_filename("content://provider/path/file.pdf"),
            "file.pdf"
        );
    }

    #[test]
    fn test_extract_filename_empty_path() {
        // Should return "unknown" for empty or invalid paths
        assert_eq!(extract_filename(""), "unknown");
    }

    #[test]
    fn test_extract_filename_just_filename() {
        assert_eq!(extract_filename("file.txt"), "file.txt");
    }

    #[test]
    fn test_extract_filename_spaces_in_name() {
        assert_eq!(
            extract_filename("/home/user/My Document.docx"),
            "My Document.docx"
        );
    }

    #[test]
    fn test_extract_filename_unicode() {
        assert_eq!(extract_filename("/home/user/文档.txt"), "文档.txt");
        assert_eq!(extract_filename("/home/user/документ.pdf"), "документ.pdf");
    }

    // ============================================================================
    // is_content_uri Tests
    // ============================================================================

    #[test]
    fn test_is_content_uri_true() {
        assert!(is_content_uri("content://com.android.providers.media.documents/document/123"));
        assert!(is_content_uri("content://provider/path"));
        assert!(is_content_uri("content://"));
    }

    #[test]
    fn test_is_content_uri_false() {
        assert!(!is_content_uri("/home/user/file.txt"));
        assert!(!is_content_uri("file:///path/to/file"));
        assert!(!is_content_uri("http://example.com"));
        assert!(!is_content_uri("https://example.com"));
        assert!(!is_content_uri(""));
        assert!(!is_content_uri("Content://uppercase")); // Case sensitive
    }

    #[test]
    fn test_is_content_uri_partial() {
        assert!(!is_content_uri("not-content://"));
        assert!(!is_content_uri("file-content://test"));
        assert!(!is_content_uri("contentx://fake"));
    }

    // ============================================================================
    // FileIoError Tests
    // ============================================================================

    #[test]
    fn test_file_io_error_read_display() {
        let error = FileIoError::ReadError {
            path: "/test/file.txt".to_string(),
            reason: "Permission denied".to_string(),
        };
        let display = format!("{}", error);
        assert!(display.contains("/test/file.txt"));
        assert!(display.contains("Permission denied"));
    }

    #[test]
    fn test_file_io_error_write_display() {
        let error = FileIoError::WriteError {
            path: "/test/output.txt".to_string(),
            reason: "Disk full".to_string(),
        };
        let display = format!("{}", error);
        assert!(display.contains("/test/output.txt"));
        assert!(display.contains("Disk full"));
    }

    #[test]
    fn test_file_io_error_invalid_path_display() {
        let error = FileIoError::InvalidPath {
            path: "invalid://path".to_string(),
            reason: "Invalid URI scheme".to_string(),
        };
        let display = format!("{}", error);
        assert!(display.contains("invalid://path"));
        assert!(display.contains("Invalid URI scheme"));
    }

    // ============================================================================
    // Desktop File I/O Tests (only run on desktop)
    // ============================================================================

    #[cfg(desktop)]
    mod desktop_tests {
        use super::*;
        use std::fs;
        use std::io::Write;
        use tempfile::TempDir;

        #[test]
        fn test_read_file_bytes_success() {
            let dir = TempDir::new().unwrap();
            let file_path = dir.path().join("test.txt");
            fs::write(&file_path, b"Hello, World!").unwrap();

            let result = read_file_bytes(file_path.to_str().unwrap());
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), b"Hello, World!");
        }

        #[test]
        fn test_read_file_bytes_not_found() {
            let result = read_file_bytes("/nonexistent/path/to/file.txt");
            assert!(result.is_err());
            match result {
                Err(FileIoError::ReadError { path, reason: _ }) => {
                    assert!(path.contains("nonexistent"));
                }
                _ => panic!("Expected ReadError"),
            }
        }

        #[test]
        fn test_read_file_bytes_binary() {
            let dir = TempDir::new().unwrap();
            let file_path = dir.path().join("binary.bin");
            let data: Vec<u8> = (0..=255).collect();
            fs::write(&file_path, &data).unwrap();

            let result = read_file_bytes(file_path.to_str().unwrap());
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), data);
        }

        #[test]
        fn test_read_file_bytes_empty() {
            let dir = TempDir::new().unwrap();
            let file_path = dir.path().join("empty.txt");
            fs::write(&file_path, b"").unwrap();

            let result = read_file_bytes(file_path.to_str().unwrap());
            assert!(result.is_ok());
            assert!(result.unwrap().is_empty());
        }

        #[test]
        fn test_write_file_bytes_success() {
            let dir = TempDir::new().unwrap();
            let file_path = dir.path().join("output.txt");

            let result = write_file_bytes(file_path.to_str().unwrap(), b"Test content");
            assert!(result.is_ok());

            let content = fs::read(&file_path).unwrap();
            assert_eq!(content, b"Test content");
        }

        #[test]
        fn test_write_file_bytes_overwrite() {
            let dir = TempDir::new().unwrap();
            let file_path = dir.path().join("overwrite.txt");
            fs::write(&file_path, b"Original").unwrap();

            let result = write_file_bytes(file_path.to_str().unwrap(), b"New content");
            assert!(result.is_ok());

            let content = fs::read(&file_path).unwrap();
            assert_eq!(content, b"New content");
        }

        #[test]
        fn test_write_file_bytes_binary() {
            let dir = TempDir::new().unwrap();
            let file_path = dir.path().join("binary.bin");
            let data: Vec<u8> = (0..=255).collect();

            let result = write_file_bytes(file_path.to_str().unwrap(), &data);
            assert!(result.is_ok());

            let content = fs::read(&file_path).unwrap();
            assert_eq!(content, data);
        }

        #[test]
        fn test_file_exists_true() {
            let dir = TempDir::new().unwrap();
            let file_path = dir.path().join("exists.txt");
            fs::write(&file_path, b"content").unwrap();

            assert!(file_exists(file_path.to_str().unwrap()));
        }

        #[test]
        fn test_file_exists_false() {
            assert!(!file_exists("/nonexistent/path/to/file.txt"));
        }

        #[test]
        fn test_file_exists_directory() {
            let dir = TempDir::new().unwrap();
            // Directories also "exist"
            assert!(file_exists(dir.path().to_str().unwrap()));
        }

        #[test]
        fn test_create_parent_dirs_success() {
            let dir = TempDir::new().unwrap();
            let nested_path = dir.path().join("a/b/c/file.txt");

            let result = create_parent_dirs(nested_path.to_str().unwrap());
            assert!(result.is_ok());

            // Parent directories should exist
            assert!(dir.path().join("a/b/c").exists());
        }

        #[test]
        fn test_create_parent_dirs_existing() {
            let dir = TempDir::new().unwrap();
            let file_path = dir.path().join("file.txt");

            // Parent already exists (it's dir itself)
            let result = create_parent_dirs(file_path.to_str().unwrap());
            assert!(result.is_ok());
        }

        #[test]
        fn test_read_file_roundtrip() {
            let dir = TempDir::new().unwrap();
            let file_path = dir.path().join("roundtrip.txt");
            let original = b"Round trip test data with special chars: \xC3\xA4\xC3\xB6\xC3\xBC";

            write_file(file_path.to_str().unwrap(), original).unwrap();
            let read_back = read_file(file_path.to_str().unwrap()).unwrap();

            assert_eq!(original.as_slice(), read_back.as_slice());
        }

        #[test]
        fn test_write_file_creates_parents() {
            let dir = TempDir::new().unwrap();
            let nested_path = dir.path().join("deep/nested/path/file.txt");

            let result = write_file(nested_path.to_str().unwrap(), b"content");
            assert!(result.is_ok());

            let content = fs::read(&nested_path).unwrap();
            assert_eq!(content, b"content");
        }

        #[test]
        fn test_large_file_read_write() {
            let dir = TempDir::new().unwrap();
            let file_path = dir.path().join("large.bin");

            // 10MB of data
            let data: Vec<u8> = (0..10 * 1024 * 1024).map(|i| (i % 256) as u8).collect();

            write_file(file_path.to_str().unwrap(), &data).unwrap();
            let read_back = read_file(file_path.to_str().unwrap()).unwrap();

            assert_eq!(data.len(), read_back.len());
            assert_eq!(data, read_back);
        }
    }
}

#[cfg(test)]
mod watcher_tests {
    use crate::extension::filesystem::watcher::*;

    // ============================================================================
    // FileChangeEvent Tests
    // ============================================================================

    #[test]
    fn test_file_change_event_serialization() {
        let event = FileChangeEvent {
            rule_id: "rule-123".to_string(),
            change_type: FileChangeType::Created,
            path: Some("path/to/file.txt".to_string()),
        };

        let json = serde_json::to_string(&event).unwrap();

        assert!(json.contains("\"ruleId\":\"rule-123\""));
        assert!(json.contains("\"changeType\":\"created\""));
        assert!(json.contains("\"path\":\"path/to/file.txt\""));
    }

    #[test]
    fn test_file_change_event_deserialization() {
        let json = r#"{
            "ruleId": "rule-456",
            "changeType": "modified",
            "path": "docs/readme.md"
        }"#;

        let event: FileChangeEvent = serde_json::from_str(json).unwrap();

        assert_eq!(event.rule_id, "rule-456");
        assert!(matches!(event.change_type, FileChangeType::Modified));
        assert_eq!(event.path, Some("docs/readme.md".to_string()));
    }

    #[test]
    fn test_file_change_event_null_path() {
        let event = FileChangeEvent {
            rule_id: "rule-789".to_string(),
            change_type: FileChangeType::Any,
            path: None,
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"path\":null"));

        let deserialized: FileChangeEvent = serde_json::from_str(&json).unwrap();
        assert!(deserialized.path.is_none());
    }

    // ============================================================================
    // FileChangeType Tests
    // ============================================================================

    #[test]
    fn test_file_change_type_created() {
        let change_type = FileChangeType::Created;
        let json = serde_json::to_string(&change_type).unwrap();
        assert_eq!(json, "\"created\"");
    }

    #[test]
    fn test_file_change_type_modified() {
        let change_type = FileChangeType::Modified;
        let json = serde_json::to_string(&change_type).unwrap();
        assert_eq!(json, "\"modified\"");
    }

    #[test]
    fn test_file_change_type_removed() {
        let change_type = FileChangeType::Removed;
        let json = serde_json::to_string(&change_type).unwrap();
        assert_eq!(json, "\"removed\"");
    }

    #[test]
    fn test_file_change_type_any() {
        let change_type = FileChangeType::Any;
        let json = serde_json::to_string(&change_type).unwrap();
        assert_eq!(json, "\"any\"");
    }

    #[test]
    fn test_file_change_type_deserialization() {
        let types = [
            ("\"created\"", FileChangeType::Created),
            ("\"modified\"", FileChangeType::Modified),
            ("\"removed\"", FileChangeType::Removed),
            ("\"any\"", FileChangeType::Any),
        ];

        for (json, expected) in types {
            let parsed: FileChangeType = serde_json::from_str(json).unwrap();
            assert!(
                std::mem::discriminant(&parsed) == std::mem::discriminant(&expected),
                "Failed for {}",
                json
            );
        }
    }

    // ============================================================================
    // Event Name Constant Tests
    // ============================================================================

    #[test]
    fn test_file_change_event_name() {
        assert_eq!(FILE_CHANGE_EVENT, "filesync:file-changed");
    }

    // ============================================================================
    // FileWatcherManager Tests (Desktop only)
    // ============================================================================

    #[cfg(desktop)]
    mod desktop_watcher_tests {
        use super::*;
        use tempfile::TempDir;

        #[test]
        fn test_file_watcher_manager_new() {
            let manager = FileWatcherManager::new();
            assert!(!manager.is_watching("any-rule"));
        }

        #[test]
        fn test_file_watcher_manager_default() {
            let manager = FileWatcherManager::default();
            assert!(!manager.is_watching("any-rule"));
        }

        #[test]
        fn test_is_watching_false_initially() {
            let manager = FileWatcherManager::new();
            assert!(!manager.is_watching("rule-1"));
            assert!(!manager.is_watching("rule-2"));
            assert!(!manager.is_watching(""));
        }

        #[test]
        fn test_unwatch_nonexistent_rule() {
            let manager = FileWatcherManager::new();
            // Should not error when unwatching a rule that doesn't exist
            let result = manager.unwatch("nonexistent-rule");
            assert!(result.is_ok());
        }

        #[test]
        fn test_unwatch_all_empty() {
            let manager = FileWatcherManager::new();
            let result = manager.unwatch_all();
            assert!(result.is_ok());
        }

        // Note: Full watch/unwatch tests with actual file watching would require
        // an AppHandle which is only available in a running Tauri application.
        // These tests cover the basic state management without the actual watching.
    }

    // ============================================================================
    // Android Stub Tests
    // ============================================================================

    #[cfg(target_os = "android")]
    mod android_watcher_tests {
        use super::*;

        #[test]
        fn test_android_stub_new() {
            let manager = FileWatcherManager::new();
            // Should compile and create without panic
        }

        #[test]
        fn test_android_stub_is_watching() {
            let manager = FileWatcherManager::new();
            // Always returns false on Android
            assert!(!manager.is_watching("any-rule"));
        }

        #[test]
        fn test_android_stub_unwatch() {
            let manager = FileWatcherManager::new();
            // Should always succeed (no-op)
            assert!(manager.unwatch("rule").is_ok());
        }

        #[test]
        fn test_android_stub_unwatch_all() {
            let manager = FileWatcherManager::new();
            // Should always succeed (no-op)
            assert!(manager.unwatch_all().is_ok());
        }
    }
}
