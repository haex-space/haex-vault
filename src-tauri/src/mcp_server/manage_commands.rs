//! Commands of "KI → Agenten" (spec 021 US7, contracts/tauri-commands.md): the list of agents, one
//! agent, and the changes the guardrail actions `settings.agents.mcp.*` make. A change reaches the
//! agent's open connections at once; only the window calls these, no agent reaches a command.

use serde::Deserialize;
use tauri::{AppHandle, Runtime, State};
use ts_rs::TS;
use uuid::Uuid;

use super::agent_log;
use super::commands::open_device;
use super::grants::{self, Rights};
use super::hello::CloseReason;
use super::manage::{self, AgentView};
use super::registry::{self, AgentMode, Transport};
use super::server::{Core, Notice};
use super::McpServer;
use crate::error::{HolziError, Result};
use crate::state::AppState;
use crate::vault_gate::VaultDb;

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct McpAgentArgs {
    pub agent_id: Uuid,
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct McpAgentRightsArgs {
    pub agent_id: Uuid,
    pub rights: Rights,
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct McpAgentModeArgs {
    pub agent_id: Uuid,
    pub mode: AgentMode,
}

fn unknown_agent() -> HolziError {
    HolziError::InvalidInput {
        reason: "no such agent".to_owned(),
    }
}

/// Whether the running server holds a connection of `agent`.
async fn connected(server: &McpServer, agent: Uuid) -> bool {
    match server.core().await {
        Some(core) => core.live.connected(agent),
        None => false,
    }
}

/// Tells the settings that the agents changed.
async fn changed(server: &McpServer) {
    server.notifier()(Notice::Changed);
}

/// Every agent of this device, newest first, blocked ones included (FR-026).
#[tauri::command]
pub async fn mcp_agents_list<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    server: State<'_, McpServer>,
) -> Result<Vec<AgentView>> {
    let device = open_device(&app, &state).await?;
    let agents = state
        .database()?
        .read(move |q| manage::list_with_rights(q, device))
        .await?;
    let core = server.core().await;
    Ok(agents
        .into_iter()
        .map(|(agent, rights)| {
            let online = core
                .as_ref()
                .is_some_and(|core| core.live.connected(agent.id));
            AgentView::new(agent, rights, online)
        })
        .collect())
}

/// One agent with all its rights.
#[tauri::command]
pub async fn mcp_agent_get<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    server: State<'_, McpServer>,
    args: McpAgentArgs,
) -> Result<AgentView> {
    let device = open_device(&app, &state).await?;
    let id = args.agent_id;
    let found = state
        .database()?
        .read(move |q| {
            let Some(agent) = registry::get(q, device, id)? else {
                return Ok(None);
            };
            let rights = registry::load_rights(q, device, id)?;
            Ok(Some((agent, rights)))
        })
        .await?;
    let (agent, rights) = found.ok_or_else(unknown_agent)?;
    let online = connected(&server, id).await;
    Ok(AgentView::new(agent, rights, online))
}

/// New rights; a connected agent uses them from its next call and hears that its tools changed
/// (FR-017). Behind `settings.agents.mcp.grants.set`.
#[tauri::command]
pub async fn mcp_agent_rights_set<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    server: State<'_, McpServer>,
    args: McpAgentRightsArgs,
) -> Result<()> {
    let device = open_device(&app, &state).await?;
    let core = server.core().await;
    let browser = args.rights.browser.is_some();
    set_rights(
        &state.database()?,
        device,
        core.as_deref(),
        args.agent_id,
        args.rights,
    )
    .await?;
    if !browser {
        super::end_browser_session(&app, args.agent_id);
    }
    changed(&server).await;
    Ok(())
}

/// Stores new rights and hands them to the agent's connections.
pub async fn set_rights(
    db: &VaultDb,
    device: Uuid,
    core: Option<&Core>,
    id: Uuid,
    rights: Rights,
) -> Result<()> {
    let _agent_guard = if let Some(core) = core {
        Some(core.live.synchronize(id).await)
    } else {
        None
    };
    grants::validate(&rights).map_err(|error| HolziError::InvalidInput {
        reason: error.to_string(),
    })?;
    let stored_rights = rights.clone();
    let stored = db
        .write(move |tx| manage::set_rights(tx, device, id, &stored_rights))
        .await?;
    if !stored {
        return Err(unknown_agent());
    }
    if let Some(core) = core {
        core.live.set_rights(id, &rights).await;
    }
    Ok(())
}

/// A new approval mode, from the next call on (FR-021). Behind `settings.agents.mcp.mode.set`.
#[tauri::command]
pub async fn mcp_agent_mode_set<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    server: State<'_, McpServer>,
    args: McpAgentModeArgs,
) -> Result<()> {
    let device = open_device(&app, &state).await?;
    let core = server.core().await;
    set_mode(
        &state.database()?,
        device,
        core.as_deref(),
        args.agent_id,
        args.mode,
    )
    .await?;
    changed(&server).await;
    Ok(())
}

/// Stores a new approval mode and hands it to the agent's connections.
pub async fn set_mode(
    db: &VaultDb,
    device: Uuid,
    core: Option<&Core>,
    id: Uuid,
    mode: AgentMode,
) -> Result<()> {
    let _agent_guard = if let Some(core) = core {
        Some(core.live.synchronize(id).await)
    } else {
        None
    };
    let stored = db
        .write(move |tx| manage::set_mode(tx, device, id, mode))
        .await?;
    if !stored {
        return Err(unknown_agent());
    }
    if let Some(core) = core {
        core.live.set_mode(id, mode);
    }
    Ok(())
}

/// Forgets the agent and ends its connections at once (FR-027). Behind
/// `settings.agents.mcp.revoke`.
#[tauri::command]
pub async fn mcp_agent_revoke<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    server: State<'_, McpServer>,
    args: McpAgentArgs,
) -> Result<()> {
    let device = open_device(&app, &state).await?;
    let core = server.core().await;
    revoke(&state.database()?, device, core.as_deref(), args.agent_id).await?;
    super::end_browser_session(&app, args.agent_id);
    changed(&server).await;
    Ok(())
}

/// Forgets the agent and ends its connections; their waiting prompts and calls end refused.
pub async fn revoke(db: &VaultDb, device: Uuid, core: Option<&Core>, id: Uuid) -> Result<()> {
    let _agent_guard = if let Some(core) = core {
        Some(core.live.synchronize(id).await)
    } else {
        None
    };
    let removed = db.write(move |tx| manage::revoke(tx, device, id)).await?;
    if let Some(core) = core {
        core.live.close(id, CloseReason::Revoked);
    }
    if removed {
        Ok(())
    } else {
        Err(unknown_agent())
    }
}

/// Lifts a block, so the agent may ask again. Behind `settings.agents.mcp.unblock`.
#[tauri::command]
pub async fn mcp_agent_unblock<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    server: State<'_, McpServer>,
    args: McpAgentArgs,
) -> Result<()> {
    let device = open_device(&app, &state).await?;
    let core = server.core().await;
    unblock(&state.database()?, device, core.as_deref(), args.agent_id).await?;
    changed(&server).await;
    Ok(())
}

/// Lifts a persisted and session-local block, so the agent can ask for registration again.
pub async fn unblock(db: &VaultDb, device: Uuid, core: Option<&Core>, id: Uuid) -> Result<()> {
    let (slug, transport) = db
        .write(move |tx| manage::unblock(tx, device, id))
        .await?
        .ok_or_else(unknown_agent)?;
    if let Some(core) = core {
        core.undecline(&slug, transport);
    }
    Ok(())
}

/// Empties the agent's log (FR-028). Behind `settings.agents.mcp.log.clear`.
#[tauri::command]
pub async fn mcp_agent_log_clear<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    args: McpAgentArgs,
) -> Result<()> {
    let device = open_device(&app, &state).await?;
    let id = args.agent_id;
    state
        .database()?
        .write(move |tx| agent_log::clear(tx, device, id))
        .await?;
    Ok(())
}
