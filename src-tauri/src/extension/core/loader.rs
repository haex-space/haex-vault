// src-tauri/src/extension/core/loader.rs
//
// Extension loading from database and filesystem.

use crate::database::core::select_with_crdt;
use crate::extension::core::manifest::{DisplayMode, ExtensionManifest, ExtensionPermissions};
use crate::extension::core::path_utils::validate_path_in_directory;
use crate::extension::core::types::{Extension, ExtensionSource};
use crate::extension::error::ExtensionError;
use crate::table_names::TABLE_EXTENSIONS;
use crate::AppState;
use std::path::PathBuf;
use std::time::SystemTime;
use tauri::{AppHandle, State};

use super::manager::{ExtensionManager, MissingExtension};

/// Config parsed from haextension.config.json
struct HaextensionConfig {
    host: String,
    port: u16,
    haextension_dir: String,
}

impl Default for HaextensionConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 5173,
            haextension_dir: "haextension".to_string(),
        }
    }
}

/// Read haextension.config.json from a directory.
fn read_haextension_config(base_path: &PathBuf) -> HaextensionConfig {
    let config_path = base_path.join("haextension.config.json");
    if !config_path.exists() {
        return HaextensionConfig::default();
    }

    let config_content = match std::fs::read_to_string(&config_path) {
        Ok(content) => content,
        Err(_) => return HaextensionConfig::default(),
    };

    let config: serde_json::Value = match serde_json::from_str(&config_content) {
        Ok(c) => c,
        Err(_) => return HaextensionConfig::default(),
    };

    let dev = config.get("dev");
    HaextensionConfig {
        host: dev
            .and_then(|d| d.get("host"))
            .and_then(|h| h.as_str())
            .unwrap_or("localhost")
            .to_string(),
        port: dev
            .and_then(|d| d.get("port"))
            .and_then(|p| p.as_u64())
            .unwrap_or(5173) as u16,
        haextension_dir: dev
            .and_then(|d| d.get("haextension_dir"))
            .and_then(|dir| dir.as_str())
            .unwrap_or("haextension")
            .to_string(),
    }
}

/// Data loaded from the database for an extension.
pub(crate) struct ExtensionDataFromDb {
    pub id: String,
    pub manifest: ExtensionManifest,
    pub enabled: bool,
    /// If set, this is a dev extension with path to the project folder
    pub dev_path: Option<String>,
}

impl ExtensionManager {
    /// Scans the filesystem at startup and loads all installed extensions.
    pub async fn load_installed_extensions(
        &self,
        app_handle: &AppHandle,
        state: &State<'_, AppState>,
    ) -> Result<Vec<String>, ExtensionError> {
        // Clear existing data
        self.available_extensions
            .lock()
            .map_err(|e| ExtensionError::MutexPoisoned {
                reason: e.to_string(),
            })?
            .clear();
        self.permission_cache
            .lock()
            .map_err(|e| ExtensionError::MutexPoisoned {
                reason: e.to_string(),
            })?
            .clear();
        self.missing_extensions
            .lock()
            .map_err(|e| ExtensionError::MutexPoisoned {
                reason: e.to_string(),
            })?
            .clear();

        // Load all data from database
        // Use select_with_crdt to automatically filter out tombstoned (soft-deleted) entries
        // Load all extensions - dev_path determines if it's a dev extension
        let sql = format!(
            "SELECT id, name, version, author, entry, icon, public_key, signature, homepage, description, enabled, single_instance, display_mode, dev_path FROM {TABLE_EXTENSIONS}"
        );
        eprintln!("DEBUG: SQL Query (will be transformed by select_with_crdt): {sql}");

        let results = select_with_crdt(sql, vec![], &state.db)?;
        eprintln!("DEBUG: Query returned {} results", results.len());

        let mut extensions = Vec::new();
        for row in results {
            // Values in the order of the SELECT statement
            let id = row[0]
                .as_str()
                .ok_or_else(|| ExtensionError::ManifestError {
                    reason: "Missing id field in database row".to_string(),
                })?
                .to_string();

            let manifest = ExtensionManifest {
                name: row[1]
                    .as_str()
                    .ok_or_else(|| ExtensionError::ManifestError {
                        reason: "Missing name field in database row".to_string(),
                    })?
                    .to_string(),
                version: row[2]
                    .as_str()
                    .ok_or_else(|| ExtensionError::ManifestError {
                        reason: "Missing version field in database row".to_string(),
                    })?
                    .to_string(),
                author: row[3].as_str().map(String::from),
                entry: row[4].as_str().map(String::from),
                icon: row[5].as_str().map(String::from),
                public_key: row[6].as_str().unwrap_or("").to_string(),
                signature: row[7].as_str().unwrap_or("").to_string(),
                permissions: ExtensionPermissions::default(),
                homepage: row[8].as_str().map(String::from),
                description: row[9].as_str().map(String::from),
                single_instance: row[11]
                    .as_bool()
                    .or_else(|| row[11].as_i64().map(|v| v != 0)),
                display_mode: row[12].as_str().and_then(|s| match s {
                    "window" => Some(DisplayMode::Window),
                    "iframe" => Some(DisplayMode::Iframe),
                    "auto" | _ => Some(DisplayMode::Auto),
                }),
                // migrations_dir is not stored in DB - it's only used during installation
                // from the manifest.json file
                migrations_dir: None,
            };

            let enabled = row[10]
                .as_bool()
                .or_else(|| row[10].as_i64().map(|v| v != 0))
                .unwrap_or(false);

            // dev_path is at index 13
            let dev_path = row.get(13).and_then(|v| v.as_str()).map(String::from);

            extensions.push(ExtensionDataFromDb {
                id,
                manifest,
                enabled,
                dev_path,
            });
        }

        // Step 2: Process the collected data (filesystem, state mutations).
        let mut loaded_extension_ids = Vec::new();

        eprintln!("DEBUG: Found {} extensions in database", extensions.len());

        for extension_data in extensions {
            let extension_id = extension_data.id;
            eprintln!(
                "DEBUG: Processing extension: {extension_id} (name={}, version={}, dev_path={:?})",
                extension_data.manifest.name, extension_data.manifest.version, extension_data.dev_path
            );

            // Check if this is a dev extension (dev_path is set)
            if let Some(dev_path) = extension_data.dev_path {
                match self.load_dev_extension_from_path(
                    &extension_id,
                    &dev_path,
                    extension_data.manifest,
                    extension_data.enabled,
                ) {
                    Ok(true) => {
                        loaded_extension_ids.push(extension_id);
                    }
                    Ok(false) => {
                        // Dev extension path not found - silently skipped
                    }
                    Err(e) => {
                        eprintln!("DEBUG: Error loading dev extension: {e}");
                    }
                }
            } else {
                // Production extension - load from extensions directory
                match self.load_production_extension(
                    app_handle,
                    &extension_id,
                    extension_data.manifest,
                    extension_data.enabled,
                ) {
                    Ok(true) => {
                        loaded_extension_ids.push(extension_id);
                    }
                    Ok(false) => {
                        // Missing extension - already added to missing_extensions
                    }
                    Err(e) => {
                        eprintln!("DEBUG: Error loading production extension: {e}");
                    }
                }
            }
        }

        Ok(loaded_extension_ids)
    }

    /// Load a dev extension from its project path.
    /// Returns Ok(true) if loaded, Ok(false) if path doesn't exist (synced from another device).
    fn load_dev_extension_from_path(
        &self,
        extension_id: &str,
        dev_path: &str,
        manifest: ExtensionManifest,
        enabled: bool,
    ) -> Result<bool, ExtensionError> {
        let dev_path_buf = PathBuf::from(dev_path);

        // If dev_path doesn't exist, skip silently (this is a synced dev extension from another device)
        if !dev_path_buf.exists() {
            eprintln!(
                "DEBUG: Skipping dev extension {} - path not found on this device: {dev_path}",
                manifest.name
            );
            return Ok(false);
        }

        let config = read_haextension_config(&dev_path_buf);
        let dev_server_url = format!("http://{}:{}", config.host, config.port);
        let manifest_path = dev_path_buf.join(&config.haextension_dir).join("manifest.json");

        // Resolve icon path from relative (stored in DB) to absolute (for frontend)
        let mut manifest = manifest;
        manifest.icon = manifest.icon.as_ref().map(|rel_path| {
            dev_path_buf.join(rel_path).to_string_lossy().to_string()
        });

        let extension = Extension {
            id: extension_id.to_string(),
            source: ExtensionSource::Development {
                dev_server_url,
                manifest_path,
                auto_reload: true,
            },
            manifest,
            enabled,
            last_accessed: SystemTime::now(),
        };

        eprintln!("DEBUG: Dev extension loaded successfully: {extension_id}");
        self.add_extension(extension)?;
        Ok(true)
    }

    /// Load a production extension from the extensions directory.
    /// Returns Ok(true) if loaded, Ok(false) if missing (added to missing_extensions).
    fn load_production_extension(
        &self,
        app_handle: &AppHandle,
        extension_id: &str,
        manifest: ExtensionManifest,
        enabled: bool,
    ) -> Result<bool, ExtensionError> {
        let extension_path = self.get_extension_dir(
            app_handle,
            &manifest.public_key,
            &manifest.name,
            &manifest.version,
        )?;

        eprintln!("DEBUG: Checking extension path: {:?}", extension_path);

        // Check if extension directory exists
        if !extension_path.exists() {
            eprintln!(
                "DEBUG: Extension directory MISSING for: {extension_id} at {extension_path:?}"
            );
            self.missing_extensions
                .lock()
                .map_err(|e| ExtensionError::MutexPoisoned {
                    reason: e.to_string(),
                })?
                .push(MissingExtension {
                    id: extension_id.to_string(),
                    public_key: manifest.public_key.clone(),
                    name: manifest.name.clone(),
                    version: manifest.version.clone(),
                });
            return Ok(false);
        }

        let config = read_haextension_config(&extension_path);

        // Validate manifest.json path using helper function
        let manifest_relative_path = format!("{}/manifest.json", config.haextension_dir);
        if validate_path_in_directory(&extension_path, &manifest_relative_path, true)?.is_none() {
            eprintln!(
                "DEBUG: manifest.json missing or invalid for: {extension_id} at {manifest_relative_path}"
            );
            self.missing_extensions
                .lock()
                .map_err(|e| ExtensionError::MutexPoisoned {
                    reason: e.to_string(),
                })?
                .push(MissingExtension {
                    id: extension_id.to_string(),
                    public_key: manifest.public_key.clone(),
                    name: manifest.name.clone(),
                    version: manifest.version.clone(),
                });
            return Ok(false);
        }

        eprintln!("DEBUG: Extension loaded successfully: {extension_id}");

        // Resolve icon path from relative (stored in DB) to absolute (for frontend)
        let mut manifest = manifest;
        manifest.icon = manifest.icon.as_ref().map(|rel_path| {
            extension_path.join(rel_path).to_string_lossy().to_string()
        });

        let extension = Extension {
            id: extension_id.to_string(),
            source: ExtensionSource::Production {
                path: extension_path,
                version: manifest.version.clone(),
            },
            manifest,
            enabled,
            last_accessed: SystemTime::now(),
        };

        self.add_extension(extension)?;
        Ok(true)
    }
}
