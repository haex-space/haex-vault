//! One tool call of an external agent (spec 021 FR-014, FR-015, FR-021 to FR-023, FR-028, FR-029,
//! contracts/mcp-surface.md): checked again against the agent's rights, held to four running calls
//! per agent, let through, asked about or refused by the agent's approval mode, run by the window's
//! action runner as caller `externalAgent`, written to the agent log, and turned into an MCP result.
//! A tool of an extension takes the same way and runs over the extension's own connection, where
//! the stricter of the agent's approval mode and the tool's own setting wins (US4, FR-018), and so
//! does holzi's command tool, which is `Risky` (US5, FR-012). A refusal never says whether a target
//! exists.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use rmcp::model::{CallToolResult, Tool, ToolAnnotations};
use serde_json::{json, Map, Value};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::agent_log::{self, NewEntry, Outcome};
use super::command_tool;
use super::connection::now_millis;
use super::extension_tools;
use super::grants::{self, DataRights, Reach};
use super::hello::CloseReason;
use super::live::Link;
use super::prompts::{
    call_gate, extension_gate, CallDecision, CallEffect, CallQuestion, CallTitles, PromptClosed,
    PromptClosedReason, SECRET_SCOPE,
};
use super::redact::{redact, target_of};
use super::server::{Core, Notice};
use super::session::SessionAgent;
use crate::caller::Caller;
use crate::chat::tools::action_bridge::{ActionCaller, ActionReply};
use crate::chat::tools::action_tool::{ActionEffect, ActionRunner, AgentActionDef};
use crate::chat::tools::native_action::{NativeCall, NativeError, NativeExecutor, NativeReply};
use crate::chat::tools::permission::Decision;
use crate::chat::tools::{RiskClass, Tool as ChatTool};
use crate::files::permissions;
use crate::prompt_park::PromptEnd;

/// What the agent learns when its rights do not cover a call, or the tool does not exist.
pub const NOT_ALLOWED_MESSAGE: &str = "This tool is not available to this agent.";

/// The tool an agent sees for an action: the same name, description and schema as the built-in
/// agent gets (FR-013), with hints about its effect.
pub fn tool_of(def: &AgentActionDef) -> Tool {
    let schema = match &def.input_schema {
        Value::Object(map) => map.clone(),
        _ => Map::new(),
    };
    Tool::new(def.tool_name.clone(), def.description.clone(), schema).with_annotations(
        ToolAnnotations::new()
            .read_only(def.effect == ActionEffect::Read)
            .destructive(def.effect == ActionEffect::Destructive),
    )
}

/// A refusal in the shape of the contract.
pub fn error_result(code: &str, message: &str, extra: Option<(&str, Value)>) -> CallToolResult {
    let mut body = json!({ "code": code, "message": message });
    if let Some((key, value)) = extra {
        body[key] = value;
    }
    CallToolResult::structured_error(body)
}

pub fn not_allowed() -> CallToolResult {
    error_result("not_allowed", NOT_ALLOWED_MESSAGE, None)
}

pub(super) fn unavailable() -> CallToolResult {
    error_result("unavailable", "holzi could not run the action now.", None)
}

/// The running calls of each agent (FR-029, research R11), across its connections.
#[derive(Default)]
pub struct CallSlots {
    agents: Mutex<HashMap<Uuid, Arc<Semaphore>>>,
}

impl CallSlots {
    /// A slot for one call of `agent`, waiting up to `wait`; `None` when all stay taken.
    pub async fn acquire(
        &self,
        agent: Uuid,
        max: usize,
        wait: Duration,
    ) -> Option<OwnedSemaphorePermit> {
        let slots = Arc::clone(
            self.agents
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(agent)
                .or_insert_with(|| Arc::new(Semaphore::new(max))),
        );
        tokio::time::timeout(wait, slots.acquire_owned())
            .await
            .ok()?
            .ok()
    }
}

/// What a call reaches: an action of the catalog, a tool of an extension or the command tool.
#[derive(Clone, Copy)]
enum Callee<'a> {
    Action(&'a AgentActionDef),
    Extension(&'a Arc<dyn ChatTool>),
    Command(&'a Arc<dyn ChatTool>),
}

impl Callee<'_> {
    fn input_schema(self) -> Value {
        match self {
            Self::Action(def) => def.input_schema.clone(),
            Self::Extension(tool) | Self::Command(tool) => tool.input_schema(),
        }
    }

    fn effect(self) -> ActionEffect {
        match self {
            Self::Action(def) => def.effect,
            Self::Extension(tool) | Self::Command(tool) => match tool.risk_class() {
                RiskClass::Safe => ActionEffect::Read,
                RiskClass::Change => ActionEffect::Write,
                RiskClass::Risky => ActionEffect::Destructive,
            },
        }
    }
}

/// The tool behind `tool_name`, whether or not the agent may call it.
fn find<'a>(
    catalog: &'a [AgentActionDef],
    tools: &'a [Arc<dyn ChatTool>],
    command: Option<&'a Arc<dyn ChatTool>>,
    tool_name: &str,
) -> Option<Callee<'a>> {
    catalog
        .iter()
        .find(|def| def.tool_name == tool_name)
        .map(Callee::Action)
        .or_else(|| {
            tools
                .iter()
                .find(|tool| tool.name() == tool_name)
                .map(Callee::Extension)
        })
        .or_else(|| {
            command
                .filter(|tool| tool.name() == tool_name)
                .map(Callee::Command)
        })
}

/// What decides the tools of `agent` besides its stored rights, as the vault says now: its rows of
/// `agent_file_permissions` (US5) and the native actions that serve external agents. A failed read
/// opens no data scope.
pub async fn reach(core: &Core, agent: Uuid) -> Reach {
    let id = agent.to_string();
    let data = core
        .db
        .read(move |q| {
            Ok((
                permissions::grants_of(q, &id)?,
                permissions::contacts_of(q, &id)?,
            ))
        })
        .await
        .map(|(files, contacts)| DataRights::of(&files, &contacts))
        .unwrap_or_else(|error| {
            log::warn!("mcp server: could not read an agent's file permissions: {error}");
            DataRights::default()
        });
    Reach {
        data,
        native: core.native.external_ids(),
    }
}

/// Runs `tool_name` for `agent` when its rights and its approval mode let it, and logs the call.
/// `seen` says the agent's tool list held the tool during this connection: when it is gone since,
/// its extension was removed and the call is `unavailable` (US4 scenario 4).
pub async fn call(
    core: &Core,
    agent: &SessionAgent,
    link: &Link,
    tool_name: &str,
    arguments: Option<Map<String, Value>>,
    seen: bool,
    cancel: &CancellationToken,
) -> CallToolResult {
    let catalog = core.catalog.snapshot();
    let tools = core.extension_tools.snapshot();
    let reach = reach(core, agent.id).await;
    let input = Value::Object(arguments.unwrap_or_default());
    let command = core.command_tool.as_ref();
    let known = find(&catalog, &tools, command, tool_name);
    let shown = redact(&known.map_or(Value::Null, Callee::input_schema), &input);
    let mut entry = NewEntry {
        tool: tool_name.to_owned(),
        effect: known.map(Callee::effect),
        target: target_of(&shown),
        input: Some(shown.to_string()),
        outcome: Outcome::DeniedRights,
        asked: false,
    };
    let callee = grants::check(&agent.rights, &reach, &catalog, tool_name)
        .map(Callee::Action)
        .or_else(|| {
            grants::check_extension_tool(&agent.rights, &tools, tool_name).map(Callee::Extension)
        })
        .or_else(|| command_tool::check(&agent.rights, command, tool_name).map(Callee::Command));
    let Some(callee) = callee else {
        let result = if known.is_none() && seen {
            entry.outcome = Outcome::Unavailable;
            unavailable()
        } else {
            not_allowed()
        };
        record(core, agent.id, &entry).await;
        return result;
    };
    let Some(_slot) = core
        .slots
        .acquire(agent.id, core.limits.max_calls, core.limits.call_wait)
        .await
    else {
        entry.outcome = Outcome::TooMany;
        record(core, agent.id, &entry).await;
        return error_result(
            "too_many_calls",
            "Too many calls of this agent are running.",
            None,
        );
    };
    let decision = match callee {
        Callee::Action(def) => {
            let confirms_itself = native_executor(core, def).is_some_and(|executor| {
                def.confirms_itself
                    && executor
                        .confirming_action_ids()
                        .contains(&def.action_id.as_str())
            });
            call_gate(agent.mode, def, confirms_itself)
        }
        Callee::Extension(tool) | Callee::Command(tool) => {
            extension_gate(agent.mode, tool.risk_class(), tool.always_ask())
        }
    };
    let result = match decision {
        Decision::Deny => {
            entry.outcome = Outcome::DeniedPlan;
            error_result(
                "plan_mode",
                "The approval mode of this agent only lets reading actions run.",
                None,
            )
        }
        Decision::Allow => run(core, agent, callee, input, cancel, &mut entry).await,
        Decision::Ask => {
            let question = question(agent, callee, entry.target.clone(), shown);
            match ask(core, &question, cancel).await {
                Ok(CallDecision::Allow) => {
                    entry.asked = true;
                    run(core, agent, callee, input, cancel, &mut entry).await
                }
                Ok(CallDecision::Deny) => {
                    entry.asked = true;
                    entry.outcome = Outcome::DeniedUser;
                    error_result("user_declined", "The user declined this call.", None)
                }
                Err(PromptEnd::Expired) => {
                    entry.asked = true;
                    entry.outcome = Outcome::NoAnswer;
                    error_result("no_answer", "The user did not answer in time.", None)
                }
                // A revocation ends the prompt with a refusal (FR-027).
                Err(PromptEnd::Cancelled) if link.reason() == Some(CloseReason::Revoked) => {
                    entry.outcome = Outcome::DeniedRights;
                    not_allowed()
                }
                Err(PromptEnd::Cancelled | PromptEnd::NoWindow) => {
                    entry.outcome = Outcome::Unavailable;
                    unavailable()
                }
            }
        }
    };
    record(core, agent.id, &entry).await;
    result
}

/// Asks the window and closes the prompt there when it ends without an answer.
async fn ask(
    core: &Core,
    question: &CallQuestion,
    cancel: &CancellationToken,
) -> Result<CallDecision, PromptEnd> {
    let mut request_id = String::new();
    let answer = core
        .calls
        .ask(question, cancel, |id| request_id = id.to_owned())
        .await;
    let reason = match &answer {
        Ok(_) => PromptClosedReason::Answered,
        Err(PromptEnd::Expired) => PromptClosedReason::Timeout,
        Err(_) => PromptClosedReason::Cancelled,
    };
    (core.notify)(Notice::PromptClosed(PromptClosed { request_id, reason }));
    answer
}

/// What the call prompt shows about `callee`. A tool of an extension is named with its extension.
fn question(
    agent: &SessionAgent,
    callee: Callee<'_>,
    target: Option<String>,
    shown: Value,
) -> CallQuestion {
    let (action_id, titles, secret) = match callee {
        Callee::Action(def) => (
            def.action_id.clone(),
            (&def.titles).into(),
            def.scope == SECRET_SCOPE,
        ),
        Callee::Extension(tool) => {
            let titles = tool.origin().map_or_else(
                || CallTitles {
                    de: String::new(),
                    en: String::new(),
                },
                |origin| CallTitles {
                    de: format!("{}: {}", origin.extension_name, origin.tool_title_de),
                    en: format!("{}: {}", origin.extension_name, origin.tool_title_en),
                },
            );
            (tool.name().to_owned(), titles, false)
        }
        Callee::Command(tool) => (tool.name().to_owned(), command_tool::titles(), false),
    };
    CallQuestion {
        agent_id: agent.id.to_string(),
        client_name: agent.client_name.clone(),
        action_id,
        titles,
        effect: CallEffect::from(callee.effect()),
        target,
        input: shown,
        secret,
    }
}

async fn run(
    core: &Core,
    agent: &SessionAgent,
    callee: Callee<'_>,
    input: Value,
    cancel: &CancellationToken,
    entry: &mut NewEntry,
) -> CallToolResult {
    let def = match callee {
        Callee::Action(def) => def,
        Callee::Extension(tool) => {
            // The extension's frame learns nothing about the agent (research R5).
            let (result, outcome) =
                extension_tools::result(tool.execute(input, cancel.clone()).await);
            entry.outcome = outcome;
            return result;
        }
        Callee::Command(tool) => {
            let (result, outcome) = command_tool::result(tool.execute(input, cancel.clone()).await);
            entry.outcome = outcome;
            return result;
        }
    };
    if let Some(executor) = native_executor(core, def) {
        let call = NativeCall {
            caller: Caller::ExternalAgent {
                id: agent.id.to_string(),
            },
            mode: agent.mode.permission_mode(),
        };
        let (result, outcome) = tokio::select! {
            biased;
            _ = cancel.cancelled() => (unavailable(), Outcome::Unavailable),
            reply = executor.run(&def.action_id, input, &call, cancel) => {
                if let Some(target) = reply.as_ref().ok().and_then(|reply| reply.target.clone()) {
                    entry.target = Some(target);
                }
                native_result(reply)
            }
        };
        entry.outcome = outcome;
        return result;
    }
    let caller = ActionCaller::ExternalAgent {
        agent_id: agent.id.to_string(),
    };
    let reply = core
        .bridge
        .call_as(caller, &def.action_id, input, cancel)
        .await;
    let (result, outcome) = into_result(reply);
    entry.outcome = outcome;
    result
}

/// The Rust executor of a native action (ADR 0011); it runs without the window.
fn native_executor(core: &Core, def: &AgentActionDef) -> Option<Arc<dyn NativeExecutor>> {
    (def.runner == Some(ActionRunner::Native))
        .then(|| core.native.get(&def.action_id))
        .flatten()
}

/// A native executor's answer as the agent sees it; a refusal of the caller reads as missing
/// rights.
fn native_result(reply: Result<NativeReply, NativeError>) -> (CallToolResult, Outcome) {
    match reply {
        Ok(reply) => (CallToolResult::structured(reply.result), Outcome::Ok),
        Err(error) if error.code == "forbidden_for_agents" => {
            (not_allowed(), Outcome::DeniedRights)
        }
        Err(error) => {
            // A native action that asks the user itself reports how that ended (spec 065).
            let outcome = match error.code.as_str() {
                "site_not_allowed" => Outcome::DeniedSite,
                // The user stopped the agent's browser session (spec 065 FR-016).
                "user_declined" | "stopped" => Outcome::DeniedUser,
                "no_answer" => Outcome::NoAnswer,
                "paused" => Outcome::Paused,
                "not_allowed" => Outcome::DeniedRights,
                _ => Outcome::Error,
            };
            (
                error_result(
                    &error.code,
                    &error.message,
                    error.field.map(|f| ("field", Value::String(f))),
                ),
                outcome,
            )
        }
    }
}

/// Writes one log entry; a failed write only warns, the call has happened.
async fn record(core: &Core, agent: Uuid, entry: &NewEntry) {
    let device = core.device;
    let entry = entry.clone();
    let now = now_millis();
    if let Err(error) = core
        .db
        .write(move |tx| agent_log::write(tx, device, agent, &entry, now))
        .await
    {
        log::warn!("mcp server: could not write the agent log: {error}");
    }
}

/// The runner's reply as the agent sees it (contracts/mcp-surface.md, "Ergebnisse"), and how the
/// log names it.
pub fn into_result(reply: ActionReply) -> (CallToolResult, Outcome) {
    match reply {
        ActionReply::Ok { result } => (CallToolResult::structured(result), Outcome::Ok),
        ActionReply::NeedsChoice {
            field,
            message,
            options,
        } => {
            let options = serde_json::to_value(options).unwrap_or(Value::Array(Vec::new()));
            let mut body = json!({ "code": "needs_choice", "message": message, "field": field });
            body["options"] = options;
            // Nothing ran; the agent calls again with a choice.
            (CallToolResult::structured_error(body), Outcome::Error)
        }
        ActionReply::Err {
            code,
            field,
            message,
        } => match code.as_str() {
            // The runner refused the caller: the same answer as missing rights.
            "forbidden_for_agents" | "unknown_action" => (not_allowed(), Outcome::DeniedRights),
            "action_timeout" | "action_unavailable" | "tool_call_cancelled" => {
                (unavailable(), Outcome::Unavailable)
            }
            _ => (
                error_result(&code, &message, field.map(|f| ("field", Value::String(f)))),
                Outcome::Error,
            ),
        },
    }
}
