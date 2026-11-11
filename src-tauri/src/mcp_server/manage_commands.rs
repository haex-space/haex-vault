//! Commands of "KI → Agenten" (spec 021 US7, contracts/tauri-commands.md): the list of agents, one
//! agent, and the changes the guardrail actions `settings.agents.mcp.*` make. A change reaches the
//! agent's open connections at once; only the window calls these, no agent reaches a command.

use serde::Deserialize;
use tauri::{AppHandle, Runtime, State};
use ts_rs::TS;
use uuid::Uuid;

use super::agent_log;
use super::commands::open_device;

use super::hello::CloseReason;
use super::manage::{self, AgentView};
use super::registry;
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
        .read(move |q| manage::list_with_profile(q, device))
        .await?;
    let core = server.core().await;
    Ok(agents
        .into_iter()
        .map(|(agent, profile)| {
            let online = core
                .as_ref()
                .is_some_and(|core| core.live.connected(agent.id));
            AgentView::new(agent, profile, online)
        })
        .collect())
}

/// One agent.
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
            let profile = crate::agent_profiles::resolve::profile_of(
                q,
                &crate::agent_profiles::Subject::Agent(id),
            )?;
            Ok(Some((agent, profile)))
        })
        .await?;
    let (agent, profile) = found.ok_or_else(unknown_agent)?;
    let online = connected(&server, id).await;
    Ok(AgentView::new(agent, profile, online))
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
    if let (Some(core), Some(transport)) = (core, transport) {
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
