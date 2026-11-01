//! holzi as MCP server for external agents (spec 021, ADR-0005): Claude Code, Codex and other MCP
//! clients start the holzi executable with `--mcp-agent` as their MCP server, register in holzi's
//! window, and then call the actions their rights cover. Off by default, per device, and only while
//! a vault is open (FR-001, FR-002).

pub mod agent_log;
mod attach;
mod callee;
pub mod command_tool;
pub mod commands;
pub mod connection;
pub mod delegates;
pub mod dispatch;
pub mod endpoint;
pub mod extension_tools;
pub mod grant_rows;
pub mod grants;
pub mod hello;
pub mod helper;
pub mod http;
pub mod http_session;
pub mod live;
pub mod manage;
pub mod manage_commands;
pub mod oauth;
mod oauth_flow;
mod oauth_http;
pub mod oauth_rows;
pub mod password_env;
pub mod prompts;
pub mod redact;
mod registration;
pub mod registry;
pub mod results;
pub mod server;
pub mod session;
pub mod settings;

#[cfg(test)]
mod agent_log_tests;
#[cfg(test)]
mod command_tool_tests;
#[cfg(test)]
mod commands_tests;
#[cfg(test)]
mod dispatch_tests;
#[cfg(test)]
mod endpoint_tests;
#[cfg(test)]
mod grant_rows_tests;
#[cfg(test)]
mod grants_tests;
#[cfg(test)]
mod hello_tests;
#[cfg(test)]
mod helper_tests;
#[cfg(test)]
mod http_tests;
#[cfg(test)]
mod manage_tests;
#[cfg(test)]
mod oauth_tests;
#[cfg(test)]
mod prompts_tests;
#[cfg(test)]
mod registry_tests;

use std::sync::{Arc, Mutex, PoisonError};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use ts_rs::TS;
use uuid::Uuid;

use self::connection::now_millis;
use self::hello::CloseReason;
use self::prompts::{
    new_call_prompts, new_registration_prompts, CallPromptEvent, CallPrompts,
    RegistrationPromptEvent, RegistrationPrompts, CALL_TIMEOUT, EVENT_AGENTS_CHANGED,
    EVENT_CALL_REQUEST, EVENT_PROMPT_CLOSED, EVENT_REGISTRATION_REQUEST, REGISTRATION_TIMEOUT,
};
use self::server::{Limits, Notice, Notifier, Running, StartArgs, StartError};
use crate::adapters::cli_delegate::holzi_mcp::HolziAccess;
use crate::chat::session::ChatState;
use crate::state::AppState;

/// The state of the access as the settings show it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum McpRunState {
    Off,
    Running,
    /// Another holzi serves the same vault on this device.
    InUse,
    NoVault,
    NotAvailable,
}

#[derive(Default)]
struct Slot {
    running: Option<Running>,
    in_use: bool,
}

/// The MCP access of this process; a Tauri managed state.
pub struct McpServer {
    prompts: RegistrationPrompts,
    calls: CallPrompts,
    delegates: Arc<delegates::DelegateHub>,
    notify: Mutex<Option<Notifier>>,
    configuration: tokio::sync::Mutex<()>,
    slot: tokio::sync::Mutex<Slot>,
}

impl Default for McpServer {
    fn default() -> Self {
        Self {
            prompts: new_registration_prompts(),
            calls: new_call_prompts(),
            delegates: Arc::default(),
            notify: Mutex::new(None),
            configuration: tokio::sync::Mutex::new(()),
            slot: tokio::sync::Mutex::new(Slot::default()),
        }
    }
}

impl McpServer {
    /// Connects the dialogs and notices to the window; called from `setup()`.
    pub fn attach<R: Runtime>(&self, app: &AppHandle<R>) {
        let handle = app.clone();
        self.prompts.set_emitter(Arc::new(move |id, question| {
            let event = RegistrationPromptEvent {
                request_id: id.to_owned(),
                expires_at: expires_at(REGISTRATION_TIMEOUT),
                question: question.clone(),
            };
            handle.emit(EVENT_REGISTRATION_REQUEST, event).is_ok()
        }));
        let handle = app.clone();
        self.calls.set_emitter(Arc::new(move |id, question| {
            let event = CallPromptEvent {
                request_id: id.to_owned(),
                expires_at: expires_at(CALL_TIMEOUT),
                question: question.clone(),
            };
            handle.emit(EVENT_CALL_REQUEST, event).is_ok()
        }));
        let handle = app.clone();
        *self.notify.lock().unwrap_or_else(PoisonError::into_inner) =
            Some(Arc::new(move |notice| {
                let sent = match notice {
                    Notice::Changed => handle.emit(EVENT_AGENTS_CHANGED, ()),
                    Notice::PromptClosed(closed) => handle.emit(EVENT_PROMPT_CLOSED, closed),
                    Notice::Disconnected(agent) => {
                        end_browser_session(&handle, agent);
                        Ok(())
                    }
                };
                if let Err(error) = sent {
                    log::warn!("mcp server: could not tell the window: {error}");
                }
            }));
    }

    pub fn prompts(&self) -> &RegistrationPrompts {
        &self.prompts
    }

    pub fn calls(&self) -> &CallPrompts {
        &self.calls
    }

    /// How a delegate in the chat reaches holzi (US6).
    pub fn delegate_access(&self) -> Arc<dyn HolziAccess> {
        Arc::clone(&self.delegates) as Arc<dyn HolziAccess>
    }

    pub(super) fn notifier(&self) -> Notifier {
        self.notify
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .unwrap_or_else(|| Arc::new(|_| {}))
    }

    /// Stops a running server and starts it again when `enabled`.
    pub async fn apply(&self, state: &AppState, chat: &ChatState, device: Uuid, enabled: bool) {
        let _configuration = self.configuration.lock().await;
        self.apply_locked(state, chat, device, enabled).await;
    }

    async fn apply_locked(&self, state: &AppState, chat: &ChatState, device: Uuid, enabled: bool) {
        let mut slot = self.slot.lock().await;
        if let Some(running) = slot.running.take() {
            running.stop(CloseReason::Disabled).await;
        }
        slot.in_use = false;
        if enabled {
            match state.database() {
                Ok(db) => {
                    let kept_port = db
                        .read(move |q| settings::read_http_port(q, device))
                        .await
                        .unwrap_or_else(|error| {
                            log::warn!("mcp server: could not read the HTTP port: {error}");
                            None
                        });
                    let capabilities = crate::platform::capabilities();
                    let args = StartArgs {
                        db: db.clone(),
                        device,
                        socket_dir: endpoint::default_socket_dir(),
                        parent: state.gate().token(),
                        catalog: chat.action_catalog.clone(),
                        extension_tools: chat.extension_tools.clone(),
                        vault_changes: state.vault_changes().clone(),
                        bridge: chat.action_bridge.clone(),
                        native: chat.native_actions.clone(),
                        command_tool: command_tool::for_device(chat.children()),
                        prompts: self.prompts.clone(),
                        calls: self.calls.clone(),
                        delegates: Arc::clone(&self.delegates),
                        notify: self.notifier(),
                        limits: Limits::default(),
                        http_port: capabilities.mcp_http.then(|| kept_port.unwrap_or(0)),
                        program_way: capabilities.mcp_helper,
                    };
                    match server::start(args).await {
                        Ok(running) => {
                            let port = running.http_address().map(|address| address.port());
                            if let Some(port) = port.filter(|port| Some(*port) != kept_port) {
                                if let Err(error) = db
                                    .write(move |tx| settings::write_http_port(tx, device, port))
                                    .await
                                {
                                    log::warn!("mcp server: could not keep the HTTP port: {error}");
                                }
                            }
                            slot.running = Some(running);
                        }
                        Err(StartError::InUse) => slot.in_use = true,
                        Err(error) => log::warn!("mcp server: {error}"),
                    }
                }
                Err(error) => log::warn!("mcp server: no open vault: {error}"),
            }
        }
        drop(slot);
        self.notifier()(Notice::Changed);
    }

    /// The running server's shared state, to reach its connections.
    pub async fn core(&self) -> Option<Arc<server::Core>> {
        let slot = self.slot.lock().await;
        slot.running
            .as_ref()
            .map(|running| Arc::clone(running.core()))
            .filter(|core| !core.token.is_cancelled())
    }

    /// The port the HTTP way listens on now, while the access runs.
    pub async fn http_port(&self) -> Option<u16> {
        let slot = self.slot.lock().await;
        slot.running
            .as_ref()
            .filter(|running| !running.core().token.is_cancelled())
            .and_then(Running::http_address)
            .map(|address| address.port())
    }

    /// The state for the settings.
    pub async fn run_state(&self, vault_open: bool) -> McpRunState {
        if !settings::available() {
            return McpRunState::NotAvailable;
        }
        if !vault_open {
            return McpRunState::NoVault;
        }
        let slot = self.slot.lock().await;
        match (&slot.running, slot.in_use) {
            (Some(running), _) if !running.core().token.is_cancelled() => McpRunState::Running,
            (_, true) => McpRunState::InUse,
            _ => McpRunState::Off,
        }
    }
}

/// When a prompt that waits `timeout` from now ends, in Unix milliseconds.
pub(crate) fn expires_at(timeout: std::time::Duration) -> i64 {
    now_millis().saturating_add(i64::try_from(timeout.as_millis()).unwrap_or(i64::MAX))
}

/// Ends the browser session of `agent` (spec 065 T058): it disconnected, was revoked or lost the
/// browser rights; its tabs go back to the user.
pub(crate) fn end_browser_session<R: Runtime>(app: &AppHandle<R>, agent: Uuid) {
    let Some(sessions) = app.try_state::<crate::agent_browser::sessions::BrowserSessions>() else {
        return;
    };
    let sessions = sessions.inner().clone();
    tauri::async_runtime::spawn(async move {
        sessions
            .end(&crate::caller::Caller::ExternalAgent {
                id: agent.to_string(),
            })
            .await;
    });
}

/// Starts the access for the vault that just opened, when this device turned it on (FR-001).
pub fn start_for_active_instance<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
    if !settings::available() {
        return;
    }
    let app = app.clone();
    let spawned = state.gate().spawn(async move {
        let state = app.state::<AppState>();
        let chat = app.state::<ChatState>();
        let server = app.state::<McpServer>();
        let device = match commands::open_device(&app, &state).await {
            Ok(device) => device,
            Err(error) => {
                log::warn!("mcp server: no device: {error}");
                return;
            }
        };
        let enabled = match state.database() {
            Ok(db) => db
                .read(move |q| settings::read_enabled(q, device))
                .await
                .unwrap_or(false),
            Err(_) => false,
        };
        if enabled {
            server.apply(&state, &chat, device, true).await;
        }
    });
    if let Err(error) = spawned {
        log::warn!("mcp server: not started: {error}");
    }
}
