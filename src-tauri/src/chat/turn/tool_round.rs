//! One tool round: the sub-flow a step triggers when its response asked
//! for tool calls — plan, execute, persist, feed back into the request.
//!
//! Split out of `chat/turn.rs` (2026-09-16); see
//! `docs/plans/2026-09-16-turn-module-split-design.md`.

use tokio_util::sync::CancellationToken;

use uuid::Uuid;

use super::call_source::CallSource;
use crate::adapters::types::{ChatMessage as LlmMessage, ChatRole, ToolCall as LlmToolCall};
use crate::chat::events::{
    strip_leaked_tool_call_markup, MessageCompleteEvent, ToolCallEvent, ToolResultEvent,
    TurnCompleteEvent, EVENT_CHAT_MESSAGE_COMPLETE, EVENT_CHAT_TOOL_CALL, EVENT_CHAT_TOOL_RESULT,
    EVENT_CHAT_TURN_COMPLETE,
};
use crate::chat::session::ChatState;
use crate::chat::tools::agent_task::{within, AgentTask};
use crate::chat::tools::offer::{found_tools, page_tools_after, widen_offer, with_search};
use crate::chat::tools::{ApprovalDecision, ToolResult as ToolExecResult};
use crate::storage::chat_messages::{ChatMessage, FinishReason, MessageRole};

use super::images;
use super::persist::{
    empty_tool_message, persist_final_message, persist_message, persist_tool_pair,
};
use super::plan_calls::{denied_answer, ToolPlan};
use super::step::StepResult;
use super::TurnRunner;

/// Fixed cap on the number of tool-calling rounds within one turn
/// (spec.md FR-016). A "round" is one step whose response contained at
/// least one tool call. Reaching the cap ends the turn with
/// `FinishReason::ToolLimitReached` instead of issuing a further step.
pub const MAX_TOOL_ROUNDS: usize = 8;

/// One executed call: the request, its result, and where its tool came from, recorded on the
/// persisted row.
pub(super) type ExecutedCall = (LlmToolCall, ToolExecResult, CallSource);

/// What the turn loop should do after a round.
pub(super) enum RoundOutcome {
    /// Issue another step.
    Continue,
    /// The turn is over and its terminal events have already been
    /// emitted — cancellation, a persistence failure, or the round cap.
    Ended,
}

impl TurnRunner<'_> {
    /// Runs one tool round to completion.
    pub(super) async fn tool_round(&mut self, step: StepResult) -> RoundOutcome {
        if self.persist_interim_text(step.assembled).await.is_err() {
            return RoundOutcome::Ended;
        }

        // Cancellation may already have fired between this step's
        // `Done`/`ToolCalls` frame and here (e.g. a stray abort that
        // raced the previous step's own completion) — checked before
        // minting any new approval wait so a freshly-inserted sender
        // never sits in `pending_tool_approvals` forever, unreachable
        // by the one-time drain in `abort_turn` (T032/FR-011).
        if self.cancel.is_cancelled() {
            self.end_cancelled().await;
            return RoundOutcome::Ended;
        }

        let plans = self.plan_calls(step.tool_calls).await;
        let task = AgentTask::of_turn(&self.assistant_message_id.to_string(), self.session);
        // A model with a small context gets results that fit it (a browser page reads less).
        let mut executed = crate::chat::tools::context_budget::within(
            self.session.adapter.context_limit(),
            within(
                task.clone(),
                Self::execute_plans(self.chat_state, self.db, self.session, &self.cancel, plans),
            ),
        )
        .await;
        within(task, self.resolve_choices(&mut executed)).await;

        // Aborted mid-round (either an in-flight `execute()` was cut
        // short, or a pending approval's sender was dropped): none of
        // this round's rows are persisted, matching the invariant that
        // an interrupted round leaves no trace (data-model.md). The
        // turn ends here — no further step is issued (FR-011/T033).
        if self.cancel.is_cancelled() {
            self.end_cancelled().await;
            return RoundOutcome::Ended;
        }

        if self.persist_round(&executed).await.is_err() {
            return RoundOutcome::Ended;
        }
        self.append_round_to_request(&executed);
        self.gate_offer().await;

        self.rounds_used += 1;
        if self.rounds_used >= MAX_TOOL_ROUNDS {
            self.end_tool_limit().await;
            return RoundOutcome::Ended;
        }

        // The next round's stream is started by `run_step` itself at the
        // top of the loop — including its own retry-on-transient-failure
        // handling (T037).
        RoundOutcome::Continue
    }

    /// Any text the model emitted before its tool use becomes its own
    /// interim assistant row (data-model.md's `assistant(*)` — optional,
    /// only present when the step actually produced text first).
    ///
    /// Strips first: a reasoning-capable local model can leak its raw
    /// `<tool_call>` tag into this text (see
    /// `strip_leaked_tool_call_markup`) — left in, that markup would
    /// otherwise be persisted and shown as if it were the model's own
    /// reply.
    async fn persist_interim_text(&mut self, assembled: String) -> Result<(), ()> {
        let assembled = strip_leaked_tool_call_markup(&assembled);
        if assembled.trim().is_empty() {
            return Ok(());
        }
        let interim_id = Uuid::new_v4();
        let msg = ChatMessage {
            role: MessageRole::Assistant,
            content: assembled.clone(),
            provider_id: self.session.provider_id,
            model_id: Some(self.session.model_id.clone()),
            ..empty_tool_message(interim_id, self.thread_id, Some(self.parent_id))
        };
        if let Err(reason) = persist_message(self.db, msg).await {
            self.fail(reason);
            return Err(());
        }
        self.parent_id = interim_id;
        self.request.messages.push(LlmMessage {
            role: ChatRole::Assistant,
            attachments: Vec::new(),
            content: assembled,
        });
        Ok(())
    }

    /// Waits out every `Ask` and runs every approved call concurrently.
    ///
    /// Takes its collaborators explicitly instead of `&self`: holding a
    /// shared `&TurnRunner` across an await would require `TurnRunner:
    /// Sync`, which the `&mut dyn FnMut` event sink cannot satisfy.
    async fn execute_plans(
        chat_state: &ChatState,
        db: &crate::vault_gate::VaultDb,
        session: &crate::chat::session::ActiveSession,
        cancel: &CancellationToken,
        plans: Vec<(LlmToolCall, ToolPlan)>,
    ) -> Vec<ExecutedCall> {
        futures::future::join_all(plans.into_iter().map(|(call, plan)| {
            let cancel = cancel.clone();
            async move {
                match plan {
                    ToolPlan::Allow(tool) => {
                        let source = CallSource::of(tool.as_ref());
                        let result = tool.execute(call.input.clone(), cancel).await;
                        (call, result, source)
                    }
                    ToolPlan::Ask {
                        tool,
                        rx,
                        request_id,
                    } => {
                        // `abort_turn` drops every pending sender
                        // (T032), so a dropped-without-answer `rx`
                        // below always means cancellation, never a
                        // silent auto-decision (FR-005).
                        let tool_cancel = cancel.clone();
                        let decision = tokio::select! {
                            biased;
                            _ = cancel.cancelled() => Err(()),
                            decision = rx => decision.map_err(|_| ()),
                        };
                        match decision {
                            Ok(ApprovalDecision::Allow) => {
                                let rights = crate::chat::tools::profile_gate::rights_of_session(
                                    db, session,
                                )
                                .await;
                                let result = match crate::chat::tools::profile_gate::permits(
                                    &rights,
                                    tool.as_ref(),
                                ) {
                                    Ok(()) => tool.execute(call.input.clone(), tool_cancel).await,
                                    Err(missing) => ToolExecResult::error(
                                        crate::chat::tools::profile_gate::refusal(&missing),
                                    ),
                                };
                                let source = CallSource::of(tool.as_ref());
                                (call, result, source)
                            }
                            Ok(ApprovalDecision::Deny) => {
                                let source = CallSource::of(tool.as_ref());
                                (call, ToolExecResult::error(denied_answer()), source)
                            }
                            Err(_) => {
                                let mut pending = chat_state
                                    .pending_tool_approvals
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner());
                                chat_state
                                    .cancelled_tool_approvals
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .insert(request_id);
                                pending.remove(&request_id);
                                (
                                    call,
                                    ToolExecResult::error("tool_call_cancelled"),
                                    CallSource::of(tool.as_ref()),
                                )
                            }
                        }
                    }
                    ToolPlan::Deny(tool) => (
                        call,
                        // Fixed, non-localized marker — the frontend
                        // translates it (CONTEXT.md i18n boundary),
                        // same convention as `LoadPhase` above.
                        ToolExecResult::error("blocked_by_plan_mode"),
                        CallSource::of(tool.as_ref()),
                    ),
                    ToolPlan::TurnedAway(tool, answer) => {
                        // Refused before the dialog: it neither runs nor asks the user.
                        let source = CallSource::of(tool.as_ref());
                        (call, ToolExecResult::error(answer), source)
                    }
                    ToolPlan::Unknown(answer) => {
                        // The model named a tool no longer in the registry (e.g. its MCP
                        // server disconnected mid-conversation, spec.md Edge Cases) or one
                        // that never existed.
                        (call, ToolExecResult::error(answer), CallSource::unknown())
                    }
                }
            }
        }))
        .await
    }

    /// Writes this round's call/result row pair per executed call and
    /// emits the matching events.
    async fn persist_round(&mut self, executed: &[ExecutedCall]) -> Result<(), ()> {
        self.bump_created_at();
        let shown = images::shows_images(&self.request);
        for (call, result, source) in executed {
            let tool_call_row_id = Uuid::new_v4();
            let input_json =
                serde_json::to_string(&call.input).unwrap_or_else(|_| "{}".to_string());
            let call_msg = ChatMessage {
                role: MessageRole::ToolCall,
                provider_id: self.session.provider_id,
                model_id: Some(self.session.model_id.clone()),
                tool_name: Some(call.name.clone()),
                tool_call_id: Some(call.id.clone()),
                tool_input: Some(input_json),
                tool_source: Some(source.source.to_string()),
                tool_origin: source.origin_json(),
                created_at: self.next_created_at,
                ..empty_tool_message(tool_call_row_id, self.thread_id, Some(self.parent_id))
            };
            self.next_created_at = self.next_created_at.saturating_add(1);
            let tool_result_row_id = Uuid::new_v4();
            let result_msg = ChatMessage {
                role: MessageRole::ToolResult,
                content: images::kept(result, shown),
                provider_id: self.session.provider_id,
                model_id: Some(self.session.model_id.clone()),
                tool_call_id: Some(call.id.clone()),
                tool_is_error: Some(result.is_error),
                created_at: self.next_created_at,
                ..empty_tool_message(tool_result_row_id, self.thread_id, Some(tool_call_row_id))
            };
            self.next_created_at = self.next_created_at.saturating_add(1);
            if let Err(reason) = persist_tool_pair(self.db, call_msg, result_msg).await {
                self.fail(reason);
                return Err(());
            }
            self.parent_id = tool_result_row_id;
            self.emit_event(
                EVENT_CHAT_TOOL_CALL,
                ToolCallEvent {
                    message_id: tool_call_row_id,
                    thread_id: self.thread_id,
                    tool_name: call.name.clone(),
                    tool_input: call.input.clone(),
                    tool_source: source.source.to_string(),
                    tool_origin: source.origin.clone(),
                },
            );
            self.emit_event(
                EVENT_CHAT_TOOL_RESULT,
                ToolResultEvent {
                    message_id: tool_result_row_id,
                    thread_id: self.thread_id,
                    tool_call_id: call.id.clone(),
                    content: images::kept(result, shown),
                    is_error: result.is_error,
                },
            );
        }
        Ok(())
    }

    /// Appends this round to the request, grouped by role in two passes
    /// (not interleaved): both adapters' wire-format builders only merge
    /// strictly consecutive same-role rows into one message, so a round
    /// with several tool calls must land as one assistant tool-use
    /// message followed by one tool-result message, not call/result pairs
    /// per call.
    fn append_round_to_request(&mut self, executed: &[ExecutedCall]) {
        // The last successful search of the round decides what the next step offers.
        let search_result = executed
            .iter()
            .rev()
            .find(|(call, result, _)| {
                call.name == crate::chat::tools::offer::FIND_ACTIONS_TOOL_NAME && !result.is_error
            })
            .and_then(|(_, result, _)| found_tools(&result.content));
        for (call, _, _) in executed {
            self.request.messages.push(LlmMessage {
                role: ChatRole::ToolCall {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    input: call.input.clone(),
                },
                attachments: Vec::new(),
                content: String::new(),
            });
        }
        let shown = images::shows_images(&self.request);
        for (call, result, _) in executed {
            let (content, attachments) = images::sent(result, shown);
            self.request.messages.push(LlmMessage {
                role: ChatRole::ToolResult {
                    call_id: call.id.clone(),
                    content,
                    is_error: result.is_error,
                },
                attachments,
                content: String::new(),
            });
        }
        self.kept_tools
            .extend(executed.iter().map(|(call, _, _)| call.name.clone()));
        let registry = self
            .chat_state
            .tool_registry
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // A page that is open can be worked on: the tools for that need no search.
        let called: Vec<(&str, bool)> = executed
            .iter()
            .map(|(call, result, _)| (call.name.as_str(), result.is_error))
            .collect();
        if with_search(&registry, &mut self.request.tools) {
            self.use_search_prompt();
        }
        let page_tools = page_tools_after(&registry, &mut self.request.tools, &called);
        self.kept_tools.extend(page_tools);
        if let Some(found) = search_result {
            widen_offer(&registry, &mut self.request.tools, &found, &self.kept_tools);
        }
        drop(registry);
        // What the model is sent must fit its context, or the engine refuses the whole prompt.
        if let Some(limit) = self.session.adapter.context_limit() {
            super::fit::fit(&mut self.request, limit);
        }
    }

    /// Swaps the system prompt for the one that goes with the tool search, once, when the search
    /// joined the offer (spec 087 research R6.3). The tools changed with it, so a local model reads
    /// its prompt anew from the start either way.
    fn use_search_prompt(&mut self) {
        let Some(prompt) = self.search_prompt.take() else {
            return;
        };
        match prompt.apply(self.request.system_prompt.as_deref()) {
            Some(next) => self.request.system_prompt = next,
            None => log::warn!("chat: the system prompt changed; it keeps the first step's"),
        }
    }

    /// Ends the turn at the round cap (spec.md FR-016) with an empty
    /// terminal assistant row carrying `ToolLimitReached`.
    async fn end_tool_limit(&mut self) {
        let created_at = self.bump_created_at();
        let (thread_id, message_id) = (self.thread_id, self.assistant_message_id);
        let final_msg = ChatMessage {
            role: MessageRole::Assistant,
            finish_reason: Some(FinishReason::ToolLimitReached),
            created_at,
            ..empty_tool_message(message_id, thread_id, Some(self.parent_id))
        };
        if let Err(reason) = persist_final_message(
            self.db,
            final_msg,
            self.session.provider_id,
            self.session.model_id.clone(),
        )
        .await
        {
            self.fail(reason);
            return;
        }
        self.emit_event(
            EVENT_CHAT_MESSAGE_COMPLETE,
            MessageCompleteEvent {
                message_id,
                thread_id,
                prompt_tokens: None,
                completion_tokens: None,
                ttft_ms: None,
            },
        );
        self.emit_event(
            EVENT_CHAT_TURN_COMPLETE,
            TurnCompleteEvent {
                thread_id,
                assistant_message_id: Some(message_id),
                finish_reason: FinishReason::ToolLimitReached,
            },
        );
    }
}
