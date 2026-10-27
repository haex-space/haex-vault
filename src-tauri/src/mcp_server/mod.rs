//! holzi as MCP server for external agents (spec 021, ADR-0005): Claude Code, Codex and other MCP
//! clients start the holzi executable with `--mcp-agent` as their MCP server, register in holzi's
//! window, and then call the actions their rights cover. Off by default, per device, and only while
//! a vault is open (FR-001, FR-002).

pub mod commands;
pub mod connection;
pub mod dispatch;
pub mod endpoint;
pub mod grants;
pub mod hello;
pub mod helper;
pub mod prompts;
pub mod registry;
pub mod server;
pub mod session;
pub mod settings;

#[cfg(test)]
mod endpoint_tests;
#[cfg(test)]
mod grants_tests;
#[cfg(test)]
mod hello_tests;
#[cfg(test)]
mod helper_tests;
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
    new_registration_prompts, RegistrationPromptEvent, RegistrationPrompts, EVENT_AGENTS_CHANGED,
    EVENT_PROMPT_CLOSED, EVENT_REGISTRATION_REQUEST, REGISTRATION_TIMEOUT,
};
use self::server::{Limits, Notice, Notifier, Running, StartArgs, StartError};
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
    notify: Mutex<Option<Notifier>>,
    configuration: tokio::sync::Mutex<()>,
    slot: tokio::sync::Mutex<Slot>,
}

impl Default for McpServer {
    fn default() -> Self {
        Self {
            prompts: new_registration_prompts(),
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
            let expires_at =
                now_millis() + i64::try_from(REGISTRATION_TIMEOUT.as_millis()).unwrap_or(i64::MAX);
            let event = RegistrationPromptEvent {
                request_id: id.to_owned(),
                expires_at,
                question: question.clone(),
            };
            handle.emit(EVENT_REGISTRATION_REQUEST, event).is_ok()
        }));
        let handle = app.clone();
        *self.notify.lock().unwrap_or_else(PoisonError::into_inner) =
            Some(Arc::new(move |notice| {
                let sent = match notice {
                    Notice::Changed => handle.emit(EVENT_AGENTS_CHANGED, ()),
                    Notice::PromptClosed(closed) => handle.emit(EVENT_PROMPT_CLOSED, closed),
                };
                if let Err(error) = sent {
                    log::warn!("mcp server: could not tell the window: {error}");
                }
            }));
    }

    pub fn prompts(&self) -> &RegistrationPrompts {
        &self.prompts
    }

    fn notifier(&self) -> Notifier {
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
                    let args = StartArgs {
                        db,
                        device,
                        socket_dir: endpoint::default_socket_dir(),
                        parent: state.gate().token(),
                        catalog: chat.action_catalog.clone(),
                        bridge: chat.action_bridge.clone(),
                        prompts: self.prompts.clone(),
                        notify: self.notifier(),
                        limits: Limits::default(),
                    };
                    match server::start(args).await {
                        Ok(running) => slot.running = Some(running),
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

    /// The state for the settings.
    pub async fn run_state(&self, vault_open: bool) -> McpRunState {
        if !crate::platform::capabilities().mcp_helper {
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

/// Starts the access for the vault that just opened, when this device turned it on (FR-001).
pub fn start_for_active_instance<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
    if !crate::platform::capabilities().mcp_helper {
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
