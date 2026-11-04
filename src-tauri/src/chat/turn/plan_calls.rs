//! Plans the calls of one tool round: the tool lookup and the permission decision, made in order
//! before any call waits or runs (moved out of `tool_round.rs`, spec 081 T004).

use std::sync::Arc;

use uuid::Uuid;

use crate::adapters::types::{ToolCall as LlmToolCall, ToolSpec};
use crate::chat::events::{
    risk_class_str, ToolPermissionRequestEvent, EVENT_TOOL_PERMISSION_REQUEST,
};
use crate::chat::tools::action_tool::ACTION_SOURCE;
use crate::chat::tools::permission;
use crate::chat::tools::{ApprovalDecision, Tool};

use super::TurnRunner;

/// The answer to a call the user declined in the dialog; a fixed marker the frontend translates.
pub const DENIED_BY_USER: &str = "denied_by_user";

/// The answer to a call of a tool that is not [`callable`].
pub fn unknown_tool_answer(name: &str) -> String {
    format!("unknown tool: {name}")
}

/// Whether a registered `tool` may run for a call in a step that offered `offered`. The registry is
/// the execution safety net, not the model's offer: a provider must not be able to call a non-core
/// action it was never given in this request by merely guessing its name. CLI and MCP tools retain
/// their registry-based behavior.
pub fn callable(tool: &dyn Tool, offered: &[ToolSpec]) -> bool {
    tool.source() != ACTION_SOURCE || offered.iter().any(|spec| spec.name == tool.name())
}

/// What to do with one tool call, decided before any concurrent
/// waiting/execution begins (see [`TurnRunner::plan_calls`]).
pub(super) enum ToolPlan {
    Allow(Arc<dyn Tool>),
    Ask {
        tool: Arc<dyn Tool>,
        rx: tokio::sync::oneshot::Receiver<ApprovalDecision>,
        request_id: Uuid,
    },
    Deny(Arc<dyn Tool>),
    Unknown,
}

impl TurnRunner<'_> {
    /// Decides — and, for `Ask`, mints the approval wait — sequentially.
    /// `emit` is a single `&mut` closure, not shareable across concurrent
    /// futures, so every `tool-permission-request` fires here, before any
    /// concurrent waiting/execution begins in [`Self::execute_plans`].
    /// This is also what lets two independent Risky calls each get their
    /// own simultaneously-pending approval (T024A): each gets its own
    /// oneshot the moment its `Ask` is decided, well before either one's
    /// wait resolves.
    pub(super) async fn plan_calls(
        &mut self,
        tool_calls: Vec<LlmToolCall>,
    ) -> Vec<(LlmToolCall, ToolPlan)> {
        let mut plans: Vec<(LlmToolCall, ToolPlan)> = Vec::with_capacity(tool_calls.len());
        for call in tool_calls {
            let tool = {
                let registry = self
                    .chat_state
                    .tool_registry
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                registry.get(&call.name)
            };
            let Some(tool) = tool.filter(|tool| callable(tool.as_ref(), &self.request.tools))
            else {
                plans.push((call, ToolPlan::Unknown));
                continue;
            };
            // Read fresh per call, not cached for the round: a mode
            // change must not retroactively affect a decision already
            // made for an earlier call, but the very next tool use
            // must observe it (T024B).
            let mode = permission::read_from_vault(self.db).await;
            match permission::decide_call(mode, tool.as_ref()) {
                permission::Decision::Allow => plans.push((call, ToolPlan::Allow(tool))),
                permission::Decision::Deny => plans.push((call, ToolPlan::Deny(tool))),
                permission::Decision::Ask => {
                    let request_id = Uuid::new_v4();
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    self.chat_state
                        .pending_tool_approvals
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(request_id, tx);
                    self.emit_event(
                        EVENT_TOOL_PERMISSION_REQUEST,
                        ToolPermissionRequestEvent {
                            request_id,
                            thread_id: self.thread_id,
                            tool_name: call.name.clone(),
                            tool_input: call.input.clone(),
                            risk_class: risk_class_str(tool.risk_class()),
                            tool_source: tool.source(),
                            tool_origin: tool.origin(),
                        },
                    );
                    plans.push((
                        call,
                        ToolPlan::Ask {
                            tool,
                            rx,
                            request_id,
                        },
                    ));
                }
            }
        }
        plans
    }
}
