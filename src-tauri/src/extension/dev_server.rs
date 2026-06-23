//! Dev-mode extension commands and config types.

use crate::{
    database::{self as db, core::with_connection, error::DatabaseError},
    extension::{
        core::{
            self, find_icon,
            path_utils::validate_path_in_directory,
            types::{Extension, ExtensionSource},
            ExtensionInfoResponse, ExtensionManifest,
        },
        database::executor::SqlExecutor,
        error::ExtensionError,
        permissions::manager::PermissionManager,
        utils,
    },
    table_names::TABLE_EXTENSIONS,
    AppState,
};
use std::path::PathBuf;
use std::time::SystemTime;
use tauri::{AppHandle, State};

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
    #[serde(default)]
    i18n: Option<std::collections::HashMap<String, core::manifest::ManifestI18nEntry>>,
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

/// Load a dev extension from a local path.
/// Dev extensions are now treated like production extensions:
/// - Registered in the database (with CRDT support)
/// - Have CRDT columns and triggers on their tables
/// - Can sync across devices
#[tauri::command]
pub async fn load_dev_extension(
    app_handle: AppHandle,
    extension_path: String,
    state: State<'_, AppState>,
) -> Result<String, ExtensionError> {
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
    let manifest_path = validate_path_in_directory(
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

    // Resolve icon path with fallback to favicon.ico (returns relative path like for prod extensions)
    let resolved_icon = find_icon(
        &app_handle,
        &extension_path_buf,
        &haextension_dir,
        partial_manifest.icon.as_deref(),
    );
    eprintln!(
        "[DEV] Icon resolution: manifest.icon={:?}, resolved_icon={:?}",
        partial_manifest.icon, resolved_icon
    );

    let manifest = ExtensionManifest {
        name: name.clone(),
        version: version.clone(),
        author,
        entry: partial_manifest.entry,
        icon: resolved_icon,
        public_key: partial_manifest.public_key.clone(),
        signature: partial_manifest.signature,
        permissions: partial_manifest.permissions,
        homepage,
        description: partial_manifest.description,
        single_instance: partial_manifest.single_instance,
        display_mode: partial_manifest.display_mode,
        migrations_dir: partial_manifest.migrations_dir,
        i18n: partial_manifest.i18n,
    };

    // 3.5. Validate public key format
    utils::validate_public_key(&manifest.public_key)?;

    // 4. Check if extension already exists in DB (UPSERT pattern)
    let check_sql = format!("SELECT id FROM {TABLE_EXTENSIONS} WHERE public_key = ? AND name = ?");

    let existing_id: Option<String> = with_connection(&state.db, |conn| {
        let mut stmt = conn.prepare(&check_sql)?;
        let result: Result<String, _> = stmt
            .query_row(rusqlite::params![&manifest.public_key, &name], |row| {
                row.get(0)
            });
        Ok(result.ok())
    })?;

    // 5. Insert or update in database
    let extension_id = with_connection(&state.db, |conn| {
        let tx = conn.transaction().map_err(DatabaseError::from)?;

        let hlc_service = state.lock_or_fail(
            &state.hlc,
            crate::critical::CriticalFailureCode::HlcMutexPoisoned,
            "extension::mod::register_dev_extension",
            serde_json::json!({}),
        )?;

        let actual_id = if let Some(existing_id) = existing_id {
            // Update existing extension
            eprintln!(
                "[DEV] Updating existing extension {}::{} with id {}",
                manifest.public_key, name, existing_id
            );
            let update_sql = format!(
                "UPDATE {TABLE_EXTENSIONS} SET version = ?, author = ?, entry = ?, icon = ?, signature = ?, homepage = ?, description = ?, enabled = ?, single_instance = ?, display_mode = ?, dev_path = ? WHERE id = ?"
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
                    true, // enabled
                    manifest.single_instance.unwrap_or(false),
                    manifest
                        .display_mode
                        .as_ref()
                        .map(|dm| format!("{:?}", dm).to_lowercase())
                        .unwrap_or_else(|| "auto".to_string()),
                    extension_path, // dev_path
                    existing_id,
                ],
            )?;
            existing_id
        } else {
            // Insert new extension
            let new_id = uuid::Uuid::new_v4().to_string();
            eprintln!(
                "[DEV] Inserting new extension {}::{} with id {}",
                manifest.public_key, name, new_id
            );
            let insert_sql = format!(
                "INSERT INTO {TABLE_EXTENSIONS} (id, name, version, author, entry, icon, public_key, signature, homepage, description, enabled, single_instance, display_mode, dev_path) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
            );

            SqlExecutor::execute_internal_typed(
                &tx,
                &hlc_service,
                &insert_sql,
                rusqlite::params![
                    new_id,
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
                    extension_path, // dev_path
                ],
            )?;
            new_id
        };

        tx.commit().map_err(DatabaseError::from)?;
        Ok::<String, DatabaseError>(actual_id)
    })?;

    // 5.5. Register permissions from manifest (if any)
    // This ensures dev extensions have their permissions available in the UI
    // Use the same conversion as production extensions (to_editable_permissions)
    let editable_permissions = manifest.to_editable_permissions();
    let internal_permissions = editable_permissions.to_internal_permissions(&extension_id);
    if !internal_permissions.is_empty() {
        // Delete any existing permissions first (in case of reload)
        PermissionManager::delete_permissions(&state, &extension_id).await?;

        eprintln!(
            "[DEV] Registering {} permissions from manifest for extension {}",
            internal_permissions.len(),
            extension_id
        );
        PermissionManager::save_permissions(&state, &internal_permissions).await?;
    }

    // 6. Remove from in-memory manager if already exists (to allow reload)
    let _ = state
        .extension_manager
        .remove_extension(&manifest.public_key, &manifest.name);

    // 7. Create extension and add to in-memory manager
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

    state.extension_manager.add_extension(extension)?;

    eprintln!(
        "✅ Dev extension loaded: {} v{} ({})",
        manifest.name, manifest.version, dev_server_url
    );

    Ok(extension_id)
}

/// Remove a dev extension.
/// Dev extensions are now treated like production extensions,
/// so this removes from both memory and database.
#[tauri::command]
pub fn remove_dev_extension(
    public_key: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<(), ExtensionError> {
    use crate::extension::database::executor::SqlExecutor;
    use crate::extension::permissions::manager::PermissionManager;
    use crate::table_names::TABLE_EXTENSIONS;

    // Find extension by public_key and name
    let extension = state
        .extension_manager
        .get_extension_by_public_key_and_name(&public_key, &name)?
        .ok_or_else(|| ExtensionError::NotFound {
            public_key: public_key.clone(),
            name: name.clone(),
        })?;

    let extension_id = extension.id.clone();

    // Remove from database. `SqlExecutor::execute_internal_typed` writes the
    // delete through the CRDT path (haex_deleted_rows delete-log).
    //
    // `with_fk_disabled` re-enables foreign-keys on every exit path — including
    // early returns via `?` — so an error mid-transaction does not leave FK
    // checks off on the shared Connection for subsequent operations.
    db::core::with_connection(&state.db, |conn| {
        crate::crdt::cleanup::with_fk_disabled(conn, |conn| {
            let tx = conn.transaction().map_err(db::error::DatabaseError::from)?;

            let hlc_service = state.lock_or_fail(
                &state.hlc,
                crate::critical::CriticalFailureCode::HlcMutexPoisoned,
                "extension::mod::uninstall_dev_extension",
                serde_json::json!({}),
            )?;

            // Delete permissions for this extension
            PermissionManager::delete_permissions_in_transaction(&tx, &hlc_service, &extension_id)?;

            // Drop all tables created by this extension
            let dropped = utils::drop_extension_tables(&tx, &public_key, &name)?;
            if !dropped.is_empty() {
                eprintln!(
                    "[DEV] Dropped {} tables for extension {}::{}",
                    dropped.len(),
                    public_key,
                    name
                );
            }

            // Delete the extension entry itself
            let delete_sql = format!("DELETE FROM {TABLE_EXTENSIONS} WHERE id = ?");
            SqlExecutor::execute_internal_typed(
                &tx,
                &hlc_service,
                &delete_sql,
                rusqlite::params![&extension_id],
            )?;

            tx.commit().map_err(db::error::DatabaseError::from)
        })
    })?;

    // Remove from in-memory manager
    state
        .extension_manager
        .remove_extension(&public_key, &name)?;

    eprintln!("✅ Dev extension removed: {name}");
    Ok(())
}

/// Get all dev extensions (extensions with Development source).
/// Since dev extensions are now stored in the same manager as production,
/// this filters by ExtensionSource::Development.
#[tauri::command]
pub fn get_all_dev_extensions(
    state: State<'_, AppState>,
) -> Result<Vec<ExtensionInfoResponse>, ExtensionError> {
    use crate::extension::core::types::ExtensionSource;

    let available_exts = state
        .extension_manager
        .available_extensions
        .lock()
        .map_err(|e| ExtensionError::MutexPoisoned {
            reason: e.to_string(),
        })?;

    let mut extensions = Vec::new();
    for ext in available_exts.values() {
        // Filter only dev extensions
        if matches!(ext.source, ExtensionSource::Development { .. }) {
            extensions.push(ExtensionInfoResponse::from_extension(ext)?);
        }
    }

    Ok(extensions)
}
