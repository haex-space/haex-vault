// src-tauri/src/extension/core/manager.rs
//
// ExtensionManager struct and core CRUD operations.
// Additional functionality is split across:
// - loader.rs: load_installed_extensions
// - installer.rs: extract, install, register extensions
// - removal.rs: remove_extension_internal
// - migrations.rs: register_bundle_migrations
// - path_utils.rs: path validation helpers

use crate::database::core::{execute_with_crdt, with_connection};
use crate::database::error::DatabaseError;
use crate::extension::core::types::Extension;
use crate::extension::database::executor::SqlExecutor;
use crate::extension::error::ExtensionError;
use crate::extension::permissions::types::ExtensionPermission;
use super::queries::{SQL_UPDATE_EXTENSION_DISPLAY_MODE, SQL_UPDATE_EXTENSION_ENABLED};
use crate::AppState;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Manager, State};

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

#[derive(Default)]
pub struct ExtensionManager {
    pub available_extensions: Mutex<HashMap<String, Extension>>,
    pub permission_cache: Mutex<HashMap<String, CachedPermission>>,
    pub missing_extensions: Mutex<Vec<MissingExtension>>,
}

impl ExtensionManager {
    pub fn new() -> Self {
        Self::default()
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

        // Ensure base directory exists
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

    /// Verifies that an extension with the given triple is actually installed.
    ///
    /// Used by the `haex-extension://` protocol handler before resolving asset
    /// paths: the handler receives `(public_key, name, version)` from a
    /// caller-controlled URL, and `get_extension_dir` only constructs a path
    /// — it does not check that the extension exists. Without this guard a
    /// webview can craft `haex-extension://<base64-of-other-extension>/asset`
    /// and read the static assets of any installed extension.
    ///
    /// Returns `Ok(())` only if an extension with the matching public_key and
    /// name is registered AND its manifest version equals `extension_version`.
    pub fn verify_extension_installed(
        &self,
        public_key: &str,
        extension_name: &str,
        extension_version: &str,
    ) -> Result<(), ExtensionError> {
        let extension = self
            .get_extension_by_public_key_and_name(public_key, extension_name)?
            .ok_or_else(|| ExtensionError::NotFound {
                public_key: public_key.to_string(),
                name: extension_name.to_string(),
            })?;

        if extension.manifest.version != extension_version {
            return Err(ExtensionError::ValidationError {
                reason: format!(
                    "Version mismatch for extension {}::{}: installed {}, requested {}",
                    public_key, extension_name, extension.manifest.version, extension_version
                ),
            });
        }

        Ok(())
    }

    /// Add an extension to the in-memory manager.
    /// Accepts both Production and Development sources.
    pub fn add_extension(&self, extension: Extension) -> Result<(), ExtensionError> {
        if extension.id.is_empty() {
            return Err(ExtensionError::ValidationError {
                reason: "Extension ID cannot be empty".to_string(),
            });
        }

        let mut extensions = self.available_extensions.lock().map_err(|e| {
            ExtensionError::MutexPoisoned {
                reason: e.to_string(),
            }
        })?;
        extensions.insert(extension.id.clone(), extension);
        Ok(())
    }

    pub fn get_extension(&self, extension_id: &str) -> Option<Extension> {
        // SAFETY: `available_extensions` is a read-mostly cache of installed
        // extensions. Poison here means a previous panic occurred while the
        // map was being modified (e.g. during install/uninstall); the worst
        // case is a missing entry, which surfaces as "extension not found"
        // at the API boundary — a recoverable, user-visible error, not a
        // silent data-corruption risk. No CRDT or HLC involvement.
        //
        // Note the asymmetry vs. other methods on this struct: `add_extension`,
        // `get_all_extensions`, etc. DO propagate the poison as
        // `ExtensionError::MutexPoisoned`. We tolerate it here because the
        // `Option<Extension>` return is already the documented "not found"
        // signal — callers handle the None path. Other methods can't degrade
        // that way because they need to mutate (add) or return a complete
        // list, where a partial map yields wrong results, not a recoverable
        // miss.
        let prod_extensions = self.available_extensions.lock().unwrap_or_else(|e| e.into_inner());
        prod_extensions.get(extension_id).cloned()
    }

    /// Get all installed extensions
    pub fn get_all_extensions(&self) -> Result<Vec<Extension>, ExtensionError> {
        let prod_extensions = self
            .available_extensions
            .lock()
            .map_err(|e| ExtensionError::MutexPoisoned {
                reason: e.to_string(),
            })?;
        Ok(prod_extensions.values().cloned().collect())
    }

    /// Find extension ID by public_key and name
    pub(crate) fn find_extension_id_by_public_key_and_name(
        &self,
        public_key: &str,
        name: &str,
    ) -> Result<Option<(String, Extension)>, ExtensionError> {
        let prod_extensions =
            self.available_extensions
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

        let mut prod_extensions =
            self.available_extensions
                .lock()
                .map_err(|e| ExtensionError::MutexPoisoned {
                    reason: e.to_string(),
                })?;
        prod_extensions.remove(&id);

        Ok(())
    }

    /// Update the display mode of an extension.
    /// Persists the change to the database.
    pub fn update_display_mode(
        &self,
        extension_id: &str,
        display_mode: crate::extension::core::manifest::DisplayMode,
        state: &State<'_, AppState>,
    ) -> Result<(), ExtensionError> {
        let mut prod_extensions =
            self.available_extensions
                .lock()
                .map_err(|e| ExtensionError::MutexPoisoned {
                    reason: e.to_string(),
                })?;

        if let Some(extension) = prod_extensions.get_mut(extension_id) {
            // Persist to database using CRDT-aware update
            let display_mode_str = format!("{:?}", display_mode).to_lowercase();

            // Update in-memory state
            extension.manifest.display_mode = Some(display_mode);
            let params = vec![
                JsonValue::String(display_mode_str),
                JsonValue::String(extension_id.to_string()),
            ];

            let hlc_guard = state.lock_or_fail(
                &state.hlc,
                crate::critical::CriticalFailureCode::HlcMutexPoisoned,
                "extension::core::manager::update_display_mode",
                serde_json::json!({}),
            )?;
            execute_with_crdt(
                SQL_UPDATE_EXTENSION_DISPLAY_MODE.clone(),
                params,
                &state.db,
                &hlc_guard,
            )?;

            return Ok(());
        }

        Err(ExtensionError::ValidationError {
            reason: format!("Extension with id '{}' not found", extension_id),
        })
    }

    /// Toggle extension enabled state in database and memory.
    pub fn toggle_extension_enabled(
        &self,
        extension_id: &str,
        enabled: bool,
        state: &State<'_, AppState>,
    ) -> Result<(), ExtensionError> {
        // Update in database
        with_connection(&state.db, |conn| {
            let tx = conn.transaction().map_err(DatabaseError::from)?;

            let hlc_service = state.lock_or_fail(
                &state.hlc,
                crate::critical::CriticalFailureCode::HlcMutexPoisoned,
                "extension::core::manager::set_enabled",
                serde_json::json!({}),
            )?;

            SqlExecutor::execute_internal_typed(
                &tx,
                &hlc_service,
                &SQL_UPDATE_EXTENSION_ENABLED,
                rusqlite::params![enabled, extension_id],
            )?;

            tx.commit().map_err(DatabaseError::from)?;
            Ok(())
        })?;

        // Update in memory
        let mut extensions = self
            .available_extensions
            .lock()
            .map_err(|e| ExtensionError::MutexPoisoned {
                reason: e.to_string(),
            })?;

        if let Some(ext) = extensions.get_mut(extension_id) {
            ext.enabled = enabled;
        }

        Ok(())
    }
}
