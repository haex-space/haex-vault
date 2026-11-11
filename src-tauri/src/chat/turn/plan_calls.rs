//! Plans the calls of one tool round: the tool lookup, the guard and the permission decision, made in
//! order before any call waits or runs (moved out of `tool_round.rs`, spec 082 T004).

use std::sync::Arc;

use uuid::Uuid;

use crate::adapters::types::{ToolCall as LlmToolCall, ToolSpec};
use crate::chat::events::{
    risk_class_str, ToolPermissionRequestEvent, EVENT_TOOL_PERMISSION_REQUEST,
};
use crate::chat::tools::action_tool::ACTION_SOURCE;
use crate::chat::tools::permission;
use crate::chat::tools::profile_gate::{self, Missing};
use crate::chat::tools::reversible;
use crate::chat::tools::route::RouteCall;
use crate::chat::tools::{ApprovalDecision, Tool};

use super::arg_forms::as_meant;
use super::guard::{closest_tools, next_step, seen_text, turn_away, Failure};
use super::TurnRunner;

/// The answer to a call the user declined in the dialog; a fixed marker the frontend translates.
pub const DENIED_BY_USER: &str = "denied_by_user";

/// The answer to a call the user declined: the marker first, then what to do next (spec 082
/// FR-014).
pub fn denied_answer() -> String {
    format!(
        "{DENIED_BY_USER}: the user declined this call. {}",
        next_step(Failure::Denied)
    )
}

/// The answer to a call of a tool that is not [`callable`], with the offered tools whose names come
/// closest (spec 082 FR-012).
pub fn unknown_tool_answer(name: &str, offered: &[ToolSpec]) -> String {
    let closest = closest_tools(name, offered);
    format!(
        "unknown tool: {name}. {}",
        next_step(Failure::UnknownTool { closest: &closest })
    )
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
    /// The guard refused the call before the dialog (spec 082 FR-011); the answer says why.
    TurnedAway(Arc<dyn Tool>, String),
    /// No such tool, or one the step did not offer; the answer names the closest offered tools.
    Unknown(String),
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
        // What the model saw before this round; a call cannot take an id from a sibling call.
        let seen = seen_text(&self.request.messages);
        // Spec 083 US5: what the agent profile of the model gives, read once for the round.
        let rights = profile_gate::rights_of_session(self.db, self.session).await;
        // Spec 088: only a turn that routes (a local model) checks a final call before the dialog.
        let model_id = self.request.model_id.clone();
        let route = RouteCall::of(self.session, &model_id);
        for mut call in tool_calls {
            let (tool, turned_away) = {
                let registry = self
                    .chat_state
                    .tool_registry
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let tool = registry.get(&call.name);
                // Spec 087 US4: a value in the wrong form counts as meant, for the guard, the
                // dialog, the tool and the stored call alike.
                if let Some(tool) = &tool {
                    call.input = as_meant(call.input, &tool.input_schema());
                }
                let turned_away = tool.as_ref().and_then(|tool| {
                    turn_away(
                        &call.input,
                        tool.as_ref(),
                        &seen,
                        &registry,
                        &self.request.tools,
                    )
                });
                (tool, turned_away)
            };
            let Some(tool) = tool else {
                let answer = unknown_tool_answer(&call.name, &self.request.tools);
                plans.push((call, ToolPlan::Unknown(answer)));
                continue;
            };
            if let Err(missing) = profile_gate::permits(&rights, tool.as_ref()) {
                self.tell_restricted(missing.clone()).await;
                plans.push((
                    call,
                    ToolPlan::TurnedAway(tool, profile_gate::refusal(&missing)),
                ));
                continue;
            }
            if !callable(tool.as_ref(), &self.request.tools) {
                let answer = unknown_tool_answer(&call.name, &self.request.tools);
                plans.push((call, ToolPlan::Unknown(answer)));
                continue;
            }
            if let Some(answer) = turned_away {
                plans.push((call, ToolPlan::TurnedAway(tool, answer)));
                continue;
            }
            // Read fresh per call, not cached for the round: a mode
            // change must not retroactively affect a decision already
            // made for an earlier call, but the very next tool use
            // must observe it (T024B).
            let mode = permission::read_from_vault(self.db).await;
            // Spec 088: a final call the user did not ask for, with an offered tool that does it
            // reversibly, is turned away before the dialog; the model's next call can use that tool.
            let check = reversible::before_dialog(
                route.as_ref(),
                mode,
                tool.as_ref(),
                &self.chat_state.tool_registry,
                &self.request.tools,
                &self.request.messages,
                &mut self.checked_final,
            )
            .await;
            if let Some(answer) = check.as_ref().and_then(reversible::Check::turned_away) {
                plans.push((call, ToolPlan::TurnedAway(tool, answer)));
                continue;
            }
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

impl TurnRunner<'_> {
    /// Takes from the next step's offer what the agent profile of the model does not give, so a
    /// profile that changed between two steps is followed (spec 083 FR-017).
    pub(super) async fn gate_offer(&mut self) {
        if self.request.tools.is_empty() {
            return;
        }
        let rights = profile_gate::rights_of_session(self.db, self.session).await;
        let registry = self
            .chat_state
            .tool_registry
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        profile_gate::retain_permitted(&rights, &registry, &mut self.request.tools);
    }

    /// Tells the chat which agent profile turned a call away and what it does not give, once per
    /// turn; the window shows it once per conversation (spec 083 US5).
    async fn tell_restricted(&mut self, missing: Missing) {
        if self.told_restricted {
            return;
        }
        self.told_restricted = true;
        let Some(subject) = crate::agent_profiles::local::subject_of_session(self.session) else {
            return;
        };
        let profile = self
            .db
            .read(move |q| crate::agent_profiles::resolve::profile_of(q, &subject))
            .await;
        let Ok((id, name)) = profile else {
            return;
        };
        self.emit_event(
            crate::chat::events::EVENT_CHAT_PROFILE_RESTRICTED,
            crate::chat::events::ProfileRestrictedEvent {
                thread_id: self.thread_id,
                profile_id: id.to_string(),
                profile_name: name,
                missing,
            },
        );
    }
}

#[cfg(test)]
#[path = "plan_calls_tests.rs"]
mod tests;
