//! The services of a vault session, started the same way whether the vault was opened, created or
//! imported. They were three copies once, and the created vault lacked the extension tools of the
//! agent (spec 018) until it was opened again.

use tauri::AppHandle;

use crate::chat::session::ChatState;
use crate::state::AppState;

/// Starts every service that runs while the vault is open; each ends with the session.
pub async fn start(app: &AppHandle, state: &AppState, chat: &ChatState) {
    // Spec 094: what an earlier session left in the vault's tool folders (an interrupted install,
    // run folders of a killed run) goes before anything can start a delegate.
    sweep_cli_tools(app, state).await;
    // Spec 024: the sync service runs as tracked session work and ends with the close.
    crate::vault_events::start_for_active_instance(app, state);
    crate::extensions::registry::lifecycle::start_for_active_instance(app, state);
    // Spec 057: channels end when their permission or the other vault's standing goes.
    crate::extensions::peers::watch::start_for_active_instance(app, state);
    // Spec 018: the confirmed tools of extensions, offered to the agent.
    crate::extensions::agent_tools::registration::start_for_active_instance(app, state, chat);
    crate::extensions::sql::changes::start_for_active_instance(app, state);
    crate::sync::start_for_active_instance(app, state).await;
    // Spec 072: the chat with contacts receives and sends over Nostr-Relays while the vault is open.
    crate::chat::peer::service::start_for_active_instance(app, state);
    // Spec 061: sync rules run on this device while the vault is open; other own devices are
    // reached through the sync just started.
    crate::file_sync::service::start_for_active_instance(app, state);
    // Spec 049: the External Bridge, when this device turned it on.
    crate::browser_bridge::start_for_active_instance(app, state);
    // Spec 084: the tracker and DNS choices of this vault (and of the old files, once).
    crate::agent_browser::choices::follow_tracker_preferences(app, state);
    if let Ok(db) = state.database() {
        crate::agent_browser::choices::apply_vault_choices(app, &db).await;
    }
    // Spec 021: the MCP access for external agents, when this device turned it on.
    crate::mcp_server::start_for_active_instance(app, state);
    // Spec 051 (FR-021): changes of contacts reach the phone's address book.
    crate::contacts::phone::trigger::watch(app);
}

/// The sweep of `cli_tools::open_sweep` for the vault that just opened. It runs off the async
/// executor (removing a half-unpacked package touches many files) and never fails the open.
async fn sweep_cli_tools(app: &AppHandle, state: &AppState) {
    let Ok(Some(name)) = state.active_name() else {
        return;
    };
    let Ok(paths) = super::paths::vault_paths(app) else {
        return;
    };
    // A failed join only means the sweep did not run; the next open repeats it.
    let _ = tokio::task::spawn_blocking(move || crate::cli_tools::open_sweep(&paths, &name)).await;
}
