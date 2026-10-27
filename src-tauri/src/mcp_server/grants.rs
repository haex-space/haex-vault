//! What a registered agent may do (spec 021 research R3, FR-008, FR-012 to FR-016). One function
//! decides the tool list and every call, so a tool an agent does not see is also a tool it cannot
//! call. This delivery has the rights group "holzi bedienen" (`operate`): per scope of the action
//! catalog nothing, reading, or reading and changing. The scope `guardrails` is never a right.
//! Later groups (extensions, files, passwords, commands, the browser) add fields that are off when
//! missing (FR-008).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::chat::action_commands::GUARDRAILS_SCOPE;
use crate::chat::tools::action_tool::{ActionEffect, AgentActionDef};

/// The scopes of the group "holzi bedienen" (`src/lib/actions/scopes.ts`): the window manager, the
/// chat and the settings an agent may reach. Files, contacts and passwords are their own groups.
pub const OPERATE_SCOPES: [&str; 8] = [
    "wm.layout",
    "wm.navigation",
    "wm.read",
    "chat.read",
    "chat.write",
    "settings.read",
    "settings.device",
    "settings.models",
];

/// How far an agent may act in one scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub enum OperateLevel {
    /// Only actions that read.
    Read,
    /// Reading, changing and destroying; the approval mode decides what asks first.
    Write,
}

impl OperateLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "read" => Some(Self::Read),
            "write" => Some(Self::Write),
            _ => None,
        }
    }
}

/// The rights of one agent. A scope that is missing is "nothing".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/bindings/")]
pub struct Rights {
    pub operate: BTreeMap<String, OperateLevel>,
}

/// What a new agent may do until the user changes it (FR-009): read in every scope of
/// "holzi bedienen", nothing else.
pub fn default_rights() -> Rights {
    Rights {
        operate: OPERATE_SCOPES
            .iter()
            .map(|scope| ((*scope).to_owned(), OperateLevel::Read))
            .collect(),
    }
}

/// Rights the window sent that holzi does not store.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidRights {
    /// `guardrails` or a scope outside "holzi bedienen" (FR-016).
    #[error("the scope {0} cannot be granted")]
    Scope(String),
}

/// Refuses a scope no agent may get, the guardrails above all (FR-016).
pub fn validate(rights: &Rights) -> Result<(), InvalidRights> {
    match rights
        .operate
        .keys()
        .find(|scope| !OPERATE_SCOPES.contains(&scope.as_str()))
    {
        Some(scope) => Err(InvalidRights::Scope(scope.clone())),
        None => Ok(()),
    }
}

/// Whether `rights` cover `def`. `changes_allowed` is `false` until the approval prompts exist
/// (tasks.md shipping note, T050): then only reading actions are offered, whatever the level.
fn covers(rights: &Rights, def: &AgentActionDef, changes_allowed: bool) -> bool {
    if def.scope == GUARDRAILS_SCOPE {
        return false;
    }
    match rights.operate.get(&def.scope) {
        None => false,
        Some(OperateLevel::Read) => def.effect == ActionEffect::Read,
        Some(OperateLevel::Write) => changes_allowed || def.effect == ActionEffect::Read,
    }
}

/// The actions of `catalog` an agent with `rights` sees (FR-012, FR-013).
pub fn offers<'a>(
    rights: &Rights,
    catalog: &'a [AgentActionDef],
    changes_allowed: bool,
) -> Vec<&'a AgentActionDef> {
    catalog
        .iter()
        .filter(|def| covers(rights, def, changes_allowed))
        .collect()
}

/// The action behind `tool_name`, if the agent may call it (FR-014); checked again on every call,
/// independent of the list the agent saw.
pub fn check<'a>(
    rights: &Rights,
    catalog: &'a [AgentActionDef],
    tool_name: &str,
    changes_allowed: bool,
) -> Option<&'a AgentActionDef> {
    catalog
        .iter()
        .find(|def| def.tool_name == tool_name)
        .filter(|def| covers(rights, def, changes_allowed))
}
