pub mod adapters;
#[cfg(feature = "voice")]
pub mod assistant;
#[cfg(feature = "voice")]
pub mod audio;
pub mod browser_bridge;
pub mod caller;
pub mod catalog;
pub mod chat;
pub mod contacts;
pub mod deep_link;
pub mod device;
pub mod error;
pub mod extensions;
pub mod file_sync;
pub mod files;
pub mod hardware;
pub mod identity;
pub mod instances;
pub mod llm;
pub mod mcp_server;
pub mod model_capabilities;
pub mod model_jobs;
pub mod models;
pub mod passwords;
pub mod patch;
pub mod platform;
#[cfg(feature = "platform-probe")]
pub mod platform_probe;
pub mod privacy;
pub mod prompt_park;
pub mod providers;
pub mod remote_storage;
pub mod state;
pub mod state_utils;
pub mod storage;
// Unconditional (unlike `audio`, above): `stt::mod`/`stt::interrupt` are
// plain, dependency-free types the `llm-cpu`-only `stt::local` adapter
// (candle Whisper) and future `voice`-gated commands both need. Keeping
// them out of the `voice` gate means `stt::local`'s tests run under
// `--features llm-cpu` alone, without pulling in `cpal` (and its ALSA/
// CoreAudio/WASAPI system dependency) at all.
pub mod stt;
pub mod sync;
#[cfg(test)]
mod test_log;
pub mod tls;
pub mod vault_events;
pub mod vault_gate;
// Unconditional like `stt`, above — see `voice.rs`'s module doc for why
// the stub commands live in the same file as the real ones.
pub mod voice;

pub use error::{HolziError, Result};
pub use state::{ActiveInstanceHandle, AppState};

use catalog::commands::catalog_recommend_tiers;
use catalog::list_catalog;
use chat::action_commands::{respond_action_call, set_agent_actions};
use chat::choices::respond_choice;
use chat::commands::{
    abort_current_generation, inspect_attachment, respond_tool_permission, send_message,
};
use chat::default_model::{model_load_status, resolve_default_model};
use chat::model_loading::{
    active_model_info, load_model, load_model_with_integrity_override, unload_local_model,
};
use chat::session::ChatState;
use chat::thread_commands::{
    create_thread, delete_thread, list_messages, list_threads, rename_thread,
};
use contacts::commands::agent::{contacts_agent_get, contacts_agent_search, contacts_agent_update};
use contacts::commands::cards::{
    contacts_block, contacts_blocked, contacts_share_accept, contacts_share_build,
    contacts_share_options, contacts_share_preview, contacts_unblock,
};
use contacts::commands::contacts::{
    contacts_create, contacts_delete, contacts_get, contacts_group_delete, contacts_group_members,
    contacts_group_save, contacts_groups, contacts_list, contacts_photo, contacts_set_photo,
    contacts_update,
};
use contacts::commands::phone::{
    contacts_phone_accounts, contacts_phone_confirm, contacts_phone_disable, contacts_phone_enable,
    contacts_phone_mass_delete, contacts_phone_review, contacts_phone_run,
    contacts_phone_set_target, contacts_phone_status,
};
use contacts::commands::targets::{contacts_phones, contacts_targets_remove, contacts_targets_set};
use device::commands::{current_device_info, list_vault_devices, update_device_alias};
use extensions::agent_tools::commands::{
    extension_agent_frame_ready, extension_agent_tools_set, extension_mcp_send,
};
use extensions::commands::dev::{
    extension_dev_confirm, extension_dev_load, extension_dev_mode_get, extension_dev_mode_set,
    extension_dev_unload,
};
use extensions::commands::frames::{
    extension_bridge_call, extension_dialog_resolve, extension_encrypted_folder_choose_resolve,
    extension_frame_close, extension_frame_open, extension_frame_reloaded,
    extension_host_context_set,
};
use extensions::commands::install::{extension_install, extension_install_preview};
use extensions::commands::manage::{
    extension_icon, extension_limits_get, extension_limits_set, extension_list,
    extension_logs_read, extension_purge_kept_data, extension_remove, extension_set_enabled,
};
use extensions::commands::model_grants::{
    extension_model_grants_list, extension_model_grants_reorder, extension_model_grants_revoke,
    extension_model_grants_set,
};
use extensions::commands::peers::{
    extension_peer_invitation_answer, extension_peers_frame_overflow,
};
use extensions::commands::permissions::{
    extension_permission_cancel, extension_permission_remove, extension_permission_resolve,
    extension_permission_set, extension_permissions_list,
};
use hardware::get_hardware_info;
use instances::{
    active_instance_name, change_vault_passphrase, cleanup_orphans_on_startup, close_instance,
    create_instance, import_instance, list_instances, open_instance, paths::get_app_local_data,
    ProcessPresence,
};
use models::commands::{
    check_huggingface_model_updates, delete_installed_model, download_model_from_catalog,
    download_model_from_hf, get_huggingface_model_details, import_model_from_file,
    install_huggingface_update, list_installed_models, preview_huggingface_install,
    search_huggingface_models,
};
use models::download_check::model_download_check;
use models::stand_in::e2e_stand_in_models;
use passwords::commands::agent::passwords_agent_search;
use passwords::commands::attachments::{
    passwords_attachment_add, passwords_attachment_open, passwords_attachment_open_system,
    passwords_attachment_preview, passwords_attachment_remove, passwords_attachment_rename,
    passwords_attachment_save, passwords_attachment_text,
};
use passwords::commands::copy::passwords_copy;
use passwords::commands::history::{
    passwords_history_copy, passwords_history_get, passwords_history_list,
    passwords_history_restore, passwords_history_reveal,
};
use passwords::commands::import::{
    passwords_icon_preview, passwords_import_cancel, passwords_import_preview,
    passwords_import_report_save, passwords_import_run,
};
use passwords::commands::items::{passwords_create_item, passwords_update_item};
use passwords::commands::organize::{
    passwords_create_group, passwords_delete_tag, passwords_move, passwords_rename_tag,
    passwords_reorder_groups, passwords_set_tag_color, passwords_set_tags, passwords_update_group,
};
use passwords::commands::passkeys::{
    passwords_passkey_delete, passwords_passkey_rename, passwords_passkey_unlink,
};
use passwords::commands::presets::{
    passwords_preset_delete, passwords_preset_list, passwords_preset_save,
};
use passwords::commands::read::{
    passwords_copy_field, passwords_copy_text, passwords_get_item, passwords_load_overview,
    passwords_reveal, passwords_totp_code,
};
use passwords::commands::references::{
    passwords_item_key_names, passwords_reference_token, passwords_reference_usage,
    passwords_references_parse,
};
use passwords::commands::trash::{
    passwords_delete_permanently, passwords_empty_trash, passwords_restore, passwords_trash,
};
use passwords::commands::usage::passwords_item_usage;
use providers::capabilities::set_model_user_capabilities;
use providers::connect::{connect_cli_delegate, submit_cli_delegate_code, DelegateConnectState};
use providers::{
    add_provider, delete_provider, list_provider_models, list_providers, refresh_provider_models,
};
use remote_storage::commands::{
    storage_connection_remove, storage_connection_save, storage_credentials_candidates,
    storage_dialog_resolve, storage_folder_create, storage_host_key_accept, storage_list,
    storage_removal_preview, storage_remove, storage_save, storage_space, storage_ssh_key_generate,
    storage_test,
};
use storage::preferences_commands::{clear_pref, get_pref, set_pref};
use storage::wm_session_commands::{
    wm_session_load, wm_session_restore_get, wm_session_restore_set, wm_session_save,
};
use stt::commands::{
    download_stt_model, list_installed_stt_models, list_stt_catalog, stt_recommend_tiers,
};
use tauri::{Emitter, Manager};
use voice::{
    cancel_voice_recording, invalidate_stt_model_cache, start_voice_recording, stop_voice_recording,
};

/// Undoes, for this process's children only, the Nix devShell's
/// `LD_LIBRARY_PATH` export (flake.nix's shellHook) — present only when
/// `build.rs`'s `configure_nix_devshell_linker` also overrode this binary's
/// own dynamic-linker interpreter to the host's, which is when that variable
/// is actually needed (to resolve the Nix-provided GTK stack the interpreter
/// override doesn't cover) and, unmodified, would otherwise leak into every
/// child process GTK/WebKit forks (WebKitWebProcess, WebKitNetworkProcess,
/// Mesa's GBM driver loader) — all built against the *host's* glibc, not
/// Nix's, so they'd hit the same `GLIBC_PRIVATE` symbol clash the interpreter
/// override exists to avoid on this binary. Safe to clear this late: the
/// dynamic linker already consulted it to resolve this process's own NEEDED
/// libraries before `main` ever ran; removing it from the environment now
/// only stops it from propagating to processes spawned from here on.
fn clear_inherited_nix_library_path() {
    if option_env!("HOLZI_NIX_DEVSHELL_LINKER").is_some() {
        std::env::remove_var("LD_LIBRARY_PATH");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Builds and starts the holzi Tauri application.
pub fn run() {
    clear_inherited_nix_library_path();
    let mut args = std::env::args().skip(1);
    let first = args.next();
    // Spec 021: an agent started holzi as its MCP server; no window, no vault, no Tauri.
    if first.as_deref() == Some(mcp_server::helper::FLAG) {
        std::process::exit(mcp_server::helper::run_process(args));
    }
    if first.as_deref() == Some("--internal-cli-delegate-approval-bridge") {
        let Some(socket_flag) = args.next().filter(|arg| arg == "--socket") else {
            eprintln!("missing --socket for CLI delegate approval bridge");
            return;
        };
        let _ = socket_flag;
        let Some(socket_path) = args.next() else {
            eprintln!("missing socket path for CLI delegate approval bridge");
            return;
        };
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                eprintln!("failed to start approval bridge runtime: {error}");
                return;
            }
        };
        if let Err(error) = runtime.block_on(
            adapters::cli_delegate::permission_mcp_server::run_bridge_process(
                std::path::Path::new(&socket_path),
            ),
        ) {
            eprintln!("CLI delegate approval bridge failed: {error}");
        }
        return;
    }
    // One gate per app process: it is managed as state and wraps the invoke handler below, so
    // every request passes through it.
    let gate = vault_gate::VaultGate::new();
    let builder = tauri::Builder::default().manage(AppState::new(gate.clone()));
    let builder = builder.manage(gate.clone());
    let builder = builder.manage(ChatState::with_children(gate.children()));
    let builder = builder.manage(DelegateConnectState::new());
    let builder = builder.manage(voice::VoiceState::new());
    // Spec 051: one run of the sync with the phone's address book at a time.
    let builder = builder.manage(std::sync::Arc::new(
        contacts::phone::trigger::PhoneRuns::default(),
    ));
    // Spec 024: commands reach the sync service of the open vault through this registry, and the
    // start page's link join runs without a vault.
    let builder = builder.manage(std::sync::Arc::new(sync::registry::SyncRegistry::default()));
    let builder = builder.manage(sync::link::join_task::LinkJoin::default());
    // Spec 017: files of extensions for their sandboxed frames.
    let builder = extensions::protocol::handler::register(builder);
    // Spec 051 (research R13): QR codes of Visitenkarten from the camera, on phones only.
    #[cfg(mobile)]
    let builder = builder.plugin(tauri_plugin_barcode_scanner::init());
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        // Password manager (spec 034, research R9): used from Rust only, so a copied secret and its
        // timed clearing never pass through the webview. No JS permission is granted for it.
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        // Spec 017 US8: system notifications of extensions, with clicks (desktop: forked, Cargo.toml).
        .plugin(tauri_plugin_notification::init())
        // Spec 043 (ADR-0010): Android platform code; does nothing elsewhere.
        .plugin(tauri_plugin_holzi_android::init())
        .plugin(tauri_plugin_deep_link::init())
        .setup(|app| {
            // Spec 043 (research R2): certificate checks need this on Android before any
            // network service starts.
            tls::init_platform_verifier()?;
            // Spec 043 (FR-011, FR-012): the space the system bars and the keyboard take.
            platform::insets::watch(app.handle());
            // Spec 043 (research R5, R10): the sync's network changes and the phone's name come
            // from the platform on Android; elsewhere both calls do nothing.
            sync::resume::watch_network(app.handle());
            {
                use tauri_plugin_holzi_android::HolziAndroidExt;
                hardware::hostname::use_platform_name(app.holzi_android().device_name());
            }
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        // iroh logs every sent packet at info; the log keeps only 40 kB, so
                        // without this the sync lines are rotated away within seconds.
                        .level_for("iroh", log::LevelFilter::Warn)
                        .level_for("tracing::span", log::LevelFilter::Warn)
                        .build(),
                )?;
            }
            // Orphan cleanup before any command handler can run, gated by presence (spec 013
            // US4, FR-026, SC-010): only the process alone on this app-local-data directory runs
            // it. A relaunch that starts while the old process is still draining is not alone,
            // so it correctly skips the cleanup here — the old process's own close is what is
            // still tidying up, not a crash to clean up after. A failure inside the cleanup
            // itself is logged but does not abort startup — an orphan just stays around, and
            // `list_instances` filters it out because of its sibling `.pending` marker.
            let app_local_data = get_app_local_data(app.handle())?;
            let app_for_cleanup = app.handle().clone();
            let presence = ProcessPresence::announce(&app_local_data, move || {
                if let Err(e) = cleanup_orphans_on_startup(&app_for_cleanup) {
                    log::warn!("startup orphan cleanup failed: {e}");
                }
            })?;
            // Spec 060 research R9: a process the system started for a link, while others run, hands
            // it to them and ends; the window of the one that takes it shows the preview.
            #[cfg(desktop)]
            if deep_link::handed_over_at_start(app.handle(), presence.was_alone()) {
                std::process::exit(0);
            }
            #[cfg(any(target_os = "linux", windows))]
            deep_link::register_for_development(app.handle());
            deep_link::watch_opened_links(app.handle());
            app.manage(presence);
            // Spec 038: deleting the credentials of a storage connection in the password manager
            // warns first (spec 034 FR-034); the provider reads the vault open when it is asked.
            let app_for_usage = app.handle().clone();
            app.state::<AppState>()
                .usage()
                .register(std::sync::Arc::new(
                    remote_storage::credentials::StorageUsage::new(move || {
                        app_for_usage.try_state::<AppState>()?.database().ok()
                    }),
                ));
            // Spec 017 US8: what holzi does outside its window for extensions (browser, notifications).
            app.state::<AppState>()
                .extensions()
                .set_desktop(std::sync::Arc::new(extensions::desktop::AppDesktop::new(
                    app.handle().clone(),
                )));
            // Spec 055 FR-033: a job on a locally installed model borrows the chat's model place.
            #[cfg(feature = "llm-cpu")]
            app.state::<AppState>()
                .extensions()
                .set_model_local(std::sync::Arc::new(model_jobs::chat_slot::ChatSlot::new(
                    app.handle().clone(),
                )));
            // Spec 017, US12: holzi's window is built here, not from `tauri.conf.json`, so its
            // document can frame development servers while developer mode is on (`dev_csp`).
            let window = app
                .config()
                .app
                .windows
                .first()
                .cloned()
                .ok_or("tauri.conf.json defines no window")?;
            let host = app.state::<AppState>().extensions();
            // Spec 044 (research R4): the media server lives as long as the vault session.
            let media = tauri::async_runtime::block_on(files::media::MediaServer::start(
                app.state::<AppState>().gate().token(),
            ))?;
            // Spec 044 (T007, T041): the CI's platform probe, only in builds with the feature.
            #[cfg(feature = "platform-probe")]
            let probe = if platform_probe::requested() {
                Some((
                    platform_probe::start(app.handle())?,
                    platform_probe::serve_fixtures(&media),
                ))
            } else {
                None
            };
            app.manage(media);
            #[cfg(feature = "platform-probe")]
            let probing = probe.is_some();
            let builder = tauri::WebviewWindowBuilder::from_config(app.handle(), &window)?
                // The frame shim for development pages, which holzi does not serve (R16).
                .initialization_script_for_all_frames(extensions::protocol::shim::dev_init_script())
                // Spec 044: a reloaded page cannot end the folder watches of the page before it.
                .on_page_load(|window, payload| {
                    if payload.event() == tauri::webview::PageLoadEvent::Started {
                        if let Some(files) = window.try_state::<files::state::FilesState>() {
                            files.unwatch_all();
                        }
                        // Spec 064: nor its subscriptions to the drive list.
                        if let Some(monitor) =
                            window.try_state::<files::local::drives::monitor::DriveMonitor>()
                        {
                            monitor.unsubscribe_all();
                        }
                    }
                })
                .on_web_resource_request(move |request, response| {
                    extensions::dev_csp::adjust(&host, request, response);
                    #[cfg(feature = "platform-probe")]
                    if probing {
                        platform_probe::allow_loopback(response);
                    }
                });
            #[cfg(feature = "platform-probe")]
            let builder = match &probe {
                Some((port, media)) => {
                    builder.initialization_script(platform_probe::init_script(*port, media))
                }
                None => builder,
            };
            builder.build()?;
            // Spec 032: actions of a model's tool call go out as events; `ChatState` is managed
            // without an `AppHandle`, so the emitter is set here.
            // Spec 044: the file browser's places, thumbnail cache and folder watches.
            let files_state = files::state::FilesState::from_app(app.handle());
            // Spec 048 US6: extensions share the encrypted folders the window opened; a vault's
            // close ends the process and them with it (ADR-0003).
            app.state::<AppState>()
                .extensions()
                .encrypted
                .set_folders(std::sync::Arc::clone(&files_state.encrypted));
            app.manage(files_state);
            app.manage(files::transfer::TransferManager::default());
            app.manage(files::search::SearchManager::default());
            // Spec 064 (research R4, R8): the live drive list while a file browser listens.
            app.manage(files::local::drives::monitor::DriveMonitor::new(
                files::local::drives::backend::backend(),
                std::time::Duration::from_secs(2),
            ));
            app.manage(file_sync::service::FileSyncState::default());
            // Spec 044 US6 (ADR 0011): the file actions of the built-in agent run here, without a
            // window; a storage without a grant is asked for through the window.
            // Spec 049: the External Bridge asks the window; it starts when a vault opens.
            let browser_bridge = browser_bridge::BrowserBridge::default();
            browser_bridge.attach(app.handle());
            app.manage(browser_bridge);
            // Spec 021: the MCP access asks the window to register agents.
            let mcp = mcp_server::McpServer::default();
            mcp.attach(app.handle());
            app.manage(mcp);
            let prompt = files::agent::prompt::PermissionPrompt::default();
            app.manage(prompt.clone());
            let files_agent: files::agent::commands::BuiltinFilesAgent =
                std::sync::Arc::new(files::agent::exec::FilesAgent::builtin(
                    files::agent::env::TauriEnv::new(app.handle().clone(), prompt),
                ));
            app.state::<ChatState>()
                .native_actions
                .add(files_agent.clone());
            app.manage(files_agent);
            // Spec 056 US6: the share actions of the built-in agent, confirmed in the window.
            let share_confirm = files::agent::share_confirm::ShareConfirm::default();
            app.manage(share_confirm.clone());
            app.state::<ChatState>()
                .native_actions
                .add(std::sync::Arc::new(
                    files::agent::shares::SharesAgent::builtin(
                        files::agent::env::TauriSharesEnv::new(app.handle().clone(), share_confirm),
                    ),
                ));
            // Spec 060 US5: the share action for contacts, confirmed in the window every time.
            let contact_share_confirm =
                contacts::agent_share_confirm::ContactShareConfirm::default();
            app.manage(contact_share_confirm.clone());
            app.state::<ChatState>()
                .native_actions
                .add(std::sync::Arc::new(
                    contacts::agent_share::ContactsShareAgent::builtin(
                        contacts::agent_share::TauriContactsShareEnv::new(
                            app.handle().clone(),
                            contact_share_confirm,
                        ),
                    ),
                ));
            // Spec 017, US9: holzi's protected places, known places and dialogs for extensions.
            app.state::<AppState>()
                .extensions()
                .fs
                .set_environment(extensions::fs::environment_for(app.handle()));
            let handle = app.handle().clone();
            app.state::<ChatState>()
                .action_bridge
                .set_emitter(std::sync::Arc::new(move |event, payload| {
                    let _ = handle.emit(event, payload);
                }));
            Ok(())
        })
        .invoke_handler(gate.wrap(tauri::generate_handler![
            #[cfg(feature = "platform-probe")]
            platform_probe::platform_probe_report,
            active_instance_name,
            platform::commands::platform_capabilities,
            files::commands::picked_file_name,
            files::browser_commands::navigate::files_sources,
            files::browser_commands::drives::files_drives_watch,
            files::browser_commands::drives::files_drives_unwatch,
            files::browser_commands::drives::files_drive_open,
            files::browser_commands::drives::files_drive_release,
            files::browser_commands::navigate::files_all_access,
            files::browser_commands::navigate::files_request_all_access,
            files::browser_commands::navigate::files_list,
            files::browser_commands::navigate::files_stat,
            files::browser_commands::view::files_read_text,
            files::browser_commands::view::files_open,
            files::browser_commands::view::files_release,
            files::browser_commands::view::files_release_tab,
            files::browser_commands::change::files_create_folder,
            files::browser_commands::change::files_rename,
            files::browser_commands::change::files_transfer_start,
            files::browser_commands::change::files_import_dropped,
            files::browser_commands::change::files_transfer_answer,
            files::browser_commands::change::files_transfer_cancel,
            files::browser_commands::change::files_transfer_pause,
            files::browser_commands::change::files_transfer_retry,
            files::browser_commands::search::files_search_start,
            files::browser_commands::search::files_search_cancel,
            files::browser_commands::view::files_thumbnail,
            files::icons::commands::files_icons_resolve,
            files::icons::commands::files_icon_image,
            files::icons::commands::file_icon_sets_list,
            files::icons::commands::file_icon_set_selection,
            files::icons::commands_import::file_icon_set_import_preview,
            files::icons::commands_import::file_icon_set_import,
            files::icons::commands_import::file_icon_set_remove,
            files::icons::commands::file_icon_set_licenses,
            files::browser_commands::navigate::files_watch,
            files::browser_commands::navigate::files_unwatch,
            files::browser_commands::peers::files_peer_roots,
            files::browser_commands::peers::files_peer_trash,
            files::browser_commands::peers::files_contact_query,
            files::share::commands::file_shares_list,
            file_sync::commands::sync_rules_list,
            file_sync::commands::sync_rules_create,
            file_sync::commands::sync_rules_update,
            file_sync::commands::sync_rules_set_enabled,
            file_sync::commands::sync_rules_delete,
            file_sync::commands::sync_rules_run_now,
            file_sync::commands::sync_rules_cancel,
            files::share::commands::file_share_of,
            files::share::commands::file_share_save,
            files::share::commands::file_share_revoke,
            files::share::commands::file_share_delete,
            files::share::commands::file_shares_of_contact,
            files::share::commands::files_device_access_get,
            files::share::commands::files_device_access_set,
            files::agent::share_confirm::file_share_confirm_answer,
            contacts::agent_share_confirm::contact_share_confirm_answer,
            files::browser_commands::view::files_open_system,
            files::browser_commands::encrypted::files_create_encrypted_folder,
            files::browser_commands::encrypted::files_encrypted_notice,
            files::browser_commands::encrypted::files_encrypted_notice_ack,
            files::browser_commands::encrypted::files_encrypted_refresh,
            files::agent::commands::files_agent_permission_answer,
            browser_bridge::commands::browser_bridge_overview,
            browser_bridge::commands::browser_bridge_configure,
            browser_bridge::commands::browser_bridge_answer,
            browser_bridge::commands::browser_bridge_client_update,
            browser_bridge::commands::browser_bridge_client_revoke,
            browser_bridge::commands::browser_bridge_client_unblock,
            browser_bridge::commands::browser_bridge_notify,
            mcp_server::commands::mcp_status,
            mcp_server::commands::mcp_configure,
            mcp_server::commands::mcp_registration_answer,
            mcp_server::commands::mcp_call_answer,
            mcp_server::commands::mcp_agent_log,
            mcp_server::manage_commands::mcp_agents_list,
            mcp_server::manage_commands::mcp_agent_get,
            mcp_server::manage_commands::mcp_agent_rights_set,
            mcp_server::manage_commands::mcp_agent_mode_set,
            mcp_server::manage_commands::mcp_agent_revoke,
            mcp_server::manage_commands::mcp_agent_unblock,
            mcp_server::manage_commands::mcp_agent_log_clear,
            mcp_server::commands::mcp_setup_snippet,
            files::agent::commands::files_agent_permissions,
            files::agent::commands::files_agent_permission_set,
            files::agent::commands::files_agent_permission_remove,
            files::agent::commands::files_agent_check,
            platform::insets::device_insets,
            privacy::screen_capture::screen_capture_protection_get,
            privacy::screen_capture::screen_capture_protection_set,
            list_instances,
            create_instance,
            import_instance,
            open_instance,
            close_instance,
            change_vault_passphrase,
            get_hardware_info,
            list_catalog,
            catalog_recommend_tiers,
            add_provider,
            list_providers,
            delete_provider,
            connect_cli_delegate,
            submit_cli_delegate_code,
            refresh_provider_models,
            set_model_user_capabilities,
            list_provider_models,
            download_model_from_catalog,
            model_download_check,
            e2e_stand_in_models,
            download_model_from_hf,
            import_model_from_file,
            list_installed_models,
            delete_installed_model,
            search_huggingface_models,
            get_huggingface_model_details,
            preview_huggingface_install,
            install_huggingface_update,
            check_huggingface_model_updates,
            load_model,
            load_model_with_integrity_override,
            unload_local_model,
            active_model_info,
            model_load_status,
            resolve_default_model,
            send_message,
            inspect_attachment,
            abort_current_generation,
            respond_tool_permission,
            set_agent_actions,
            respond_action_call,
            respond_choice,
            create_thread,
            list_threads,
            list_messages,
            rename_thread,
            delete_thread,
            current_device_info,
            update_device_alias,
            list_vault_devices,
            sync::commands::link_code_create,
            sync::commands::link_code_cancel,
            sync::commands::link_confirm,
            sync::commands::link_reject,
            sync::commands::link_join_start,
            sync::commands::link_join_status,
            sync::commands::link_join_cancel,
            sync::commands::device_remove,
            sync::commands::admission_decide,
            sync::commands::sync_copy_notice_dismiss,
            sync::commands::sync_status,
            sync::commands::vault_public_identity,
            sync::commands::sync_servers_defaults,
            sync::commands::sync_servers_get,
            sync::commands::sync_servers_set,
            get_pref,
            set_pref,
            clear_pref,
            wm_session_restore_get,
            wm_session_restore_set,
            wm_session_load,
            wm_session_save,
            start_voice_recording,
            stop_voice_recording,
            cancel_voice_recording,
            invalidate_stt_model_cache,
            list_stt_catalog,
            stt_recommend_tiers,
            list_installed_stt_models,
            download_stt_model,
            // Password manager (spec 034).
            passwords_load_overview,
            passwords_get_item,
            passwords_reveal,
            passwords_totp_code,
            passwords_copy_field,
            passwords_copy_text,
            contacts_list,
            contacts_get,
            contacts_create,
            contacts_update,
            contacts_delete,
            contacts_set_photo,
            contacts_photo,
            contacts_groups,
            contacts_group_save,
            contacts_group_delete,
            contacts_group_members,
            contacts_phone_enable,
            contacts_phone_disable,
            contacts_phone_accounts,
            contacts_phone_set_target,
            contacts_phone_review,
            contacts_phone_confirm,
            contacts_phone_status,
            contacts_phone_mass_delete,
            contacts_phone_run,
            contacts_phones,
            contacts_targets_set,
            contacts_targets_remove,
            contacts_share_options,
            contacts_share_build,
            contacts_share_preview,
            contacts_share_accept,
            contacts_blocked,
            contacts_block,
            contacts_unblock,
            contacts_agent_search,
            contacts_agent_get,
            contacts_agent_update,
            passwords_create_item,
            passwords_update_item,
            passwords_passkey_rename,
            passwords_passkey_delete,
            passwords_passkey_unlink,
            passwords_create_group,
            passwords_update_group,
            passwords_reorder_groups,
            passwords_move,
            passwords_set_tags,
            passwords_rename_tag,
            passwords_set_tag_color,
            passwords_delete_tag,
            passwords_preset_list,
            passwords_preset_save,
            passwords_preset_delete,
            passwords_item_usage,
            storage_list,
            storage_space,
            storage_connection_save,
            storage_credentials_candidates,
            storage_connection_remove,
            storage_save,
            storage_remove,
            storage_removal_preview,
            storage_test,
            storage_folder_create,
            storage_ssh_key_generate,
            storage_host_key_accept,
            storage_dialog_resolve,
            passwords_references_parse,
            passwords_copy,
            passwords_reference_token,
            passwords_item_key_names,
            passwords_reference_usage,
            passwords_agent_search,
            passwords_trash,
            passwords_restore,
            passwords_delete_permanently,
            passwords_empty_trash,
            passwords_history_list,
            passwords_history_get,
            passwords_history_reveal,
            passwords_history_copy,
            passwords_history_restore,
            passwords_attachment_add,
            passwords_attachment_rename,
            passwords_attachment_remove,
            passwords_attachment_save,
            passwords_attachment_preview,
            passwords_attachment_text,
            passwords_attachment_open,
            passwords_attachment_open_system,
            passwords_import_preview,
            passwords_import_run,
            passwords_import_cancel,
            passwords_import_report_save,
            passwords_icon_preview,
            extension_install_preview,
            extension_install,
            extension_list,
            extension_remove,
            extension_set_enabled,
            extension_purge_kept_data,
            extension_limits_get,
            extension_limits_set,
            extension_logs_read,
            extension_dev_mode_get,
            extension_dev_mode_set,
            extension_dev_load,
            extension_dev_confirm,
            extension_dev_unload,
            extension_icon,
            extension_frame_open,
            extension_frame_close,
            extension_frame_reloaded,
            extension_bridge_call,
            extension_mcp_send,
            extension_agent_frame_ready,
            extension_agent_tools_set,
            extension_host_context_set,
            extension_dialog_resolve,
            extension_encrypted_folder_choose_resolve,
            extension_peer_invitation_answer,
            extension_peers_frame_overflow,
            extension_permissions_list,
            extension_permission_set,
            extension_permission_remove,
            extension_permission_resolve,
            extension_permission_cancel,
            extension_model_grants_list,
            extension_model_grants_set,
            extension_model_grants_revoke,
            extension_model_grants_reorder,
        ]))
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| match event {
            // Closing the window or quitting from the system menu is the user leaving: it goes
            // through the same close as the lock control, and the process ends when it is done
            // (spec 013 FR-007). A relaunch is the app restarting itself and must pass.
            tauri::RunEvent::WindowEvent {
                event: tauri::WindowEvent::CloseRequested { api, .. },
                ..
            } => {
                if instances::take_over_exit(app) {
                    api.prevent_close();
                }
            }
            tauri::RunEvent::ExitRequested { code, api, .. } => {
                let relaunching = code == Some(tauri::RESTART_EXIT_CODE);
                if !relaunching && instances::take_over_exit(app) {
                    api.prevent_exit();
                }
            }
            // Spec 043 (FR-018, research R5): back in the foreground, the sync catches up at once
            // instead of on its next tick. Nothing runs on `Suspended`: no background sync.
            #[cfg(target_os = "android")]
            tauri::RunEvent::Resumed => {
                sync::resume::wake_app(app, sync::resume::Wake::Resumed);
                // Spec 051 (FR-020): changes of the phone's address book from the time away.
                contacts::phone::trigger::start(app);
            }
            // Spec 043 (research R4): Android keeps the process of a finished activity cached.
            // End it, so the next start is a new process at the vault picker (ADR-0003).
            #[cfg(target_os = "android")]
            tauri::RunEvent::Exit => platform::end_process(0),
            _ => {}
        });
}
