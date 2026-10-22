//! The External Bridge for haex-pass-browser (spec 049): a loopback WebSocket service through which
//! a browser extension fills, reads TOTP codes, saves entries and uses passkeys through the password
//! service, as its own caller with the grant the user gave it. Off by default, desktop only, and
//! only while a vault is open (FR-001, FR-003, FR-043).

pub mod admin;
pub mod approvals;
pub mod commands;
pub mod connection;
pub mod declaration;
pub mod envelope;
pub mod methods;
pub mod origin;
pub mod prompts;
pub mod protocol;
pub mod runtime;
pub mod server;
pub mod settings;
pub mod write_access;

#[cfg(test)]
mod approvals_tests;
#[cfg(test)]
mod declaration_tests;
#[cfg(test)]
mod envelope_tests;
#[cfg(test)]
mod origin_tests;
#[cfg(test)]
mod protocol_tests;
#[cfg(test)]
mod settings_tests;

use std::sync::{Arc, Mutex, PoisonError};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use ts_rs::TS;

use self::prompts::{
    new_prompts, BridgePromptCancelled, BridgePromptEvent, BridgePrompts, EVENT_PROMPT,
    EVENT_PROMPT_CANCELLED, PROMPT_TIMEOUT,
};
use self::runtime::{now_millis, BridgeCore, Limits, Notice, Notifier};
use self::server::{Running, StartArgs, StartError};
use self::settings::BridgeSettings;
use crate::state::AppState;

/// Event the settings reload on: a client was decided, connected or left, or the state changed.
pub const EVENT_CHANGED: &str = "browser-bridge-changed";

/// The state of the bridge as the settings show it (FR-037).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum BridgeRunState {
    Off,
    Running,
    PortInUse,
    NoVault,
    NotAvailable,
}

#[derive(Default)]
struct Slot {
    running: Option<Running>,
    port_in_use: bool,
}

/// The bridge of this process; a Tauri managed state.
pub struct BrowserBridge {
    prompts: BridgePrompts,
    notify: Mutex<Option<Notifier>>,
    slot: tokio::sync::Mutex<Slot>,
}

impl Default for BrowserBridge {
    fn default() -> Self {
        Self {
            prompts: new_prompts(),
            notify: Mutex::new(None),
            slot: tokio::sync::Mutex::new(Slot::default()),
        }
    }
}

impl BrowserBridge {
    /// Connects the questions and notices to the window; called from `setup()`.
    pub fn attach<R: Runtime>(&self, app: &AppHandle<R>) {
        let handle = app.clone();
        self.prompts.set_emitter(Arc::new(move |id, question| {
            let expires_at =
                now_millis() + i64::try_from(PROMPT_TIMEOUT.as_millis()).unwrap_or(i64::MAX);
            let event = BridgePromptEvent {
                prompt_id: id.to_owned(),
                expires_at,
                question: question.clone(),
            };
            handle.emit(EVENT_PROMPT, event).is_ok()
        }));
        let handle = app.clone();
        *self.notify.lock().unwrap_or_else(PoisonError::into_inner) =
            Some(Arc::new(move |notice| {
                let sent = match notice {
                    Notice::PromptCancelled(prompt_id) => {
                        handle.emit(EVENT_PROMPT_CANCELLED, BridgePromptCancelled { prompt_id })
                    }
                    Notice::Changed => handle.emit(EVENT_CHANGED, ()),
                };
                if let Err(error) = sent {
                    log::warn!("browser bridge: could not tell the window: {error}");
                }
            }));
    }

    pub fn prompts(&self) -> &BridgePrompts {
        &self.prompts
    }

    fn notifier(&self) -> Notifier {
        self.notify
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .unwrap_or_else(|| Arc::new(|_| {}))
    }

    /// Stops a running bridge and starts it again when `settings` say so.
    pub async fn apply(&self, state: &AppState, device: uuid::Uuid, settings: BridgeSettings) {
        let mut slot = self.slot.lock().await;
        if let Some(running) = slot.running.take() {
            running.stop().await;
        }
        slot.port_in_use = false;
        if settings.enabled {
            match state.database() {
                Ok(db) => {
                    let args = StartArgs {
                        db,
                        device,
                        port: settings.port,
                        parent: state.gate().token(),
                        prompts: self.prompts.clone(),
                        notify: self.notifier(),
                        limits: Limits::default(),
                    };
                    match server::start(args).await {
                        Ok(running) => slot.running = Some(running),
                        Err(StartError::PortInUse) => slot.port_in_use = true,
                        Err(error) => log::warn!("browser bridge: {error}"),
                    }
                }
                Err(error) => log::warn!("browser bridge: no open vault: {error}"),
            }
        }
        drop(slot);
        self.notifier()(Notice::Changed);
    }

    /// The running bridge, for the settings to reach its decisions and connections.
    pub async fn core(&self) -> Option<Arc<BridgeCore>> {
        let slot = self.slot.lock().await;
        slot.running
            .as_ref()
            .map(|running| Arc::clone(running.core()))
            .filter(|core| !core.token.is_cancelled())
    }

    /// The state for the settings.
    pub async fn run_state(&self, vault_open: bool) -> BridgeRunState {
        if !crate::platform::capabilities().browser_bridge {
            return BridgeRunState::NotAvailable;
        }
        if !vault_open {
            return BridgeRunState::NoVault;
        }
        let slot = self.slot.lock().await;
        match (&slot.running, slot.port_in_use) {
            (Some(running), _) if !running.core().token.is_cancelled() => BridgeRunState::Running,
            (_, true) => BridgeRunState::PortInUse,
            _ => BridgeRunState::Off,
        }
    }
}

/// Starts the bridge for the vault that just opened, when this device turned it on (FR-001).
pub fn start_for_active_instance<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
    if !crate::platform::capabilities().browser_bridge {
        return;
    }
    let app = app.clone();
    let spawned = state.gate().spawn(async move {
        let state = app.state::<AppState>();
        let bridge = app.state::<BrowserBridge>();
        match commands::device_settings(&app, &state).await {
            Ok((device, settings)) if settings.enabled => {
                bridge.apply(&state, device, settings).await;
            }
            Ok(_) => {}
            Err(error) => log::warn!("browser bridge: could not read the settings: {error}"),
        }
    });
    if let Err(error) = spawned {
        log::warn!("browser bridge: not started: {error}");
    }
}
