#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod external_bridge;
mod crypto;
mod crdt;
pub mod database;
mod device;
mod extension;
pub mod file_sync;
mod filesystem;
mod logging;
pub mod mls;
#[cfg(desktop)]
mod shortcuts;
mod passwords;
pub mod peer_storage;
mod remote_storage;
pub mod space_delivery;
pub mod ucan;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod window;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use crate::external_bridge::ExternalBridge;
use crate::{
    crdt::hlc::HlcService,
    database::{connection_context::ConnectionContext, DbConnection},
    extension::core::ExtensionManager,
    file_sync::commands::SyncManager,
};

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use crate::extension::webview::ExtensionWebviewManager;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::Manager;

pub mod table_names {
    include!(concat!(env!("OUT_DIR"), "/tableNames.rs"));
}

pub mod event_names {
    include!(concat!(env!("OUT_DIR"), "/eventNames.rs"));
}

pub struct AppState {
    pub db: DbConnection,
    pub hlc: Mutex<HlcService>,
    /// Exclusive advisory lock on the currently-open vault's DB file.
    /// Populated by `open_encrypted_database` / `create_encrypted_database`
    /// and cleared by `close_database`. Prevents the same vault from being
    /// mounted by two instances at once (which would corrupt CRDT HLC + WAL).
    /// `None` means no vault is currently open.
    pub vault_lock: Mutex<Option<crate::database::vault_lock::VaultLock>>,
    /// Per-session CRDT connection state. Holds the transaction-scoped HLC slot
    /// that is shared between the `current_hlc()` UDF, BEFORE-DELETE triggers,
    /// and literal-injection in the SqlExecutor.
    pub connection_context: Mutex<ConnectionContext>,
    pub extension_manager: ExtensionManager,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub extension_webview_manager: ExtensionWebviewManager,
    /// Application context (theme, locale, platform, device_id) shared with extensions.
    /// On desktop: accessed via Tauri commands for native webviews.
    /// On mobile: shared via postMessage to iframes.
    pub context: Arc<Mutex<extension::core::context::ApplicationContext>>,
    /// External bridge for WebSocket connections (desktop only)
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub external_bridge: tokio::sync::Mutex<ExternalBridge>,
    /// File watcher for sync rules (no-op on Android)
    pub file_watcher: extension::filesystem::watcher::FileWatcherManager,
    /// Session-based permission store (in-memory, cleared on restart)
    pub session_permissions: extension::permissions::session::SessionPermissionStore,
    /// Extension resource limits service (database, filesystem, web)
    pub limits: extension::limits::LimitsService,
    /// Peer storage endpoint for P2P file sharing via iroh/QUIC
    pub peer_storage: Arc<tokio::sync::Mutex<peer_storage::endpoint::PeerEndpoint>>,
    /// Active P2P transfer control (transfer_id → (cancel_token, pause_flag))
    pub transfer_tokens: tokio::sync::Mutex<HashMap<String, (tokio_util::sync::CancellationToken, Arc<std::sync::atomic::AtomicBool>)>>,
    /// Active file sync loops (rule_id → cancellation token)
    pub sync_manager: tokio::sync::Mutex<SyncManager>,
    /// Supabase JWT auth token, synced from frontend for Rust HTTP calls.
    pub auth_token: Arc<Mutex<Option<String>>>,
    /// PTY manager for shell/terminal sessions
    pub pty_manager: extension::shell::pty::PtyManager,
    /// Active local sync loops (space_id -> handle)
    pub local_sync_loops: tokio::sync::Mutex<HashMap<String, space_delivery::local::sync_loop::SyncLoopHandle>>,
    /// Leader states for local space delivery, keyed by space_id.
    /// RwLock because reads (QUIC stream routing) are frequent and concurrent,
    /// writes (start/stop leader) are rare.
    pub leader_state: Arc<tokio::sync::RwLock<HashMap<String, Arc<space_delivery::local::leader::LeaderState>>>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use extension::core::EXTENSION_PROTOCOL_NAME;

    // Install a tracing subscriber so iroh's internal events (relay
    // actor lifecycle, socket transport errors, connection state
    // changes) become visible in stderr alongside our own eprintln!
    // logs. Without this, iroh emits via the `tracing` crate and the
    // events are silently dropped — exactly the "endpoint is closed
    // but we don't know why" blind spot we're chasing on the
    // diag/multi-leader-quic-logging branch.
    //
    // Filter via the `HAEX_LOG` env var (e.g. `HAEX_LOG=iroh=debug`).
    // The default keeps the volume sane: `info` for iroh, `warn` for
    // everything else, so a user without env vars still gets relay /
    // close-reason events but not full debug noise.
    let filter = tracing_subscriber::EnvFilter::try_from_env("HAEX_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn,iroh=info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_writer(std::io::stderr)
        .try_init();

    // Reassigned under #[cfg(mobile)] / #[cfg(target_os = "android")] below;
    // on desktop-linux the compiler doesn't see those paths and warns about
    // the `mut` — allow it.
    #[allow(unused_mut)]
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

    // Note: previously `tauri_plugin_single_instance` was registered here to
    // lock the app to one running instance per user, with a secondary purpose
    // of forwarding `haexvault://` deep-link CLI args from a 2nd launch to
    // the already-running window. That coupling is gone: we now allow
    // multiple simultaneous instances (e.g. for side-by-side testing of two
    // vaults on the same machine). The deep-link forwarding fallback is
    // dropped with it — if a URL arrives as a CLI arg to a fresh instance
    // it will be handled by that instance's own startup flow.

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
            vault_lock: Mutex::new(None),
            connection_context: Mutex::new(ConnectionContext::new()),
            extension_manager: ExtensionManager::new(),
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension_webview_manager: ExtensionWebviewManager::new(),
            context: Arc::new(Mutex::new(extension::core::context::ApplicationContext {
                theme: "dark".to_string(),
                locale: "en".to_string(),
                platform: std::env::consts::OS.to_string(),
                // Device ID is set after vault opens (loaded from instance.json store)
                device_id: String::new(),
            })),
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge: tokio::sync::Mutex::new(ExternalBridge::new()),
            file_watcher: extension::filesystem::watcher::FileWatcherManager::new(),
            session_permissions: extension::permissions::session::SessionPermissionStore::new(),
            limits: extension::limits::LimitsService::new(),
            peer_storage: Arc::new(tokio::sync::Mutex::new(peer_storage::endpoint::PeerEndpoint::new_ephemeral())),
            transfer_tokens: tokio::sync::Mutex::new(HashMap::new()),
            sync_manager: tokio::sync::Mutex::new(SyncManager::new()),
            auth_token: Arc::new(Mutex::new(None)),
            pty_manager: extension::shell::pty::PtyManager::new(),
            local_sync_loops: tokio::sync::Mutex::new(HashMap::new()),
            leader_state: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
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
            let _ = &app;
            // Enable camera/media stream access in WebKitGTK on Linux
            #[cfg(target_os = "linux")]
            {
                if let Some(main_window) = app.get_webview_window("main") {
                    main_window.with_webview(|webview| {
                        use webkit2gtk::{WebViewExt, SettingsExt, PermissionRequestExt};
                        let wv = webview.inner();

                        if let Some(settings) = wv.settings() {
                            settings.set_enable_media_stream(true);
                            settings.set_enable_webrtc(true);
                            settings.set_media_playback_requires_user_gesture(false);
                        }

                        wv.connect_permission_request(|_, request| {
                            request.allow();
                            true
                        });
                    }).ok();
                }
            }

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
            crypto::encrypt_for_identity,
            crypto::decrypt_for_identity,
            database::close_database,
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
            database::crdt_cleanup_deleted_rows,
            database::crdt_get_stats,
            database::database_vacuum,
            database::change_vault_password,
            database::stats::get_database_info,
            database::migrations::apply_core_migrations,
            database::migrations::get_applied_core_migrations,
            database::migrations::get_unapplied_core_migrations,
            database::migrations::get_all_core_migrations,
            database::migrations::get_pending_columns,
            database::migrations::clear_pending_column,
            logging::commands::log_write_system,
            logging::commands::log_read,
            logging::commands::log_cleanup,
            logging::commands::log_delete,
            logging::commands::log_clear_all,
            crdt::commands::get_table_schema,
            crdt::commands::get_dirty_tables,
            crdt::commands::clear_dirty_table,
            crdt::commands::clear_all_dirty_tables,
            crdt::commands::get_all_crdt_tables,
            crdt::commands::ensure_extension_triggers,
            crdt::commands::apply_remote_changes_in_transaction,
            extension::database::commands::extension_database_execute,
            extension::database::commands::extension_database_transaction,
            extension::database::commands::extension_database_query,
            extension::database::commands::extension_database_register_migrations,
            extension::database::commands::apply_synced_extension_migrations,
            extension::spaces::commands::extension_space_assign,
            passwords::commands::extension_password_list,
            extension::spaces::commands::extension_space_unassign,
            extension::spaces::commands::extension_space_get_assignments,
            extension::spaces::commands::extension_space_list,
            extension::spaces::commands::set_auth_token,
            extension::web::commands::extension_web_fetch,
            extension::web::commands::extension_web_open,
            extension::permissions::commands::extension_permissions_check_web,
            extension::permissions::commands::extension_permissions_check_database,
            extension::permissions::commands::extension_permissions_check_filesystem,
            extension::permissions::commands::resolve_permission_prompt,
            extension::permissions::commands::grant_session_permission,
            extension::permissions::commands::get_extension_session_permissions,
            extension::permissions::commands::remove_extension_session_permission,
            extension::logging::commands::extension_logging_write,
            extension::logging::commands::extension_logging_read,
            extension::limits::commands::get_extension_limits,
            extension::limits::commands::update_extension_limits,
            extension::limits::commands::reset_extension_limits,
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
            // Context commands (from core::context module)
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::core::context::extension_context_get,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::core::context::extension_context_set,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::core::context::extension_webview_broadcast,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::core::context::extension_webview_emit,
            // Sync table filtering - needed for all platforms (mobile uses iframe forwarding)
            extension::extension_filter_sync_tables,
            // Sync table emission to webviews - desktop only
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            extension::extension_emit_sync_tables,
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
            external_bridge::external_bridge_get_session_blocked_clients,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::external_bridge_unblock_session_client,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            external_bridge::extension_signal_ready,
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
            filesystem::filesystem_copy_dir,
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
            extension::filesystem::commands::extension_filesystem_known_paths,
            // File watcher commands
            extension::filesystem::commands::extension_filesystem_watch,
            extension::filesystem::commands::extension_filesystem_unwatch,
            extension::filesystem::commands::extension_filesystem_is_watching,
            // Shell/PTY commands
            extension::shell::commands::extension_shell_list_available,
            extension::shell::commands::extension_shell_create,
            extension::shell::commands::extension_shell_write,
            extension::shell::commands::extension_shell_resize,
            extension::shell::commands::extension_shell_close,
            // Device identity
            device::device_init_key,
            // Peer Storage (P2P file sharing via iroh/QUIC)
            peer_storage::peer_storage_start,
            peer_storage::peer_storage_stop,
            peer_storage::peer_storage_status,
            peer_storage::peer_storage_reload_shares,
            peer_storage::peer_storage_remote_list,
            peer_storage::peer_storage_remote_read,
            peer_storage::peer_storage_transfer_cancel,
            peer_storage::peer_storage_transfer_pause,
            peer_storage::peer_storage_transfer_resume,
            peer_storage::open_file_system,
            // Space Delivery (local leader mode)
            space_delivery::local::commands::local_delivery_start,
            space_delivery::local::commands::local_delivery_stop,
            space_delivery::local::commands::local_delivery_broadcast_commit,
            space_delivery::local::commands::local_delivery_status,
            space_delivery::local::commands::local_delivery_get_leader,
            space_delivery::local::commands::local_delivery_elect,
            space_delivery::local::commands::local_delivery_connect,
            space_delivery::local::commands::local_delivery_disconnect,
            space_delivery::local::commands::local_delivery_create_invite,
            space_delivery::local::commands::local_delivery_list_invites,
            space_delivery::local::commands::local_delivery_revoke_invite,
            space_delivery::local::commands::local_delivery_claim_invite,
            space_delivery::local::commands::local_delivery_push_invite,
            // MLS (RFC 9420) group key management
            mls::commands::mls_init_tables,
            mls::commands::mls_init_identity,
            mls::commands::mls_find_member_index,
            mls::commands::mls_create_group,
            mls::commands::mls_add_member,
            mls::commands::mls_remove_member,
            mls::commands::mls_encrypt,
            mls::commands::mls_decrypt,
            mls::commands::mls_process_message,
            mls::commands::mls_get_key_packages,
            mls::commands::mls_has_group,
            mls::commands::mls_export_epoch_key,
            mls::commands::mls_get_epoch_key,
            mls::commands::mls_get_group_info,
            mls::commands::mls_join_by_external_commit,
            // File Sync commands
            file_sync::commands::file_sync_start_rule,
            file_sync::commands::file_sync_stop_rule,
            file_sync::commands::file_sync_trigger_now,
            file_sync::commands::file_sync_trigger_by_watcher,
            file_sync::commands::file_sync_status,
            file_sync::commands::file_sync_stop_all,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
