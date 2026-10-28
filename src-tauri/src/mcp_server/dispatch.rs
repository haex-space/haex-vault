//! One tool call of an external agent (spec 021 FR-014, FR-015, FR-021 to FR-023, FR-028, FR-029,
//! contracts/mcp-surface.md): checked again against the agent's rights, held to four running calls
//! per agent, let through, asked about or refused by the agent's approval mode, run by the window's
//! action runner as caller `externalAgent`, written to the agent log, and turned into an MCP result.
//! A refusal never says whether a target exists.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use rmcp::model::{CallToolResult, Tool, ToolAnnotations};
use serde_json::{json, Map, Value};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::agent_log::{self, NewEntry, Outcome};
use super::connection::now_millis;
use super::grants;
use super::hello::CloseReason;
use super::live::Link;
use super::prompts::{call_gate, CallDecision, CallQuestion, PromptClosed, PromptClosedReason};
use super::redact::{redact, target_of};
use super::server::{Core, Notice};
use super::session::SessionAgent;
use crate::chat::tools::action_bridge::{ActionCaller, ActionReply};
use crate::chat::tools::action_tool::{ActionEffect, AgentActionDef};
use crate::chat::tools::permission::Decision;
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

fn unavailable() -> CallToolResult {
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

/// Runs `tool_name` for `agent` when its rights and its approval mode let it, and logs the call.
pub async fn call(
    core: &Core,
    agent: &SessionAgent,
    link: &Link,
    tool_name: &str,
    arguments: Option<Map<String, Value>>,
    cancel: &CancellationToken,
) -> CallToolResult {
    let catalog = core.catalog.snapshot();
    let input = Value::Object(arguments.unwrap_or_default());
    let known = catalog.iter().find(|def| def.tool_name == tool_name);
    let shown = redact(known.map_or(&Value::Null, |def| &def.input_schema), &input);
    let mut entry = NewEntry {
        tool: tool_name.to_owned(),
        effect: known.map(|def| def.effect),
        target: target_of(&shown),
        input: Some(shown.to_string()),
        outcome: Outcome::DeniedRights,
        asked: false,
    };
    let Some(def) = grants::check(&agent.rights, &catalog, tool_name) else {
        record(core, agent.id, &entry).await;
        return not_allowed();
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
    let result = match call_gate(agent.mode, def) {
        Decision::Deny => {
            entry.outcome = Outcome::DeniedPlan;
            error_result(
                "plan_mode",
                "The approval mode of this agent only lets reading actions run.",
                None,
            )
        }
        Decision::Allow => run(core, agent, def, input, cancel, &mut entry).await,
        Decision::Ask => {
            let question = CallQuestion {
                agent_id: agent.id.to_string(),
                client_name: agent.client_name.clone(),
                action_id: def.action_id.clone(),
                titles: (&def.titles).into(),
                effect: def.effect.into(),
                target: entry.target.clone(),
                input: shown,
                secret: def.scope == super::prompts::SECRET_SCOPE,
            };
            match ask(core, &question, cancel).await {
                Ok(CallDecision::Allow) => {
                    entry.asked = true;
                    run(core, agent, def, input, cancel, &mut entry).await
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

async fn run(
    core: &Core,
    agent: &SessionAgent,
    def: &AgentActionDef,
    input: Value,
    cancel: &CancellationToken,
    entry: &mut NewEntry,
) -> CallToolResult {
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
