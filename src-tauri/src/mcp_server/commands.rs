//! Commands of the window (spec 021, contracts/tauri-commands.md): the state of the MCP access, the
//! switch behind the guardrail action `settings.agents.mcp.enable`, the answers to a registration
//! dialog and a call prompt, the agent log and the setup snippets. Only the window calls them; no
//! agent reaches a command.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime, State};
use ts_rs::TS;
use uuid::Uuid;

use super::agent_log::{self, AgentLogEntry};
use super::connection::now_millis;
use super::prompts::{CallDecision, RegistrationAnswer};
use super::settings;
use super::{McpRunState, McpServer};
use crate::chat::session::ChatState;
use crate::error::{HolziError, Result};
use crate::state::AppState;
use crate::storage::wm_session_commands::current_device_uuid;

/// What the settings show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct McpStatus {
    pub enabled: bool,
    pub state: McpRunState,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct McpConfigureArgs {
    pub enabled: bool,
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct McpRegistrationAnswerArgs {
    pub request_id: String,
    pub answer: RegistrationAnswer,
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct McpCallAnswerArgs {
    pub request_id: String,
    pub decision: CallDecision,
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct McpAgentLogArgs {
    pub agent_id: Uuid,
    /// Entries before this Unix millisecond; the newest when absent.
    #[ts(type = "number | null")]
    pub before: Option<i64>,
    /// At most 200.
    pub limit: u32,
}

/// The agents the setup snippets are written for (FR-005).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum SnippetClient {
    ClaudeCode,
    Codex,
    Other,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct McpSnippetArgs {
    pub client: SnippetClient,
}

/// This device in the open vault.
pub(crate) async fn open_device<R: Runtime>(app: &AppHandle<R>, state: &AppState) -> Result<Uuid> {
    let db = state.database()?;
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || current_device_uuid(&app, &db))
        .await
        .map_err(|error| HolziError::Io {
            reason: format!("device lookup: {error}"),
        })?
}

async fn status<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    server: &McpServer,
) -> Result<McpStatus> {
    let (enabled, vault_open) = match open_device(app, state).await {
        Ok(device) => {
            let db = state.database()?;
            (
                db.read(move |q| settings::read_enabled(q, device)).await?,
                true,
            )
        }
        Err(HolziError::NoActiveInstance | HolziError::VaultClosed) => (false, false),
        Err(error) => return Err(error),
    };
    Ok(McpStatus {
        enabled,
        state: server.run_state(vault_open).await,
    })
}

#[tauri::command]
pub async fn mcp_status<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    server: State<'_, McpServer>,
) -> Result<McpStatus> {
    status(&app, &state, &server).await
}

/// Turns the access on or off for this device (FR-001) and starts or stops it.
#[tauri::command]
pub async fn mcp_configure<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    chat: State<'_, ChatState>,
    server: State<'_, McpServer>,
    args: McpConfigureArgs,
) -> Result<McpStatus> {
    if !crate::platform::capabilities().mcp_helper {
        return Err(HolziError::InvalidInput {
            reason: "the MCP access is not available on this device".to_owned(),
        });
    }
    let device = open_device(&app, &state).await?;
    let enabled = args.enabled;
    let _configuration = server.configuration.lock().await;
    state
        .database()?
        .write(move |tx| settings::write_enabled(tx, device, enabled))
        .await?;
    server.apply_locked(&state, &chat, device, enabled).await;
    status(&app, &state, &server).await
}

/// The window's answer to a registration dialog. An unknown id is no error: the dialog may have
/// timed out or the vault closed meanwhile.
#[tauri::command]
pub async fn mcp_registration_answer(
    server: State<'_, McpServer>,
    args: McpRegistrationAnswerArgs,
) -> Result<bool> {
    Ok(server.prompts().answer(&args.request_id, args.answer))
}

/// The window's answer to a call prompt (FR-022). An unknown id is no error, as above.
#[tauri::command]
pub async fn mcp_call_answer(
    server: State<'_, McpServer>,
    args: McpCallAnswerArgs,
) -> Result<bool> {
    Ok(server.calls().answer(&args.request_id, args.decision))
}

/// The log of one agent on this device, newest first (FR-028).
#[tauri::command]
pub async fn mcp_agent_log<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    args: McpAgentLogArgs,
) -> Result<Vec<AgentLogEntry>> {
    let device = open_device(&app, &state).await?;
    let now = now_millis();
    state
        .database()?
        .write(move |tx| agent_log::read(tx, device, args.agent_id, args.before, args.limit, now))
        .await
}

/// The text to paste into the agent (FR-005). Never carries a credential; the `--vault` value only
/// picks the instance when several run (contracts/agent-helper.md).
#[tauri::command]
pub async fn mcp_setup_snippet<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    args: McpSnippetArgs,
) -> Result<String> {
    let device = open_device(&app, &state).await?;
    let executable = helper_executable()?;
    Ok(snippet(args.client, &executable, device))
}

/// The executable an agent starts: the AppImage itself when holzi runs as one, else this binary.
fn helper_executable() -> Result<String> {
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        return Ok(appimage.to_string_lossy().into_owned());
    }
    std::env::current_exe()
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|error| HolziError::Io {
            reason: format!("the path of holzi: {error}"),
        })
}

/// The snippet for `client`.
pub fn snippet(client: SnippetClient, executable: &str, device: Uuid) -> String {
    let flag = super::helper::FLAG;
    match client {
        SnippetClient::ClaudeCode => {
            format!("claude mcp add holzi -- \"{executable}\" {flag} claude-code --vault {device}")
        }
        SnippetClient::Codex => {
            format!("codex mcp add holzi -- \"{executable}\" {flag} codex --vault {device}")
        }
        SnippetClient::Other => {
            let config = serde_json::json!({
                "mcpServers": {
                    "holzi": {
                        "command": executable,
                        "args": [flag, "my-agent", "--vault", device.to_string()],
                    }
                }
            });
            serde_json::to_string_pretty(&config).unwrap_or_default()
        }
    }
}
