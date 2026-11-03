//! Asks a model the everyday requests (spec 078 FR-002, FR-005, research R2, R3). Per request up to
//! [`MAX_STEPS`] steps with the offer a chat turn has: the core tools, `find_actions`, `ask_user`,
//! `run_command` and the haex-mail tools the message matches; a search widens the next offer as in the
//! app. Every other call is answered by the fixture world, so a run reads no real data and changes
//! nothing.

use std::collections::HashSet;
use std::time::Instant;

use tokio_util::sync::CancellationToken;

use crate::adapters::types::{
    ChatMessage, ChatRequest, ChatRole, Sampling, StreamChunk, ToolCall, ToolSpec,
};
use crate::adapters::ProviderAdapter;
use crate::chat::tools::action_tool::AgentActionDef;
use crate::chat::tools::ask_user::ASK_USER_TOOL_NAME;
use crate::chat::tools::extension_offer::matched_offer;
use crate::chat::tools::find_actions::{FindActionsTool, FIND_ACTIONS_TOOL_NAME};
use crate::chat::tools::offer::{extend_offer, found_tools, tool_spec, ExtensionScope};
use crate::chat::tools::prompt::system_prompt;
use crate::chat::tools::Tool;
use crate::chat::turn::MAX_TOOL_ROUNDS;
use crate::extensions::agent_tools::defs::ExtensionToolDef;
use crate::local_time::Clock;

use super::super::runner::{core_offer, extension_registry, EvalError, MAX_NEW_TOKENS};
use super::scoring::{End, Trace, TraceCall};
use super::world::{Answer, FixtureWorld};
use super::{EverydaySet, Request};

/// Steps per request: as many as a chat turn allows (it ends after this many tool rounds).
pub const MAX_STEPS: usize = MAX_TOOL_ROUNDS;

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

fn request(
    model: &str,
    messages: Vec<ChatMessage>,
    tools: Vec<ToolSpec>,
    deterministic: bool,
    clock: Clock,
) -> ChatRequest {
    ChatRequest {
        model_id: model.to_owned(),
        thread_id: None,
        // What a voice turn sends today: no given prompt, the tool instruction and the clock (R3).
        system_prompt: system_prompt(None, &tools, Some(clock)),
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
    let mut stream = adapter
        .stream_chat(req)
        .await
        .map_err(|e| model_error(e.to_string()))?;
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
/// app offers them; `tools` the core snapshot (`chat/eval/tools.json`).
#[allow(clippy::too_many_arguments)]
pub async fn run_everyday(
    adapter: &dyn ProviderAdapter,
    model: &str,
    deterministic: bool,
    set: &EverydaySet,
    tools: &[AgentActionDef],
    mail: &[ExtensionToolDef],
    world: &FixtureWorld,
    cancel: &CancellationToken,
) -> Result<Vec<RequestRun>, EvalError> {
    let core = core_offer(tools);
    let registry = extension_registry(mail);
    let search = FindActionsTool::with_extensions(tools.to_vec(), &registry);
    let (mail_specs, entries) = {
        let registry = registry.lock().unwrap_or_else(|e| e.into_inner());
        let specs: Vec<ToolSpec> = registry.iter().map(|t| tool_spec(t.as_ref())).collect();
        (specs, registry.match_entries())
    };
    let all_names: HashSet<String> = mail_specs.iter().map(|s| s.name.clone()).collect();
    // What the app's registry holds: every core action, every mail tool, the search and ask_user.
    let mut known: HashSet<String> = tools.iter().map(|d| d.tool_name.clone()).collect();
    known.extend(all_names.iter().cloned());
    known.extend([FIND_ACTIONS_TOOL_NAME, ASK_USER_TOOL_NAME].map(str::to_owned));
    let clock = world.clock();
    let mut runs = Vec::new();

    for item in &set.requests {
        let matched = matched_offer(&item.text, &entries);
        let mut offer = core.clone();
        offer.extend(
            mail_specs
                .iter()
                .filter(|s| matched.contains(&s.name))
                .cloned(),
        );
        let scope = ExtensionScope {
            names: all_names.clone(),
            pinned: matched.iter().cloned().collect(),
        };
        let mut history = vec![message(ChatRole::User, &item.text)];
        let mut trace = Trace {
            calls: Vec::new(),
            text: String::new(),
            end: End::StepLimit,
            first_chunk_ms: None,
        };

        'steps: for number in 0..MAX_STEPS {
            let step = match ask(
                adapter,
                request(model, history.clone(), offer.clone(), deterministic, clock),
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
            for call in &step.calls {
                let is_known = known.contains(&call.name);
                if call.name == ASK_USER_TOOL_NAME {
                    trace.calls.push(TraceCall {
                        tool: call.name.clone(),
                        args: call.input.clone(),
                        result: None,
                        known: is_known,
                    });
                    trace.text = call
                        .input
                        .get("question")
                        .and_then(|q| q.as_str())
                        .unwrap_or_default()
                        .to_owned();
                    trace.end = End::Question;
                    break 'steps;
                }
                let answer = if !is_known {
                    // As the chat's tool round answers a name it does not know.
                    Answer {
                        content: format!("unknown tool: {}", call.name),
                        is_error: true,
                    }
                } else if call.name == FIND_ACTIONS_TOOL_NAME {
                    let result = search.execute(call.input.clone(), cancel.clone()).await;
                    if let Some(found) = found_tools(&result.content).filter(|_| !result.is_error) {
                        extend_offer(&mut next, &found, tools, &scope);
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
                    known: is_known,
                });
                results.push((call, answer));
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
