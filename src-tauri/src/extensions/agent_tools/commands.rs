//! The tool connection's way through holzi's window (contracts/tauri-commands.md, research R1):
//! Rust's messages leave as the event `extension-mcp-message`, the extension's come back with
//! `extension_mcp_send`. The frame session, never the message, names the extension, and neither
//! reaches a method of the bridge.

use serde_json::{json, Value};
use tauri::State;

use crate::error::Result;
use crate::extensions::bridge::dispatch::Emit;
use crate::extensions::bridge::frames::FrameRegistry;
use crate::state::AppState;

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
