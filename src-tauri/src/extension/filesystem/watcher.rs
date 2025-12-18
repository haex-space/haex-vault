// src-tauri/src/extension/filesystem/watcher.rs
//!
//! File System Watcher for Desktop platforms
//!
//! Monitors sync rule directories for file changes and emits events to the frontend.
//! Only available on desktop platforms (not Android).
//!

#[cfg(desktop)]
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind, Debouncer};
#[cfg(desktop)]
use notify::RecursiveMode;
#[cfg(desktop)]
use std::collections::HashMap;
#[cfg(desktop)]
use std::path::PathBuf;
#[cfg(desktop)]
use std::sync::{Arc, Mutex};
#[cfg(desktop)]
use std::time::Duration;
#[cfg(desktop)]
use tauri::{AppHandle, Emitter};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Event emitted when files change in a watched directory
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileChangeEvent {
    /// The sync rule ID that was affected
    pub rule_id: String,
    /// Type of change
    pub change_type: FileChangeType,
    /// Relative path of the changed file (if available)
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum FileChangeType {
    /// A file or directory was created
    Created,
    /// A file or directory was modified
    Modified,
    /// A file or directory was deleted
    Removed,
    /// Multiple changes occurred (batch)
    Any,
}

/// Event name for file change events
pub const FILE_CHANGE_EVENT: &str = "filesync:file-changed";

#[cfg(desktop)]
type WatcherHandle = Debouncer<notify::RecommendedWatcher>;

/// File watcher manager for desktop platforms
#[cfg(desktop)]
pub struct FileWatcherManager {
    /// Map of rule_id -> watcher handle
    watchers: Arc<Mutex<HashMap<String, WatcherHandle>>>,
    /// Map of path -> rule_id for reverse lookup
    path_to_rule: Arc<Mutex<HashMap<PathBuf, String>>>,
}

#[cfg(desktop)]
impl FileWatcherManager {
    pub fn new() -> Self {
        Self {
            watchers: Arc::new(Mutex::new(HashMap::new())),
            path_to_rule: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Start watching a directory for a sync rule
    pub fn watch(
        &self,
        app_handle: AppHandle,
        rule_id: String,
        path: String,
    ) -> Result<(), String> {
        let path_buf = PathBuf::from(&path);

        if !path_buf.exists() {
            return Err(format!("Path does not exist: {}", path));
        }

        if !path_buf.is_dir() {
            return Err(format!("Path is not a directory: {}", path));
        }

        // Check if already watching this rule
        {
            let watchers = self.watchers.lock().map_err(|e| e.to_string())?;
            if watchers.contains_key(&rule_id) {
                return Ok(()); // Already watching
            }
        }

        let rule_id_clone = rule_id.clone();
        let base_path = path_buf.clone();

        // Create debounced watcher (500ms debounce)
        let mut debouncer = new_debouncer(
            Duration::from_millis(500),
            move |result: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
                match result {
                    Ok(events) => {
                        // Determine the change type from events
                        let change_type = if events.is_empty() {
                            return;
                        } else if events.len() == 1 {
                            match events[0].kind {
                                DebouncedEventKind::Any => FileChangeType::Any,
                                DebouncedEventKind::AnyContinuous => FileChangeType::Modified,
                                _ => FileChangeType::Any,
                            }
                        } else {
                            FileChangeType::Any
                        };

                        // Get relative path of first event
                        let relative_path = events.first().and_then(|e| {
                            e.path.strip_prefix(&base_path)
                                .ok()
                                .map(|p| p.to_string_lossy().to_string())
                        });

                        let event = FileChangeEvent {
                            rule_id: rule_id_clone.clone(),
                            change_type,
                            path: relative_path,
                        };

                        // Emit event to frontend
                        if let Err(e) = app_handle.emit(FILE_CHANGE_EVENT, &event) {
                            eprintln!("[FileWatcher] Failed to emit event: {}", e);
                        }
                    }
                    Err(e) => {
                        eprintln!("[FileWatcher] Watch error for rule {}: {:?}", rule_id_clone, e);
                    }
                }
            },
        ).map_err(|e| format!("Failed to create watcher: {}", e))?;

        // Start watching the directory
        debouncer
            .watcher()
            .watch(&path_buf, RecursiveMode::Recursive)
            .map_err(|e| format!("Failed to watch path: {}", e))?;

        // Store the watcher
        {
            let mut watchers = self.watchers.lock().map_err(|e| e.to_string())?;
            watchers.insert(rule_id.clone(), debouncer);
        }

        // Store path -> rule mapping
        {
            let mut path_to_rule = self.path_to_rule.lock().map_err(|e| e.to_string())?;
            path_to_rule.insert(path_buf, rule_id.clone());
        }

        println!("[FileWatcher] Started watching rule {} at path: {}", rule_id, path);
        Ok(())
    }

    /// Stop watching a directory for a sync rule
    pub fn unwatch(&self, rule_id: &str) -> Result<(), String> {
        // Remove the watcher (this will stop watching automatically when dropped)
        let removed = {
            let mut watchers = self.watchers.lock().map_err(|e| e.to_string())?;
            watchers.remove(rule_id).is_some()
        };

        // Clean up path mapping
        {
            let mut path_to_rule = self.path_to_rule.lock().map_err(|e| e.to_string())?;
            path_to_rule.retain(|_, v| v != rule_id);
        }

        if removed {
            println!("[FileWatcher] Stopped watching rule: {}", rule_id);
        }

        Ok(())
    }

    /// Stop all watchers
    pub fn unwatch_all(&self) -> Result<(), String> {
        {
            let mut watchers = self.watchers.lock().map_err(|e| e.to_string())?;
            watchers.clear();
        }
        {
            let mut path_to_rule = self.path_to_rule.lock().map_err(|e| e.to_string())?;
            path_to_rule.clear();
        }
        println!("[FileWatcher] Stopped all watchers");
        Ok(())
    }

    /// Check if a rule is being watched
    pub fn is_watching(&self, rule_id: &str) -> bool {
        self.watchers
            .lock()
            .map(|w| w.contains_key(rule_id))
            .unwrap_or(false)
    }
}

#[cfg(desktop)]
impl Default for FileWatcherManager {
    fn default() -> Self {
        Self::new()
    }
}

// Stub implementation for Android
#[cfg(target_os = "android")]
pub struct FileWatcherManager;

#[cfg(target_os = "android")]
impl FileWatcherManager {
    pub fn new() -> Self {
        Self
    }

    pub fn watch(
        &self,
        _app_handle: tauri::AppHandle,
        _rule_id: String,
        _path: String,
    ) -> Result<(), String> {
        // File watching is not supported on Android
        Ok(())
    }

    pub fn unwatch(&self, _rule_id: &str) -> Result<(), String> {
        Ok(())
    }

    pub fn unwatch_all(&self) -> Result<(), String> {
        Ok(())
    }

    pub fn is_watching(&self, _rule_id: &str) -> bool {
        false
    }
}

#[cfg(target_os = "android")]
impl Default for FileWatcherManager {
    fn default() -> Self {
        Self::new()
    }
}
