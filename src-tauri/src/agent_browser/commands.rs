//! The window's commands for the agent browser (spec 065, contracts/tauri-commands.md): answering
//! a browser question, the running sessions with stopping, taking over and giving back, the state
//! for "KI → Browser", and the settings behind the guardrail actions `settings.agentBrowser.*`,
//! which no agent can call.

use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime, State};
use ts_rs::TS;

use super::cookies::{self, CookieRef, CookieTarget, CookieView};
use super::env;
use super::filter_lists::TrackerLists;
use super::filter_settings::{TrackerOverview, TrackerSettings};
use super::locate::{self, Os, System};
use super::prompts::{BrowserDecision, BrowserPrompts, Remember};
use super::sessions::{BrowserControl, BrowserSessionView, BrowserSessions};
use super::AgentBrowser;
use crate::caller::Caller;
use crate::error::{HolziError, Result};
use crate::mcp_server::grants::{BrowserRights, PasswordRights};

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentBrowserAnswerArgs {
    pub request_id: String,
    pub decision: BrowserDecision,
    #[serde(default)]
    #[ts(optional)]
    pub remember: Option<Remember>,
    #[serde(default)]
    #[ts(optional)]
    pub entry: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub skip: Option<Vec<String>>,
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
            entry: args.entry,
            skip: args.skip,
        },
    ))
}

/// The running browser sessions, for the sessions bar (FR-015).
#[tauri::command]
pub async fn agent_browser_sessions(
    sessions: State<'_, BrowserSessions>,
) -> Result<Vec<BrowserSessionView>> {
    Ok(sessions.list())
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentBrowserControlArgs {
    pub session_id: String,
    pub command: BrowserControl,
}

/// "Stoppen", "Eingreifen" and "Zurückgeben" in the sessions bar (FR-016, FR-017); `false` when
/// the session no longer runs or is in that state already.
#[tauri::command]
pub async fn agent_browser_control(
    sessions: State<'_, BrowserSessions>,
    args: AgentBrowserControlArgs,
) -> Result<bool> {
    Ok(sessions.control(&args.session_id, args.command).await)
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
    /// The entries the built-in agent's fills may use; `None` is off.
    pub builtin_passwords: Option<PasswordRights>,
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
            builtin_passwords: None,
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
        builtin_passwords: env::builtin_password_rights(&app).await,
    })
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentBrowserBuiltinArgs {
    /// `None` switches the browser off for the built-in agent.
    pub rights: Option<BrowserRights>,
}

/// Behind `settings.agentBrowser.builtin.set`; switching it off ends the built-in agent's session.
#[tauri::command]
pub async fn agent_browser_builtin_set<R: Runtime>(
    app: AppHandle<R>,
    sessions: State<'_, BrowserSessions>,
    args: AgentBrowserBuiltinArgs,
) -> Result<()> {
    let off = args.rights.is_none();
    env::set_builtin_rights(&app, args.rights)
        .await
        .map_err(|reason| HolziError::InvalidInput { reason })?;
    if off {
        sessions.end(&Caller::BuiltinAgent).await;
    }
    Ok(())
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentBrowserBuiltinPasswordsArgs {
    /// `None` switches fills from the password manager off for the built-in agent.
    pub rights: Option<PasswordRights>,
}

/// Behind `settings.agentBrowser.builtinPasswords.set` (decided 2026-11-02).
#[tauri::command]
pub async fn agent_browser_builtin_passwords_set<R: Runtime>(
    app: AppHandle<R>,
    args: AgentBrowserBuiltinPasswordsArgs,
) -> Result<()> {
    env::set_builtin_password_rights(&app, args.rights)
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

/// The tracker blocking for "KI → Browser": the lists, their files and errors (FR-045).
#[tauri::command]
pub async fn agent_browser_trackers(lists: State<'_, TrackerLists>) -> Result<TrackerOverview> {
    let lists = lists.get()?;
    Ok(lists.overview())
}

/// The user's choice for the tracker blocking: the lists that are on, their own lists by `https`
/// address and blocking as a whole.
#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentBrowserTrackersArgs {
    pub settings: TrackerSettings,
}

/// Behind `settings.agentBrowser.trackers.set`: checked, kept on this device and applied at once,
/// to the open tabs as well. Answers the state as it is now.
#[tauri::command]
pub async fn agent_browser_trackers_set(
    lists: State<'_, TrackerLists>,
    args: AgentBrowserTrackersArgs,
) -> Result<TrackerOverview> {
    let lists = lists.get()?;
    lists
        .apply(args.settings)
        .await
        .map_err(|reason| HolziError::InvalidInput { reason })?;
    Ok(lists.overview())
}

/// The running browser's connection for the profile view, started first when none runs (R17: the
/// profile is unlocked then). The lease keeps it from ending for being idle while it is used.
async fn profile_connection<R: Runtime>(
    app: &AppHandle<R>,
    browser: &AgentBrowser,
) -> Result<(std::sync::Arc<super::cdp::Connection>, super::Lease)> {
    let lease = browser.lease();
    let config = env::start_config(app).await;
    let connection =
        browser
            .ensure(move || config)
            .await
            .map_err(|error| HolziError::InvalidInput {
                reason: error.to_string(),
            })?;
    Ok((connection, lease))
}

fn cookie_error(error: super::error::BrowserError) -> HolziError {
    HolziError::InvalidInput {
        reason: error.to_string(),
    }
}

/// The cookies of the agent browser's profile without their values (FR-046); starts the browser
/// when it is not running.
#[tauri::command]
pub async fn agent_browser_cookies<R: Runtime>(
    app: AppHandle<R>,
    browser: State<'_, AgentBrowser>,
) -> Result<Vec<CookieView>> {
    let (connection, _lease) = profile_connection(&app, &browser).await?;
    let cookies = cookies::list(&connection).await.map_err(cookie_error)?;
    Ok(cookies.iter().map(cookies::Cookie::view).collect())
}

/// The value of one cookie, for the window only (FR-046); never logged.
#[tauri::command]
pub async fn agent_browser_cookie_value<R: Runtime>(
    app: AppHandle<R>,
    browser: State<'_, AgentBrowser>,
    args: CookieRef,
) -> Result<String> {
    let (connection, _lease) = profile_connection(&app, &browser).await?;
    cookies::value_of(&connection, args)
        .await
        .map_err(cookie_error)
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentBrowserCookieSetArgs {
    pub cookie: CookieRef,
    pub value: String,
    /// Seconds since 1970; `None` makes it a cookie of the session.
    #[serde(default)]
    #[ts(optional = nullable, type = "number | null")]
    pub expires: Option<i64>,
}

/// Behind `settings.agentBrowser.profile.cookie.set`: a new value and expiry for one cookie.
#[tauri::command]
pub async fn agent_browser_cookie_set<R: Runtime>(
    app: AppHandle<R>,
    browser: State<'_, AgentBrowser>,
    args: AgentBrowserCookieSetArgs,
) -> Result<()> {
    let (connection, _lease) = profile_connection(&app, &browser).await?;
    cookies::change(&connection, args.cookie, args.value, args.expires)
        .await
        .map_err(cookie_error)
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct AgentBrowserCookieDeleteArgs {
    pub target: CookieTarget,
}

/// Behind `settings.agentBrowser.profile.cookie.delete`: one cookie or every cookie of a website.
/// Answers how many cookies went.
#[tauri::command]
pub async fn agent_browser_cookie_delete<R: Runtime>(
    app: AppHandle<R>,
    browser: State<'_, AgentBrowser>,
    args: AgentBrowserCookieDeleteArgs,
) -> Result<usize> {
    let (connection, _lease) = profile_connection(&app, &browser).await?;
    cookies::delete(&connection, args.target)
        .await
        .map_err(cookie_error)
}
