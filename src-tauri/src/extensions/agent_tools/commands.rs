//! The tool connection's way through holzi's window (contracts/tauri-commands.md, research R1):
//! Rust's messages leave as the event `extension-mcp-message`, the extension's come back with
//! `extension_mcp_send`. The frame session, never the message, names the extension, and neither
//! reaches a method of the bridge.

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::State;
use uuid::Uuid;

use crate::error::{HolziError, Result};
use crate::extensions::bridge::dispatch::Emit;
use crate::extensions::bridge::frames::FrameRegistry;
use crate::extensions::ids::permission_id;
use crate::extensions::permissions::store::{self, NewPermission};
use crate::extensions::permissions::VAULT_WIDE;
use crate::passwords::clock::unix_millis;
use crate::state::AppState;
use crate::state_utils::active_database;
use crate::vault_gate::VaultDb;

/// Event: a message of holzi's MCP client for one frame.
pub const MCP_MESSAGE: &str = "extension-mcp-message";

/// Hands a message of the extension to its frame's connection; `false` for an unknown frame,
/// whose message is dropped.
pub fn deliver(frames: &FrameRegistry, frame: &str, message: Value) -> bool {
    let Some(session) = frames.get(frame) else {
        log::warn!("agent tools: dropped a message for an unknown frame");
        return false;
    };
    session.mcp.deliver(message);
    true
}

/// Where the connection of `frame` sends holzi's messages.
pub fn to_frame<E: Emit + 'static>(emit: E, frame: String) -> impl Fn(Value) + Send + 'static {
    move |message| emit.emit(MCP_MESSAGE, json!({ "frame": frame, "message": message }))
}

/// A message of an extension's MCP server, relayed from its frame's port.
#[tauri::command]
pub async fn extension_mcp_send(
    state: State<'_, AppState>,
    frame: String,
    message: Value,
) -> Result<()> {
    deliver(&state.extensions().frames, &frame, message);
    Ok(())
}

/// holzi's window answers `extension-agent-frame`: the frame to call the extension's tools in, or
/// `None` when it could not provide one.
#[tauri::command]
pub async fn extension_agent_frame_ready(
    state: State<'_, AppState>,
    request_id: String,
    frame: Option<String>,
) -> Result<()> {
    state.extensions().agent_frames.answer(&request_id, frame);
    Ok(())
}

/// The switch "Für den Agenten verfügbar" (spec 018, research R4): off writes the user's row
/// `agentTool / * / denied`, which offers none of the extension's tools; on removes it, and the
/// tools are offered as they were confirmed. The registry follows the write by itself
/// (`registration.rs`), so it applies without a restart (SC-006).
pub fn set_available(db: &VaultDb, extension_id: Uuid, available: bool, now_ms: i64) -> Result<()> {
    const KIND: &str = "agentTool";
    db.write_blocking(move |tx| {
        if available {
            store::delete(tx, permission_id(extension_id, KIND, "*", "*", VAULT_WIDE))?;
        } else {
            store::put(
                tx,
                extension_id,
                &NewPermission {
                    kind: KIND,
                    action: "*",
                    target: "*",
                    status: "denied",
                    declared: false,
                    vault_device_uuid: VAULT_WIDE,
                },
                now_ms,
            )?;
        }
        Ok(())
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolsSetArgs {
    pub extension_id: String,
    pub available: bool,
}

/// The switch in the extension's settings.
#[tauri::command]
pub async fn extension_agent_tools_set(
    state: State<'_, AppState>,
    args: AgentToolsSetArgs,
) -> Result<()> {
    let extension_id =
        Uuid::parse_str(&args.extension_id).map_err(|_| HolziError::ExtensionNotFound)?;
    let db = active_database(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        set_available(
            &db,
            extension_id,
            args.available,
            unix_millis(std::time::SystemTime::now()),
        )
    })
    .await
    .map_err(|e| HolziError::InvalidInput {
        reason: format!("agent tools task: {e}"),
    })?
}
