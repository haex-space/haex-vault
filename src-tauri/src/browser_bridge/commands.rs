//! Commands of the window (spec 049, `contracts/tauri-commands.md`): the state and settings of the
//! bridge, the approved and blocked clients, and the answers to its questions. Only the window calls them; the settings go through
//! guardrail actions no agent can call (FR-013).

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime, State};
use tauri_plugin_notification::NotificationExt;
use ts_rs::TS;
use uuid::Uuid;

use super::admin::{BridgeClientUpdate, BridgeClientView, ClientAdmin, ClientLists};
use super::prompts::BridgeAnswer;
use super::runtime::Notice;
use super::settings::{self, BridgeSettings};
use super::{BridgeRunState, BrowserBridge};
use crate::error::{HolziError, Result};
use crate::state::AppState;
use crate::storage::wm_session_commands::current_device_uuid;

/// What the settings show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct BridgeOverview {
    pub enabled: bool,
    pub port: u16,
    pub state: BridgeRunState,
    /// Approved clients, remembered and for this session (FR-037).
    pub clients: Vec<BridgeClientView>,
    pub blocked: Vec<BridgeClientView>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct BridgeConfigureArgs {
    pub enabled: bool,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct BridgeClientArgs {
    pub client_id: String,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct BridgeAnswerArgs {
    pub prompt_id: String,
    pub answer: BridgeAnswer,
}

fn available() -> Result<()> {
    if crate::platform::capabilities().browser_bridge {
        Ok(())
    } else {
        Err(HolziError::BrowserBridgeUnavailable)
    }
}

/// This device and its bridge settings.
pub(super) async fn device_settings<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
) -> Result<(Uuid, BridgeSettings)> {
    let db = state.database()?;
    let lookup = {
        let app = app.clone();
        let db = db.clone();
        tauri::async_runtime::spawn_blocking(move || current_device_uuid(&app, &db))
    };
    let device = lookup.await.map_err(|error| HolziError::Io {
        reason: format!("device lookup: {error}"),
    })??;
    let settings = db.read(move |q| settings::read(q, device)).await?;
    Ok((device, settings))
}

async fn overview<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    bridge: &BrowserBridge,
) -> Result<BridgeOverview> {
    let (settings, lists, vault_open) = match device_settings(app, state).await {
        Ok((device, settings)) => {
            let lists = admin_for(state, bridge, device).await?.overview().await?;
            (settings, lists, true)
        }
        Err(HolziError::NoActiveInstance | HolziError::VaultClosed) => {
            (BridgeSettings::default(), ClientLists::default(), false)
        }
        Err(error) => return Err(error),
    };
    Ok(BridgeOverview {
        enabled: settings.enabled,
        port: settings.port,
        state: bridge.run_state(vault_open).await,
        clients: lists.clients,
        blocked: lists.blocked,
    })
}

async fn admin_for(state: &AppState, bridge: &BrowserBridge, device: Uuid) -> Result<ClientAdmin> {
    Ok(ClientAdmin {
        db: state.database()?,
        device,
        core: bridge.core().await,
    })
}

async fn admin<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    bridge: &BrowserBridge,
) -> Result<ClientAdmin> {
    available()?;
    let (device, _) = device_settings(app, state).await?;
    admin_for(state, bridge, device).await
}

/// Tells the settings and answers with the new overview.
async fn changed<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    bridge: &BrowserBridge,
) -> Result<BridgeOverview> {
    bridge.notifier()(Notice::Changed);
    overview(app, state, bridge).await
}

#[tauri::command]
pub async fn browser_bridge_overview(
    app: AppHandle,
    state: State<'_, AppState>,
    bridge: State<'_, BrowserBridge>,
) -> Result<BridgeOverview> {
    overview(&app, &state, &bridge).await
}

/// Saves on/off and the port for this device and starts, stops or restarts the bridge (FR-001,
/// FR-002).
#[tauri::command]
pub async fn browser_bridge_configure(
    app: AppHandle,
    state: State<'_, AppState>,
    bridge: State<'_, BrowserBridge>,
    args: BridgeConfigureArgs,
) -> Result<BridgeOverview> {
    available()?;
    let port = settings::validate_port(args.port).map_err(|error| HolziError::InvalidInput {
        reason: error.to_string(),
    })?;
    let (device, _) = device_settings(&app, &state).await?;
    let wanted = BridgeSettings {
        enabled: args.enabled,
        port,
    };
    state
        .database()?
        .write(move |tx| settings::write(tx, device, wanted))
        .await?;
    bridge.apply(&state, device, wanted).await;
    overview(&app, &state, &bridge).await
}

/// Changes the approval of a client; it holds from the next request on (FR-038).
#[tauri::command]
pub async fn browser_bridge_client_update(
    app: AppHandle,
    state: State<'_, AppState>,
    bridge: State<'_, BrowserBridge>,
    args: BridgeClientUpdate,
) -> Result<BridgeOverview> {
    admin(&app, &state, &bridge).await?.update(args).await?;
    changed(&app, &state, &bridge).await
}

/// Withdraws the approval of a client and closes its connections (FR-011).
#[tauri::command]
pub async fn browser_bridge_client_revoke(
    app: AppHandle,
    state: State<'_, AppState>,
    bridge: State<'_, BrowserBridge>,
    args: BridgeClientArgs,
) -> Result<BridgeOverview> {
    admin(&app, &state, &bridge)
        .await?
        .revoke(&args.client_id)
        .await?;
    changed(&app, &state, &bridge).await
}

/// Lifts the block of a client; the next connect asks again.
#[tauri::command]
pub async fn browser_bridge_client_unblock(
    app: AppHandle,
    state: State<'_, AppState>,
    bridge: State<'_, BrowserBridge>,
    args: BridgeClientArgs,
) -> Result<BridgeOverview> {
    admin(&app, &state, &bridge)
        .await?
        .unblock(&args.client_id)
        .await?;
    changed(&app, &state, &bridge).await
}

/// The window's answer to a question; an unknown or late id has no effect.
#[tauri::command]
pub fn browser_bridge_answer(bridge: State<'_, BrowserBridge>, args: BridgeAnswerArgs) {
    bridge.prompts().answer(&args.prompt_id, args.answer);
}

/// A system notification that leads to a question, when holzi could not come to the front for it.
/// The window sends the text in the user's language; Rust holds no text of its own.
#[tauri::command]
pub fn browser_bridge_notify(app: AppHandle, title: String, body: String) -> Result<()> {
    available()?;
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|error| HolziError::Io {
            reason: format!("notification: {error}"),
        })
}
