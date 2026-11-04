//! Asks a model the everyday requests (spec 078 FR-002, FR-005, research R2, R3). Per request up to
//! [`MAX_STEPS`] steps with the offer a chat turn has on the measured device, built from the app's
//! own registry and offer (spec 081 FR-001); a search widens the next offer as in the app. Each call
//! is decided as the chat decides it in Auto mode, with a played user in the dialog (`consent.rs`).
//! Every call that runs is answered by the fixture world, so a run reads no real data and changes
//! nothing.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::Serialize;
use tokio_util::sync::CancellationToken;

use crate::adapters::types::{
    ChatMessage, ChatRequest, ChatRole, Sampling, StreamChunk, ToolCall, ToolSpec,
};
use crate::adapters::ProviderAdapter;
use crate::chat::action_commands::register_action_tools;
use crate::chat::reply_language::ReplyLanguage;
use crate::chat::session::host_tools;
use crate::chat::tools::action_bridge::ActionBridge;
use crate::chat::tools::action_tool::{ActionTool, AgentActionDef};
use crate::chat::tools::ask_user::ASK_USER_TOOL_NAME;
use crate::chat::tools::availability::ToolAvailability;
use crate::chat::tools::find_actions::FIND_ACTIONS_TOOL_NAME;
use crate::chat::tools::offer::{first_offer, found_tools, widen_offer};
use crate::chat::tools::permission::{decide_call, PermissionMode};
use crate::chat::tools::prompt::system_prompt;
use crate::chat::tools::{RiskClass, ToolRegistry};
use crate::chat::turn::guard::{seen_text, turn_away};
use crate::chat::turn::{callable, denied_answer, unknown_tool_answer, MAX_TOOL_ROUNDS};
use crate::extensions::agent_tools::defs::ExtensionToolDef;
use crate::local_time::Clock;
use crate::vault_gate::ChildRegistry;

use super::super::runner::{register_extensions, EvalError, MAX_NEW_TOKENS};
use super::super::scoring::Lang;
use super::consent::{play, CallEnding};
use super::scoring::{effects, End, Trace, TraceCall};
use super::world::{Answer, FixtureWorld};
use super::{EverydaySet, Request};

/// Steps per request: as many as a chat turn allows (it ends after this many tool rounds).
pub const MAX_STEPS: usize = MAX_TOOL_ROUNDS;

/// The device whose offer a run measures (spec 081 FR-001): a phone has no command tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    #[default]
    Desktop,
    Phone,
}

impl Profile {
    /// `desktop` or `phone`.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "desktop" => Some(Self::Desktop),
            "phone" => Some(Self::Phone),
            _ => None,
        }
    }
}

/// The registry a chat holds on `profile` with holzi's `actions` and the installed `extensions`,
/// built as the app builds it (`host_tools`, `register_action_tools`); no tool in it ever runs.
pub(crate) fn app_registry(
    actions: &[AgentActionDef],
    extensions: &[ExtensionToolDef],
    profile: Profile,
) -> Arc<Mutex<ToolRegistry>> {
    let handle = Arc::new(Mutex::new(host_tools(
        &ChildRegistry::default(),
        profile == Profile::Desktop,
    )));
    {
        let mut registry = handle.lock().unwrap_or_else(|e| e.into_inner());
        register_extensions(&mut registry, extensions);
        let bridge = ActionBridge::default();
        register_action_tools(&mut registry, actions, &handle, |def| {
            Arc::new(ActionTool::new(def, bridge.clone()))
        });
    }
    handle
}

/// One request and what the model did.
#[derive(Debug, Clone)]
pub struct RequestRun {
    pub request: Request,
    pub trace: Trace,
}

fn message(role: ChatRole, content: &str) -> ChatMessage {
    ChatMessage {
        role,
        content: content.to_owned(),
        attachments: Vec::new(),
    }
}

/// The language a spoken turn is held in: the request's own, as for a user whose conversation
/// language is the one they speak.
fn reply_language(lang: Lang) -> ReplyLanguage {
    match lang {
        Lang::De => ReplyLanguage::De,
        Lang::En => ReplyLanguage::En,
    }
}

fn request(
    model: &str,
    messages: Vec<ChatMessage>,
    tools: Vec<ToolSpec>,
    deterministic: bool,
    clock: Clock,
    lang: Lang,
) -> ChatRequest {
    ChatRequest {
        model_id: model.to_owned(),
        thread_id: None,
        // What a voice turn sends today: no given prompt, the tool instruction, the reply language
        // and the clock (R3).
        system_prompt: system_prompt(None, &tools, Some(clock), Some(reply_language(lang))),
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

struct Step {
    calls: Vec<ToolCall>,
    text: String,
    first_chunk_ms: Option<u64>,
}

async fn ask(
    adapter: &dyn ProviderAdapter,
    req: ChatRequest,
    id: &str,
    cancel: &CancellationToken,
) -> Result<Step, EvalError> {
    let model_error = |reason: String| EvalError::Model {
        sentence: id.to_owned(),
        reason,
    };
    let start = Instant::now();
    // A server can accept the connection and then stall before its first byte.
    let mut stream = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(EvalError::Cancelled),
        opened = adapter.stream_chat(req) => opened.map_err(|e| model_error(e.to_string()))?,
    };
    let mut step = Step {
        calls: Vec::new(),
        text: String::new(),
        first_chunk_ms: None,
    };
    loop {
        let item = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(EvalError::Cancelled),
            item = stream.next() => item,
        };
        let first = |step: &mut Step| {
            step.first_chunk_ms
                .get_or_insert(start.elapsed().as_millis() as u64);
        };
        match item {
            None | Some(Ok(StreamChunk::Done { .. })) => return Ok(step),
            Some(Ok(StreamChunk::ToolCalls(batch))) => {
                first(&mut step);
                step.calls.extend(batch);
            }
            Some(Ok(StreamChunk::Delta { content, .. })) => {
                if !content.is_empty() {
                    first(&mut step);
                }
                step.text.push_str(&content);
            }
            Some(Ok(_)) => {}
            Some(Err(error)) => return Err(model_error(error.to_string())),
        }
    }
}

/// Asks every request of `set` and returns what the model did. `mail` are haex-mail's tools as the
/// app offers them; `tools` the action snapshot (`chat/eval/tools.json`); `profile` the device whose
/// offer the run measures. Every provider gets the offer of a model that calls tools, as through
/// its API (spec 081 research R2).
#[allow(clippy::too_many_arguments)]
pub async fn run_everyday(
    adapter: &dyn ProviderAdapter,
    model: &str,
    deterministic: bool,
    set: &EverydaySet,
    tools: &[AgentActionDef],
    mail: &[ExtensionToolDef],
    world: &FixtureWorld,
    profile: Profile,
    cancel: &CancellationToken,
) -> Result<Vec<RequestRun>, EvalError> {
    let registry = app_registry(tools, mail, profile);
    let effects = effects(tools, mail);
    let clock = world.clock();
    let mut runs = Vec::new();

    for item in &set.requests {
        let mut offer = {
            let registry = registry.lock().unwrap_or_else(|e| e.into_inner());
            first_offer(ToolAvailability::Offered, &registry, &item.text)
        };
        let first: HashSet<String> = offer.iter().map(|spec| spec.name.clone()).collect();
        let mut history = vec![message(ChatRole::User, &item.text)];
        let mut trace = Trace {
            calls: Vec::new(),
            text: String::new(),
            end: End::StepLimit,
            first_chunk_ms: None,
        };

        for number in 0..MAX_STEPS {
            let step = match ask(
                adapter,
                request(
                    model,
                    history.clone(),
                    offer.clone(),
                    deterministic,
                    clock,
                    item.lang,
                ),
                &item.id,
                cancel,
            )
            .await
            {
                Ok(step) => step,
                // One request the model cannot take (a prompt past its context) has no answer; the
                // rest of the set still runs.
                Err(EvalError::Model { reason, .. }) => {
                    trace.text = reason;
                    trace.end = End::Error;
                    break;
                }
                Err(cancelled) => return Err(cancelled),
            };
            if number == 0 {
                trace.first_chunk_ms = step.first_chunk_ms;
            }
            trace.text = step.text.clone();
            if step.calls.is_empty() {
                trace.end = End::Answer;
                break;
            }
            let mut next = offer.clone();
            let mut results = Vec::new();
            let mut question = None;
            let seen = seen_text(&history);
            for call in &step.calls {
                // As `chat/turn/plan_calls.rs` plans a call: a tool the registry holds, one of
                // holzi's actions only when this step offered it, and the guard before the dialog.
                let (tool, refusal) = {
                    let registry = registry.lock().unwrap_or_else(|e| e.into_inner());
                    let tool = registry
                        .get(&call.name)
                        .filter(|tool| callable(tool.as_ref(), &offer));
                    let refusal = match &tool {
                        None => Some((unknown_tool_answer(&call.name, &offer), false)),
                        Some(tool) => {
                            turn_away(&call.input, tool.as_ref(), &seen, &registry, &offer)
                                .map(|answer| (answer, tool.risk_class() == RiskClass::Risky))
                        }
                    };
                    (tool, refusal)
                };
                if let Some((content, risky)) = refusal {
                    let answer = Answer {
                        content,
                        is_error: true,
                    };
                    trace.calls.push(TraceCall {
                        tool: call.name.clone(),
                        args: call.input.clone(),
                        result: Some(answer.clone()),
                        ending: CallEnding::TurnedAway,
                        unasked: None,
                        risky,
                    });
                    results.push((call, answer));
                    continue;
                }
                let Some(tool) = tool else { continue };
                let played = play(
                    item,
                    decide_call(PermissionMode::Auto, tool.as_ref()),
                    &trace.calls,
                    &call.name,
                    &call.input,
                    &effects,
                );
                let risky = tool.risk_class() == RiskClass::Risky;
                if call.name == ASK_USER_TOOL_NAME {
                    // The question back; the rest of the batch still runs, as in the chat.
                    trace.calls.push(TraceCall {
                        tool: call.name.clone(),
                        args: call.input.clone(),
                        result: None,
                        ending: played.ending,
                        unasked: played.unasked,
                        risky,
                    });
                    question.get_or_insert_with(|| {
                        call.input
                            .get("question")
                            .and_then(|q| q.as_str())
                            .unwrap_or_default()
                            .to_owned()
                    });
                    continue;
                }
                let answer = if played.ending == CallEnding::Stopped {
                    Answer {
                        content: denied_answer(),
                        is_error: true,
                    }
                } else if call.name == FIND_ACTIONS_TOOL_NAME {
                    let result = tool.execute(call.input.clone(), cancel.clone()).await;
                    if let Some(found) = found_tools(&result.content).filter(|_| !result.is_error) {
                        let registry = registry.lock().unwrap_or_else(|e| e.into_inner());
                        widen_offer(&registry, &mut next, &found, &first);
                    }
                    Answer {
                        content: result.content,
                        is_error: result.is_error,
                    }
                } else {
                    world.answer(&call.name, &call.input)
                };
                trace.calls.push(TraceCall {
                    tool: call.name.clone(),
                    args: call.input.clone(),
                    result: Some(answer.clone()),
                    ending: played.ending,
                    unasked: played.unasked,
                    risky,
                });
                results.push((call, answer));
            }
            if let Some(question) = question {
                trace.text = question;
                trace.end = End::Question;
                break;
            }
            // As the chat history holds them: the calls of a step, then their results.
            for (call, _) in &results {
                history.push(message(
                    ChatRole::ToolCall {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        input: call.input.clone(),
                    },
                    "",
                ));
            }
            for (call, answer) in results {
                history.push(message(
                    ChatRole::ToolResult {
                        call_id: call.id.clone(),
                        content: answer.content,
                        is_error: answer.is_error,
                    },
                    "",
                ));
            }
            offer = next;
        }
        runs.push(RequestRun {
            request: item.clone(),
            trace,
        });
    }
    Ok(runs)
}
