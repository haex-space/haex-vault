/// src-tauri/src/extension/mod.rs
use crate::{
    extension::{
        core::{
            manager::ExtensionManager, EditablePermissions, ExtensionInfoResponse,
            ExtensionPreview, PermissionEntry,
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
    state: State<'_, AppState>,
) -> Result<(), ExtensionError> {
    state
        .extension_manager
        .remove_extension_internal(&app_handle, &public_key, &name, &version, &state)
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
    #[serde(default)]
    keys: KeysConfig,
}

#[derive(serde::Deserialize, Debug, Default)]
struct KeysConfig {
    #[serde(default)]
    public_key_path: Option<String>,
    #[serde(default)]
    private_key_path: Option<String>,
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

    // 3. Read and parse manifest
    let manifest_content =
        std::fs::read_to_string(&manifest_path).map_err(|e| ExtensionError::ManifestError {
            reason: format!("Failed to read manifest: {e}"),
        })?;

    let manifest: ExtensionManifest = serde_json::from_str(&manifest_content)?;

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
        .update_display_mode(&extension_id, display_mode)
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
) -> Result<String, ExtensionError> {
    eprintln!(
        "[open_extension_webview_window] Received extension_id: {}",
        extension_id
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
