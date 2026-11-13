//! Asks a model the everyday requests (spec 078 FR-002, FR-005, research R2, R3). Per request up to
//! [`MAX_STEPS`] steps with the offer a chat turn has on the measured device, built from the app's
//! own registry and offer (spec 082 FR-001); a search widens the next offer as in the app. Each call
//! is decided as the chat decides it in Auto mode, with a played user in the dialog (`consent.rs`).
//! Every call that runs is answered by the fixture world, so a run reads no real data and changes
//! nothing.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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
use crate::chat::tools::availability::ToolAvailability;
use crate::chat::tools::domains::domain_of;
use crate::chat::tools::offer::{first_offer, with_search};
use crate::chat::tools::prompt::system_prompt;
use crate::chat::tools::route::{first_step_offer, RouteCall};
use crate::chat::tools::ToolRegistry;
use crate::chat::turn::arg_forms::as_meant;
use crate::chat::turn::guard::seen_text;
use crate::chat::turn::MAX_TOOL_ROUNDS;
use crate::extensions::agent_tools::defs::ExtensionToolDef;
use crate::local_time::Clock;
use crate::vault_gate::ChildRegistry;

use super::super::embedded_domains;
use super::super::runner::{register_extensions, EvalError, MAX_NEW_TOKENS};
use super::super::scoring::Lang;
use super::mail_names::MailNames;
use super::plan_call::{Back, RequestChecks, StepCalls};
use super::report::CheckRecord;
use super::scoring::{effects, End, Trace};
use super::world::FixtureWorld;
use super::{EverydaySet, Expectation, Request};

/// Steps per request: as many as a chat turn allows (it ends after this many tool rounds).
pub const MAX_STEPS: usize = MAX_TOOL_ROUNDS;

/// The device whose offer a run measures (spec 082 FR-001): a phone has no command tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    #[default]
    Desktop,
    Phone,
}

impl Profile {
    /// `desktop` or `phone`, as [`Self::parse`] reads it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Desktop => "desktop",
            Self::Phone => "phone",
        }
    }

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
        registry.set_domains(embedded_domains());
    }
    handle
}

/// What the routing call of one request gave (spec 082 US2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteRecord {
    /// The routed domain keys, the mail extension's as the set names it (`x_mail`).
    pub routed: Vec<String>,
    pub ms: u64,
    /// Whether the routed domains hold the domain of the expected first tool; `None` for a
    /// request that expects no call.
    pub hit: Option<bool>,
}

/// The domain of the first tool `request` expects, if it expects a call.
fn expected_domain(
    request: &Request,
    registry: &ToolRegistry,
    names: &MailNames,
) -> Option<String> {
    let Expectation::Calls(chains) = &request.expect else {
        return None;
    };
    let tool = registry.get(names.in_app(&chains.first()?.first()?.tool))?;
    domain_of(tool.as_ref())
}

/// One request and what the model did.
#[derive(Debug, Clone)]
pub struct RequestRun {
    pub request: Request,
    pub trace: Trace,
    /// The routing call, when the run routes.
    pub route: Option<RouteRecord>,
    /// The tools the first step offered, in the set's names, sorted.
    pub first_offer: Vec<String>,
    /// The checks before the dialog, in the set's names (spec 088).
    pub checks: Vec<CheckRecord>,
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
    routed: bool,
) -> ChatRequest {
    ChatRequest {
        model_id: model.to_owned(),
        thread_id: None,
        // What a voice turn sends today: no given prompt, the tool instruction (with the routed
        // turn's sentence when it routed), the reply language and the clock (R3).
        system_prompt: system_prompt(
            None,
            &tools,
            Some(clock),
            Some(reply_language(lang)),
            routed,
        ),
        messages,
        reasoning_requested: false,
        max_new_tokens: Some(MAX_NEW_TOKENS),
        tools,
        autonomy_mode: Default::default(),
        autonomy_deny: Default::default(),
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
/// its API (spec 082 research R2). With `routing`, the first offer is routed as in the chat, the
/// routing call bounded by that timeout (spec 082 US2).
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
    routing: Option<Duration>,
    cancel: &CancellationToken,
) -> Result<Vec<RequestRun>, EvalError> {
    let registry = app_registry(tools, mail, profile);
    let effects = effects(tools, mail);
    // The model calls the app's names; the trace, the dialog and the world use the set's.
    let names = MailNames::of(mail);
    let clock = world.clock();
    let mut runs = Vec::new();
    // The routing call's model; with it, a final call is also checked before the dialog (spec 088).
    let route_call = routing.map(|timeout| RouteCall {
        adapter,
        model_id: model,
        timeout,
        sampling: if deterministic {
            Sampling::Deterministic
        } else {
            Sampling::Default
        },
    });

    for item in &set.requests {
        let (mut offer, route) = match &route_call {
            None => {
                let registry = registry.lock().unwrap_or_else(|e| e.into_inner());
                (
                    first_offer(ToolAvailability::Offered, &registry, &item.text),
                    None,
                )
            }
            Some(call) => {
                let (offer, routed) =
                    first_step_offer(ToolAvailability::Offered, &registry, Some(call), &item.text)
                        .await;
                let expected = {
                    let registry = registry.lock().unwrap_or_else(|e| e.into_inner());
                    expected_domain(item, &registry, &names)
                };
                let record = RouteRecord {
                    hit: expected.map(|domain| routed.keys.contains(&domain)),
                    routed: routed
                        .keys
                        .iter()
                        .map(|key| names.domain_in_set(key).to_owned())
                        .collect(),
                    ms: routed.ms.unwrap_or(0),
                };
                (offer, Some(record))
            }
        };
        let first_offer = names.offer_in_set(&offer);
        // As in the chat: the first offer and every tool the request called stay after a search.
        let mut kept: HashSet<String> = offer.iter().map(|spec| spec.name.clone()).collect();
        let mut history = vec![message(ChatRole::User, &item.text)];
        let mut checks = RequestChecks::default();
        let mut trace = Trace {
            calls: Vec::new(),
            text: String::new(),
            end: End::StepLimit,
            first_chunk_ms: None,
        };

        for number in 0..MAX_STEPS {
            let mut step = match ask(
                adapter,
                request(
                    model,
                    history.clone(),
                    offer.clone(),
                    deterministic,
                    clock,
                    item.lang,
                    route.is_some(),
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
            // As `plan_calls` does: values in the wrong form count as meant (spec 087 US4).
            for call in &mut step.calls {
                let registry = registry.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(tool) = registry.get(&call.name) {
                    call.input = as_meant(std::mem::take(&mut call.input), &tool.input_schema());
                }
            }
            let mut next = offer.clone();
            with_search(
                &registry.lock().unwrap_or_else(|e| e.into_inner()),
                &mut next,
            );
            kept.extend(step.calls.iter().map(|call| call.name.clone()));
            let mut results = Vec::new();
            let mut question = None;
            let seen = seen_text(&history);
            let calls = StepCalls {
                item,
                registry: &registry,
                names: &names,
                effects: &effects,
                world,
                offer: &offer,
                seen: &seen,
                kept: &kept,
                cancel,
                route: route_call.as_ref(),
                history: &history,
            };
            for call in &step.calls {
                match calls.answer(call, &mut trace, &mut next, &mut checks).await {
                    Back::Answer(answer) => results.push((call, answer)),
                    Back::Question(asked) => {
                        question.get_or_insert(asked);
                    }
                }
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
            route,
            first_offer,
            checks: checks.done,
        });
    }
    Ok(runs)
}
