//! The window's commands for the agent browser (spec 065, contracts/tauri-commands.md): answering
//! a browser question, the state for "KI → Browser", and the settings behind the guardrail actions
//! `settings.agentBrowser.*`, which no agent can call.

use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime, State};
use ts_rs::TS;

use super::env;
use super::locate::{self, Os, System};
use super::prompts::{BrowserDecision, BrowserPrompts, Remember};
use super::AgentBrowser;
use crate::error::{HolziError, Result};
use crate::mcp_server::grants::BrowserRights;

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentBrowserAnswerArgs {
    pub request_id: String,
    pub decision: BrowserDecision,
    #[serde(default)]
    #[ts(optional)]
    pub remember: Option<Remember>,
}

/// The window's answer to a browser question. An unknown id (the question ended) is no error.
#[tauri::command]
pub async fn agent_browser_prompt_answer(
    prompts: State<'_, BrowserPrompts>,
    args: AgentBrowserAnswerArgs,
) -> Result<bool> {
    Ok(prompts.answer(
        &args.request_id,
        super::prompts::BrowserAnswer {
            decision: args.decision,
            remember: args.remember,
        },
    ))
}

/// What "KI → Browser" shows.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentBrowserStatus {
    /// This device can have an agent browser (research R13).
    pub available: bool,
    /// The browser chosen in the settings; `None`: search.
    pub chosen_path: Option<String>,
    /// The browser holzi would start now.
    pub found_path: Option<String>,
    /// The product of the running browser (`Chrome/156.0.8078.73`).
    pub running: Option<String>,
    /// Why the last profile could not be read.
    pub profile_problem: Option<String>,
    /// The browser rights of the built-in agent; `None` is off.
    pub builtin: Option<BrowserRights>,
}

#[tauri::command]
pub async fn agent_browser_status<R: Runtime>(
    app: AppHandle<R>,
    browser: State<'_, AgentBrowser>,
) -> Result<AgentBrowserStatus> {
    let available = crate::platform::capabilities().agent_browser;
    if !available {
        return Ok(AgentBrowserStatus {
            available,
            chosen_path: None,
            found_path: None,
            running: None,
            profile_problem: None,
            builtin: None,
        });
    }
    let chosen = env::chosen_path(&app).await;
    let found = Os::current().and_then(|os| locate::find(os, chosen.as_deref(), &System));
    Ok(AgentBrowserStatus {
        available,
        chosen_path: chosen.map(|path| path.to_string_lossy().into_owned()),
        found_path: found.map(|path| path.to_string_lossy().into_owned()),
        running: browser.product().await,
        profile_problem: browser.profile_problem(),
        builtin: env::builtin_rights(&app).await,
    })
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentBrowserBuiltinArgs {
    /// `None` switches the browser off for the built-in agent.
    pub rights: Option<BrowserRights>,
}

/// Behind `settings.agentBrowser.builtin.set`.
#[tauri::command]
pub async fn agent_browser_builtin_set<R: Runtime>(
    app: AppHandle<R>,
    args: AgentBrowserBuiltinArgs,
) -> Result<()> {
    env::set_builtin_rights(&app, args.rights)
        .await
        .map_err(|reason| HolziError::InvalidInput { reason })
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentBrowserPathArgs {
    /// An absolute path to the browser program; `None` searches again.
    pub path: Option<String>,
}

/// Behind `settings.agentBrowser.path.set`: the next start uses it.
#[tauri::command]
pub async fn agent_browser_path_set<R: Runtime>(
    app: AppHandle<R>,
    args: AgentBrowserPathArgs,
) -> Result<()> {
    let path = args.path.filter(|path| !path.trim().is_empty());
    if let Some(path) = &path {
        if !Path::new(path).is_absolute() || !Path::new(path).is_file() {
            return Err(HolziError::InvalidInput {
                reason: "not a program on this device".to_owned(),
            });
        }
    }
    env::set_chosen_path(&app, path)
        .await
        .map_err(|reason| HolziError::InvalidInput { reason })
}

/// Behind `settings.agentBrowser.profile.clear`: ends the browser and removes its profile, so the
/// next start begins empty (every website login of the agent browser is gone).
#[tauri::command]
pub async fn agent_browser_profile_clear<R: Runtime>(
    app: AppHandle<R>,
    browser: State<'_, AgentBrowser>,
) -> Result<()> {
    let _ = browser.stop().await;
    let dir = env::profile_dir(&app).map_err(|error| HolziError::InvalidInput {
        reason: error.to_string(),
    })?;
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(HolziError::Io {
            reason: error.to_string(),
        }),
    }
}
