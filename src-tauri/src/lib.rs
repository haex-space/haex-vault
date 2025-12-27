#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod external_bridge;
mod crdt;
mod database;
mod extension;
mod filesystem;
mod shortcuts;
mod remote_storage;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod window;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use crate::external_bridge::ExternalBridge;
use crate::{crdt::hlc::HlcService, database::DbConnection, extension::core::ExtensionManager};

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use crate::extension::webview::ExtensionWebviewManager;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

pub mod table_names {
    include!(concat!(env!("OUT_DIR"), "/tableNames.rs"));
}

pub mod event_names {
    include!(concat!(env!("OUT_DIR"), "/eventNames.rs"));
}

pub struct AppState {
    pub db: DbConnection,
    pub hlc: Mutex<HlcService>,
    pub extension_manager: ExtensionManager,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub extension_webview_manager: ExtensionWebviewManager,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub context: Arc<Mutex<extension::webview::web::ApplicationContext>>,
    /// External bridge for WebSocket connections (desktop only)
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub external_bridge: tokio::sync::Mutex<ExternalBridge>,
    /// File watcher for sync rules (desktop only)
    #[cfg(desktop)]
    pub file_watcher: extension::filesystem::watcher::FileWatcherManager,
    /// Session-based permission store (in-memory, cleared on restart)
    pub session_permissions: extension::permissions::session::SessionPermissionStore,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use extension::core::EXTENSION_PROTOCOL_NAME;

    let mut builder = tauri::Builder::default();

    // Biometry plugin (mobile only) - provides biometric auth + secure storage
    #[cfg(mobile)]
    {
        builder = builder.plugin(tauri_plugin_biometry::init());
    }

    // Android FS plugin (Android only) - provides file/folder picker with SAF support
    #[cfg(target_os = "android")]
    {
        builder = builder.plugin(tauri_plugin_android_fs::init());
    }

    // Single-instance plugin must be registered first (desktop only)
    // This handles deep-link URLs passed as CLI arguments when a new instance is launched
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // Deep-link URLs come as CLI arguments
            // Emit event to frontend for handling
            if let Some(url) = argv.iter().find(|arg| arg.starts_with("haexvault://")) {
                let _ = app.emit("deep-link-received", url.clone());
            }
            // Focus the main window
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }));
    }

    builder
        .register_uri_scheme_protocol(EXTENSION_PROTOCOL_NAME, move |context, request| {
            // Hole den AppState aus dem Context
            let app_handle = context.app_handle();
            let state = app_handle.state::<AppState>();

            // Rufe den Handler mit allen benötigten Parametern auf
            match extension::core::extension_protocol_handler(state, app_handle, &request) {
                Ok(response) => response,
                Err(e) => {
                    eprintln!(
                        "Fehler im Custom Protocol Handler für URI '{}': {}",
                        request.uri(),
                        e
                    );
                    tauri::http::Response::builder()
                        .status(500)
                        .header("Content-Type", "text/plain")
                        .body(Vec::from(format!(
                            "Interner Serverfehler im Protokollhandler: {e}"
                        )))
                        .unwrap_or_else(|build_err| {
                            eprintln!("Konnte Fehler-Response nicht erstellen: {build_err}");
                            tauri::http::Response::builder()
                                .status(500)
                                .body(Vec::new())
                                .expect("Konnte minimale Fallback-Response nicht erstellen")
                        })
                }
            }
        })
        .manage(AppState {
            db: DbConnection(Arc::new(Mutex::new(None))),
            hlc: Mutex::new(HlcService::new()),
            extension_manager: ExtensionManager::new(),
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension_webview_manager: ExtensionWebviewManager::new(),
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            context: Arc::new(Mutex::new(extension::webview::web::ApplicationContext {
                theme: "dark".to_string(),
                locale: "en".to_string(),
                platform: std::env::consts::OS.to_string(),
                // Device ID is set after vault opens (loaded from instance.json store)
                device_id: String::new(),
            })),
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge: tokio::sync::Mutex::new(ExternalBridge::new()),
            #[cfg(desktop)]
            file_watcher: extension::filesystem::watcher::FileWatcherManager::new(),
            session_permissions: extension::permissions::session::SessionPermissionStore::new(),
        })
        //.manage(ExtensionState::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_persisted_scope::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_deep_link::init())
        // Auto-start browser bridge on desktop and register main window close handler
        .setup(|app| {
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            {
                let app_handle = app.handle().clone();

                // Auto-start external bridge with default port
                // Port can be changed later via settings when vault is opened
                let app_handle_for_bridge = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app_handle_for_bridge.state::<AppState>();
                    let mut bridge = state.external_bridge.lock().await;
                    if let Err(e) = bridge.start(app_handle_for_bridge.clone(), None).await {
                        eprintln!("Failed to auto-start external bridge: {}", e);
                    } else {
                        println!("External bridge auto-started on port {}", bridge.get_port());
                    }
                });

                // Register main window close handler to close all extension windows
                if let Some(main_window) = app.get_webview_window("main") {
                    let app_handle_for_close = app_handle.clone();
                    main_window.on_window_event(move |event| {
                        if let tauri::WindowEvent::CloseRequested { .. } = event {
                            eprintln!("[Main Window] Close requested, closing all extension windows...");
                            let state = app_handle_for_close.state::<AppState>();
                            if let Err(e) = state.extension_webview_manager.close_all_extension_windows(&app_handle_for_close) {
                                eprintln!("[Main Window] Failed to close extension windows: {:?}", e);
                            }
                        }
                    });
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            database::create_encrypted_database,
            database::delete_vault,
            database::move_vault_to_trash,
            database::list_vaults,
            database::open_encrypted_database,
            database::sql_execute_with_crdt,
            database::sql_execute,
            database::sql_query_with_crdt,
            database::sql_select_with_crdt,
            database::sql_select,
            database::sql_with_crdt,
            database::vault_exists,
            database::import_vault,
            database::crdt_cleanup_tombstones,
            database::crdt_get_stats,
            database::database_vacuum,
            database::change_vault_password,
            database::stats::get_database_info,
            database::migrations::apply_core_migrations,
            database::migrations::get_applied_core_migrations,
            database::migrations::get_unapplied_core_migrations,
            database::migrations::get_all_core_migrations,
            crdt::commands::get_table_schema,
            crdt::commands::get_dirty_tables,
            crdt::commands::clear_dirty_table,
            crdt::commands::clear_all_dirty_tables,
            crdt::commands::get_all_crdt_tables,
            crdt::commands::apply_remote_changes_in_transaction,
            extension::database::commands::extension_database_execute,
            extension::database::commands::extension_database_query,
            extension::database::commands::extension_database_register_migrations,
            extension::database::commands::apply_synced_extension_migrations,
            extension::web::commands::extension_web_fetch,
            extension::web::commands::extension_web_open,
            extension::permissions::commands::extension_permissions_check_web,
            extension::permissions::commands::extension_permissions_check_database,
            extension::permissions::commands::extension_permissions_check_filesystem,
            extension::permissions::commands::resolve_permission_prompt,
            extension::permissions::commands::grant_session_permission,
            extension::get_all_dev_extensions,
            extension::get_all_extensions,
            extension::get_extension_info,
            extension::install_extension_files,
            extension::install_extension_with_permissions,
            extension::is_extension_installed,
            extension::register_extension_in_database,
            extension::load_dev_extension,
            extension::preview_extension,
            extension::remove_dev_extension,
            extension::remove_extension,
            extension::get_extension_permissions,
            extension::update_extension_permissions,
            extension::update_extension_display_mode,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::open_extension_webview_window,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::close_extension_webview_window,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::focus_extension_webview_window,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::update_extension_webview_window_position,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::update_extension_webview_window_size,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::close_all_extension_webview_windows,
            // WebView-specific API commands (for native window extensions, desktop only)
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::webview::web::extension_get_info,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::webview::web::extension_context_get,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::webview::web::extension_context_set,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::webview::web::extension_emit_to_all,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::webview::filesystem::extension_filesystem_save_file,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::webview::filesystem::extension_filesystem_open_file,
            // Window management (desktop only)
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            window::focus_main_window,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            window::focus_window_by_label,
            // Desktop shortcuts (desktop only)
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            shortcuts::create_desktop_shortcut,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            shortcuts::remove_desktop_shortcut,
            // External bridge (desktop only)
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_bridge_start,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_bridge_stop,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_bridge_get_status,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_bridge_get_port,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_bridge_get_default_port,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_bridge_respond,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_bridge_get_authorized_clients,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_bridge_get_session_authorizations,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_bridge_revoke_session_authorization,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_bridge_revoke_client,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_bridge_approve_client,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_bridge_deny_client,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_bridge_get_pending_authorizations,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_bridge_client_allow,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_bridge_client_block,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_bridge_get_blocked_clients,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_bridge_unblock_client,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_bridge_is_client_blocked,
            // Remote Storage API commands (internal - use extension_remote_storage_* for extensions)
            remote_storage::remote_storage_list_backends,
            remote_storage::remote_storage_add_backend,
            remote_storage::remote_storage_update_backend,
            remote_storage::remote_storage_remove_backend,
            remote_storage::remote_storage_test_backend,
            remote_storage::remote_storage_upload,
            remote_storage::remote_storage_download,
            remote_storage::remote_storage_delete,
            remote_storage::remote_storage_list,
            // Extension Remote Storage commands (with permission checks)
            extension::remote_storage::commands::extension_remote_storage_list_backends,
            extension::remote_storage::commands::extension_remote_storage_add_backend,
            extension::remote_storage::commands::extension_remote_storage_update_backend,
            extension::remote_storage::commands::extension_remote_storage_remove_backend,
            extension::remote_storage::commands::extension_remote_storage_test_backend,
            extension::remote_storage::commands::extension_remote_storage_upload,
            extension::remote_storage::commands::extension_remote_storage_download,
            extension::remote_storage::commands::extension_remote_storage_delete,
            extension::remote_storage::commands::extension_remote_storage_list,
            // Filesystem API commands (generische Filesystem Operationen - internal use)
            filesystem::filesystem_read_file,
            filesystem::filesystem_write_file,
            filesystem::filesystem_read_dir,
            filesystem::filesystem_mkdir,
            filesystem::filesystem_remove,
            filesystem::filesystem_exists,
            filesystem::filesystem_stat,
            filesystem::filesystem_select_folder,
            filesystem::filesystem_select_file,
            filesystem::filesystem_rename,
            filesystem::filesystem_copy,
            // Extension Filesystem commands (with permission checks)
            extension::filesystem::commands::extension_filesystem_read_file,
            extension::filesystem::commands::extension_filesystem_write_file,
            extension::filesystem::commands::extension_filesystem_read_dir,
            extension::filesystem::commands::extension_filesystem_mkdir,
            extension::filesystem::commands::extension_filesystem_remove,
            extension::filesystem::commands::extension_filesystem_exists,
            extension::filesystem::commands::extension_filesystem_stat,
            extension::filesystem::commands::extension_filesystem_select_folder,
            extension::filesystem::commands::extension_filesystem_select_file,
            extension::filesystem::commands::extension_filesystem_rename,
            extension::filesystem::commands::extension_filesystem_copy,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
