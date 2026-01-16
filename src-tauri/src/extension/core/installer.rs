// src-tauri/src/extension/core/installer.rs
//
// Extension extraction, validation, and installation.

use crate::database::core::{select_with_crdt, with_connection};
use crate::database::error::DatabaseError;
use crate::extension::core::manifest::{EditablePermissions, ExtensionManifest, ExtensionPreview};
use crate::extension::core::path_utils::{find_icon, validate_path_in_directory};
use crate::extension::core::types::{copy_directory, Extension, ExtensionSource};
use crate::extension::crypto::ExtensionCrypto;
use crate::extension::database::executor::SqlExecutor;
use crate::extension::error::ExtensionError;
use crate::extension::permissions::manager::PermissionManager;
use crate::extension::utils::validate_public_key;
use crate::table_names::{TABLE_EXTENSIONS, TABLE_EXTENSION_PERMISSIONS};
use crate::AppState;
use serde_json::Value as JsonValue;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;
use tauri::{AppHandle, Manager, State};
use zip::ZipArchive;

use super::manager::ExtensionManager;
use super::migrations::register_bundle_migrations;

/// Temporary extraction result that cleans up on drop.
pub(crate) struct ExtractedExtension {
    pub temp_dir: PathBuf,
    pub manifest: ExtensionManifest,
    pub content_hash: String,
}

impl Drop for ExtractedExtension {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.temp_dir).ok();
    }
}

impl ExtensionManager {
    /// Extracts an extension ZIP file and validates the manifest.
    pub(crate) fn extract_and_validate_extension(
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

            config
                .get("dev")
                .and_then(|dev| dev.get("haextension_dir"))
                .and_then(|dir| dir.as_str())
                .unwrap_or("haextension")
                .to_string()
        } else {
            "haextension".to_string()
        };

        // Validate manifest path using helper function
        let manifest_relative_path = format!("{haextension_dir}/manifest.json");
        let manifest_path = validate_path_in_directory(&temp, &manifest_relative_path, true)?
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
        manifest.icon = find_icon(
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
    pub(crate) fn install_extension_files(
        &self,
        app_handle: &AppHandle,
        extracted: &ExtractedExtension,
        extension_id: &str,
    ) -> Result<PathBuf, ExtensionError> {
        eprintln!(
            "DEBUG: [install_extension_files] Installing extension id={}, name={}, version={}",
            extension_id, extracted.manifest.name, extracted.manifest.version
        );

        let extensions_dir = self.get_extension_dir(
            app_handle,
            &extracted.manifest.public_key,
            &extracted.manifest.name,
            &extracted.manifest.version,
        )?;

        eprintln!(
            "DEBUG: [install_extension_files] Target directory: {:?}",
            extensions_dir
        );

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

        self.add_extension(extension)?;

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
        register_bundle_migrations(&extensions_dir, &extracted.manifest, extension_id, state)
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
        register_bundle_migrations(&extensions_dir, &extracted.manifest, &extension_id, state)
            .await?;

        Ok(extension_id)
    }
}
