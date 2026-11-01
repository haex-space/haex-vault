//! What an external agent's tool call reaches (spec 021 FR-014, FR-018, FR-012): an action of the
//! catalog, a tool of an extension (US4) or holzi's command tool (US5). Found by the tool's name,
//! with or without the agent's rights, so the log names the effect of a refused call too.

use std::sync::Arc;

use serde_json::Value;

use super::command_tool;
use super::grants::{self, Reach, Rights};
use super::prompts::{CallTitles, SECRET_SCOPE};
use crate::chat::tools::action_tool::{ActionEffect, AgentActionDef};
use crate::chat::tools::{RiskClass, Tool};

/// The tools a call can reach, as the server holds them now.
pub(super) struct Tools<'a> {
    pub catalog: &'a [AgentActionDef],
    pub extensions: &'a [Arc<dyn Tool>],
    /// `None` where the device runs no commands.
    pub command: Option<&'a Arc<dyn Tool>>,
}

#[derive(Clone, Copy)]
pub(super) enum Callee<'a> {
    Action(&'a AgentActionDef),
    Extension(&'a Arc<dyn Tool>),
    Command(&'a Arc<dyn Tool>),
}

impl<'a> Callee<'a> {
    /// The tool behind `tool_name`, whether or not the agent may call it.
    pub fn find(tools: &Tools<'a>, tool_name: &str) -> Option<Self> {
        tools
            .catalog
            .iter()
            .find(|def| def.tool_name == tool_name)
            .map(Self::Action)
            .or_else(|| {
                tools
                    .extensions
                    .iter()
                    .find(|tool| tool.name() == tool_name)
                    .map(Self::Extension)
            })
            .or_else(|| {
                tools
                    .command
                    .filter(|tool| tool.name() == tool_name)
                    .map(Self::Command)
            })
    }

    /// The tool behind `tool_name`, if `rights` let the agent call it (FR-014); checked again on
    /// every call, independent of the list the agent saw.
    pub fn permitted(
        tools: &Tools<'a>,
        rights: &Rights,
        reach: &Reach,
        tool_name: &str,
    ) -> Option<Self> {
        grants::check(rights, reach, tools.catalog, tool_name)
            .map(Self::Action)
            .or_else(|| {
                grants::check_extension_tool(rights, tools.extensions, tool_name)
                    .map(Self::Extension)
            })
            .or_else(|| command_tool::check(rights, tools.command, tool_name).map(Self::Command))
    }

    pub fn input_schema(self) -> Value {
        match self {
            Self::Action(def) => def.input_schema.clone(),
            Self::Extension(tool) | Self::Command(tool) => tool.input_schema(),
        }
    }

    pub fn effect(self) -> ActionEffect {
        match self {
            Self::Action(def) => def.effect,
            Self::Extension(tool) | Self::Command(tool) => match tool.risk_class() {
                RiskClass::Safe => ActionEffect::Read,
                RiskClass::Change => ActionEffect::Write,
                RiskClass::Risky => ActionEffect::Destructive,
            },
        }
    }

    /// What the call prompt names it: its id, its titles and whether it hands out a secret. A tool
    /// of an extension is named with its extension.
    pub fn described(self) -> (String, CallTitles, bool) {
        match self {
            Self::Action(def) => (
                def.action_id.clone(),
                (&def.titles).into(),
                def.scope == SECRET_SCOPE,
            ),
            Self::Extension(tool) => {
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
            Self::Command(tool) => (tool.name().to_owned(), command_tool::titles(), false),
        }
    }
}
