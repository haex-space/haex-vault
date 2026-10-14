//! Asks a model the example sentences and scores what it does (spec 032 US5, research R9).
//!
//! Per sentence the runner takes up to two steps with the same offer a chat turn starts with
//! (the core tools plus `find_actions`, research R6): when the expected tool lies outside the core
//! offer, the model must search first; the runner runs the real search over the tool snapshot,
//! offers the hits as the app does, and scores the whole batch of calls of the second step.
//! No action is ever executed, so a run cannot change any data (FR-021).

use tokio_util::sync::CancellationToken;

use crate::adapters::types::{
    ChatMessage, ChatRequest, ChatRole, Sampling, StreamChunk, ToolCall, ToolSpec,
};
use crate::adapters::ProviderAdapter;
use crate::chat::tools::action_tool::AgentActionDef;
use crate::chat::tools::ask_user::{AskUserTool, ASK_USER_TOOL_NAME};
use crate::chat::tools::find_actions::{FindActionsTool, FIND_ACTIONS_TOOL_NAME};
use crate::chat::tools::offer::{extend_offer, found_tools, tool_spec, ExtensionScope};
use crate::chat::tools::prompt::system_prompt;
use crate::chat::tools::Tool;

use super::scoring::{report, score, EvalReport, EvalSet, Expect, Observed, ObservedCall, Scored};

/// Cap on a model's answer per step: enough for any call, bounded for a model that rambles.
const MAX_NEW_TOKENS: usize = 1024;

/// Which sentences of the set to ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    /// The complete set (the full measurement).
    All,
    /// The small subset marked `selfTest` (the background check in the app).
    SelfTest,
}

/// Failure while asking a model or cancelling an evaluation run.
#[derive(Debug, thiserror::Error)]
pub enum EvalError {
    #[error("the model could not be asked ({sentence}): {reason}")]
    Model { sentence: String, reason: String },
    #[error("the evaluation was cancelled")]
    Cancelled,
}

/// Converts a catalog action into the provider-neutral tool specification.
fn spec_of(def: &AgentActionDef) -> ToolSpec {
    ToolSpec {
        name: def.tool_name.clone(),
        description: def.description.clone(),
        input_schema: def.input_schema.clone(),
    }
}

/// What a chat turn offers first: the core tools, the search and `ask_user`.
fn core_offer(tools: &[AgentActionDef]) -> Vec<ToolSpec> {
    let mut offer: Vec<ToolSpec> = tools.iter().filter(|d| d.core).map(spec_of).collect();
    offer.push(tool_spec(&FindActionsTool::new(tools.to_vec())));
    offer.push(tool_spec(&AskUserTool));
    offer
}

/// Builds a history message without attachments for an evaluation request.
fn message(role: ChatRole, content: &str) -> ChatMessage {
    ChatMessage {
        role,
        content: content.to_owned(),
        attachments: Vec::new(),
    }
}

/// Builds one adapter request with the evaluation's bounded output and sampling mode.
fn request(
    model: &str,
    messages: Vec<ChatMessage>,
    tools: Vec<ToolSpec>,
    deterministic: bool,
) -> ChatRequest {
    ChatRequest {
        model_id: model.to_owned(),
        thread_id: None,
        // The instruction a chat turn with tools carries, so the run measures what the app does.
        system_prompt: system_prompt(None, &tools),
        messages,
        reasoning_requested: false,
        max_new_tokens: Some(MAX_NEW_TOKENS),
        tools,
        autonomy_mode: Default::default(),
        reasoning_option: None,
        capabilities: None,
        sampling: if deterministic {
            Sampling::Deterministic
        } else {
            Sampling::Default
        },
    }
}

/// What a model did in one step: the tool calls it made and the text it wrote.
struct Answer {
    calls: Vec<ToolCall>,
    text: String,
}

/// Runs one step and returns what the model did.
async fn ask(
    adapter: &dyn ProviderAdapter,
    req: ChatRequest,
    sentence: &str,
    cancel: &CancellationToken,
) -> Result<Answer, EvalError> {
    let model_error = |reason: String| EvalError::Model {
        sentence: sentence.to_owned(),
        reason,
    };
    let mut stream = adapter
        .stream_chat(req)
        .await
        .map_err(|e| model_error(e.to_string()))?;
    let mut calls = Vec::new();
    let mut text = String::new();
    loop {
        let item = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(EvalError::Cancelled),
            item = stream.next() => item,
        };
        match item {
            None | Some(Ok(StreamChunk::Done { .. })) => return Ok(Answer { calls, text }),
            Some(Ok(StreamChunk::ToolCalls(batch))) => calls.extend(batch),
            Some(Ok(StreamChunk::Delta { content, .. })) => text.push_str(&content),
            Some(Ok(_)) => {}
            Some(Err(error)) => return Err(model_error(error.to_string())),
        }
    }
}

/// Copies provider-neutral tool calls into the pure scoring representation.
fn observed_calls(calls: &[ToolCall]) -> Vec<ObservedCall> {
    calls
        .iter()
        .map(|call| ObservedCall {
            tool: call.name.clone(),
            args: call.input.clone(),
        })
        .collect()
}

/// Asks the sentences of `which` and scores them. `model` is the id the adapter expects;
/// `deterministic` asks for greedy decoding where the adapter can do it.
pub async fn run_eval(
    adapter: &dyn ProviderAdapter,
    model: &str,
    deterministic: bool,
    set: &EvalSet,
    tools: &[AgentActionDef],
    which: Which,
    cancel: &CancellationToken,
) -> Result<EvalReport, EvalError> {
    let offer = core_offer(tools);
    let search = FindActionsTool::new(tools.to_vec());
    let mut scored = Vec::new();

    for sentence in set
        .sentences
        .iter()
        .filter(|s| which == Which::All || s.self_test)
    {
        let user = message(ChatRole::User, &sentence.text);
        let first = ask(
            adapter,
            request(model, vec![user.clone()], offer.clone(), deterministic),
            &sentence.id,
            cancel,
        )
        .await?;

        let expected: Vec<&str> = match &sentence.expect {
            Expect::None => Vec::new(),
            Expect::Calls(calls) => calls.iter().map(|c| c.tool.as_str()).collect(),
        };
        // `ask_user` is offered from the first step on, like the core actions.
        let needs_search = expected.iter().any(|name| {
            *name != ASK_USER_TOOL_NAME
                && !tools.iter().any(|def| def.core && def.tool_name == *name)
        });

        let observed = if !needs_search {
            Observed {
                calls: observed_calls(&first.calls),
                searched: false,
                reached: true,
                text: first.text.clone(),
            }
        } else if let [lone] = first.calls.as_slice() {
            if lone.name == FIND_ACTIONS_TOOL_NAME {
                let result = search.execute(lone.input.clone(), cancel.clone()).await;
                let mut next = offer.clone();
                // A result that is no list of tools leaves the offer as it is, as in a chat turn.
                if let Some(found) = found_tools(&result.content).filter(|_| !result.is_error) {
                    extend_offer(&mut next, &found, tools, &ExtensionScope::default());
                }
                let reached = expected
                    .iter()
                    .all(|name| next.iter().any(|spec| spec.name == *name));
                let (calls, text) = if reached {
                    let messages = vec![
                        user,
                        message(
                            ChatRole::ToolCall {
                                id: lone.id.clone(),
                                name: lone.name.clone(),
                                input: lone.input.clone(),
                            },
                            "",
                        ),
                        message(
                            ChatRole::ToolResult {
                                call_id: lone.id.clone(),
                                content: result.content.clone(),
                                is_error: result.is_error,
                            },
                            "",
                        ),
                    ];
                    let second = ask(
                        adapter,
                        request(model, messages, next, deterministic),
                        &sentence.id,
                        cancel,
                    )
                    .await?;
                    (observed_calls(&second.calls), second.text)
                } else {
                    (Vec::new(), first.text.clone())
                };
                Observed {
                    calls,
                    searched: true,
                    reached,
                    text,
                }
            } else {
                Observed {
                    calls: observed_calls(&first.calls),
                    searched: false,
                    reached: false,
                    text: first.text.clone(),
                }
            }
        } else {
            Observed {
                calls: observed_calls(&first.calls),
                searched: false,
                reached: false,
                text: first.text.clone(),
            }
        };

        let result = score(sentence, &observed, tools);
        scored.push(Scored {
            sentence,
            observed,
            result,
        });
    }
    Ok(report(set.version, model, deterministic, &scored))
}
