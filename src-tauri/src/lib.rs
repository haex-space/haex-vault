#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod external_bridge;
mod crdt;
mod database;
mod extension;
mod shortcuts;
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
            })),
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge: tokio::sync::Mutex::new(ExternalBridge::new()),
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
            extension::database::extension_sql_execute,
            extension::database::extension_sql_select,
            extension::database::register_extension_migrations,
            extension::database::apply_synced_extension_migrations,
            extension::web::extension_web_fetch,
            extension::web::extension_web_open,
            extension::permissions::commands::check_web_permission,
            extension::permissions::commands::check_database_permission,
            extension::permissions::commands::check_filesystem_permission,
            extension::permissions::commands::resolve_permission_prompt,
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
            // WebView API commands (for native window extensions, desktop only)
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::webview::web::webview_extension_get_info,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::webview::web::webview_extension_context_get,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::webview::web::webview_extension_context_set,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::webview::database::webview_extension_db_query,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::webview::database::webview_extension_db_execute,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::webview::database::webview_extension_db_register_migrations,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::webview::web::webview_extension_check_web_permission,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::webview::web::webview_extension_check_database_permission,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::webview::web::webview_extension_check_filesystem_permission,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::webview::web::webview_extension_web_open,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::webview::web::webview_extension_web_request,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::webview::web::webview_extension_emit_to_all,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::webview::filesystem::webview_extension_fs_save_file,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::webview::filesystem::webview_extension_fs_open_file,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::webview::external::webview_extension_external_respond,
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
            external_bridge::external_get_authorized_clients,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_get_session_authorizations,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_revoke_session_authorization,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_revoke_client,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_approve_client,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_deny_client,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_get_pending_authorizations,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_respond,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_client_allow,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_client_block,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_get_blocked_clients,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_unblock_client,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_is_client_blocked,
            // FileSync commands (extension/filesystem)
            extension::filesystem::commands::filesync_list_spaces,
            extension::filesystem::commands::filesync_create_space,
            extension::filesystem::commands::filesync_delete_space,
            extension::filesystem::commands::filesync_list_files,
            extension::filesystem::commands::filesync_get_file,
            extension::filesystem::commands::filesync_upload_file,
            extension::filesystem::commands::filesync_download_file,
            extension::filesystem::commands::filesync_delete_file,
            extension::filesystem::commands::filesync_list_backends,
            extension::filesystem::commands::filesync_add_backend,
            extension::filesystem::commands::filesync_remove_backend,
            extension::filesystem::commands::filesync_test_backend,
            extension::filesystem::commands::filesync_list_sync_rules,
            extension::filesystem::commands::filesync_add_sync_rule,
            extension::filesystem::commands::filesync_remove_sync_rule,
            extension::filesystem::commands::filesync_get_sync_status,
            extension::filesystem::commands::filesync_trigger_sync,
            extension::filesystem::commands::filesync_pause_sync,
            extension::filesystem::commands::filesync_resume_sync,
            extension::filesystem::commands::filesync_resolve_conflict,
            extension::filesystem::commands::filesync_select_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
