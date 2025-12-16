//! External request handling for native WebView extensions
//!
//! Provides the Tauri command for extensions to respond to external requests
//! from browser extensions, CLI tools, servers, etc.

use crate::AppState;
use serde_json::Value as JsonValue;
use tauri::State;

/// Respond to an external request
///
/// Called by haex-vault extensions (via SDK) to send responses
/// back to external clients (browser extensions, CLI, servers, etc.)
#[tauri::command]
pub async fn webview_extension_external_respond(
    request_id: String,
    success: bool,
    data: Option<JsonValue>,
    error: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let bridge = state.external_bridge.lock().await;
    let pending_responses = bridge.get_pending_responses();

    // Build the response JSON
    let response = serde_json::json!({
        "requestId": request_id,
        "success": success,
        "data": data,
        "error": error,
    });

    // Get and remove the sender for this request
    let sender = {
        let mut pending = pending_responses.write().await;
        pending.remove(&request_id)
    };

    match sender {
        Some(tx) => {
            // Send response through the oneshot channel
            tx.send(response)
                .map_err(|_| "Failed to send response: receiver dropped".to_string())
        }
        None => {
            // No pending request with this ID (may have timed out)
            Err(format!("No pending request found with ID: {}", request_id))
        }
    }
}
