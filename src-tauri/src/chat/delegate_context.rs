//! The context a delegate (a CLI agent) is given when its adapter is built: the chat's approval
//! map, an emitter that ties a delegate's approval request to the turn that runs in its conversation,
//! and the haex-vault tools of this vault. Shared by a model load and by the adapter a turn builds
//! for its own conversation's model (spec 068 T012).

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

use super::events::EVENT_TOOL_PERMISSION_REQUEST;
use super::session::ChatState;
use crate::adapters::cli_delegate::{DelegateChatContext, EventEmitter};
use crate::error::Result;
use crate::state::AppState;
use crate::state_utils::active_database;

/// Builds the context of a delegate adapter for the open vault.
pub(crate) fn delegate_chat_context(
    app: &AppHandle,
    state: &State<'_, AppState>,
    chat: &ChatState,
) -> Result<DelegateChatContext> {
    let app_for_emit = app.clone();
    let turns_for_emit = Arc::clone(&chat.turns);
    let emit: EventEmitter = Arc::new(move |event, payload| {
        // A delegate asks for its approvals from inside its stream, where only the conversation
        // is known: the turn running there owns the request, so stopping another conversation's
        // turn leaves it open (spec 068 R1).
        if event == EVENT_TOOL_PERMISSION_REQUEST {
            if let (Some(request), Some(thread)) = (
                payload
                    .get("requestId")
                    .and_then(|v| v.as_str())
                    .and_then(|v| v.parse::<Uuid>().ok()),
                payload
                    .get("threadId")
                    .and_then(|v| v.as_str())
                    .and_then(|v| v.parse::<Uuid>().ok()),
            ) {
                turns_for_emit.own_prompt_of_thread(thread, request);
            }
        }
        let _ = app_for_emit.emit(event, payload);
    });
    Ok(DelegateChatContext {
        pending_tool_approvals: Arc::clone(&chat.pending_tool_approvals),
        emit,
        database: Some(active_database(state)?),
        children: chat.children().clone(),
        // Spec 021 US6: a registered delegate gets haex-vault's own MCP server.
        haex: app
            .try_state::<crate::mcp_server::McpServer>()
            .map(|server| server.delegate_access()),
        deny_rules: Vec::new(),
        // Spec 094: the tools of this vault, looked up when a run starts.
        tools: crate::cli_tools::tools_of_open_vault(app, state).ok(),
    })
}
