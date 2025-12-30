// src-tauri/src/extension/webview/tests.rs
//!
//! Unit tests for ExtensionWebviewManager
//!

use super::manager::ExtensionWebviewManager;

#[cfg(test)]
mod manager_tests {
    use super::*;

    #[test]
    fn test_new_manager_is_empty() {
        let manager = ExtensionWebviewManager::new();
        let windows = manager.windows.lock().unwrap();
        assert!(windows.is_empty());
    }

    #[test]
    fn test_has_window_for_extension_returns_false_when_empty() {
        let manager = ExtensionWebviewManager::new();
        assert!(!manager.has_window_for_extension("test-extension"));
    }

    #[test]
    fn test_has_window_for_extension_returns_true_when_registered() {
        let manager = ExtensionWebviewManager::new();

        // Manually register a window (simulating open_extension_window)
        {
            let mut windows = manager.windows.lock().unwrap();
            windows.insert("ext_abc123".to_string(), "test-extension".to_string());
        }

        assert!(manager.has_window_for_extension("test-extension"));
        assert!(!manager.has_window_for_extension("other-extension"));
    }

    #[test]
    fn test_get_window_for_extension_returns_none_when_empty() {
        let manager = ExtensionWebviewManager::new();
        assert!(manager.get_window_for_extension("test-extension").is_none());
    }

    #[test]
    fn test_get_window_for_extension_returns_window_id() {
        let manager = ExtensionWebviewManager::new();

        // Register a window
        {
            let mut windows = manager.windows.lock().unwrap();
            windows.insert("ext_window1".to_string(), "extension-a".to_string());
        }

        let result = manager.get_window_for_extension("extension-a");
        assert_eq!(result, Some("ext_window1".to_string()));
    }

    #[test]
    fn test_get_window_for_extension_returns_first_window_when_multiple() {
        let manager = ExtensionWebviewManager::new();

        // Register multiple windows for the same extension
        // Note: HashMap iteration order is not guaranteed, but we only need ONE window
        {
            let mut windows = manager.windows.lock().unwrap();
            windows.insert("ext_window1".to_string(), "extension-a".to_string());
            windows.insert("ext_window2".to_string(), "extension-a".to_string());
            windows.insert("ext_window3".to_string(), "extension-b".to_string());
        }

        // Should return one of the windows for extension-a
        let result = manager.get_window_for_extension("extension-a");
        assert!(result.is_some());
        let window_id = result.unwrap();
        assert!(window_id == "ext_window1" || window_id == "ext_window2");

        // Should return the window for extension-b
        let result_b = manager.get_window_for_extension("extension-b");
        assert_eq!(result_b, Some("ext_window3".to_string()));
    }

    #[test]
    fn test_default_creates_empty_manager() {
        let manager = ExtensionWebviewManager::default();
        let windows = manager.windows.lock().unwrap();
        assert!(windows.is_empty());
    }

    #[test]
    fn test_window_registry_thread_safety() {
        use std::sync::Arc;
        use std::thread;

        let manager = Arc::new(ExtensionWebviewManager::new());

        // Spawn multiple threads to register windows
        let mut handles = vec![];
        for i in 0..10 {
            let manager_clone = Arc::clone(&manager);
            let handle = thread::spawn(move || {
                let mut windows = manager_clone.windows.lock().unwrap();
                windows.insert(
                    format!("ext_window_{}", i),
                    format!("extension_{}", i % 3),  // 3 different extensions
                );
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Verify all windows were registered
        let windows = manager.windows.lock().unwrap();
        assert_eq!(windows.len(), 10);
    }

    #[test]
    fn test_window_removal() {
        let manager = ExtensionWebviewManager::new();

        // Register a window
        {
            let mut windows = manager.windows.lock().unwrap();
            windows.insert("ext_window1".to_string(), "test-extension".to_string());
        }

        assert!(manager.has_window_for_extension("test-extension"));

        // Remove the window
        {
            let mut windows = manager.windows.lock().unwrap();
            windows.remove("ext_window1");
        }

        assert!(!manager.has_window_for_extension("test-extension"));
    }
}
