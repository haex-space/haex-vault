use crate::database::core::{execute_with_crdt, select_with_crdt, with_connection};
use crate::database::error::DatabaseError;
use crate::extension::core::manifest::{
    EditablePermissions, ExtensionManifest, ExtensionPreview, MigrationJournal,
};
use crate::extension::core::types::{copy_directory, Extension, ExtensionSource};
use crate::extension::core::{DisplayMode, ExtensionPermissions};
use crate::extension::crypto::ExtensionCrypto;
use crate::extension::database::executor::SqlExecutor;
use crate::extension::database::{execute_migration_statements, ExtensionSqlContext};
use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::permissions::types::ExtensionPermission;
use crate::extension::utils::{drop_extension_tables, validate_public_key};
use crate::table_names::{
    TABLE_EXTENSIONS, TABLE_EXTENSION_MIGRATIONS, TABLE_EXTENSION_PERMISSIONS,
};
use crate::AppState;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_fs::FsExt;
use zip::ZipArchive;

#[derive(Debug, Clone)]
pub struct CachedPermission {
    pub permissions: Vec<ExtensionPermission>,
    pub cached_at: SystemTime,
    pub ttl: Duration,
}

#[derive(Debug, Clone)]
pub struct MissingExtension {
    pub id: String,
    pub public_key: String,
    pub name: String,
    pub version: String,
}

struct ExtensionDataFromDb {
    id: String,
    manifest: ExtensionManifest,
    enabled: bool,
}

#[derive(Default)]
pub struct ExtensionManager {
    pub production_extensions: Mutex<HashMap<String, Extension>>,
    pub dev_extensions: Mutex<HashMap<String, Extension>>,
    pub permission_cache: Mutex<HashMap<String, CachedPermission>>,
    pub missing_extensions: Mutex<Vec<MissingExtension>>,
}

struct ExtractedExtension {
    temp_dir: PathBuf,
    manifest: ExtensionManifest,
    content_hash: String,
}

impl Drop for ExtractedExtension {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.temp_dir).ok();
    }
}

impl ExtensionManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Helper function to validate path and check for path traversal
    /// Returns the cleaned path if valid, or None if invalid/not found
    /// If require_exists is true, returns None if path doesn't exist
    pub fn validate_path_in_directory(
        base_dir: &PathBuf,
        relative_path: &str,
        require_exists: bool,
    ) -> Result<Option<PathBuf>, ExtensionError> {
        // Check for path traversal patterns
        if relative_path.contains("..") {
            return Err(ExtensionError::SecurityViolation {
                reason: format!("Path traversal attempt: {relative_path}"),
            });
        }

        // Clean the path (same logic as in protocol.rs)
        let clean_path = relative_path
            .replace('\\', "/")
            .trim_start_matches('/')
            .split('/')
            .filter(|&part| !part.is_empty() && part != "." && part != "..")
            .collect::<PathBuf>();

        let full_path = base_dir.join(&clean_path);

        // Check if file/directory exists (if required)
        if require_exists && !full_path.exists() {
            return Ok(None);
        }

        // Verify path is within base directory
        let canonical_base = base_dir
            .canonicalize()
            .map_err(|e| ExtensionError::Filesystem { source: e })?;

        if let Ok(canonical_path) = full_path.canonicalize() {
            if !canonical_path.starts_with(&canonical_base) {
                return Err(ExtensionError::SecurityViolation {
                    reason: format!("Path outside base directory: {relative_path}"),
                });
            }
            Ok(Some(canonical_path))
        } else {
            // Path doesn't exist yet - still validate it would be within base
            if full_path.starts_with(&canonical_base) {
                Ok(Some(full_path))
            } else {
                Err(ExtensionError::SecurityViolation {
                    reason: format!("Path outside base directory: {relative_path}"),
                })
            }
        }
    }

    /// Validates icon path and falls back to favicon.ico if not specified
    pub fn validate_and_resolve_icon_path(
        extension_dir: &PathBuf,
        haextension_dir: &str,
        icon_path: Option<&str>,
    ) -> Result<Option<String>, ExtensionError> {
        // If icon is specified in manifest, validate it
        if let Some(icon) = icon_path {
            if let Some(clean_path) = Self::validate_path_in_directory(extension_dir, icon, true)? {
                return Ok(Some(clean_path.to_string_lossy().to_string()));
            }
            // Icon not found, continue to fallback logic
        }

        // Fallback 1: Check haextension/favicon.ico
        let haextension_favicon = format!("{haextension_dir}/favicon.ico");
        if let Some(clean_path) =
            Self::validate_path_in_directory(extension_dir, &haextension_favicon, true)?
        {
            return Ok(Some(clean_path.to_string_lossy().to_string()));
        }

        // Fallback 2: Check favicon.ico in root
        if let Some(clean_path) =
            Self::validate_path_in_directory(extension_dir, "favicon.ico", true)?
        {
            return Ok(Some(clean_path.to_string_lossy().to_string()));
        }

        // No icon found
        Ok(None)
    }

    /// Find icon path using FsExt (works better on Android)
    /// Returns the relative path if found, None otherwise
    fn find_icon(
        app_handle: &AppHandle,
        extension_dir: &PathBuf,
        haextension_dir: &str,
        icon_path: Option<&str>,
    ) -> Option<String> {
        let fs = app_handle.fs();

        // Helper to check if path contains traversal
        let is_safe_path = |path: &str| -> bool { !path.contains("..") };

        // Helper to clean relative path
        let clean_relative = |path: &str| -> String {
            path.replace('\\', "/")
                .trim_start_matches('/')
                .to_string()
        };

        // Helper to check if file exists using FsExt
        // We try to read a small portion of the file to check existence
        let file_exists = |relative_path: &str| -> bool {
            if !is_safe_path(relative_path) {
                return false;
            }
            let clean = clean_relative(relative_path);
            let full_path = extension_dir.join(&clean);
            // Use FsExt to check if file can be read
            fs.read(&full_path).is_ok()
        };

        // 1. Check manifest icon path
        if let Some(icon) = icon_path {
            if file_exists(icon) {
                return Some(clean_relative(icon));
            }
        }

        // 2. Fallback: Check haextension/favicon.ico
        let haextension_favicon = format!("{haextension_dir}/favicon.ico");
        if file_exists(&haextension_favicon) {
            return Some(clean_relative(&haextension_favicon));
        }

        // 3. Fallback: Check favicon.ico in root
        if file_exists("favicon.ico") {
            return Some("favicon.ico".to_string());
        }

        None
    }

    /// Extrahiert eine Extension-ZIP-Datei und validiert das Manifest
    fn extract_and_validate_extension(
        bytes: Vec<u8>,
        temp_prefix: &str,
        app_handle: &AppHandle,
    ) -> Result<ExtractedExtension, ExtensionError> {
        // Use app_cache_dir for better Android compatibility
        let cache_dir =
            app_handle
                .path()
                .app_cache_dir()
                .map_err(|e| ExtensionError::InstallationFailed {
                    reason: format!("Cannot get app cache dir: {e}"),
                })?;

        let temp_id = uuid::Uuid::new_v4();
        let temp = cache_dir.join(format!("{temp_prefix}_{temp_id}"));
        let zip_file_path = cache_dir.join(format!(
            "{}_{}_{}.haextension",
            temp_prefix, temp_id, "temp"
        ));

        // Write bytes to a temporary ZIP file first (important for Android file system)
        fs::write(&zip_file_path, &bytes).map_err(|e| {
            ExtensionError::filesystem_with_path(zip_file_path.display().to_string(), e)
        })?;

        // Create extraction directory
        fs::create_dir_all(&temp)
            .map_err(|e| ExtensionError::filesystem_with_path(temp.display().to_string(), e))?;

        // Open ZIP file from disk (more reliable on Android than from memory)
        let zip_file = fs::File::open(&zip_file_path).map_err(|e| {
            ExtensionError::filesystem_with_path(zip_file_path.display().to_string(), e)
        })?;

        let mut archive =
            ZipArchive::new(zip_file).map_err(|e| ExtensionError::InstallationFailed {
                reason: format!("Invalid ZIP: {e}"),
            })?;

        archive
            .extract(&temp)
            .map_err(|e| ExtensionError::InstallationFailed {
                reason: format!("Cannot extract ZIP: {e}"),
            })?;

        // Clean up temporary ZIP file
        let _ = fs::remove_file(&zip_file_path);

        // Read haextension_dir from config if it exists, otherwise use default
        let config_path = temp.join("haextension.config.json");
        let haextension_dir = if config_path.exists() {
            let config_content = std::fs::read_to_string(&config_path).map_err(|e| {
                ExtensionError::ManifestError {
                    reason: format!("Cannot read haextension.config.json: {e}"),
                }
            })?;

            let config: serde_json::Value = serde_json::from_str(&config_content).map_err(|e| {
                ExtensionError::ManifestError {
                    reason: format!("Invalid haextension.config.json: {e}"),
                }
            })?;

            let dir = config
                .get("dev")
                .and_then(|dev| dev.get("haextension_dir"))
                .and_then(|dir| dir.as_str())
                .unwrap_or("haextension")
                .to_string();

            dir
        } else {
            "haextension".to_string()
        };

        // Validate manifest path using helper function
        let manifest_relative_path = format!("{haextension_dir}/manifest.json");
        let manifest_path = Self::validate_path_in_directory(&temp, &manifest_relative_path, true)?
            .ok_or_else(|| ExtensionError::ManifestError {
                reason: format!("manifest.json not found at {haextension_dir}/manifest.json"),
            })?;

        let actual_dir = temp.clone();
        let manifest_content =
            std::fs::read_to_string(&manifest_path).map_err(|e| ExtensionError::ManifestError {
                reason: format!("Cannot read manifest: {e}"),
            })?;

        let mut manifest: ExtensionManifest = serde_json::from_str(&manifest_content)?;

        // Find icon path using FsExt for better Android compatibility
        // Returns relative path directly (no conversion needed)
        manifest.icon = Self::find_icon(
            app_handle,
            &actual_dir,
            &haextension_dir,
            manifest.icon.as_deref(),
        );

        let content_hash =
            ExtensionCrypto::hash_directory(&actual_dir, &manifest_path).map_err(|e| {
                ExtensionError::SignatureVerificationFailed {
                    reason: e.to_string(),
                }
            })?;

        Ok(ExtractedExtension {
            temp_dir: actual_dir,
            manifest,
            content_hash,
        })
    }

    pub fn get_base_extension_dir(
        &self,
        app_handle: &AppHandle,
    ) -> Result<PathBuf, ExtensionError> {
        let path = app_handle
            .path()
            .app_local_data_dir()
            .map_err(|e| ExtensionError::Filesystem {
                source: std::io::Error::new(std::io::ErrorKind::NotFound, e.to_string()),
            })?
            .join("extensions");

        // Sicherstellen, dass das Basisverzeichnis existiert
        if !path.exists() {
            fs::create_dir_all(&path)
                .map_err(|e| ExtensionError::filesystem_with_path(path.display().to_string(), e))?;
        }
        Ok(path)
    }

    pub fn get_extension_dir(
        &self,
        app_handle: &AppHandle,
        public_key: &str,
        extension_name: &str,
        extension_version: &str,
    ) -> Result<PathBuf, ExtensionError> {
        let specific_extension_dir = self
            .get_base_extension_dir(app_handle)?
            .join(public_key)
            .join(extension_name)
            .join(extension_version);

        Ok(specific_extension_dir)
    }

    pub fn add_production_extension(&self, extension: Extension) -> Result<(), ExtensionError> {
        if extension.id.is_empty() {
            return Err(ExtensionError::ValidationError {
                reason: "Extension ID cannot be empty".to_string(),
            });
        }

        match &extension.source {
            ExtensionSource::Production { .. } => {
                let mut extensions = self.production_extensions.lock().unwrap();
                extensions.insert(extension.id.clone(), extension);
                Ok(())
            }
            _ => Err(ExtensionError::ValidationError {
                reason: "Expected Production source".to_string(),
            }),
        }
    }

    pub fn add_dev_extension(&self, extension: Extension) -> Result<(), ExtensionError> {
        if extension.id.is_empty() {
            return Err(ExtensionError::ValidationError {
                reason: "Extension ID cannot be empty".to_string(),
            });
        }

        match &extension.source {
            ExtensionSource::Development { .. } => {
                let mut extensions = self.dev_extensions.lock().unwrap();
                extensions.insert(extension.id.clone(), extension);
                Ok(())
            }
            _ => Err(ExtensionError::ValidationError {
                reason: "Expected Development source".to_string(),
            }),
        }
    }

    pub fn get_extension(&self, extension_id: &str) -> Option<Extension> {
        let dev_extensions = self.dev_extensions.lock().unwrap();
        if let Some(extension) = dev_extensions.get(extension_id) {
            return Some(extension.clone());
        }

        let prod_extensions = self.production_extensions.lock().unwrap();
        prod_extensions.get(extension_id).cloned()
    }

    /// Get all installed extensions (both dev and production)
    pub fn get_all_extensions(&self) -> Result<Vec<Extension>, ExtensionError> {
        let mut extensions = Vec::new();

        // Collect dev extensions
        let dev_extensions = self
            .dev_extensions
            .lock()
            .map_err(|e| ExtensionError::MutexPoisoned {
                reason: e.to_string(),
            })?;
        extensions.extend(dev_extensions.values().cloned());

        // Collect production extensions
        let prod_extensions = self
            .production_extensions
            .lock()
            .map_err(|e| ExtensionError::MutexPoisoned {
                reason: e.to_string(),
            })?;
        extensions.extend(prod_extensions.values().cloned());

        Ok(extensions)
    }

    /// Find extension ID by public_key and name (checks dev extensions first, then production)
    fn find_extension_id_by_public_key_and_name(
        &self,
        public_key: &str,
        name: &str,
    ) -> Result<Option<(String, Extension)>, ExtensionError> {
        // 1. Check dev extensions first (higher priority)
        let dev_extensions =
            self.dev_extensions
                .lock()
                .map_err(|e| ExtensionError::MutexPoisoned {
                    reason: e.to_string(),
                })?;

        for (id, ext) in dev_extensions.iter() {
            if ext.manifest.public_key == public_key && ext.manifest.name == name {
                return Ok(Some((id.clone(), ext.clone())));
            }
        }

        // 2. Check production extensions
        let prod_extensions =
            self.production_extensions
                .lock()
                .map_err(|e| ExtensionError::MutexPoisoned {
                    reason: e.to_string(),
                })?;

        for (id, ext) in prod_extensions.iter() {
            if ext.manifest.public_key == public_key && ext.manifest.name == name {
                return Ok(Some((id.clone(), ext.clone())));
            }
        }

        Ok(None)
    }

    /// Get extension by public_key and name (used by frontend)
    pub fn get_extension_by_public_key_and_name(
        &self,
        public_key: &str,
        name: &str,
    ) -> Result<Option<Extension>, ExtensionError> {
        Ok(self
            .find_extension_id_by_public_key_and_name(public_key, name)?
            .map(|(_, ext)| ext))
    }

    pub fn remove_extension(&self, public_key: &str, name: &str) -> Result<(), ExtensionError> {
        let (id, _) = self
            .find_extension_id_by_public_key_and_name(public_key, name)?
            .ok_or_else(|| ExtensionError::NotFound {
                public_key: public_key.to_string(),
                name: name.to_string(),
            })?;

        // Remove from dev extensions first
        {
            let mut dev_extensions =
                self.dev_extensions
                    .lock()
                    .map_err(|e| ExtensionError::MutexPoisoned {
                        reason: e.to_string(),
                    })?;
            if dev_extensions.remove(&id).is_some() {
                return Ok(());
            }
        }

        // Remove from production extensions
        {
            let mut prod_extensions =
                self.production_extensions
                    .lock()
                    .map_err(|e| ExtensionError::MutexPoisoned {
                        reason: e.to_string(),
                    })?;
            prod_extensions.remove(&id);
        }

        Ok(())
    }

    /// Update the display mode of an extension (works for both dev and production extensions)
    /// For production extensions, also persists the change to the database.
    pub fn update_display_mode(
        &self,
        extension_id: &str,
        display_mode: crate::extension::core::manifest::DisplayMode,
        state: &State<'_, AppState>,
    ) -> Result<(), ExtensionError> {
        // Try dev extensions first (in-memory only, no database persistence)
        {
            let mut dev_extensions =
                self.dev_extensions
                    .lock()
                    .map_err(|e| ExtensionError::MutexPoisoned {
                        reason: e.to_string(),
                    })?;
            if let Some(extension) = dev_extensions.get_mut(extension_id) {
                extension.manifest.display_mode = Some(display_mode);
                return Ok(());
            }
        }

        // Try production extensions (update in-memory + persist to database)
        {
            let mut prod_extensions =
                self.production_extensions
                    .lock()
                    .map_err(|e| ExtensionError::MutexPoisoned {
                        reason: e.to_string(),
                    })?;
            if let Some(extension) = prod_extensions.get_mut(extension_id) {
                // Persist to database using CRDT-aware update
                let display_mode_str = format!("{:?}", display_mode).to_lowercase();

                // Update in-memory state
                extension.manifest.display_mode = Some(display_mode);
                let sql = format!(
                    "UPDATE {} SET display_mode = ? WHERE id = ?",
                    TABLE_EXTENSIONS
                );
                let params = vec![
                    JsonValue::String(display_mode_str),
                    JsonValue::String(extension_id.to_string()),
                ];

                let hlc_guard = state.hlc.lock().map_err(|e| ExtensionError::MutexPoisoned {
                    reason: format!("Failed to lock HLC: {}", e),
                })?;
                execute_with_crdt(sql, params, &state.db, &hlc_guard)?;

                return Ok(());
            }
        }

        Err(ExtensionError::ValidationError {
            reason: format!("Extension with id '{}' not found", extension_id),
        })
    }

    /// Removes an extension from the system
    ///
    /// # Arguments
    /// * `app_handle` - Tauri app handle
    /// * `public_key` - Extension's public key
    /// * `extension_name` - Extension name
    /// * `extension_version` - Extension version
    /// * `delete_data` - If true, deletes all extension tables and data. If false, only removes the extension entry (data persists for sync).
    /// * `state` - App state
    pub async fn remove_extension_internal(
        &self,
        app_handle: &AppHandle,
        public_key: &str,
        extension_name: &str,
        extension_version: &str,
        delete_data: bool,
        state: &State<'_, AppState>,
    ) -> Result<(), ExtensionError> {
        // Get the extension from memory to get its ID
        let extension = self
            .get_extension_by_public_key_and_name(public_key, extension_name)?
            .ok_or_else(|| ExtensionError::NotFound {
                public_key: public_key.to_string(),
                name: extension_name.to_string(),
            })?;

        eprintln!("DEBUG: Removing extension with ID: {}", extension.id);
        eprintln!("DEBUG: Extension name: {extension_name}, version: {extension_version}, delete_data: {delete_data}");

        // Only delete DB entries if delete_data is true (complete removal)
        // For updates (delete_data=false), we keep the DB entry and permissions
        if delete_data {
            // Lösche Permissions und Extension-Eintrag in einer Transaktion
            with_connection(&state.db, |conn| {
                // Disable foreign key constraints BEFORE starting the transaction
                // (PRAGMA changes don't take effect within an active transaction)
                conn.execute("PRAGMA foreign_keys = OFF", [])
                    .map_err(DatabaseError::from)?;

                let tx = conn.transaction().map_err(DatabaseError::from)?;

                let hlc_service = state.hlc.lock().map_err(|_| DatabaseError::MutexPoisoned {
                    reason: "Failed to lock HLC service".to_string(),
                })?;

                // Lösche alle Permissions mit extension_id
                eprintln!(
                    "DEBUG: Deleting permissions for extension_id: {}",
                    extension.id
                );
                PermissionManager::delete_permissions_in_transaction(&tx, &hlc_service, &extension.id)?;

                // Lösche alle Tabellen der Extension
                eprintln!(
                    "DEBUG: Dropping tables for extension {}::{}",
                    public_key, extension_name
                );
                let dropped_tables = drop_extension_tables(&tx, public_key, extension_name)?;
                if !dropped_tables.is_empty() {
                    eprintln!("DEBUG: Dropped tables: {:?}", dropped_tables);
                }

                // Lösche Extension-Eintrag mit extension_id
                let sql = format!("DELETE FROM {TABLE_EXTENSIONS} WHERE id = ?");
                eprintln!("DEBUG: Executing SQL: {} with id = {}", sql, extension.id);
                SqlExecutor::execute_internal_typed(
                    &tx,
                    &hlc_service,
                    &sql,
                    rusqlite::params![&extension.id],
                )?;

                eprintln!("DEBUG: Committing transaction");
                let commit_result = tx.commit().map_err(DatabaseError::from);

                // Re-enable foreign key constraints after transaction
                conn.execute("PRAGMA foreign_keys = ON", [])
                    .map_err(DatabaseError::from)?;

                commit_result
            })?;

            eprintln!("DEBUG: Transaction committed successfully");
        } else {
            eprintln!("DEBUG: Keeping DB entry and permissions (delete_data=false, update mode)");
        }

        // Entferne aus dem In-Memory-Manager
        self.remove_extension(public_key, extension_name)?;

        // Lösche nur den spezifischen Versions-Ordner: public_key/name/version
        let extension_dir =
            self.get_extension_dir(app_handle, public_key, extension_name, extension_version)?;

        if extension_dir.exists() {
            std::fs::remove_dir_all(&extension_dir).map_err(|e| {
                ExtensionError::filesystem_with_path(extension_dir.display().to_string(), e)
            })?;

            // Versuche, leere Parent-Ordner zu löschen
            // 1. Extension-Name-Ordner (key_hash/name)
            if let Some(name_dir) = extension_dir.parent() {
                if name_dir.exists() {
                    if let Ok(entries) = std::fs::read_dir(name_dir) {
                        if entries.count() == 0 {
                            let _ = std::fs::remove_dir(name_dir);

                            // 2. Key-Hash-Ordner (key_hash) - nur wenn auch leer
                            if let Some(key_hash_dir) = name_dir.parent() {
                                if key_hash_dir.exists() {
                                    if let Ok(entries) = std::fs::read_dir(key_hash_dir) {
                                        if entries.count() == 0 {
                                            let _ = std::fs::remove_dir(key_hash_dir);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn preview_extension_internal(
        &self,
        app_handle: &AppHandle,
        file_bytes: Vec<u8>,
    ) -> Result<ExtensionPreview, ExtensionError> {
        let extracted =
            Self::extract_and_validate_extension(file_bytes, "haexspace_preview", app_handle)?;

        // Validate public key format (early error for invalid extensions)
        validate_public_key(&extracted.manifest.public_key)?;

        let is_valid_signature = ExtensionCrypto::verify_signature(
            &extracted.manifest.public_key,
            &extracted.content_hash,
            &extracted.manifest.signature,
        )
        .is_ok();

        let editable_permissions = extracted.manifest.to_editable_permissions();

        Ok(ExtensionPreview {
            manifest: extracted.manifest.clone(),
            is_valid_signature,
            editable_permissions,
        })
    }

    /// Register extension metadata in the database (UPSERT pattern).
    /// This handles extensions that may already exist from sync.
    /// Returns the extension ID (existing or newly generated).
    pub fn register_extension_in_database(
        &self,
        manifest: &ExtensionManifest,
        custom_permissions: &EditablePermissions,
        state: &State<'_, AppState>,
    ) -> Result<String, ExtensionError> {
        // 1. Check if extension already exists (e.g., from sync) using select_with_crdt
        // This automatically filters out tombstoned (soft-deleted) entries
        let check_sql = format!(
            "SELECT id FROM {TABLE_EXTENSIONS} WHERE public_key = ? AND name = ?"
        );
        let check_params = vec![
            JsonValue::String(manifest.public_key.clone()),
            JsonValue::String(manifest.name.clone()),
        ];
        let existing_results = select_with_crdt(check_sql, check_params, &state.db)?;
        let existing_id: Option<String> = existing_results
            .first()
            .and_then(|row| row.first())
            .and_then(|v| v.as_str())
            .map(String::from);

        eprintln!(
            "DEBUG: [register_extension_in_database] Check for existing extension: public_key={}, name={}, found={:?}",
            manifest.public_key, manifest.name, existing_id
        );

        // 2. Perform the actual INSERT or UPDATE in a transaction
        let actual_id = with_connection(&state.db, |conn| {
            let tx = conn.transaction().map_err(DatabaseError::from)?;

            let hlc_service_guard = state.hlc.lock().map_err(|_| DatabaseError::MutexPoisoned {
                reason: "Failed to lock HLC service".to_string(),
            })?;
            let hlc_service = hlc_service_guard.clone();
            drop(hlc_service_guard);

            let actual_id = if let Some(existing_id) = existing_id {
                // Extension exists (probably from sync), update it
                eprintln!(
                    "Extension {}:{} already exists with id {}, updating instead of inserting",
                    manifest.public_key, manifest.name, existing_id
                );
                let update_ext_sql = format!(
                    "UPDATE {TABLE_EXTENSIONS} SET version = ?, author = ?, entry = ?, icon = ?, signature = ?, homepage = ?, description = ?, enabled = ?, single_instance = ?, display_mode = ? WHERE id = ?"
                );

                SqlExecutor::execute_internal_typed(
                    &tx,
                    &hlc_service,
                    &update_ext_sql,
                    rusqlite::params![
                        manifest.version,
                        manifest.author,
                        manifest.entry,
                        manifest.icon,
                        manifest.signature,
                        manifest.homepage,
                        manifest.description,
                        true, // enabled
                        manifest.single_instance.unwrap_or(false),
                        manifest
                            .display_mode
                            .as_ref()
                            .map(|dm| format!("{:?}", dm).to_lowercase())
                            .unwrap_or_else(|| "auto".to_string()),
                        existing_id,
                    ],
                )?;
                existing_id
            } else {
                // New extension, generate UUID and insert
                let new_extension_id = uuid::Uuid::new_v4().to_string();
                eprintln!(
                    "DEBUG: [register_extension_in_database] Inserting NEW extension: id={}, name={}, version={}",
                    new_extension_id, manifest.name, manifest.version
                );
                let insert_ext_sql = format!(
                    "INSERT INTO {TABLE_EXTENSIONS} (id, name, version, author, entry, icon, public_key, signature, homepage, description, enabled, single_instance, display_mode) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
                );

                SqlExecutor::execute_internal_typed(
                    &tx,
                    &hlc_service,
                    &insert_ext_sql,
                    rusqlite::params![
                        new_extension_id,
                        manifest.name,
                        manifest.version,
                        manifest.author,
                        manifest.entry,
                        manifest.icon,
                        manifest.public_key,
                        manifest.signature,
                        manifest.homepage,
                        manifest.description,
                        true, // enabled
                        manifest.single_instance.unwrap_or(false),
                        manifest
                            .display_mode
                            .as_ref()
                            .map(|dm| format!("{:?}", dm).to_lowercase())
                            .unwrap_or_else(|| "auto".to_string()),
                    ],
                )?;
                new_extension_id
            };

            // 2. Permissions: Delete existing permissions for this extension (if updating)
            // Use CRDT-aware delete function to properly handle tombstones
            PermissionManager::delete_permissions_in_transaction(&tx, &hlc_service, &actual_id)?;

            // 3. Permissions: Recreate with correct extension_id
            let permissions = custom_permissions.to_internal_permissions(&actual_id);
            let insert_perm_sql = format!(
                "INSERT INTO {TABLE_EXTENSION_PERMISSIONS} (id, extension_id, resource_type, action, target, constraints, status) VALUES (?, ?, ?, ?, ?, ?, ?)"
            );

            for perm in &permissions {
                use crate::database::generated::HaexExtensionPermissions;
                let db_perm: HaexExtensionPermissions = perm.into();

                SqlExecutor::execute_internal_typed(
                    &tx,
                    &hlc_service,
                    &insert_perm_sql,
                    rusqlite::params![
                        db_perm.id,
                        db_perm.extension_id,
                        db_perm.resource_type,
                        db_perm.action,
                        db_perm.target,
                        db_perm.constraints,
                        db_perm.status,
                    ],
                )?;
            }

            tx.commit().map_err(DatabaseError::from)?;
            Ok(actual_id)
        })?;

        Ok(actual_id)
    }

    /// Install extension files to the local filesystem.
    /// Extracts the bundle, copies files to the extensions directory,
    /// and loads the extension into memory.
    fn install_extension_files(
        &self,
        app_handle: &AppHandle,
        extracted: &ExtractedExtension,
        extension_id: &str,
    ) -> Result<PathBuf, ExtensionError> {
        eprintln!("DEBUG: [install_extension_files] Installing extension id={}, name={}, version={}",
            extension_id, extracted.manifest.name, extracted.manifest.version);

        let extensions_dir = self.get_extension_dir(
            app_handle,
            &extracted.manifest.public_key,
            &extracted.manifest.name,
            &extracted.manifest.version,
        )?;

        eprintln!("DEBUG: [install_extension_files] Target directory: {:?}", extensions_dir);

        // If extension version already exists, remove it completely before installing
        if extensions_dir.exists() {
            eprintln!(
                "Extension version already exists at {}, removing old version",
                extensions_dir.display()
            );
            std::fs::remove_dir_all(&extensions_dir).map_err(|e| {
                ExtensionError::filesystem_with_path(extensions_dir.display().to_string(), e)
            })?;
        }

        std::fs::create_dir_all(&extensions_dir).map_err(|e| {
            ExtensionError::filesystem_with_path(extensions_dir.display().to_string(), e)
        })?;

        // Copy contents of extracted.temp_dir to extensions_dir
        for entry in fs::read_dir(&extracted.temp_dir).map_err(|e| {
            ExtensionError::filesystem_with_path(extracted.temp_dir.display().to_string(), e)
        })? {
            let entry = entry.map_err(|e| ExtensionError::Filesystem { source: e })?;
            let path = entry.path();
            let file_name = entry.file_name();
            let dest_path = extensions_dir.join(&file_name);

            if path.is_dir() {
                copy_directory(
                    path.to_string_lossy().to_string(),
                    dest_path.to_string_lossy().to_string(),
                )?;
            } else {
                fs::copy(&path, &dest_path).map_err(|e| {
                    ExtensionError::filesystem_with_path(path.display().to_string(), e)
                })?;
            }
        }

        // Update icon path to point to installed location (instead of temp dir)
        let mut installed_manifest = extracted.manifest.clone();
        if let Some(ref temp_icon_path) = installed_manifest.icon {
            let temp_icon = PathBuf::from(temp_icon_path);
            if let Ok(relative_icon) = temp_icon.strip_prefix(&extracted.temp_dir) {
                installed_manifest.icon = Some(
                    extensions_dir
                        .join(relative_icon)
                        .to_string_lossy()
                        .to_string(),
                );
            }
        }

        // Load extension into memory
        let extension = Extension {
            id: extension_id.to_string(),
            source: ExtensionSource::Production {
                path: extensions_dir.clone(),
                version: installed_manifest.version.clone(),
            },
            manifest: installed_manifest,
            enabled: true,
            last_accessed: SystemTime::now(),
        };

        self.add_production_extension(extension)?;

        Ok(extensions_dir)
    }

    /// Install extension files from bytes.
    /// Use when extension is already registered in DB (e.g., from sync or update).
    /// Validates signature, extracts files, registers migrations.
    /// Also updates the version in the database to the new version from the manifest.
    pub async fn install_extension_files_from_bytes(
        &self,
        app_handle: &AppHandle,
        file_bytes: Vec<u8>,
        extension_id: &str,
        state: &State<'_, AppState>,
    ) -> Result<String, ExtensionError> {
        let extracted =
            Self::extract_and_validate_extension(file_bytes, "haexspace_ext", app_handle)?;

        // Validate that the public key is a valid Ed25519 key format
        validate_public_key(&extracted.manifest.public_key)?;

        // Verify signature
        ExtensionCrypto::verify_signature(
            &extracted.manifest.public_key,
            &extracted.content_hash,
            &extracted.manifest.signature,
        )
        .map_err(|e| ExtensionError::SignatureVerificationFailed { reason: e })?;

        // Install files locally
        let extensions_dir = self.install_extension_files(app_handle, &extracted, extension_id)?;

        // Update version and other metadata in DB (for updates)
        self.update_extension_version_in_database(&extracted.manifest, extension_id, state)?;

        // Register and apply migrations from the bundle
        Self::register_bundle_migrations(&extensions_dir, &extracted.manifest, extension_id, state)
            .await?;

        Ok(extension_id.to_string())
    }

    /// Update extension version and metadata in database.
    /// Used when installing a new version of an existing extension.
    fn update_extension_version_in_database(
        &self,
        manifest: &ExtensionManifest,
        extension_id: &str,
        state: &State<'_, AppState>,
    ) -> Result<(), ExtensionError> {
        with_connection(&state.db, |conn| {
            let tx = conn.transaction().map_err(DatabaseError::from)?;

            let hlc_service_guard = state.hlc.lock().map_err(|_| DatabaseError::MutexPoisoned {
                reason: "Failed to lock HLC service".to_string(),
            })?;
            let hlc_service = hlc_service_guard.clone();
            drop(hlc_service_guard);

            eprintln!(
                "Updating extension {} to version {}",
                extension_id, manifest.version
            );

            let update_sql = format!(
                "UPDATE {TABLE_EXTENSIONS} SET version = ?, author = ?, entry = ?, icon = ?, signature = ?, homepage = ?, description = ? WHERE id = ?"
            );

            SqlExecutor::execute_internal_typed(
                &tx,
                &hlc_service,
                &update_sql,
                rusqlite::params![
                    manifest.version,
                    manifest.author,
                    manifest.entry,
                    manifest.icon,
                    manifest.signature,
                    manifest.homepage,
                    manifest.description,
                    extension_id,
                ],
            )?;

            tx.commit().map_err(DatabaseError::from)?;
            Ok(())
        })
        .map_err(ExtensionError::from)
    }

    /// Full installation: Register in DB + Install files.
    pub async fn install_extension_with_permissions_internal(
        &self,
        app_handle: AppHandle,
        file_bytes: Vec<u8>,
        custom_permissions: EditablePermissions,
        state: &State<'_, AppState>,
    ) -> Result<String, ExtensionError> {
        let extracted =
            Self::extract_and_validate_extension(file_bytes, "haexspace_ext", &app_handle)?;

        // Validate that the public key is a valid Ed25519 key format
        validate_public_key(&extracted.manifest.public_key)?;

        // Verify signature
        ExtensionCrypto::verify_signature(
            &extracted.manifest.public_key,
            &extracted.content_hash,
            &extracted.manifest.signature,
        )
        .map_err(|e| ExtensionError::SignatureVerificationFailed { reason: e })?;

        // Step 1: Register in database (UPSERT - handles sync case)
        let extension_id =
            self.register_extension_in_database(&extracted.manifest, &custom_permissions, state)?;

        // Step 2: Install files locally
        let extensions_dir =
            self.install_extension_files(&app_handle, &extracted, &extension_id)?;

        // Step 3: Register and apply migrations from the bundle
        Self::register_bundle_migrations(
            &extensions_dir,
            &extracted.manifest,
            &extension_id,
            state,
        )
        .await?;

        Ok(extension_id)
    }

    /// Scannt das Dateisystem beim Start und lädt alle installierten Erweiterungen.
    pub async fn load_installed_extensions(
        &self,
        app_handle: &AppHandle,
        state: &State<'_, AppState>,
    ) -> Result<Vec<String>, ExtensionError> {
        // Clear existing data
        self.production_extensions
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

        // Lade alle Daten aus der Datenbank
        // Use select_with_crdt to automatically filter out tombstoned (soft-deleted) entries
        let sql = format!(
            "SELECT id, name, version, author, entry, icon, public_key, signature, homepage, description, enabled, single_instance, display_mode FROM {TABLE_EXTENSIONS}"
        );
        eprintln!("DEBUG: SQL Query (will be transformed by select_with_crdt): {sql}");

        let results = select_with_crdt(sql, vec![], &state.db)?;
        eprintln!("DEBUG: Query returned {} results", results.len());

        let mut extensions = Vec::new();
        for row in results {
            // Wir erwarten die Werte in der Reihenfolge der SELECT-Anweisung
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

            extensions.push(ExtensionDataFromDb {
                id,
                manifest,
                enabled,
            });
        }

        // Schritt 2: Die gesammelten Daten verarbeiten (Dateisystem, State-Mutationen).
        let mut loaded_extension_ids = Vec::new();

        eprintln!("DEBUG: Found {} extensions in database", extensions.len());

        for extension_data in extensions {
            let extension_id = extension_data.id;
            eprintln!("DEBUG: Processing extension: {extension_id} (name={}, version={})",
                extension_data.manifest.name, extension_data.manifest.version);

            // Use public_key/name/version path structure
            let extension_path = self.get_extension_dir(
                app_handle,
                &extension_data.manifest.public_key,
                &extension_data.manifest.name,
                &extension_data.manifest.version,
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
                        id: extension_id.clone(),
                        public_key: extension_data.manifest.public_key.clone(),
                        name: extension_data.manifest.name.clone(),
                        version: extension_data.manifest.version.clone(),
                    });
                continue;
            }

            // Read haextension_dir from config if it exists, otherwise use default
            let config_path = extension_path.join("haextension.config.json");
            let haextension_dir = if config_path.exists() {
                match std::fs::read_to_string(&config_path) {
                    Ok(config_content) => {
                        match serde_json::from_str::<serde_json::Value>(&config_content) {
                            Ok(config) => config
                                .get("dev")
                                .and_then(|dev| dev.get("haextension_dir"))
                                .and_then(|dir| dir.as_str())
                                .unwrap_or("haextension")
                                .to_string(),
                            Err(_) => "haextension".to_string(),
                        }
                    }
                    Err(_) => "haextension".to_string(),
                }
            } else {
                "haextension".to_string()
            };

            // Validate manifest.json path using helper function
            let manifest_relative_path = format!("{haextension_dir}/manifest.json");
            if Self::validate_path_in_directory(&extension_path, &manifest_relative_path, true)?
                .is_none()
            {
                eprintln!(
                    "DEBUG: manifest.json missing or invalid for: {extension_id} at {haextension_dir}/manifest.json"
                );
                self.missing_extensions
                    .lock()
                    .map_err(|e| ExtensionError::MutexPoisoned {
                        reason: e.to_string(),
                    })?
                    .push(MissingExtension {
                        id: extension_id.clone(),
                        public_key: extension_data.manifest.public_key.clone(),
                        name: extension_data.manifest.name.clone(),
                        version: extension_data.manifest.version.clone(),
                    });
                continue;
            }

            eprintln!("DEBUG: Extension loaded successfully: {extension_id}");

            // Resolve icon path to installed location
            let mut manifest = extension_data.manifest;
            manifest.icon = Self::validate_and_resolve_icon_path(
                &extension_path,
                &haextension_dir,
                manifest.icon.as_deref(),
            )?;

            let extension = Extension {
                id: extension_id.clone(),
                source: ExtensionSource::Production {
                    path: extension_path,
                    version: manifest.version.clone(),
                },
                manifest,
                enabled: extension_data.enabled,
                last_accessed: SystemTime::now(),
            };

            loaded_extension_ids.push(extension_id.clone());
            self.add_production_extension(extension)?;
        }

        Ok(loaded_extension_ids)
    }

    /// Registers and applies migrations from the extension bundle at install time.
    ///
    /// This reads the migrations from the bundle's migrations_dir (specified in manifest),
    /// validates them, executes them, and stores them as applied in the database.
    ///
    /// # Arguments
    /// * `extension_dir` - Path to the installed extension directory
    /// * `manifest` - The extension manifest
    /// * `extension_id` - The database ID of the extension
    /// * `state` - App state
    pub async fn register_bundle_migrations(
        extension_dir: &PathBuf,
        manifest: &ExtensionManifest,
        extension_id: &str,
        state: &State<'_, AppState>,
    ) -> Result<(), ExtensionError> {
        let migrations_dir = match &manifest.migrations_dir {
            Some(dir) => dir,
            None => {
                eprintln!(
                    "[INSTALL_MIGRATIONS] No migrations_dir in manifest for {}::{}",
                    manifest.public_key, manifest.name
                );
                return Ok(());
            }
        };

        eprintln!(
            "[INSTALL_MIGRATIONS] Loading migrations from {} for {}::{}",
            migrations_dir, manifest.public_key, manifest.name
        );

        // Validate migrations_dir path to prevent path traversal attacks
        // The migrations directory MUST be within the extension directory
        let _migrations_path = Self::validate_path_in_directory(
            extension_dir,
            migrations_dir,
            true,
        )?
        .ok_or_else(|| ExtensionError::ValidationError {
            reason: format!(
                "Migrations directory '{}' does not exist or is outside extension directory",
                migrations_dir
            ),
        })?;

        // Read _journal.json to get migration order
        let journal_relative_path = format!("{}/meta/_journal.json", migrations_dir);
        let journal_path =
            Self::validate_path_in_directory(extension_dir, &journal_relative_path, true)?
                .ok_or_else(|| {
                    eprintln!(
                        "[INSTALL_MIGRATIONS] No _journal.json found at {}",
                        journal_relative_path
                    );
                    ExtensionError::ValidationError {
                        reason: format!(
                            "_journal.json not found at {}/meta/_journal.json",
                            migrations_dir
                        ),
                    }
                })?;

        let journal_content = fs::read_to_string(&journal_path).map_err(|e| {
            ExtensionError::filesystem_with_path(journal_path.display().to_string(), e)
        })?;

        let journal: MigrationJournal =
            serde_json::from_str(&journal_content).map_err(|e| ExtensionError::ManifestError {
                reason: format!("Failed to parse _journal.json: {}", e),
            })?;

        eprintln!(
            "[INSTALL_MIGRATIONS] Found {} migrations in journal",
            journal.entries.len()
        );

        // Sort entries by idx to ensure correct order
        let mut entries = journal.entries.clone();
        entries.sort_by_key(|e| e.idx);

        // Process each migration in order
        for entry in &entries {
            // Validate SQL file path to prevent path traversal
            let sql_relative_path = format!("{}/{}.sql", migrations_dir, entry.tag);
            let sql_file_path =
                match Self::validate_path_in_directory(extension_dir, &sql_relative_path, true)? {
                    Some(path) => path,
                    None => {
                        eprintln!(
                            "[INSTALL_MIGRATIONS] SQL file not found: {}",
                            sql_relative_path
                        );
                        continue;
                    }
                };

            let sql_content = fs::read_to_string(&sql_file_path).map_err(|e| {
                ExtensionError::filesystem_with_path(sql_file_path.display().to_string(), e)
            })?;

            eprintln!("[INSTALL_MIGRATIONS] Processing migration: {}", entry.tag);

            // Create context for SQL execution (production mode for installed extensions)
            let ctx = ExtensionSqlContext::new(
                manifest.public_key.clone(),
                manifest.name.clone(),
                false, // is_dev_mode = false for production installations
            );

            // Execute all statements using the helper function
            // This validates table prefixes and executes with CRDT support
            let stmt_count = execute_migration_statements(&ctx, &sql_content, state.inner())?;

            eprintln!(
                "[INSTALL_MIGRATIONS] Migration '{}' executed ({} statements)",
                entry.tag, stmt_count
            );

            // Store migration as applied in the database
            with_connection(&state.db, |conn| {
                let tx = conn.transaction().map_err(DatabaseError::from)?;
                let migration_id = uuid::Uuid::new_v4().to_string();

                let hlc_service = state.hlc.lock().map_err(|_| DatabaseError::MutexPoisoned {
                    reason: "Failed to lock HLC service".to_string(),
                })?;

                let insert_sql = format!(
                    "INSERT OR IGNORE INTO {TABLE_EXTENSION_MIGRATIONS}
                     (id, extension_id, extension_version, migration_name, sql_statement)
                     VALUES (?, ?, ?, ?, ?)"
                );
                let params: Vec<JsonValue> = vec![
                    JsonValue::String(migration_id),
                    JsonValue::String(extension_id.to_string()),
                    JsonValue::String(manifest.version.clone()),
                    JsonValue::String(entry.tag.clone()),
                    JsonValue::String(sql_content.clone()),
                ];
                SqlExecutor::execute_internal(&tx, &hlc_service, &insert_sql, &params)?;

                tx.commit().map_err(DatabaseError::from)?;
                Ok::<(), DatabaseError>(())
            })?;

            eprintln!(
                "[INSTALL_MIGRATIONS] Migration '{}' applied and stored",
                entry.tag
            );
        }

        eprintln!(
            "[INSTALL_MIGRATIONS] ✅ Completed migration registration for {}::{}",
            manifest.public_key, manifest.name
        );

        Ok(())
    }
}
