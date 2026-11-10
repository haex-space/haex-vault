//! The two commands of the action bridge (spec 032, contracts/tauri-commands.md): the frontend
//! pushes the definitions of the actions the built-in agent may call, and answers the calls the
//! bridge sends out. Both need an open vault: they are not on the gate's app-scoped list.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use super::session::ChatState;
use super::tools::action_bridge::ActionOutcomeWire;
use super::tools::action_tool::{
    is_valid_tool_name, schema_in_subset, ActionRunner, ActionTool, AgentActionDef, ACTION_SOURCE,
};
use super::tools::ask_user::{AskUserTool, ASK_USER_TOOL_NAME};
use super::tools::domains::{AgentDomain, NO_DOMAIN};
use super::tools::find_actions::{FindActionsTool, FIND_ACTIONS_TOOL_NAME};
use super::tools::native_action::NativeActionTool;
use super::tools::{Tool, ToolRegistry};
use crate::error::{HolziError, Result};

/// The scope no agent may ever call (spec 020 FR-032).
pub const GUARDRAILS_SCOPE: &str = "guardrails";

#[cfg(test)]
#[path = "action_commands_tests.rs"]
mod tests;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetAgentActionsArgs {
    pub actions: Vec<AgentActionDef>,
    /// The domains of the actions (spec 082 contracts/domains.md); none from an older frontend.
    #[serde(default)]
    pub domains: Vec<AgentDomain>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetAgentActionsResult {
    pub registered: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RespondActionCallArgs {
    pub request_id: Uuid,
    pub outcome: ActionOutcomeWire,
}

/// Replaces every tool of the source `action` by the actions the built-in agent may call; the
/// host-CLI and MCP tools stay. The full list, including actions only external agents may call,
/// goes to the [`ChatState::action_catalog`] the MCP server offers from (spec 021 research R4).
/// Validates first, so a bad push leaves registry and catalog as they were. A native action needs
/// an executor and every executor its definition (ADR 0011). No agent action may lie in the scope
/// `guardrails` (spec 020 FR-032).
pub fn register_agent_actions(chat: &ChatState, actions: Vec<AgentActionDef>) -> Result<usize> {
    let mut seen = HashSet::new();
    let mut native = HashSet::new();
    for def in &actions {
        if !is_valid_tool_name(&def.tool_name) {
            return Err(HolziError::InvalidInput {
                reason: format!("invalid tool name: {}", def.tool_name),
            });
        }
        if !seen.insert(def.tool_name.as_str()) {
            return Err(HolziError::InvalidInput {
                reason: format!("duplicate tool name: {}", def.tool_name),
            });
        }
        if def.tool_name == FIND_ACTIONS_TOOL_NAME || def.tool_name == ASK_USER_TOOL_NAME {
            return Err(HolziError::InvalidInput {
                reason: format!("reserved tool name: {}", def.tool_name),
            });
        }
        if !schema_in_subset(&def.input_schema) {
            return Err(HolziError::InvalidInput {
                reason: format!(
                    "input schema of {} is outside the supported subset",
                    def.tool_name
                ),
            });
        }
        if def.scope == GUARDRAILS_SCOPE {
            return Err(HolziError::InvalidInput {
                reason: format!("guardrail action offered to agents: {}", def.action_id),
            });
        }
        if def.runner == Some(ActionRunner::Native) {
            if chat.native_actions.get(&def.action_id).is_none() {
                return Err(HolziError::InvalidInput {
                    reason: format!("native action without executor: {}", def.action_id),
                });
            }
            native.insert(def.action_id.as_str());
        }
    }
    if let Some(id) = chat
        .native_actions
        .ids()
        .into_iter()
        .find(|id| !native.contains(id))
    {
        return Err(HolziError::InvalidInput {
            reason: format!("native executor without definition: {id}"),
        });
    }
    let mut registry = chat
        .tool_registry
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    registry.remove_source(ACTION_SOURCE);
    let count = register_action_tools(
        &mut registry,
        &actions,
        &chat.tool_registry,
        |def| match chat.native_actions.get(&def.action_id) {
            Some(executor) if def.runner == Some(ActionRunner::Native) => Arc::new(
                NativeActionTool::new(def, executor, chat.native_actions.clone()),
            ),
            _ => Arc::new(ActionTool::new(def, chat.action_bridge.clone())),
        },
    );
    // Replace the catalog while the registry is still locked, so two overlapping pushes cannot
    // leave the registry with one and the catalog with the other.
    chat.action_catalog.replace(actions);
    drop(registry);
    Ok(count)
}

/// Registers the `actions` the built-in agent may call into `registry`, each as `tool_of` makes it,
/// then the search over them and over the extensions of `search_over`, and `ask_user`; returns how
/// many actions it registered. Shared with the everyday measurement, so it holds the tools a chat
/// holds (spec 082 FR-001).
pub(crate) fn register_action_tools(
    registry: &mut ToolRegistry,
    actions: &[AgentActionDef],
    search_over: &Arc<Mutex<ToolRegistry>>,
    tool_of: impl Fn(AgentActionDef) -> Arc<dyn Tool>,
) -> usize {
    let builtin: Vec<AgentActionDef> = actions
        .iter()
        .filter(|def| def.builtin_agent_callable)
        .cloned()
        .collect();
    let count = builtin.len();
    for def in builtin.iter().cloned() {
        registry.register(tool_of(def));
    }
    if !builtin.is_empty() {
        registry.register(Arc::new(FindActionsTool::with_extensions(
            builtin,
            search_over,
        )));
        registry.register(Arc::new(AskUserTool));
    }
    count
}

/// Checks what the routing prompt relies on: unique keys and names, no extension key, and no
/// domain called like the answer for none.
pub fn validate_domains(domains: &[AgentDomain]) -> Result<()> {
    let mut keys = HashSet::new();
    let mut names = HashSet::new();
    for domain in domains {
        let bad = domain.key.starts_with("x_")
            || domain.name == NO_DOMAIN
            || domain.name.is_empty()
            || !keys.insert(domain.key.as_str())
            || !names.insert(domain.name.as_str());
        if bad {
            return Err(HolziError::InvalidInput {
                reason: format!("invalid agent domain: {}", domain.key),
            });
        }
    }
    Ok(())
}

/// Offers the actions the frontend lists to the built-in agent as tools, with their domains.
#[tauri::command]
pub async fn set_agent_actions(
    chat: State<'_, ChatState>,
    args: SetAgentActionsArgs,
) -> Result<SetAgentActionsResult> {
    validate_domains(&args.domains)?;
    let registered = register_agent_actions(&chat, args.actions)?;
    chat.tool_registry
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .set_domains(args.domains);
    Ok(SetAgentActionsResult { registered })
}

/// The frontend's answer to an `action-call-request`. An unknown or late request id is not an
/// error: the turn may have been cancelled, or the call timed out, in the meantime.
#[tauri::command]
pub async fn respond_action_call(
    chat: State<'_, ChatState>,
    args: RespondActionCallArgs,
) -> Result<()> {
    chat.action_bridge
        .resolve(args.request_id, args.outcome.into());
    Ok(())
}
