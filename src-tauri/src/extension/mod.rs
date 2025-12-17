/// src-tauri/src/extension/mod.rs
use crate::{
    database as db,
    extension::{
        core::{
            manager::ExtensionManager, EditablePermissions, ExtensionInfoResponse,
            ExtensionManifest, ExtensionPreview, PermissionEntry,
        },
        error::ExtensionError,
        permissions::{
            manager::PermissionManager,
            types::{ExtensionPermission, ResourceType},
        },
    },
    AppState,
};
use tauri::{AppHandle, State};
pub mod core;
pub mod crypto;
pub mod database;
pub mod error;
pub mod filesystem;
pub mod permissions;
pub mod utils;
pub mod web;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod webview;

#[tauri::command]
pub fn get_extension_info(
    public_key: String,
    name: String,
    state: State<AppState>,
) -> Result<ExtensionInfoResponse, ExtensionError> {
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)?
        .ok_or_else(|| ExtensionError::NotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

    ExtensionInfoResponse::from_extension(&extension)
}

#[tauri::command]
pub async fn get_all_extensions(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<ExtensionInfoResponse>, String> {
    // Check if extensions are loaded, if not load them first
    /*  let needs_loading = {
        let prod_exts = state
            .extension_manager
            .production_extensions
            .lock()
            .unwrap();
        let dev_exts = state.extension_manager.dev_extensions.lock().unwrap();
        prod_exts.is_empty() && dev_exts.is_empty()
    }; */

    /* if needs_loading { */
    state
        .extension_manager
        .load_installed_extensions(&app_handle, &state)
        .await
        .map_err(|e| format!("Failed to load extensions: {e:?}"))?;
    /* } */

    let mut extensions = Vec::new();

    // Production Extensions
    {
        let prod_exts = state
            .extension_manager
            .production_extensions
            .lock()
            .unwrap();
        for ext in prod_exts.values() {
            extensions.push(ExtensionInfoResponse::from_extension(ext)?);
        }
    }

    // Dev Extensions
    {
        let dev_exts = state.extension_manager.dev_extensions.lock().unwrap();
        for ext in dev_exts.values() {
            extensions.push(ExtensionInfoResponse::from_extension(ext)?);
        }
    }

    Ok(extensions)
}

#[tauri::command]
pub async fn preview_extension(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    file_bytes: Vec<u8>,
) -> Result<ExtensionPreview, ExtensionError> {
    state
        .extension_manager
        .preview_extension_internal(&app_handle, file_bytes)
        .await
}

/// Register extension metadata in database (UPSERT - handles sync case).
/// Takes manifest data directly - call preview_extension first to get the manifest.
/// Returns the extension ID.
#[tauri::command]
pub fn register_extension_in_database(
    manifest: ExtensionManifest,
    custom_permissions: EditablePermissions,
    state: State<'_, AppState>,
) -> Result<String, ExtensionError> {
    state
        .extension_manager
        .register_extension_in_database(&manifest, &custom_permissions, &state)
}

/// Install extension files to local filesystem.
/// Use this after register_extension_in_database or when extension
/// already exists in DB (e.g., from sync).
/// Returns the extension ID.
#[tauri::command]
pub async fn install_extension_files(
    app_handle: AppHandle,
    file_bytes: Vec<u8>,
    extension_id: String,
    state: State<'_, AppState>,
) -> Result<String, ExtensionError> {
    state
        .extension_manager
        .install_extension_files_from_bytes(&app_handle, file_bytes, &extension_id, &state)
        .await
}

/// Full installation: Register in DB + Install files.
/// Convenience function that calls both steps.
#[tauri::command]
pub async fn install_extension_with_permissions(
    app_handle: AppHandle,
    file_bytes: Vec<u8>,
    custom_permissions: EditablePermissions,
    state: State<'_, AppState>,
) -> Result<String, ExtensionError> {
    state
        .extension_manager
        .install_extension_with_permissions_internal(
            app_handle,
            file_bytes,
            custom_permissions,
            &state,
        )
        .await
}

#[tauri::command]
pub async fn remove_extension(
    app_handle: AppHandle,
    public_key: String,
    name: String,
    version: String,
    delete_data: Option<bool>,
    state: State<'_, AppState>,
) -> Result<(), ExtensionError> {
    state
        .extension_manager
        .remove_extension_internal(
            &app_handle,
            &public_key,
            &name,
            &version,
            delete_data.unwrap_or(false),
            &state,
        )
        .await
}

#[tauri::command]
pub fn is_extension_installed(
    public_key: String,
    name: String,
    extension_version: String,
    state: State<'_, AppState>,
) -> Result<bool, ExtensionError> {
    if let Some(ext) = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)?
    {
        Ok(ext.manifest.version == extension_version)
    } else {
        Ok(false)
    }
}

#[derive(serde::Deserialize, Debug)]
struct HaextensionConfig {
    dev: DevConfig,
}

#[derive(serde::Deserialize, Debug)]
struct DevConfig {
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default = "default_host")]
    host: String,
    #[serde(default = "default_haextension_dir")]
    haextension_dir: String,
}

fn default_port() -> u16 {
    5173
}

fn default_host() -> String {
    "localhost".to_string()
}

fn default_haextension_dir() -> String {
    "haextension".to_string()
}

/// Package.json structure for fallback values
#[derive(serde::Deserialize, Debug, Default)]
struct PackageJson {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    homepage: Option<String>,
}

/// Partial manifest for initial parsing (allows missing name for fallback)
#[derive(serde::Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct PartialManifest {
    name: Option<String>,
    version: Option<String>,
    author: Option<String>,
    entry: Option<String>,
    icon: Option<String>,
    public_key: String,
    signature: String,
    #[serde(default)]
    permissions: core::manifest::ExtensionPermissions,
    homepage: Option<String>,
    description: Option<String>,
    #[serde(default)]
    single_instance: Option<bool>,
    #[serde(default)]
    display_mode: Option<core::manifest::DisplayMode>,
    #[serde(default)]
    migrations_dir: Option<String>,
}

/// Check if a dev server is reachable by making a simple HTTP request
async fn check_dev_server_health(url: &str) -> bool {
    use std::time::Duration;
    use tauri_plugin_http::reqwest;

    // Try to connect with a short timeout
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build();

    if let Ok(client) = client {
        // Just check if the root responds (most dev servers respond to / with their app)
        if let Ok(response) = client.get(url).send().await {
            // Accept any response (200, 404, etc.) - we just want to know the server is running
            return response.status().as_u16() < 500;
        }
    }

    false
}

#[tauri::command]
pub async fn load_dev_extension(
    extension_path: String,
    state: State<'_, AppState>,
) -> Result<String, ExtensionError> {
    use crate::extension::core::{
        manifest::ExtensionManifest,
        types::{Extension, ExtensionSource},
    };
    use std::path::PathBuf;
    use std::time::SystemTime;

    let extension_path_buf = PathBuf::from(&extension_path);

    // 1. Read haextension.config.json to get dev server config and haextension directory
    let config_path = extension_path_buf.join("haextension.config.json");
    let (host, port, haextension_dir) = if config_path.exists() {
        let config_content =
            std::fs::read_to_string(&config_path).map_err(|e| ExtensionError::ValidationError {
                reason: format!("Failed to read haextension.config.json: {e}"),
            })?;

        let config: HaextensionConfig =
            serde_json::from_str(&config_content).map_err(|e| ExtensionError::ValidationError {
                reason: format!("Failed to parse haextension.config.json: {e}"),
            })?;

        (config.dev.host, config.dev.port, config.dev.haextension_dir)
    } else {
        // Default values if config doesn't exist
        (default_host(), default_port(), default_haextension_dir())
    };

    let dev_server_url = format!("http://{host}:{port}");
    eprintln!("📡 Dev server URL: {dev_server_url}");
    eprintln!("📁 Haextension directory: {haextension_dir}");

    // 1.5. Check if dev server is running
    if !check_dev_server_health(&dev_server_url).await {
        return Err(ExtensionError::ValidationError {
            reason: format!(
                "Dev server at {dev_server_url} is not reachable. Please start your dev server first (e.g., 'npm run dev')"
            ),
        });
    }
    eprintln!("✅ Dev server is reachable");

    // 2. Validate and build path to manifest: <extension_path>/<haextension_dir>/manifest.json
    let manifest_relative_path = format!("{haextension_dir}/manifest.json");
    let manifest_path = ExtensionManager::validate_path_in_directory(
        &extension_path_buf,
        &manifest_relative_path,
        true,
    )?
    .ok_or_else(|| ExtensionError::ManifestError {
        reason: format!(
            "Manifest not found at: {haextension_dir}/manifest.json. Make sure you run 'npx @haexspace/sdk init' first."
        ),
    })?;

    // 3. Read and parse manifest (using partial struct to allow missing fields)
    let manifest_content =
        std::fs::read_to_string(&manifest_path).map_err(|e| ExtensionError::ManifestError {
            reason: format!("Failed to read manifest: {e}"),
        })?;

    let partial_manifest: PartialManifest =
        serde_json::from_str(&manifest_content).map_err(|e| ExtensionError::ManifestError {
            reason: format!("Manifest error: {e}"),
        })?;

    // 3.5. Read package.json for fallback values (like SDK does)
    let package_json_path = extension_path_buf.join("package.json");
    let package_json: PackageJson = if package_json_path.exists() {
        let pkg_content = std::fs::read_to_string(&package_json_path).unwrap_or_default();
        serde_json::from_str(&pkg_content).unwrap_or_default()
    } else {
        PackageJson::default()
    };

    // 3.6. Merge manifest with package.json fallbacks
    let name = partial_manifest.name.or(package_json.name).ok_or_else(|| {
        ExtensionError::ManifestError {
            reason: "No name found in manifest or package.json".to_string(),
        }
    })?;

    let version = partial_manifest
        .version
        .or(package_json.version)
        .unwrap_or_else(|| "0.0.0-dev".to_string());

    let author = partial_manifest.author.or(package_json.author);
    let homepage = partial_manifest.homepage.or(package_json.homepage);

    // Resolve icon path with fallback to favicon.ico
    let resolved_icon = ExtensionManager::validate_and_resolve_icon_path(
        &extension_path_buf,
        &haextension_dir,
        partial_manifest.icon.as_deref(),
    )?;

    let manifest = ExtensionManifest {
        name,
        version,
        author,
        entry: partial_manifest.entry,
        icon: resolved_icon,
        public_key: partial_manifest.public_key,
        signature: partial_manifest.signature,
        permissions: partial_manifest.permissions,
        homepage,
        description: partial_manifest.description,
        single_instance: partial_manifest.single_instance,
        display_mode: partial_manifest.display_mode,
        migrations_dir: partial_manifest.migrations_dir,
    };

    // 3.5. Validate public key format
    utils::validate_public_key(&manifest.public_key)?;

    // 4. Generate a unique ID for dev extension: dev_<public_key>_<name>
    let extension_id = format!("dev_{}_{}", manifest.public_key, manifest.name);

    // 5. Check if dev extension already exists (allow reload)
    if let Some(existing) = state
        .extension_manager
        .get_extension_by_public_key_and_name(&manifest.public_key, &manifest.name)?
    {
        // If it's already a dev extension, remove it first (to allow reload)
        if let ExtensionSource::Development { .. } = &existing.source {
            state
                .extension_manager
                .remove_extension(&manifest.public_key, &manifest.name)?;
        }
        // Note: Production extensions can coexist with dev extensions
        // Dev extensions have priority during lookup
    }

    // 6. Create dev extension
    let extension = Extension {
        id: extension_id.clone(),
        source: ExtensionSource::Development {
            dev_server_url: dev_server_url.clone(),
            manifest_path: manifest_path.clone(),
            auto_reload: true,
        },
        manifest: manifest.clone(),
        enabled: true,
        last_accessed: SystemTime::now(),
    };

    // 7. Add to dev extensions (no database entry for dev extensions)
    state.extension_manager.add_dev_extension(extension)?;

    eprintln!(
        "✅ Dev extension loaded: {} v{} ({})",
        manifest.name, manifest.version, dev_server_url
    );

    Ok(extension_id)
}

#[tauri::command]
pub fn remove_dev_extension(
    public_key: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<(), ExtensionError> {
    // Only remove from dev_extensions, not production_extensions
    let mut dev_exts = state.extension_manager.dev_extensions.lock().map_err(|e| {
        ExtensionError::MutexPoisoned {
            reason: e.to_string(),
        }
    })?;

    // Find and remove by public_key and name
    let to_remove = dev_exts
        .iter()
        .find(|(_, ext)| ext.manifest.public_key == public_key && ext.manifest.name == name)
        .map(|(id, _)| id.clone());

    if let Some(id) = to_remove {
        dev_exts.remove(&id);

        // Drop all tables created by this dev extension
        // (Dev extension tables have no CRDT triggers, so they're local-only)
        db::core::with_connection(&state.db, |conn| {
            // Disable foreign key constraints BEFORE starting the transaction
            // (PRAGMA changes don't take effect within an active transaction)
            conn.execute("PRAGMA foreign_keys = OFF", [])
                .map_err(db::error::DatabaseError::from)?;

            let tx = conn.transaction().map_err(db::error::DatabaseError::from)?;
            let dropped = utils::drop_extension_tables(&tx, &public_key, &name)?;
            let commit_result = tx.commit().map_err(db::error::DatabaseError::from);

            // Re-enable foreign key constraints after transaction
            conn.execute("PRAGMA foreign_keys = ON", [])
                .map_err(db::error::DatabaseError::from)?;

            commit_result?;

            if !dropped.is_empty() {
                eprintln!(
                    "[DEV] Dropped {} tables for dev extension {}::{}",
                    dropped.len(),
                    public_key,
                    name
                );
            }
            Ok::<(), db::error::DatabaseError>(())
        })?;

        eprintln!("✅ Dev extension removed: {name}");
        Ok(())
    } else {
        Err(ExtensionError::NotFound { public_key, name })
    }
}

#[tauri::command]
pub fn get_all_dev_extensions(
    state: State<'_, AppState>,
) -> Result<Vec<ExtensionInfoResponse>, ExtensionError> {
    let dev_exts = state.extension_manager.dev_extensions.lock().map_err(|e| {
        ExtensionError::MutexPoisoned {
            reason: e.to_string(),
        }
    })?;

    let mut extensions = Vec::new();
    for ext in dev_exts.values() {
        extensions.push(ExtensionInfoResponse::from_extension(ext)?);
    }

    Ok(extensions)
}

// ============================================================================
// Permission Management Commands
// ============================================================================

/// Converts internal ExtensionPermission list to UI-friendly EditablePermissions format
fn convert_to_editable_permissions(permissions: Vec<ExtensionPermission>) -> EditablePermissions {
    let mut database = Vec::new();
    let mut filesystem = Vec::new();
    let mut http = Vec::new();
    let mut shell = Vec::new();

    for perm in permissions {
        let entry = PermissionEntry {
            target: perm.target,
            operation: Some(perm.action.as_str()),
            constraints: perm
                .constraints
                .map(|c| serde_json::to_value(c).unwrap_or_default()),
            status: Some(perm.status),
        };

        match perm.resource_type {
            ResourceType::Db => database.push(entry),
            ResourceType::Fs => filesystem.push(entry),
            ResourceType::Web => http.push(entry),
            ResourceType::Shell => shell.push(entry),
        }
    }

    EditablePermissions {
        database: if database.is_empty() {
            None
        } else {
            Some(database)
        },
        filesystem: if filesystem.is_empty() {
            None
        } else {
            Some(filesystem)
        },
        http: if http.is_empty() { None } else { Some(http) },
        shell: if shell.is_empty() { None } else { Some(shell) },
    }
}

#[tauri::command]
pub async fn get_extension_permissions(
    extension_id: String,
    state: State<'_, AppState>,
) -> Result<EditablePermissions, ExtensionError> {
    use crate::extension::core::types::ExtensionSource;

    // Check if this is a dev extension - if so, get permissions from manifest
    if let Some(extension) = state.extension_manager.get_extension(&extension_id) {
        match &extension.source {
            ExtensionSource::Development { .. } => {
                // Dev extension - return permissions from manifest with Granted status
                return Ok(extension.manifest.to_editable_permissions());
            }
            ExtensionSource::Production { .. } => {
                // Production extension - load from database
                let permissions = PermissionManager::get_permissions(&state, &extension_id).await?;
                return Ok(convert_to_editable_permissions(permissions));
            }
        }
    }

    // Extension not found in memory, try loading from database anyway
    let permissions = PermissionManager::get_permissions(&state, &extension_id).await?;
    Ok(convert_to_editable_permissions(permissions))
}

#[tauri::command]
pub async fn update_extension_permissions(
    extension_id: String,
    permissions: EditablePermissions,
    state: State<'_, AppState>,
) -> Result<(), ExtensionError> {
    // Delete old permissions
    PermissionManager::delete_permissions(&state, &extension_id).await?;

    // Convert to internal format and save
    let internal_permissions = permissions.to_internal_permissions(&extension_id);
    PermissionManager::save_permissions(&state, &internal_permissions).await?;

    Ok(())
}

#[tauri::command]
pub fn update_extension_display_mode(
    extension_id: String,
    display_mode: crate::extension::core::manifest::DisplayMode,
    state: State<'_, AppState>,
) -> Result<(), ExtensionError> {
    state
        .extension_manager
        .update_display_mode(&extension_id, display_mode, &state)
}

// ============================================================================
// WebviewWindow Commands (Desktop only)
// ============================================================================

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub fn open_extension_webview_window(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    extension_id: String,
    title: String,
    width: f64,
    height: f64,
    x: Option<f64>,
    y: Option<f64>,
    minimized: Option<bool>,
) -> Result<String, ExtensionError> {
    eprintln!(
        "[open_extension_webview_window] Received extension_id: {}, minimized: {:?}",
        extension_id, minimized
    );
    // Returns the window_id (generated UUID without dashes)
    state.extension_webview_manager.open_extension_window(
        &app_handle,
        &state.extension_manager,
        extension_id,
        title,
        width,
        height,
        x,
        y,
        minimized,
    )
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub fn close_extension_webview_window(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    window_id: String,
) -> Result<(), ExtensionError> {
    state
        .extension_webview_manager
        .close_extension_window(&app_handle, &window_id)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub fn focus_extension_webview_window(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    window_id: String,
) -> Result<(), ExtensionError> {
    state
        .extension_webview_manager
        .focus_extension_window(&app_handle, &window_id)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub fn update_extension_webview_window_position(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    window_id: String,
    x: f64,
    y: f64,
) -> Result<(), ExtensionError> {
    state
        .extension_webview_manager
        .update_extension_window_position(&app_handle, &window_id, x, y)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub fn update_extension_webview_window_size(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    window_id: String,
    width: f64,
    height: f64,
) -> Result<(), ExtensionError> {
    state
        .extension_webview_manager
        .update_extension_window_size(&app_handle, &window_id, width, height)
}

/// Close all extension webview windows.
/// Called when the vault is closed or becomes unavailable (e.g., webview reload).
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub fn close_all_extension_webview_windows(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), ExtensionError> {
    state
        .extension_webview_manager
        .close_all_extension_windows(&app_handle)
}
