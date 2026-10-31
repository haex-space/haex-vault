//! The extension tools in the chat's registry (spec 018, FR-010): built when a vault opens and
//! again after every change to the extensions or their permissions, whichever device made it.
//! Closing the vault empties the registry like the action tools. The same tools are published for
//! external agents (spec 021 US4), which see those of the extensions granted to them.

use std::sync::Arc;

use tauri::AppHandle;
use tokio::sync::broadcast::error::RecvError;

use super::defs::{load, ExtensionToolDef};
use super::frame_request::WindowLinks;
use super::tool::{ExtensionTool, Links, SOURCE};
use crate::chat::session::ChatState;
use crate::chat::tools::{Tool, ToolRegistry};
use crate::state::AppState;
use crate::storage::wm_session_commands::current_device_uuid;

/// Tables whose changes can change which tools are offered.
const WATCHED: [&str; 7] = [
    "extensions",
    "extension_bundles",
    "extension_device_status",
    "extension_permissions",
    "dev_extensions_no_sync",
    "dev_extension_permissions_no_sync",
    // Developer mode is a preference of the device (spec 018 FR-020).
    "preferences",
];

pub(crate) fn touches_tools(tables: &[String]) -> bool {
    tables.iter().any(|t| WATCHED.contains(&t.as_str()))
}

/// Replaces every extension tool in `registry` with tools for `defs`, and returns them.
pub fn register_extension_tools(
    registry: &mut ToolRegistry,
    defs: Vec<ExtensionToolDef>,
    links: &Arc<dyn Links>,
) -> Vec<Arc<dyn Tool>> {
    registry.remove_source(SOURCE);
    let tools: Vec<Arc<dyn Tool>> = defs
        .into_iter()
        .map(|def| Arc::new(ExtensionTool::new(def, Arc::clone(links))) as Arc<dyn Tool>)
        .collect();
    for tool in &tools {
        registry.register(Arc::clone(tool));
    }
    tools
}

/// Follows the vault session `state` just published, until it ends. A failure is logged; the
/// agent then has no extension tools (Constitution VII: never blocks the session).
pub fn start_for_active_instance(app: &AppHandle, state: &AppState, chat: &ChatState) {
    let db = match state.database() {
        Ok(db) => db,
        Err(error) => {
            log::warn!("agent tools: no open vault: {error}");
            return;
        }
    };
    let device = match current_device_uuid(app, &db) {
        Ok(device) => device,
        Err(error) => {
            log::warn!("agent tools: this device is unknown: {error}");
            return;
        }
    };
    let mut changes = state.vault_changes().subscribe();
    let token = state.gate().token();
    let registry = Arc::clone(&chat.tool_registry);
    let published = chat.extension_tools.clone();
    let links: Arc<dyn Links> = Arc::new(WindowLinks(app.clone()));
    let follow = async move {
        let mut due = true;
        loop {
            if due {
                match db.read(move |q| load(q, device).map_err(Into::into)).await {
                    Ok(defs) => {
                        if token.is_cancelled() {
                            return;
                        }
                        let tools = {
                            let mut registry = registry.lock().unwrap_or_else(|e| e.into_inner());
                            register_extension_tools(&mut registry, defs, &links)
                        };
                        published.replace(tools);
                    }
                    Err(error) => log::warn!("agent tools: reading the extensions failed: {error}"),
                }
            }
            due = tokio::select! {
                () = token.cancelled() => return,
                received = changes.recv() => match received {
                    Ok(tables) => touches_tools(&tables),
                    Err(RecvError::Lagged(_)) => true,
                    Err(RecvError::Closed) => return,
                },
            };
        }
    };
    if let Err(error) = state.gate().spawn(follow) {
        log::warn!("agent tools: following the extensions did not start: {error}");
    }
}
